use std::pin::pin;

use async_stream::stream;
use futures_util::Stream;
use tokio::{
    sync::{broadcast, mpsc, oneshot},
    task::JoinHandle,
};
use tokio_stream::StreamExt;

use super::{
    Event,
    history::{Chat, ChatError, Message},
};

/// A shared chat. Allows multiple clients to communicate with each other by writing and reading
/// messages to the same chat.
#[cfg_attr(test, double_trait::dummies)]
pub trait SharedChat: Sized {
    /// A stream which yields future and past events of the chat.
    ///
    /// # Parameters
    ///
    /// - `last_event_id`: The last event id received by the client. Event ids are ordered. The
    ///   stream will only yield events with an id greater than `last_event_id`, so that clients
    ///   only receive events they have not yet seen. Use `0` to receive all events from the
    ///   beginning of the chat. Filtering of events is only applied to historic events. Future
    ///   events will always be delivered.
    fn events(self, last_event_id: u64) -> impl Stream<Item = Event> + Send;

    /// Add a new message to the chat.
    fn add_message(
        &mut self,
        message: Message,
    ) -> impl Future<Output = Result<(), ChatError>> + Send;
}

/// Can be used to create multiple instances of [`ChatClient`] which provide an API to interact with
/// a shared chat. The runtime takes care that messages are forwarded between different clients.
pub struct ChatRuntime {
    sender: mpsc::Sender<ActorMsg>,
    join_handle: JoinHandle<()>,
}

impl ChatRuntime {
    pub fn new(history: impl Chat + Send + 'static) -> Self {
        let (sender, receiver) = mpsc::channel(5);
        let actor = Actor::new(history, receiver);
        let join_handle = tokio::spawn(async move { actor.run().await });
        ChatRuntime {
            sender,
            join_handle,
        }
    }

    /// A client which implements the [`SharedChat`] trait.
    pub fn client(&self) -> ChatClient {
        ChatClient {
            sender: self.sender.clone(),
        }
    }

    /// Shuts down the chat runtime. In order for this to complete, all clients must have been
    /// dropped.
    pub async fn shutdown(self) {
        // At this point we should be the only owner of the sender, since all clients should have
        // been dropped. This might be unecessary restrictive if we want to shutdown things in
        // parallel. Right now however the invariant holds. The panic might save us some time if we
        // forget to clean up all senders in a test.
        debug_assert_eq!(self.sender.strong_count(), 1);
        // We drop the sender, to signal to the actor thread that it can no longer receive messages
        // and should stop.
        drop(self.sender);
        self.join_handle.await.unwrap();
    }
}

#[derive(Clone)]
pub struct ChatClient {
    sender: mpsc::Sender<ActorMsg>,
}

impl SharedChat for ChatClient {
    fn events(self, mut last_event_id: u64) -> impl Stream<Item = Event> + Send {
        stream! {
            loop {
                let (responder, response) = oneshot::channel();
                self.sender
                    .send(ActorMsg::ReadEvents{ responder, last_event_id})
                    .await
                    .expect("Actor must outlive client.");
                let mut events = pin!(response.await.unwrap().into_stream());
                while let Some(event) = events.next().await {
                    last_event_id = event.id;
                    yield event;
                }
            }
        }
    }

    async fn add_message(&mut self, message: Message) -> Result<(), ChatError> {
        let (responder, response) = oneshot::channel();
        self.sender
            .send(ActorMsg::AddMessage { message, responder })
            .await
            .expect("Actor must outlive client.");
        response.await.unwrap()
    }
}

enum ActorMsg {
    ReadEvents {
        responder: oneshot::Sender<Events>,
        last_event_id: u64,
    },
    AddMessage {
        message: Message,
        responder: oneshot::Sender<Result<(), ChatError>>,
    },
}

/// Transports a set of events from the actor to the client.
enum Events {
    History(Vec<Event>),
    Current(broadcast::Receiver<Event>),
}

impl Events {
    pub fn into_stream(self) -> impl Stream<Item = Event> + Send {
        stream! {
            match self {
                Events::History(history) => {
                    for event in history {
                        yield event;
                    }
                },
                Events::Current(mut current) => {
                    loop {
                        match current.recv().await {
                            Ok(event) => {
                                yield event;
                            },
                            // Slow receiver. Receiver is lagging and messages have been dropped.
                            Err(broadcast::error::RecvError::Lagged(_skipped)) => {
                                break;
                            },
                            Err(broadcast::error::RecvError::Closed) => {
                                // Runtime outlives clients
                                unreachable!("Currently Sender must always outlive receiver.")
                            }
                        }
                    }
                }
            }
        }
    }
}

struct Actor<H> {
    /// The chat's persistent state.
    history: H,
    /// Used to broadcast new events to clients who have caught up with the chat.
    current: broadcast::Sender<Event>,
    receiver: mpsc::Receiver<ActorMsg>,
}

impl<H: Chat> Actor<H> {
    pub fn new(history: H, receiver: mpsc::Receiver<ActorMsg>) -> Self {
        let (current, _) = broadcast::channel(10);
        Actor {
            receiver,
            history,
            current,
        }
    }

    pub async fn run(mut self) {
        while let Some(msg) = self.receiver.recv().await {
            self.handle_message(msg);
        }
    }

    pub fn handle_message(&mut self, msg: ActorMsg) {
        match msg {
            ActorMsg::ReadEvents {
                responder,
                last_event_id,
            } => {
                let remaining_history = self.history.events_since(last_event_id);
                // We ignore send errors, since it only happens if the receiver has been dropped. In
                // that case the receiver is no longer interested in the response, anyway.
                let events = if remaining_history.is_empty() {
                    let current_receiver = self.current.subscribe();
                    Events::Current(current_receiver)
                } else {
                    Events::History(remaining_history)
                };
                let _ = responder.send(events);
            }
            ActorMsg::AddMessage { message, responder } => {
                let result = match self.history.record_message(message) {
                    // New message — broadcast to listening clients. Only fails if there are no
                    // active receivers, which is fine.
                    Ok(Some(event)) => {
                        let _ = self.current.send(event);
                        Ok(())
                    }
                    // Duplicate — silently accepted, nothing to broadcast
                    Ok(None) => Ok(()),
                    // Conflict — forward error to the client
                    Err(err) => Err(err),
                };
                let _ = responder.send(result);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use double_trait::Dummy;
    use futures_util::StreamExt;
    use std::{
        mem::take,
        sync::{Arc, Mutex},
        time::{Duration, SystemTime},
    };
    use tokio::time::timeout;
    use uuid::Uuid;

    #[tokio::test]
    async fn events_forwards_history() {
        // Given
        let canned = vec![
            Event {
                id: 1,
                message: Message {
                    id: "019c0ab6-9d11-75ef-ab02-60f070b1582a".parse().unwrap(),
                    sender: "Alice".to_string(),
                    content: "One".to_string(),
                },
                timestamp: SystemTime::UNIX_EPOCH + Duration::from_secs(1_000_000_000),
            },
            Event {
                id: 2,
                message: Message {
                    id: "019c0ab6-9d11-7a5b-abde-cb349e5fd995".parse().unwrap(),
                    sender: "Bob".to_string(),
                    content: "Two".to_string(),
                },
                timestamp: SystemTime::UNIX_EPOCH + Duration::from_secs(1_000_000_001),
            },
        ];
        struct HistoryStub(Vec<Event>);
        impl Chat for HistoryStub {
            fn events_since(&self, _last_event_id: u64) -> Vec<Event> {
                self.0.clone()
            }
        }
        let chat = ChatRuntime::new(HistoryStub(canned.clone()));

        // When
        let events = chat.client().events(0).take(2).collect::<Vec<_>>().await;

        // Then
        assert_eq!(events, canned);

        // Cleanup
        chat.shutdown().await;
    }

    #[tokio::test]
    async fn add_message_forwards_to_history() {
        // Given
        let history = HistorySpy::new();
        let chat = ChatRuntime::new(history.clone());

        // When
        let msg = Message {
            id: "019c0ab6-9d11-75ef-ab02-60f070b1582a".parse().unwrap(),
            sender: "Alice".to_string(),
            content: "Hello".to_string(),
        };
        chat.client().add_message(msg.clone()).await.unwrap();

        // Then
        let recorded = history.take_recorded_messages();
        assert_eq!(recorded.len(), 1);
        assert_eq!(recorded[0], msg);

        // Cleanup
        chat.shutdown().await;
    }

    #[tokio::test]
    async fn duplicate_message_is_not_broadcast() {
        // Given a history that treats one specific message ID as a duplicate
        let duplicate_id: Uuid = "019c0ab6-9d11-75ef-ab02-60f070b1582a".parse().unwrap();
        let fresh_id: Uuid = "019c0ab6-9d11-7a5b-abde-cb349e5fd995".parse().unwrap();
        struct HistoryStub {
            duplicate_id: Uuid,
        }
        impl Chat for HistoryStub {
            fn events_since(&self, _last_event_id: u64) -> Vec<Event> {
                Vec::new()
            }
            fn record_message(&mut self, message: Message) -> Result<Option<Event>, ChatError> {
                if message.id == self.duplicate_id {
                    Ok(None)
                } else {
                    Ok(Some(Event {
                        id: 1,
                        message,
                        timestamp: SystemTime::UNIX_EPOCH,
                    }))
                }
            }
        }
        let chat = ChatRuntime::new(HistoryStub { duplicate_id });

        // and a receiver subscribed to live broadcast
        let mut events = chat.client().events(0).boxed();
        let mut next_event = tokio_test::task::spawn(events.next());
        assert!(next_event.poll().is_pending());

        // When a sender sends a duplicate followed by a fresh message
        let mut sender = chat.client();
        sender
            .add_message(Message {
                id: duplicate_id,
                sender: "dummy".to_owned(),
                content: "dummy".to_owned(),
            })
            .await
            .unwrap();
        sender
            .add_message(Message {
                id: fresh_id,
                sender: "dummy".to_owned(),
                content: "dummy".to_owned(),
            })
            .await
            .unwrap();

        // Then the first event received is the fresh message — the duplicate was not broadcast
        let event = timeout(Duration::from_secs(1), next_event)
            .await
            .expect("timed out waiting for event")
            .unwrap();
        assert_eq!(event.message.id, fresh_id);

        // Cleanup
        drop(sender);
        drop(events);
        chat.shutdown().await;
    }

    #[tokio::test]
    async fn conflict_error_is_forwarded_to_client() {
        // Given a chat that reports any message as a conflict
        struct ChatSaboteur;
        impl Chat for ChatSaboteur {
            fn record_message(&mut self, _: Message) -> Result<Option<Event>, ChatError> {
                Err(ChatError::Conflict)
            }
        }
        let chat = ChatRuntime::new(ChatSaboteur);

        // When a message is sent
        let result = chat
            .client()
            .add_message(Message {
                id: "019c0ab6-9d11-75ef-ab02-60f070b1582a".parse().unwrap(),
                sender: "dummy".to_owned(),
                content: "dummy".to_owned(),
            })
            .await;

        // Then the error is forwarded to the client
        assert!(matches!(result, Err(ChatError::Conflict)));

        // Cleanup
        chat.shutdown().await;
    }

    #[tokio::test]
    async fn shutdown_completes_within_one_second() {
        // Given
        let chat = ChatRuntime::new(Dummy);

        // When
        let result = timeout(Duration::from_secs(1), chat.shutdown()).await;

        // Then
        assert!(result.is_ok(), "Shutdown did not complete within 1 second");
    }

    #[tokio::test]
    async fn event_stream_seamlessly_transitions_from_history_replay_to_live_broadcast() {
        // Given a history with one event
        fn canned_event() -> Event {
            Event {
                id: 1,
                message: Message {
                    id: "019c0ab6-9d11-75ef-ab02-60f070b1582a".parse().unwrap(),
                    sender: "Alice".to_string(),
                    content: "One".to_string(),
                },
                timestamp: SystemTime::UNIX_EPOCH + Duration::from_secs(1_000_000_000),
            }
        }

        struct HistoryDouble;
        impl Chat for HistoryDouble {
            fn events_since(&self, last_event_id: u64) -> Vec<Event> {
                if last_event_id == 0 {
                    vec![canned_event()]
                } else {
                    Vec::new()
                }
            }
            fn record_message(&mut self, message: Message) -> Result<Option<Event>, ChatError> {
                Ok(Some(Event {
                    id: 2,
                    message,
                    timestamp: SystemTime::UNIX_EPOCH,
                }))
            }
        }
        let chat = ChatRuntime::new(HistoryDouble);

        // When a client subscribes and consume the historic event
        let mut events_stream = chat.client().events(0).boxed();
        let historic = events_stream.next().await.unwrap();

        // and after that it waits for the next event
        let mut live = tokio_test::task::spawn(events_stream.next());
        debug_assert!(live.poll().is_pending());

        // while the client is waiting another client sends a message.
        let live_msg = Message {
            id: "019c0ab6-9d11-7a5b-abde-cb349e5fd995".parse().unwrap(),
            sender: "Bob".to_string(),
            content: "Two".to_string(),
        };
        chat.client().add_message(live_msg.clone()).await.unwrap();

        // Then we receive the live event within a reasonable time frame
        let live = timeout(Duration::from_secs(1), live)
            .await
            .expect("timed out waiting for live event")
            .unwrap();

        // The historic event matches the canned data
        assert_eq!(historic, canned_event());
        // Live event carries the message we just sent
        assert_eq!(live.message, live_msg);

        // Cleanup
        drop(events_stream);
        chat.shutdown().await;
    }

    #[tokio::test]
    async fn events_stream_delivers_new_history_on_re_request() {
        // Given: a history that grows between requests
        struct HistoryStub;
        impl Chat for HistoryStub {
            fn events_since(&self, last_event_id: u64) -> Vec<Event> {
                match last_event_id {
                    0 => vec![Event {
                        id: 1,
                        message: Message {
                            id: "019c0ab6-9d11-75ef-ab02-60f070b1582a".parse().unwrap(),
                            sender: "Alice".to_string(),
                            content: "One".to_string(),
                        },
                        timestamp: SystemTime::UNIX_EPOCH + Duration::from_secs(1_000_000_000),
                    }],
                    1 => vec![Event {
                        id: 2,
                        message: Message {
                            id: "019c0ab6-9d11-7a5b-abde-cb349e5fd995".parse().unwrap(),
                            sender: "Bob".to_string(),
                            content: "Two".to_string(),
                        },
                        timestamp: SystemTime::UNIX_EPOCH + Duration::from_secs(1_000_000_001),
                    }],
                    _ => Vec::new(),
                }
            }
        }
        let chat = ChatRuntime::new(HistoryStub);

        // When
        let events = chat.client().events(0).take(2).collect::<Vec<_>>().await;

        // Then
        assert_eq!(events[0].message.sender, "Alice");
        assert_eq!(events[1].message.sender, "Bob");

        // Cleanup
        chat.shutdown().await;
    }

    #[tokio::test]
    async fn events_passes_last_event_id_to_history() {
        // Given
        let history = HistorySpy::new();
        let spy = history.clone();
        let chat = ChatRuntime::new(history);

        // When requesting events with last_event_id = 42
        let _event = chat.client().events(42).boxed().next().await;

        // Then history was queried with the provided last_event_id
        let ids = spy.take_observed_last_event_ids();
        assert_eq!(ids[0], 42);

        // Cleanup
        chat.shutdown().await;
    }

    #[tokio::test]
    async fn state_is_shared_between_clients() {
        // Given two clients from the same runtime
        let history = HistorySpy::new();
        let spy = history.clone();
        let chat = ChatRuntime::new(history);
        let mut client_a = chat.client();
        let mut client_b = chat.client();

        // When each client sends a message
        let msg_a = Message {
            id: "019c0ab6-9d11-75ef-ab02-60f070b1582a".parse().unwrap(),
            sender: "Alice".to_string(),
            content: "From Alice".to_string(),
        };
        let msg_b = Message {
            id: "019c0ab6-9d11-7a5b-abde-cb349e5fd995".parse().unwrap(),
            sender: "Bob".to_string(),
            content: "From Bob".to_string(),
        };
        client_a.add_message(msg_a.clone()).await.unwrap();
        client_b.add_message(msg_b.clone()).await.unwrap();

        // Then both messages are recorded in the same history
        let recorded = spy.take_recorded_messages();
        assert_eq!(recorded, vec![msg_a, msg_b]);

        // Cleanup
        drop(client_a);
        drop(client_b);
        chat.shutdown().await;
    }

    /// Verifies that a client which is slow in receiving messages (pulling them from the stream)
    /// does not miss any messages. I.e. if a sender insertes a lot of messages in between a
    /// receiver pulling two events, the receiver will still receive all messages.
    ///
    /// Another way to look at this, is that this is the reverse of the seamless transition from
    /// replaying hisoric messages to broadcasting current ones. The receiver has been so slow that
    /// the current messages are now considered history and have to be fetched from it again.
    #[tokio::test]
    async fn slow_receiver() {
        // Given: a chat and two clients
        let chat = ChatRuntime::new(FakeHistory::new());
        let mut sender_client = chat.client();
        let receiver_client = chat.client();

        // And one message in the chat history
        sender_client
            .add_message(Message {
                id: Uuid::now_v7(),
                sender: "a".to_string(),
                content: "Initial message".to_string(),
            })
            .await
            .unwrap();

        // One of the clients has an event stream open, which already has received all messages in the
        // history so far (one in this case).
        let mut events_stream = receiver_client.events(0).boxed();
        events_stream.next().await; // Consume initial message

        // When: Sender sends a burst of messages while the reader does not pull them. While we
        // want to keep our test indepenend from the implementation, it might be helpful to know
        // that this is designed to set the reader in a lagged state.
        const NUM_MESSAGES_IN_BURST: usize = 1000;
        for _ in 0..NUM_MESSAGES_IN_BURST {
            let msg = Message {
                id: Uuid::now_v7(),
                sender: "b".to_string(),
                content: "dummy".to_owned(),
            };
            sender_client.add_message(msg).await.unwrap();
        }

        // Then: receiver extracts all 100 messages without timeout
        let received_events = tokio::time::timeout(
            std::time::Duration::from_secs(2),
            events_stream
                .take(NUM_MESSAGES_IN_BURST)
                .collect::<Vec<_>>(),
        )
        .await
        .expect("timed out waiting for events");

        assert_eq!(
            received_events.len(),
            NUM_MESSAGES_IN_BURST,
            "Receiver did not get all 100 messages"
        );

        // Cleanup
        drop(sender_client);
        chat.shutdown().await;
    }

    #[derive(Clone)]
    struct HistorySpy {
        recorded_messages: Arc<Mutex<Vec<Message>>>,
        observed_last_event_ids: Arc<Mutex<Vec<u64>>>,
    }

    impl HistorySpy {
        fn new() -> Self {
            HistorySpy {
                recorded_messages: Arc::new(Mutex::new(Vec::new())),
                observed_last_event_ids: Arc::new(Mutex::new(Vec::new())),
            }
        }

        fn take_recorded_messages(&self) -> Vec<Message> {
            take(&mut *self.recorded_messages.lock().unwrap())
        }

        fn take_observed_last_event_ids(&self) -> Vec<u64> {
            take(&mut *self.observed_last_event_ids.lock().unwrap())
        }
    }

    impl Chat for HistorySpy {
        fn events_since(&self, last_event_id: u64) -> Vec<Event> {
            self.observed_last_event_ids
                .lock()
                .unwrap()
                .push(last_event_id);
            vec![Event {
                id: last_event_id + 1,
                message: Message {
                    id: Uuid::nil(),
                    sender: "dummy".to_owned(),
                    content: "dummy".to_owned(),
                },
                timestamp: SystemTime::UNIX_EPOCH,
            }]
        }

        fn record_message(&mut self, message: Message) -> Result<Option<Event>, ChatError> {
            self.recorded_messages.lock().unwrap().push(message.clone());
            Ok(Some(Event {
                id: 1,
                message,
                timestamp: SystemTime::now(),
            }))
        }
    }

    struct FakeHistory {
        events: Vec<Event>,
    }

    impl FakeHistory {
        fn new() -> Self {
            FakeHistory { events: Vec::new() }
        }
    }

    impl Chat for FakeHistory {
        fn events_since(&self, last_event_id: u64) -> Vec<Event> {
            let start = (last_event_id as usize).min(self.events.len());
            self.events[start..].to_vec()
        }

        fn record_message(&mut self, message: Message) -> Result<Option<Event>, ChatError> {
            let event = Event {
                id: self.events.len() as u64 + 1,
                message,
                timestamp: SystemTime::UNIX_EPOCH,
            };
            self.events.push(event.clone());
            Ok(Some(event))
        }
    }
}

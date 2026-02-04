use std::{cmp::min, time::SystemTime};

use async_stream::stream;
use futures_util::Stream;
use serde::Deserialize;
use tokio::{
    sync::{broadcast, mpsc, oneshot},
    task::JoinHandle,
};
use uuid::Uuid;

/// Interaction with a conversation.
#[cfg_attr(test, double_trait::dummies)]
pub trait Conversation: Sized {
    /// A stream which yields future and past events of the conversation.
    ///
    /// # Parameters
    ///
    /// - `last_event_id`: The last event id received by the client. Event ids are ordered. The
    ///   stream will only yield events with an id greater than `last_event_id`, so that clients
    ///   only receive events they have not yet seen. Use `0` to receive all events from the
    ///   beginning of the conversation. Filtering of events is only applied to historic events.
    ///   Future events will always be delivered.
    fn events(self, last_event_id: u64) -> impl Stream<Item = Event> + Send;

    /// Add a new message to the conversation.
    fn add_message(&mut self, message: Message) -> impl Future<Output = ()> + Send;
}

/// Can be used to create multiple instances of [`ConversationClient`] which provide an API to
/// interact with a shared conversation. The runtime takes care that messages are forwarded between
/// different clients.
pub struct ConversationRuntime {
    sender: mpsc::Sender<ActorMsg>,
    join_handle: JoinHandle<()>,
}

impl ConversationRuntime {
    pub fn new() -> Self {
        let (sender, receiver) = mpsc::channel(5);
        let actor = Actor::new(receiver);
        let join_handle = tokio::spawn(async move { actor.run().await });
        ConversationRuntime {
            sender,
            join_handle,
        }
    }

    /// A client which implements the `Conversation` trait.
    pub fn api(&self) -> ConversationClient {
        ConversationClient {
            sender: self.sender.clone(),
        }
    }

    /// Shuts down the conversation runtime. In order for this to complete, all clients must have
    /// been dropped.
    pub async fn shutdown(self) {
        // We drop the sender, to signal to the actor thread that it can no longer receive messages
        // and should stop.
        drop(self.sender);
        self.join_handle.await.unwrap();
    }
}

#[derive(Clone)]
pub struct ConversationClient {
    sender: mpsc::Sender<ActorMsg>,
}

impl Conversation for ConversationClient {
    fn events(self, last_event_id: u64) -> impl Stream<Item = Event> + Send {
        stream! {
            let (responder, response) = oneshot::channel();
            self.sender
                .send(ActorMsg::ReadEvents{ responder, last_event_id})
                .await
                .expect("Actor must outlive client.");
            let EventReader { history, mut current } = response.await.unwrap();
            for message in history {
                yield message;
            }
            while let Ok(message) = current.recv().await {
                yield message;
            }
        }
    }

    async fn add_message(&mut self, message: Message) {
        self.sender
            .send(ActorMsg::AddMessage(message))
            .await
            .unwrap();
    }
}

/// A message as it is stored and represented as part of a conversation.
#[derive(Clone)]
pub struct Event {
    /// One based ordered identifier of the events in the conversation.
    pub id: u64,
    pub message: Message,
    pub timestamp: SystemTime,
}

/// A message as it is created by the frontend and sent to the server. It is then relied to all
/// participants in the conversation as part of an `Event`.
#[derive(Deserialize, PartialEq, Eq, Debug, Clone)]
pub struct Message {
    /// Sender generated unique identifier for the message. It is used to recover from errors
    /// sending messages. It also a key for the UI to efficiently update data structures then
    /// rendering messages.
    pub id: Uuid,
    /// Author of the message
    pub sender: String,
    /// Text content of the message. I.e. the actual message
    pub content: String,
}

enum ActorMsg {
    ReadEvents {
        responder: oneshot::Sender<EventReader>,
        last_event_id: u64,
    },
    AddMessage(Message),
}

struct EventReader {
    history: Vec<Event>,
    current: broadcast::Receiver<Event>,
}

struct Actor {
    /// All the events so far
    history: Vec<Event>,
    /// Used to broadcast new events to clients whom already have consumed the history.
    current: broadcast::Sender<Event>,
    receiver: mpsc::Receiver<ActorMsg>,
}

impl Actor {
    pub fn new(receiver: mpsc::Receiver<ActorMsg>) -> Self {
        let messages = Vec::new();
        let (current, _) = broadcast::channel(10);
        Actor {
            receiver,
            history: messages,
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
                let last_event_id = min(last_event_id as usize, self.history.len());
                let history = self.history[last_event_id..].to_owned();
                // We ignore send errors, since it only happens if the receiver has been dropped. In
                // that case the receiver is no longer interested in the response, anyway.
                let _ = responder.send(EventReader {
                    history,
                    current: self.current.subscribe(),
                });
            }
            ActorMsg::AddMessage(message) => {
                let event = Event {
                    id: self.history.len() as u64 + 1,
                    message,
                    timestamp: SystemTime::now(),
                };
                self.history.push(event.clone());
                // This method only fails if there are no active receivers. This is also fine, we
                // can safely ignore that.
                let _ = self.current.send(event);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::{pin::pin, time::Duration};

    use super::*;
    use futures_util::StreamExt;
    use tokio::time::timeout;

    #[tokio::test]
    async fn messages_are_added_and_read_in_order() {
        // Given
        let id_1: Uuid = "019c0ab6-9d11-75ef-ab02-60f070b1582a".parse().unwrap();
        let msg_1 = Message {
            id: id_1.clone(),
            sender: "Alice".to_string(),
            content: "One".to_string(),
        };
        let id_2: Uuid = "019c0ab6-9d11-7a5b-abde-cb349e5fd995".parse().unwrap();
        let msg_2 = Message {
            id: id_2.clone(),
            sender: "Bob".to_string(),
            content: "Two".to_string(),
        };
        let conversation = ConversationRuntime::new();

        // When
        conversation.api().add_message(msg_1).await;
        conversation.api().add_message(msg_2).await;

        // This line is a bit more tricky than it seems. We need to make sure messages is freed so
        // that the cleanup won't block. It is not enough to clear the pinned wrapper.
        let history = conversation
            .api()
            .events(0)
            .take(2)
            .collect::<Vec<_>>()
            .await;

        // Then
        let first = &history[0].message;
        assert_eq!(first.id, id_1);
        assert_eq!(first.sender, "Alice");
        assert_eq!(first.content, "One");

        let second = &history[1].message;
        assert_eq!(second.id, id_2);
        assert_eq!(second.sender, "Bob");
        assert_eq!(second.content, "Two");

        // Cleanup
        conversation.shutdown().await;
    }

    #[tokio::test]
    async fn shutdown_completes_within_one_second() {
        // Given
        let conversation = ConversationRuntime::new();

        // When
        let result = timeout(Duration::from_secs(1), conversation.shutdown()).await;

        // Then
        assert!(result.is_ok(), "Shutdown did not complete within 1 second");
    }

    #[tokio::test]
    async fn events_stream_includes_future_events() {
        use futures_util::StreamExt;

        // Given: a conversation and one initial message
        let id_1: Uuid = "019c0ab6-9d11-75ef-ab02-60f070b1582a".parse().unwrap();
        let msg_1 = Message {
            id: id_1.clone(),
            sender: "Alice".to_string(),
            content: "One".to_string(),
        };
        let id_2: Uuid = "019c0ab6-9d11-7a5b-abde-cb349e5fd995".parse().unwrap();
        let msg_2 = Message {
            id: id_2.clone(),
            sender: "Bob".to_string(),
            content: "Two".to_string(),
        };
        let id_3: Uuid = "019c0ab6-9d11-7fff-abde-cb349e5fd996".parse().unwrap();
        let msg_3 = Message {
            id: id_3.clone(),
            sender: "Carol".to_string(),
            content: "Three".to_string(),
        };

        let conversation = ConversationRuntime::new();

        // Add one message before subscribing
        conversation.api().add_message(msg_1).await;

        // When: subscribe to events, then add more messages
        let mut events_stream = conversation.api().events(0).boxed();

        // Extract historic messages so far
        let _initial_message = events_stream.next().await;

        // Add messages after history has already been consumed
        conversation.api().add_message(msg_2).await;
        conversation.api().add_message(msg_3).await;

        // Then: we expect to receive the initial and the later messages (3 total)
        let collected = tokio::time::timeout(Duration::from_millis(200), async {
            events_stream.take(2).collect::<Vec<_>>().await
        })
        .await
        .expect("timed out waiting for events");

        assert_eq!(
            collected.len(),
            2,
            "expected 2 events (2 added after historic messages extracted)"
        );

        // Cleanup
        conversation.shutdown().await;
    }

    #[tokio::test]
    async fn events_only_return_events_with_id_greater_than_last_event_id() {
        // Given a conversation with three messages
        let id_1: Uuid = "019c0ab6-9d11-75ef-ab02-60f070b1582a".parse().unwrap();
        let msg_1 = Message {
            id: id_1.clone(),
            sender: "Alice".to_string(),
            content: "One".to_string(),
        };
        let id_2: Uuid = "019c0ab6-9d11-7a5b-abde-cb349e5fd995".parse().unwrap();
        let msg_2 = Message {
            id: id_2.clone(),
            sender: "Bob".to_string(),
            content: "Two".to_string(),
        };
        let id_3: Uuid = "019c0ab6-9d11-7fff-abde-cb349e5fd996".parse().unwrap();
        let msg_3 = Message {
            id: id_3.clone(),
            sender: "Carol".to_string(),
            content: "Three".to_string(),
        };

        let conversation = ConversationRuntime::new();
        conversation.api().add_message(msg_1).await;
        conversation.api().add_message(msg_2).await;
        conversation.api().add_message(msg_3).await;

        // When: requesting two events since last_event_id = 1
        let history = conversation
            .api()
            .events(1)
            .take(2)
            .collect::<Vec<_>>()
            .await;

        // Then events 2 and 3 are returned (as opposed to 1, 2)
        assert_eq!(history[0].id, 2);
        assert_eq!(history[1].id, 3);

        // Cleanup
        conversation.shutdown().await;
    }

    #[tokio::test]
    async fn last_event_id_exceeds_total_number_of_events() {
        // Given a conversation with one existing message
        let msg = Message {
            id: "019c0ab6-9d11-75ef-ab02-60f070b1582a".parse().unwrap(),
            sender: "Bob".to_string(),
            content: "Hello".to_string(),
        };
        let conversation = ConversationRuntime::new();
        conversation.api().add_message(msg.clone()).await;

        // When: requesting events with a last_event_id `2`.
        let mut events_stream = conversation.api().events(2).boxed();
        let received = timeout(Duration::ZERO, events_stream.next()).await;

        // Then: no events should be returned from conversation history
        assert!(
            received.is_err(),
            "expected no historic events to be returned"
        );

        // Cleanup
        drop(events_stream);
        conversation.shutdown().await;
    }

    #[tokio::test]
    async fn state_and_messages_are_shared_between_clients() {
        // Given a two clients. One of them listening for new events.
        let conversation = ConversationRuntime::new();
        let mut sender_client = conversation.api();
        let receiver_client = conversation.api();

        // Start listening for events on the receiver
        let events_stream = receiver_client.events(0);

        // When sending a message from the other client
        let msg = Message {
            id: "019c0ab6-9d11-75ef-ab02-60f070b1582a".parse().unwrap(),
            sender: "Alice".to_string(),
            content: "Hello from Alice".to_string(),
        };
        sender_client.add_message(msg.clone()).await;

        // Then the receiver should get the message as an event
        let received_events = tokio::time::timeout(
            std::time::Duration::from_millis(200),
            events_stream.take(1).collect::<Vec<_>>(),
        )
        .await
        .expect("timed out waiting for event");
        assert_eq!(received_events.len(), 1);
        assert_eq!(received_events[0].message, msg);
        assert_eq!(received_events[0].id, 1);

        // Cleanup
        drop(sender_client);
        conversation.shutdown().await;
    }

    #[tokio::test]
    #[should_panic] // Not implemented yet. TODO
    async fn slow_receiver() {
        // Given: a conversation and two clients
        let conversation = ConversationRuntime::new();
        let mut sender_client = conversation.api();
        let receiver_client = conversation.api();

        // And one message in the chat history
        sender_client
            .add_message(Message {
                id: Uuid::now_v7(),
                sender: "a".to_string(),
                content: "Initial message".to_string(),
            })
            .await;

        // One of the clients has an event stream open, which already has received all messages in the
        // history so far (one in this case).
        {
            // This pin! is why we need the scope around events stream. It needs to be dropped
            // before we can clean up the runtime.
            let mut events_stream = pin!(receiver_client.events(0));
            events_stream.next().await; // Consume initial message

            // When: sender sends 100 messages
            const NUM_MESSAGES_IN_BURST: usize = 1000;
            for _ in 0..NUM_MESSAGES_IN_BURST {
                let msg = Message {
                    id: Uuid::now_v7(),
                    sender: "b".to_string(),
                    content: "dummy".to_owned(),
                };
                sender_client.add_message(msg).await;
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
        }

        // Cleanup
        drop(sender_client);
        conversation.shutdown().await;
    }
}

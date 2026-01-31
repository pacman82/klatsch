use std::time::SystemTime;

use async_stream::stream;
use futures_util::Stream;
use serde::Deserialize;
use tokio::{
    sync::{mpsc, oneshot},
    task::JoinHandle,
};
use uuid::Uuid;

#[cfg_attr(test, double_trait::dummies)]
pub trait Conversation: Sized {
    /// A stream which yields future and past events of the conversation.
    fn events(self) -> impl Stream<Item = Event> + Send;
    /// Add a new message to the conversation.
    fn add_message(&mut self, message: Message) -> impl Future<Output = ()> + Send;
}

/// Manages the lifetime of conversations api.
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

    pub fn api(&self) -> ConversationClient {
        ConversationClient {
            sender: self.sender.clone(),
        }
    }

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
    fn events(self) -> impl Stream<Item = Event> + Send {
        stream! {
            let (request, response) = oneshot::channel();
            self.sender
                .send(ActorMsg::ReadMessages(request))
                .await
                .expect("Actor must outlive client.");
            let messages = response.await.unwrap();
            for message in messages {
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
    ReadMessages(oneshot::Sender<Vec<Event>>),
    AddMessage(Message),
}

struct Actor {
    history: Vec<Event>,
    receiver: mpsc::Receiver<ActorMsg>,
}

impl Actor {
    pub fn new(receiver: mpsc::Receiver<ActorMsg>) -> Self {
        let messages = Vec::new();
        Actor { receiver, history: messages }
    }

    pub async fn run(mut self) {
        while let Some(msg) = self.receiver.recv().await {
            self.handle_message(msg);
        }
    }

    pub fn handle_message(&mut self, msg: ActorMsg) {
        match msg {
            ActorMsg::ReadMessages(responder) => {
                let messages = self.history.clone();
                // We ignore send errors, since it only happens if the receiver has been dropped. In
                // that case the receiver is no longer interested in the response, anyway.
                let _ = responder.send(messages);
            }
            ActorMsg::AddMessage(message) => {
                let event = Event {
                    id: self.history.len() as u64 + 1,
                    message,
                    timestamp: SystemTime::now(),
                };
                self.history.push(event);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

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
            .events()
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
}

use std::{
    sync::{Arc, Mutex},
    time::SystemTime,
};

use futures_util::Stream;
use serde::Serialize;
use uuid::Uuid;

#[cfg_attr(test, double_trait::dummies)]
pub trait ConversationApi: Sized {
    fn messages(self) -> impl Stream<Item = Message> + Send + 'static;
    fn add_message(&self, id: Uuid, sender: String, content: String);
}

#[derive(Clone)]
pub struct Conversation {
    messages: Arc<Mutex<Vec<Message>>>,
}

impl Conversation {
    pub fn new() -> Self {
        Conversation {
            messages: Arc::new(Mutex::new(Vec::new())),
        }
    }
}

impl ConversationApi for Conversation {
    fn messages(self) -> impl Stream<Item = Message> + Send + 'static {
        let messages = self.messages.lock().unwrap().clone();
        tokio_stream::iter(messages)
    }

    fn add_message(&self, id: Uuid, sender: String, content: String) {
        let message = Message {
            id,
            sender,
            content,
            timestamp_ms: SystemTime::now()
                .duration_since(SystemTime::UNIX_EPOCH)
                .unwrap()
                .as_millis() as u64,
        };
        let mut messages = self.messages.lock().unwrap();
        messages.push(message);
    }
}

#[derive(Serialize, Clone)]
pub struct Message {
    /// Sender generated unique identifier for the message. It is used to recover from errors
    /// sending messages. It also a key for the UI to efficiently update data structures then
    /// rendering messages.
    pub id: Uuid,
    /// Author of the message
    pub sender: String,
    /// Text content of the message. I.e. the actual message
    pub content: String,
    /// Unix timestamp. Milliseconds since epoch
    pub timestamp_ms: u64,
}

#[cfg(test)]
mod tests {
    use super::*;
    use futures_util::StreamExt;

    #[tokio::test]
    async fn messages_are_added_and_read_in_order() {
        let conversation = Conversation::new();

        let id1 = "019c0ab6-9d11-75ef-ab02-60f070b1582a".parse().unwrap();
        let id2 = "019c0ab6-9d11-7a5b-abde-cb349e5fd995".parse().unwrap();

        conversation.add_message(id1, "Alice".to_string(), "One".to_string());
        conversation.add_message(id2, "Bob".to_string(), "Two".to_string());

        let mut messages = conversation.clone().messages();

        let msg1 = messages.next().await.expect("First message should exist");
        let msg2 = messages.next().await.expect("Second message should exist");

        assert_eq!(msg1.id, id1);
        assert_eq!(msg1.sender, "Alice");
        assert_eq!(msg1.content, "One");

        assert_eq!(msg2.id, id2);
        assert_eq!(msg2.sender, "Bob");
        assert_eq!(msg2.content, "Two");
    }
}

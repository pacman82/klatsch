use futures_util::Stream;
use serde::Serialize;
use uuid::Uuid;

pub trait ConversationApi {
    fn messages(self) -> impl Stream<Item = Message> + Send + 'static;
}

#[derive(Clone)]
pub struct Conversation {}

impl Conversation {
    pub fn new() -> Self {
        Conversation {}
    }
}

impl ConversationApi for Conversation {
    fn messages(self) -> impl Stream<Item = Message> + Send + 'static {
        let messages = vec![
            Message {
                id: "019c0050-e4d7-7447-9d8f-81cde690f4a1".parse().unwrap(),
                sender: "Alice".to_string(),
                content: "Hey there! 👋".to_string(),
                timestamp_ms: 1704531600000,
            },
            Message {
                id: "019c0051-c29d-7968-b953-4adc898b1360".parse().unwrap(),
                sender: "Bob".to_string(),
                content: "Hi Alice! How are you?".to_string(),
                timestamp_ms: 1704531601000,
            },
            Message {
                id: "019c0051-e50d-7ea7-8a0e-f7df4176dd93".parse().unwrap(),
                sender: "Alice".to_string(),
                content: "I'm good, thanks! Working on the chat server project.".to_string(),
                timestamp_ms: 1704531602000,
            },
            Message {
                id: "019c0052-09b0-73be-a145-3767cb10cdf6".parse().unwrap(),
                sender: "Bob".to_string(),
                content: "That's awesome! Let me know if you need any help.".to_string(),
                timestamp_ms: 1704531603000,
            },
        ];
        tokio_stream::iter(messages)
    }
}

#[derive(Serialize)]
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

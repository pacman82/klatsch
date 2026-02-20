use std::time::{SystemTime, UNIX_EPOCH};

use serde::Deserialize;
use uuid::Uuid;

/// A message as it is created by the frontend and sent to the server. It is then relied to all
/// participants in the chat as part of an `Event`.
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

/// A message as it is stored and represented as part of a chat.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Event {
    /// One based ordered identifier of the events in the chat.
    pub id: u64,
    pub message: Message,
    /// Milliseconds since Unix epoch
    pub timestamp_ms: u64,
}

impl Event {
    pub fn new(id: u64, message: Message) -> Self {
        // u64 covers ~584 million years since epoch, so we can afford to downcast from u128.
        let timestamp_ms = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_millis() as u64;
        Event {
            id,
            message,
            timestamp_ms,
        }
    }

    pub fn with_timestamp(id: u64, message: Message, timestamp: SystemTime) -> Self {
        let timestamp_ms = timestamp.duration_since(UNIX_EPOCH).unwrap().as_millis() as u64;
        Event {
            id,
            message,
            timestamp_ms,
        }
    }
}

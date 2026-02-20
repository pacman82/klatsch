use std::{
    fmt::{self, Display},
    num::ParseIntError,
    str::FromStr,
    time::{SystemTime, UNIX_EPOCH},
};

use serde::Deserialize;
use uuid::Uuid;

/// A message as it is stored and represented as part of a chat.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Event {
    pub id: EventId,
    pub message: Message,
    /// Milliseconds since Unix epoch
    pub timestamp_ms: u64,
}

impl Event {
    pub fn new(id: EventId, message: Message) -> Self {
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

    pub fn with_timestamp(id: EventId, message: Message, timestamp: SystemTime) -> Self {
        let timestamp_ms = timestamp.duration_since(UNIX_EPOCH).unwrap().as_millis() as u64;
        Event {
            id,
            message,
            timestamp_ms,
        }
    }
}

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

#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub struct EventId(pub u64);

impl EventId {
    pub fn before_all() -> Self {
        EventId(0)
    }

    pub fn successor(self) -> Self {
        EventId(self.0 + 1)
    }
}

impl Display for EventId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

impl FromStr for EventId {
    type Err = ParseIntError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        s.parse::<u64>().map(EventId)
    }
}

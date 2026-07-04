use std::{
    fmt::{self, Display},
    num::ParseIntError,
    str::FromStr,
    time::{SystemTime, UNIX_EPOCH},
};

use uuid::Uuid;

use crate::{
    persistence::{Argument, AsArgument, FromField, GetFieldNative},
    user::UserId,
};

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

/// A message as it is stored and broadcast to participants in the chat as part of an `Event`.
#[derive(PartialEq, Eq, Debug, Clone)]
pub struct Message {
    /// Sender generated unique identifier for the message. It is used to recover from errors
    /// sending messages. It also a key for the UI to efficiently update data structures then
    /// rendering messages.
    ///
    /// UUID v7, because we care about newer messages more than older ones in the database.
    pub id: Uuid,
    /// User ID of the author.
    pub author: UserId,
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

impl FromField for EventId {
    fn from_at(row: &impl GetFieldNative, index: usize) -> EventId {
        let id: i64 = row.get(index);
        let id: u64 = id.try_into().expect("event id must be non-negative");
        EventId(id)
    }
}

impl FromField for Option<EventId> {
    fn from_at(row: &impl GetFieldNative, index: usize) -> Option<EventId> {
        let maybe_id: Option<i64> = row.get(index);
        let id = maybe_id?;
        let id: u64 = id.try_into().expect("event id must be non-negative");
        Some(EventId(id))
    }
}

impl AsArgument for EventId {
    fn as_argument(&self) -> Argument<'_> {
        Argument::I64(
            self.0
                .try_into()
                .expect("event id must fit in signed 64Bit integer"),
        )
    }
}

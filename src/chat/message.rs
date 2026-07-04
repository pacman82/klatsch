use std::{fmt, str::FromStr};

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{
    persistence::{Argument, AsArgument, FromField, GetFieldNative},
    user::UserId,
};

/// Sender generated unique identifier for the message. It is used to recover from errors
/// sending messages. It also a key for the UI to efficiently update data structures then
/// rendering messages.
///
/// UUID v7, because we care about newer messages more than older ones in the database.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct MessageId(Uuid);

impl MessageId {
    const fn from_uuid(uuid: Uuid) -> Self {
        MessageId(uuid)
    }

    #[cfg(test)]
    pub fn new() -> Self {
        Self::from_uuid(Uuid::now_v7())
    }

    #[cfg(test)]
    pub fn nil() -> Self {
        Self::from_uuid(Uuid::nil())
    }

    /// ALPHA, BETA and GAMMA are for testing purposes. They are valid UUID v7 with time order ALPHA
    /// < BETA < GAMMA. Very much what you could expect if a client send three messages after each
    /// other.
    #[cfg(test)]
    pub const ALPHA: MessageId = MessageId::from_uuid(Uuid::from_bytes([
        0x01, 0x9c, 0x00, 0x50, 0x00, 0x00, 0x70, 0x00, 0x80, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
        0x01,
    ]));

    #[cfg(test)]
    pub const BETA: MessageId = MessageId::from_uuid(Uuid::from_bytes([
        0x01, 0x9c, 0x00, 0x50, 0x00, 0x00, 0x70, 0x00, 0x80, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
        0x02,
    ]));

    #[cfg(test)]
    pub const GAMMA: MessageId = MessageId::from_uuid(Uuid::from_bytes([
        0x01, 0x9c, 0x00, 0x50, 0x00, 0x00, 0x70, 0x00, 0x80, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
        0x03,
    ]));
}

impl fmt::Display for MessageId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

impl FromStr for MessageId {
    type Err = uuid::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        s.parse().map(MessageId)
    }
}

impl AsArgument for MessageId {
    fn as_argument(&self) -> Argument<'_> {
        (&self.0).as_argument()
    }
}

impl FromField for MessageId {
    fn from_at(row: &impl GetFieldNative, index: usize) -> Self {
        MessageId::from_uuid(row.get(index))
    }
}

/// A message as it is stored and broadcast to participants in the chat as part of an `Event`.
#[derive(PartialEq, Eq, Debug, Clone)]
pub struct Message {
    /// Sender generated unique identifier for the message. It is used to recover from errors
    /// sending messages. It also a key for the UI to efficiently update data structures then
    /// rendering messages.
    pub id: MessageId,
    /// User ID of the author.
    pub author: UserId,
    /// Text content of the message. I.e. the actual message
    pub content: String,
}

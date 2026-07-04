use uuid::Uuid;

use crate::user::UserId;

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

use std::{
    cmp::min,
    future::Future,
    time::{SystemTime, UNIX_EPOCH},
};

use async_sqlite::{
    Client, ClientBuilder,
    rusqlite::{self, ffi},
};
use serde::Deserialize;
use uuid::Uuid;

#[cfg_attr(test, double_trait::dummies)]
pub trait Chat {
    /// All events since the event with the given `last_event_id` (exclusive).
    fn events_since(&self, last_event_id: u64) -> impl Future<Output = Vec<Event>> + Send;

    /// Record a message and return the corresponding event. `None` indiactes that no event should
    /// be emitted due to the message being a duplicate of an already recorded message.
    fn record_message(
        &mut self,
        message: Message,
    ) -> impl Future<Output = Result<Option<Event>, ChatError>> + Send;
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

#[derive(Debug)]
pub enum ChatError {
    Conflict,
}

impl Chat for InMemoryChatHistory {
    async fn events_since(&self, last_event_id: u64) -> Vec<Event> {
        let last_event_id = min(last_event_id as usize, self.events.len());
        self.events[last_event_id..].to_owned()
    }

    async fn record_message(&mut self, message: Message) -> Result<Option<Event>, ChatError> {
        let event = Event::new(self.events.len() as u64 + 1, message);
        let row = event.clone();
        let insert_result = self
            .conn
            .conn_mut(move |conn| {
                conn.prepare_cached(
                    "INSERT INTO events (id, message_id, sender, content, timestamp_ms)
                     VALUES (?1, ?2, ?3, ?4, ?5)",
                )
                .expect("hardcoded SQL must be valid")
                .execute((
                    i64::try_from(row.id).unwrap(),
                    row.message.id.as_bytes().as_slice(),
                    row.message.sender,
                    row.message.content,
                    row.timestamp_ms as i64,
                ))?;
                Ok(())
            })
            .await;
        match insert_result {
            Ok(()) => {
                self.events.push(event.clone());
                Ok(Some(event))
            }
            Err(async_sqlite::Error::Rusqlite(rusqlite::Error::SqliteFailure(
                ffi::Error {
                    code: ffi::ErrorCode::ConstraintViolation,
                    extended_code: ffi::SQLITE_CONSTRAINT_UNIQUE,
                },
                _,
            ))) => {
                let message_id = event.message.id;
                let existing = self
                    .conn
                    .conn(move |conn| {
                        conn.prepare_cached(
                            "SELECT sender, content FROM events WHERE message_id = ?1",
                        )
                        .expect("hardcoded SQL must be valid")
                        .query_row(
                            [message_id.as_bytes().as_slice()],
                            |row| {
                                let sender = row
                                    .get::<_, String>(0)
                                    .expect("sender must be a non-null TEXT column");
                                let content = row
                                    .get::<_, String>(1)
                                    .expect("content must be a non-null TEXT column");
                                Ok((sender, content))
                            },
                        )
                    })
                    .await
                    .unwrap();
                if existing == (event.message.sender, event.message.content) {
                    Ok(None)
                } else {
                    Err(ChatError::Conflict)
                }
            }
            Err(err) => panic!("Unexpected database error: {err}"),
        }
    }
}

pub struct InMemoryChatHistory {
    events: Vec<Event>,
    conn: Client,
}

impl InMemoryChatHistory {
    pub async fn new() -> Self {
        // Opening the database without a path creates an in-memory database.
        let db = ClientBuilder::new().open().await.unwrap();
        db.conn(|conn| {
            conn.execute(
                "CREATE TABLE events (
                    id INTEGER PRIMARY KEY,
                    message_id BLOB UNIQUE NOT NULL,
                    sender TEXT NOT NULL,
                    content TEXT NOT NULL,
                    timestamp_ms INTEGER NOT NULL
                )",
                (),
            )
        })
        .await
        .unwrap();
        InMemoryChatHistory {
            events: Vec::new(),
            conn: db,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dummy_message(id: Uuid) -> Message {
        Message {
            id,
            sender: "dummy".to_owned(),
            content: "dummy".to_owned(),
        }
    }

    #[tokio::test]
    async fn recorded_message_is_preserved_in_event() {
        // Given a chat history
        let mut history = InMemoryChatHistory::new().await;

        // When recording a message ...
        let msg = Message {
            id: "019c0ab6-9d11-75ef-ab02-60f070b1582a".parse().unwrap(),
            sender: "Alice".to_string(),
            content: "Hello".to_string(),
        };
        // ... and retrieving its corresponding event
        let event = history.record_message(msg.clone()).await.unwrap().unwrap();

        // Then the event contains the same message.
        assert_eq!(event.message, msg);
    }

    #[tokio::test]
    async fn messages_are_retrieved_in_insertion_order() {
        // Given an empty chat history
        let mut history = InMemoryChatHistory::new().await;

        // When recording two messages after each other...
        let id_1 = "019c0ab6-9d11-75ef-ab02-60f070b1582a".parse().unwrap();
        let id_2 = "019c0ab6-9d11-7a5b-abde-cb349e5fd995".parse().unwrap();
        history.record_message(dummy_message(id_1)).await.unwrap();
        history.record_message(dummy_message(id_2)).await.unwrap();
        // ...and retrieving these messages after insertion
        let events = history.events_since(0).await;

        // Then the messages are retrieved in the order they were inserted.
        assert_eq!(events.len(), 2);
        assert_eq!(events[0].message.id, id_1);
        assert_eq!(events[1].message.id, id_2);
    }

    #[tokio::test]
    async fn events_since_excludes_events_up_to_last_event_id() {
        // Given a history with three messages
        let mut history = InMemoryChatHistory::new().await;
        let id_1 = "019c0ab6-9d11-75ef-ab02-60f070b1582a".parse().unwrap();
        let id_2 = "019c0ab6-9d11-7a5b-abde-cb349e5fd995".parse().unwrap();
        let id_3 = "019c0ab6-9d11-7fff-abde-cb349e5fd996".parse().unwrap();
        for id in [id_1, id_2, id_3] {
            history.record_message(dummy_message(id)).await.unwrap();
        }

        // When retrieving events since event 1
        let events = history.events_since(1).await;

        // Then only events 2 and 3 are returned
        assert_eq!(events.len(), 2);
        assert_eq!(events[0].message.id, id_2);
        assert_eq!(events[1].message.id, id_3);
    }

    #[tokio::test]
    async fn duplicate_message_id_is_not_stored() {
        // Given a history with one message
        let mut history = InMemoryChatHistory::new().await;
        let id = "019c0ab6-9d11-75ef-ab02-60f070b1582a".parse().unwrap();
        history
            .record_message(Message {
                id,
                sender: "Bob".to_owned(),
                content: "Hello, World!".to_owned(),
            })
            .await
            .unwrap();

        // When recording a duplicate message with the same id
        let result = history
            .record_message(Message {
                id,
                sender: "Bob".to_owned(),
                content: "Hello, World!".to_owned(),
            })
            .await
            .unwrap();

        // Then no event is emitted and the history remains unchanged
        assert!(result.is_none());
        assert_eq!(history.events_since(0).await.len(), 1);
    }

    #[tokio::test]
    async fn different_message_with_same_id_is_a_conflict() {
        // Given a history with one message
        let mut history = InMemoryChatHistory::new().await;
        let id = "019c0ab6-9d11-75ef-ab02-60f070b1582a".parse().unwrap();
        history
            .record_message(Message {
                id,
                sender: "Alice".to_owned(),
                content: "Hello".to_owned(),
            })
            .await
            .unwrap();

        // When recording a different message with the same id
        let result = history
            .record_message(Message {
                id,
                sender: "Alice".to_owned(),
                content: "Goodbye".to_owned(),
            })
            .await;

        // Then a conflict error is returned
        assert!(matches!(result, Err(ChatError::Conflict)));
    }

    #[tokio::test]
    async fn last_event_id_exceeds_total_number_of_events() {
        // Given a history with one message
        let mut history = InMemoryChatHistory::new().await;
        history
            .record_message(dummy_message(
                "019c0ab6-9d11-75ef-ab02-60f070b1582a".parse().unwrap(),
            ))
            .await
            .unwrap();

        // When retrieving events since an id beyond the history
        let events = history.events_since(2).await;

        // Then no events are returned
        assert!(events.is_empty());
    }
}

use std::{cmp::min, future::Future};

use async_sqlite::{
    Client, ClientBuilder,
    rusqlite::{self, ffi},
};

use super::event::{Event, EventId, Message};

#[cfg_attr(test, double_trait::dummies)]
pub trait Chat {
    /// All events since the event with the given `last_event_id` (exclusive).
    fn events_since(&self, last_event_id: EventId) -> impl Future<Output = Vec<Event>> + Send;

    /// Record a message and return the corresponding event. `None` indiactes that no event should
    /// be emitted due to the message being a duplicate of an already recorded message.
    fn record_message(
        &mut self,
        message: Message,
    ) -> impl Future<Output = Result<Option<Event>, ChatError>> + Send;
}

#[derive(Debug)]
pub enum ChatError {
    Conflict,
}

impl Chat for InMemoryChatHistory {
    async fn events_since(&self, last_event_id: EventId) -> Vec<Event> {
        let last_event_id = min(last_event_id.0 as usize, self.events.len());
        self.events[last_event_id..].to_owned()
    }

    async fn record_message(&mut self, message: Message) -> Result<Option<Event>, ChatError> {
        let event = Event::new(EventId(self.events.len() as u64 + 1), message);
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
                    row.id.as_i64(),
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
    use uuid::Uuid;

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
        let events = history.events_since(EventId::before_all()).await;

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
        let events = history.events_since(EventId(1)).await;

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
        assert_eq!(history.events_since(EventId::before_all()).await.len(), 1);
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
        let events = history.events_since(EventId(2)).await;

        // Then no events are returned
        assert!(events.is_empty());
    }
}

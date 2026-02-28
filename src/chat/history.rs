use std::{future::Future, path::Path};

use anyhow::bail;
use tracing::{error, info};

use async_sqlite::{
    Client, ClientBuilder, JournalMode,
    rusqlite::{
        self, ToSql, ffi,
        types::{FromSql, FromSqlResult, ToSqlOutput, ValueRef},
    },
};

use uuid::Uuid;

use super::event::{Event, EventId, Message};

#[cfg_attr(test, double_trait::dummies)]
pub trait Chat {
    /// All events since the event with the given `last_event_id` (exclusive).
    fn events_since(
        &self,
        last_event_id: EventId,
    ) -> impl Future<Output = anyhow::Result<Vec<Event>>> + Send;

    /// Record a message and return the corresponding event. `None` indiactes that no event should
    /// be emitted due to the message being a duplicate of an already recorded message.
    fn record_message(
        &mut self,
        message: Message,
    ) -> impl Future<Output = Result<Option<Event>, ChatError>> + Send;
}

#[derive(Debug)]
pub enum ChatError {
    /// The message which was attempt to record is conflicting with an already recorded message.
    /// I.e. the message id is identical with that of an already recorded message, but the message
    /// itself is different. This makes it different from a duplicate which can occur than retrying
    /// a message. The message has not been recorded.
    Conflict,
    /// An error caused by the runtime, due to accidential complexity. E.g. a failing I/O operation.
    /// The nature of the internal error is relevant for the operater. It can be assumed an error
    /// has been logged.
    Internal,
}

impl Chat for SqLiteChatHistory {
    async fn events_since(&self, last_event_id: EventId) -> anyhow::Result<Vec<Event>> {
        self.conn
            .conn(move |conn| fetch_events_since(conn, last_event_id))
            .await
            .inspect_err(|err| error!("Failed to read events: {err}"))
            .map_err(Into::into)
    }

    async fn record_message(&mut self, message: Message) -> Result<Option<Event>, ChatError> {
        let event_id = self.last_event_id.successor();
        let event = Event::new(event_id, message);
        let row = event.clone();
        let result = self
            .conn
            .conn_mut(move |conn| insert_event(conn, &row))
            .await;
        match result {
            Ok(InsertOutcome::New) => {
                self.last_event_id = event_id;
                Ok(Some(event))
            }
            Ok(InsertOutcome::Duplicate) => Ok(None),
            Ok(InsertOutcome::Conflict) => Err(ChatError::Conflict),
            Err(err) => {
                error!("Failed to record event: {err}");
                Err(ChatError::Internal)
            }
        }
    }
}

pub struct SqLiteChatHistory {
    conn: Client,
    /// Identifying the event which has last been emited.
    last_event_id: EventId,
}

impl SqLiteChatHistory {
    pub async fn new(persistence: Option<&Path>) -> anyhow::Result<Self> {
        let mut builder = ClientBuilder::new();
        if let Some(path) = persistence {
            builder = builder.path(path).journal_mode(JournalMode::Wal);
        }
        let db = builder
            .open()
            .await
            .inspect_err(|err| error!("Failed to open database: {err}"))?;
        let outcome = db
            .conn_mut(|conn| migrate(conn))
            .await
            .inspect_err(|err| error!("Failed to migrate database: {err}"))?;
        outcome.report_migration_status()?;
        let last_event_id = db
            .conn(|conn| {
                conn.query_row("SELECT MAX(id) FROM events", [], |row| {
                    Ok(row
                        .get::<_, Option<EventId>>(0)?
                        .unwrap_or(EventId::before_all()))
                })
            })
            .await
            .inspect_err(|err| error!("Failed to read last event id: {err}"))?;
        let new = SqLiteChatHistory {
            conn: db,
            last_event_id,
        };
        Ok(new)
    }
}

enum MigrationOutcome {
    /// Found an empty database and created the schema from scratch.
    Created,
    /// Found a recent schema. No migration was necessary.
    NoMigration,
    /// Found a future schema version. Aborted to prevent data loss.
    Future { version: u32 },
}

impl MigrationOutcome {
    fn report_migration_status(self) -> anyhow::Result<()> {
        match self {
            MigrationOutcome::Created => {
                info!("New database created");
                Ok(())
            }
            MigrationOutcome::NoMigration => Ok(()),
            MigrationOutcome::Future { version } => {
                error!(
                    "Database schema version ({version}) is newer than supported. Aborting to \
                    prevent data corruption."
                );
                bail!(
                    "Chat History has been created by a newer version. To load it you need to \
                    upgrade to a newer version."
                )
            }
        }
    }
}

fn migrate(conn: &mut rusqlite::Connection) -> Result<MigrationOutcome, rusqlite::Error> {
    let version: u32 = conn.pragma_query_value(None, "user_version", |row| row.get(0))?;
    // Version 0 is the initial version of an empty database. We regard creating a new database as a
    // migration from version 0 to the current version.
    let outcome = match version {
        // New empty database. Create schema from scratch
        0 => {
            let tx = conn.transaction()?;
            create_schema(&tx)?;
            tx.pragma_update(None, "user_version", 1)?;
            tx.commit()?;
            MigrationOutcome::Created
        }
        // Current version, do nothing.
        1 => MigrationOutcome::NoMigration,
        // Future version. Abort and report error in order to prevent data loss.
        future_version => MigrationOutcome::Future {
            version: future_version,
        },
    };
    Ok(outcome)
}

fn create_schema(conn: &rusqlite::Connection) -> Result<(), rusqlite::Error> {
    conn.execute(
        "CREATE TABLE events (
            id INTEGER PRIMARY KEY,
            message_id BLOB UNIQUE NOT NULL,
            sender TEXT NOT NULL,
            content TEXT NOT NULL,
            timestamp_ms INTEGER NOT NULL
        )",
        (),
    )?;
    Ok(())
}

enum InsertOutcome {
    /// The message has not been previously recorded and has been added to the record.
    New,
    /// Exactly the same message has been previously recorded. No change to the record
    Duplicate,
    /// A different message with the same id has been previously recorded. No change to the record
    Conflict,
}

fn insert_event(
    conn: &rusqlite::Connection,
    event: &Event,
) -> Result<InsertOutcome, rusqlite::Error> {
    let Err(err) = conn
        .prepare_cached(
            "INSERT INTO events (id, message_id, sender, content, timestamp_ms)
             VALUES (?1, ?2, ?3, ?4, ?5)",
        )
        .expect("hardcoded SQL must be valid")
        .execute((
            &event.id,
            event.message.id.as_bytes().as_slice(),
            &event.message.sender,
            &event.message.content,
            event.timestamp_ms as i64,
        ))
    else {
        // Message successfully inserted, let's return.
        return Ok(InsertOutcome::New);
    };

    // We had an error, but did something go wrong with accesing the database or is this a unique
    // constraint violation?
    if !matches!(
        err,
        rusqlite::Error::SqliteFailure(
            ffi::Error {
                code: ffi::ErrorCode::ConstraintViolation,
                extended_code: ffi::SQLITE_CONSTRAINT_UNIQUE,
            },
            _,
        )
    ) {
        // Something went wrong, lets report the error.
        return Err(err);
    }

    // So it is a unique constraint violation, but is it a duplicate or a conflict?
    let (sender, content) = conn
        .prepare_cached("SELECT sender, content FROM events WHERE message_id = ?1")
        .expect("hardcoded SQL must be valid")
        .query_row([event.message.id.as_bytes().as_slice()], |row| {
            let sender: String = row.get(0).expect("sender must be a non-null TEXT column");
            let content: String = row.get(1).expect("content must be a non-null TEXT column");
            Ok((sender, content))
        })?;
    if sender == event.message.sender && content == event.message.content {
        Ok(InsertOutcome::Duplicate)
    } else {
        Ok(InsertOutcome::Conflict)
    }
}

fn fetch_events_since(
    conn: &rusqlite::Connection,
    last_event_id: EventId,
) -> Result<Vec<Event>, rusqlite::Error> {
    let mut stmt = conn
        .prepare_cached(
            "SELECT id, message_id, sender, content, timestamp_ms
             FROM events WHERE id > ?1 ORDER BY id",
        )
        .expect("hardcoded SQL must be valid");
    stmt.query_map([&last_event_id], |row| {
        let event_id: EventId = row.get(0).expect("id must be a non-null INTEGER column");
        let message_id: Vec<u8> = row
            .get(1)
            .expect("message_id must be a non-null BLOB column");
        let message_id = Uuid::from_bytes(
            message_id
                .try_into()
                .expect("message_id must be a 16-byte BLOB column"),
        );
        let sender = row.get(2).expect("sender must be a non-null TEXT column");
        let content = row.get(3).expect("content must be a non-null TEXT column");
        let timestamp_ms: i64 = row
            .get(4)
            .expect("timestamp_ms must be a non-null INTEGER column");
        let message = Message {
            id: message_id,
            sender,
            content,
        };
        let event = Event {
            id: event_id,
            message,
            timestamp_ms: timestamp_ms as u64,
        };
        Ok(event)
    })?
    .collect()
}

impl ToSql for EventId {
    fn to_sql(&self) -> rusqlite::Result<ToSqlOutput<'_>> {
        Ok(ToSqlOutput::from(self.0 as i64))
    }
}

impl FromSql for EventId {
    fn column_result(value: ValueRef<'_>) -> FromSqlResult<Self> {
        let id = i64::column_result(value)?;
        Ok(EventId(id as u64))
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
        let mut history = SqLiteChatHistory::new(None).await.unwrap();

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
        let mut history = SqLiteChatHistory::new(None).await.unwrap();

        // When recording two messages after each other...
        let id_1 = "019c0ab6-9d11-75ef-ab02-60f070b1582a".parse().unwrap();
        let id_2 = "019c0ab6-9d11-7a5b-abde-cb349e5fd995".parse().unwrap();
        history.record_message(dummy_message(id_1)).await.unwrap();
        history.record_message(dummy_message(id_2)).await.unwrap();
        // ...and retrieving these messages after insertion
        let events = history.events_since(EventId::before_all()).await.unwrap();

        // Then the messages are retrieved in the order they were inserted.
        assert_eq!(events.len(), 2);
        assert_eq!(events[0].message.id, id_1);
        assert_eq!(events[1].message.id, id_2);
    }

    #[tokio::test]
    async fn events_since_excludes_events_up_to_last_event_id() {
        // Given a history with three messages
        let mut history = SqLiteChatHistory::new(None).await.unwrap();
        let id_1 = "019c0ab6-9d11-75ef-ab02-60f070b1582a".parse().unwrap();
        let id_2 = "019c0ab6-9d11-7a5b-abde-cb349e5fd995".parse().unwrap();
        let id_3 = "019c0ab6-9d11-7fff-abde-cb349e5fd996".parse().unwrap();
        for id in [id_1, id_2, id_3] {
            history.record_message(dummy_message(id)).await.unwrap();
        }

        // When retrieving events since event 1
        let events = history.events_since(EventId(1)).await.unwrap();

        // Then only events 2 and 3 are returned
        assert_eq!(events.len(), 2);
        assert_eq!(events[0].message.id, id_2);
        assert_eq!(events[1].message.id, id_3);
    }

    #[tokio::test]
    async fn duplicate_message_id_is_not_stored() {
        // Given a history with one message
        let mut history = SqLiteChatHistory::new(None).await.unwrap();
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
        assert_eq!(
            history
                .events_since(EventId::before_all())
                .await
                .unwrap()
                .len(),
            1
        );
    }

    #[tokio::test]
    async fn different_message_with_same_id_is_a_conflict() {
        // Given a history with one message
        let mut history = SqLiteChatHistory::new(None).await.unwrap();
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
        let mut history = SqLiteChatHistory::new(None).await.unwrap();
        history
            .record_message(dummy_message(
                "019c0ab6-9d11-75ef-ab02-60f070b1582a".parse().unwrap(),
            ))
            .await
            .unwrap();

        // When retrieving events since an id beyond the history
        let events = history.events_since(EventId(2)).await.unwrap();

        // Then no events are returned
        assert!(events.is_empty());
    }

    #[tokio::test]
    async fn persistence() {
        // Given a history backed by a file with two recorded messages
        let dir = tempfile::tempdir().unwrap();
        let db_path = dir.path().join("chat.db");
        let mut history = SqLiteChatHistory::new(Some(&db_path)).await.unwrap();
        history
            .record_message(Message {
                id: "019c0ab6-9d11-75ef-ab02-60f070b1582a".parse().unwrap(),
                sender: "Alice".to_owned(),
                content: "Hello".to_owned(),
            })
            .await
            .unwrap();
        history
            .record_message(Message {
                id: "019c0ab6-9d11-7a5b-abde-cb349e5fd995".parse().unwrap(),
                sender: "Bob".to_owned(),
                content: "Hi there".to_owned(),
            })
            .await
            .unwrap();
        let before = history.events_since(EventId::before_all()).await.unwrap();

        // When reopening the history from the same file
        drop(history);
        let history = SqLiteChatHistory::new(Some(&db_path)).await.unwrap();

        // Then all events are restored
        let after = history.events_since(EventId::before_all()).await.unwrap();
        assert_eq!(before, after);
    }

    #[tokio::test]
    async fn rejects_database_from_newer_version() {
        // Given a database with a schema version newer than supported
        let dir = tempfile::tempdir().unwrap();
        let db_path = dir.path().join("chat.db");
        let history = SqLiteChatHistory::new(Some(&db_path)).await.unwrap();
        history
            .conn
            .conn_mut(|conn| conn.pragma_update(None, "user_version", 1_000))
            .await
            .unwrap();
        drop(history);

        // When trying to open the database
        let result = SqLiteChatHistory::new(Some(&db_path)).await;

        // Then it fails with a clear error
        let Err(err) = result else {
            panic!("Must reject newer schema version");
        };
        assert_eq!(
            err.to_string(),
            "Chat History has been created by a newer version. To load it you need to upgrade to a \
            newer version."
        );
    }
}

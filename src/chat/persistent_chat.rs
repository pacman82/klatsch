use super::event::{Event, EventId, Message};
use crate::persistence::{ExecuteSql, FieldAccess, Persistence, PersistenceError};
use std::future::Future;
use uuid::Uuid;

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

impl<P> Chat for PersistentChat<P>
where
    P: Persistence + Sync + Send,
{
    async fn events_since(&self, last_event_id: EventId) -> anyhow::Result<Vec<Event>> {
        let query = "SELECT events.id, message_id, events.author_id, content, timestamp_ms \
            FROM events \
            WHERE events.id > ?1 ORDER BY events.id";

        let map = |row: &P::Row<'_>| {
            let event_id = row.get_i64(0);
            let event_id = EventId(event_id.try_into().unwrap());
            let message_id = row.get_uuid(1);
            let author = row.get_uuid(2);
            let content = row.get_text(3);
            let timestamp_ms = row.get_i64(4);
            let timestamp_ms: u64 = timestamp_ms.try_into().unwrap();
            let message = Message {
                id: message_id,
                author,
                content,
            };
            let event = Event {
                id: event_id,
                message,
                timestamp_ms,
            };
            Ok(event)
        };

        let last_event_id: i64 = last_event_id.0.try_into().unwrap();
        self.persistence.rows_vec(query, last_event_id, map).await
    }

    async fn record_message(&mut self, message: Message) -> Result<Option<Event>, ChatError> {
        let event_id = self.last_event_id.successor();
        let event = Event::new(event_id, message);
        let row = event.clone();
        let result = self
            .persistence
            .transaction(move |conn| insert_event(conn, &row))
            .await;
        match result {
            Ok(InsertOutcome::New) => {
                self.last_event_id = event_id;
                Ok(Some(event))
            }
            Ok(InsertOutcome::Duplicate) => Ok(None),
            Ok(InsertOutcome::Conflict) => Err(ChatError::Conflict),
            Err(_err) => Err(ChatError::Internal),
        }
    }
}

pub struct PersistentChat<P> {
    persistence: P,
    /// Identifying the event which has last been emited.
    last_event_id: EventId,
}

impl<P> PersistentChat<P>
where
    P: Persistence,
{
    pub async fn new(persistence: P) -> anyhow::Result<Self> {
        let last_event_id = persistence
            .row("SELECT MAX(id) FROM events", (), |row| {
                let event_id = row
                    .get_i64_opt(0)
                    .map(|event_id| EventId(event_id.try_into().unwrap()))
                    .unwrap_or_else(EventId::before_all);
                Ok(event_id)
            })
            .await?;
        let new = PersistentChat {
            persistence,
            last_event_id,
        };
        Ok(new)
    }
}

pub fn migrate_chat_persistence<C>(conn: &C, from_version: u32) -> Result<(), C::Error>
where
    C: ExecuteSql,
{
    match from_version {
        // No prior database found create current schema from scratch
        0 => {
            create_schema_from_scratch(conn)?;
        }
        1 => {
            migrate_v1_to_v2(conn)?;
        }
        _ => (),
    }
    Ok(())
}

fn migrate_v1_to_v2<C>(conn: &C) -> Result<(), C::Error>
where
    C: ExecuteSql,
{
    conn.execute(
        "CREATE TABLE users (
            id BLOB PRIMARY KEY,
            name TEXT NOT NULL,
            password_hash TEXT
        )",
        (),
    )?;
    let senders = conn.rows_vec("SELECT DISTINCT sender FROM events", (), |row| {
        Ok(row.get_text(0))
    })?;
    for sender in &senders {
        let user_id = Uuid::new_v4();
        conn.execute(
            "INSERT INTO users (id, name) VALUES (?1, ?2)",
            (user_id.as_bytes().as_slice(), sender),
        )?;
    }
    conn.execute("ALTER TABLE events RENAME TO events_old", ())?;
    conn.execute(
        "CREATE TABLE events (
            id INTEGER PRIMARY KEY,
            message_id BLOB UNIQUE NOT NULL,
            author_id BLOB NOT NULL,
            content TEXT NOT NULL,
            timestamp_ms INTEGER NOT NULL
        )",
        (),
    )?;
    conn.execute(
        "INSERT INTO events (id, message_id, author_id, content, timestamp_ms) \
            SELECT events_old.id, events_old.message_id, users.id, events_old.content, events_old.timestamp_ms \
            FROM events_old \
            JOIN users ON users.name = events_old.sender",
        (),
    )?;
    conn.execute("DROP TABLE events_old", ())?;
    Ok(())
}

fn create_schema_from_scratch<C>(conn: &C) -> Result<(), C::Error>
where
    C: ExecuteSql,
{
    conn.execute(
        "CREATE TABLE events (
            id INTEGER PRIMARY KEY,
            message_id BLOB UNIQUE NOT NULL,
            author_id BLOB NOT NULL,
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

fn insert_event<C>(conn: &C, event: &Event) -> Result<InsertOutcome, C::Error>
where
    C: ExecuteSql,
{
    let event_id: i64 = event.id.0.try_into().unwrap();
    let Err(err) = conn.execute(
        "INSERT INTO events (id, message_id, author_id, content, timestamp_ms) \
        VALUES (?1, ?2, ?3, ?4, ?5)",
        (
            event_id,
            // TODO: pass as Uuid
            event.message.id.as_bytes().as_slice(),
            &event.message.author,
            &event.message.content,
            event.timestamp_ms as i64,
        ),
    ) else {
        // Message successfully inserted, let's return.
        return Ok(InsertOutcome::New);
    };

    // We had an error, but did something go wrong with accesing the database or is due to a message
    // id being already present?
    if !err.is_unique_constraint_violation() {
        // Something went wrong, lets report the error.
        return Err(err);
    }

    // So it is a unique constraint violation, but is it a duplicate or a conflict?
    let (author, content) = conn.row(
        "SELECT author_id, content FROM events WHERE message_id = ?1",
        event.message.id.as_bytes().as_slice(),
        |row| {
            let author = row.get_uuid(0);
            let content = row.get_text(1);
            Ok((author, content))
        },
    )?;
    if author == event.message.author && content == event.message.content {
        Ok(InsertOutcome::Duplicate)
    } else {
        Ok(InsertOutcome::Conflict)
    }
}

#[cfg(test)]
mod tests {
    use std::time::{SystemTime, UNIX_EPOCH};

    use tempfile::tempdir;

    use super::{Chat as _, PersistentChat};
    use crate::{
        chat::{ChatError, EventId, Message, migrate_chat_persistence},
        persistence::{ExecuteSql, Persistence, SqlitePersistence},
    };
    use uuid::Uuid;

    #[tokio::test]
    async fn events_since_excludes_events_up_to_last_event_id() {
        // Given a history with three events
        let id_1: Uuid = "019c0ab6-9d11-7a5b-abde-cb349e5fd994".parse().unwrap();
        let id_2: Uuid = "019c0ab6-9d11-7a5b-abde-cb349e5fd995".parse().unwrap();
        let id_3: Uuid = "019c0ab6-9d11-7fff-abde-cb349e5fd996".parse().unwrap();
        let persistence = persistence_fake().await;
        let mut history = PersistentChat::new(persistence).await.unwrap();
        history
            .record_message(Message {
                id: id_1,
                author: Uuid::nil(),
                content: "Dummy".to_owned(),
            })
            .await
            .unwrap();
        history
            .record_message(Message {
                id: id_2,
                author: Uuid::nil(),
                content: "Dummy".to_owned(),
            })
            .await
            .unwrap();
        history
            .record_message(Message {
                id: id_3,
                author: Uuid::nil(),
                content: "Dummy".to_owned(),
            })
            .await
            .unwrap();

        // When retrieving events since event 1
        let events = history.events_since(EventId(1)).await.unwrap();

        // Then only events 2 and 3 are returned
        assert_eq!(events.len(), 2);
        assert_eq!(events[0].message.id, id_2);
        assert_eq!(events[1].message.id, id_3);
    }

    #[tokio::test]
    async fn duplicate_same_id_same_message() {
        // Given a message id that already exists with the same content
        let id: Uuid = "019c0ab6-9d11-75ef-ab02-60f070b1582a".parse().unwrap();
        let persistence = persistence_fake().await;
        let mut history = PersistentChat::new(persistence).await.unwrap();
        let message = Message {
            id: "019c0ab6-9d11-75ef-ab02-60f070b1582a".parse().unwrap(),
            author: ALICE_ID,
            content: "Hello".to_owned(),
        };
        history.record_message(message.clone()).await.unwrap();

        // When recording a duplicate message
        let maybe_event = history
            .record_message(Message {
                id,
                author: ALICE_ID,
                content: "Hello".to_owned(),
            })
            .await
            .unwrap();

        // Then no event is emitted
        assert!(maybe_event.is_none());
    }

    #[tokio::test]
    async fn conflict_same_id_different_message() {
        // Given a message id that already exists with the same content
        let id: Uuid = "019c0ab6-9d11-75ef-ab02-60f070b1582a".parse().unwrap();
        let persistence = persistence_fake().await;
        let mut history = PersistentChat::new(persistence).await.unwrap();
        let message = Message {
            id: "019c0ab6-9d11-75ef-ab02-60f070b1582a".parse().unwrap(),
            author: ALICE_ID,
            content: "Hello".to_owned(),
        };
        history.record_message(message.clone()).await.unwrap();

        // When recording a message whose id already exists with different content
        let result = history
            .record_message(Message {
                id,
                author: ALICE_ID,
                content: "Goodbye".to_owned(),
            })
            .await;

        // Then a conflict error is returned
        assert!(matches!(result, Err(ChatError::Conflict)));
    }

    #[tokio::test]
    async fn last_event_id_exceeds_total_number_of_events() {
        // Given a history with one message
        // Given a message id that already exists with the same content
        let persistence = persistence_fake().await;
        let mut history = PersistentChat::new(persistence).await.unwrap();
        let message = Message {
            id: "019c0ab6-9d11-75ef-ab02-60f070b1582a".parse().unwrap(),
            author: Uuid::nil(),
            content: "dummy".to_owned(),
        };
        history.record_message(message.clone()).await.unwrap();

        // When retrieving events since an id beyond the history
        let events = history.events_since(EventId(2)).await.unwrap();

        // Then no events are returned
        assert!(events.is_empty());
    }

    #[tokio::test]
    async fn inserting_new_record_emits_event() {
        // Given
        let persistence = persistence_fake().await;
        let mut history = PersistentChat::new(persistence).await.unwrap();
        let start = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_millis() as u64;

        // When inserting a new record
        let message = Message {
            id: "019c0ab6-9d11-75ef-ab02-60f070b1582a".parse().unwrap(),
            author: ALICE_ID,
            content: "Hi".to_owned(),
        };
        let event = history.record_message(message.clone()).await.unwrap();

        // Then
        let stop = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_millis() as u64;
        assert!(event.is_some());
        let event = event.unwrap();
        assert_eq!(EventId(1), event.id);
        assert_eq!(message, event.message);
        assert!(start <= event.timestamp_ms && event.timestamp_ms <= stop);
    }

    #[tokio::test]
    async fn persist_messages() {
        // Given
        let tmp = tempdir().unwrap();
        let persistence = SqlitePersistence::new(Some(tmp.path()), migrate_user_and_chat)
            .await
            .unwrap();
        let mut history = PersistentChat::new(persistence).await.unwrap();

        // When inserting a new record ...
        let message = Message {
            id: "019c0ab6-9d11-75ef-ab02-60f070b1582a".parse().unwrap(),
            author: ALICE_ID,
            content: "Hi".to_owned(),
        };
        let _event = history.record_message(message.clone()).await.unwrap();
        drop(history);
        // ...and rebooting the persistence layer
        let persistence = SqlitePersistence::new(Some(tmp.path()), migrate_user_and_chat)
            .await
            .unwrap();
        let history = PersistentChat::new(persistence).await.unwrap();

        // Then the event is still present
        let mut events = history.events_since(EventId::before_all()).await.unwrap();
        assert_eq!(1, events.len());
        let event = events.pop().unwrap();
        assert_eq!(event.id, EventId(1));
        assert_eq!(event.message, message);
    }

    async fn persistence_fake() -> impl Persistence {
        SqlitePersistence::new(None, migrate_user_and_chat)
            .await
            .unwrap()
    }

    fn migrate_user_and_chat<C>(conn: &C, from_version: u32) -> Result<(), C::Error>
    where
        C: ExecuteSql,
    {
        conn.execute(
            "CREATE TABLE users (
                id BLOB PRIMARY KEY,
                name TEXT NOT NULL
            )",
            (),
        )?;
        migrate_chat_persistence(conn, from_version)?;
        Ok(())
    }

    const ALICE_ID: Uuid = Uuid::from_bytes([
        0xab, 0x70, 0xb6, 0xca, 0x41, 0x39, 0x49, 0x9f, 0xa6, 0x6d, 0x15, 0xe8, 0x8f, 0x08, 0x1f,
        0xb1,
    ]);
}

use super::{
    event::{Event, EventId},
    message::Message,
};
use crate::{
    persistence::{ExecuteSql, GetField as _, Persistence, PersistenceError as _},
    user::UserId,
};
use uuid::Uuid;

pub enum InsertOutcome {
    /// The message has not been previously recorded and has been added to the record.
    New,
    /// Exactly the same message has been previously recorded. No change to the record
    Duplicate,
    /// A different message with the same id has been previously recorded. No change to the record
    Conflict,
}

#[cfg_attr(test, double_trait::dummies)]
pub trait ChatPersistence {
    /// All events since the event with the given `last_event_id` (exclusive).
    fn events_since(
        &self,
        last_event_id: EventId,
    ) -> impl Future<Output = anyhow::Result<Vec<Event>>> + Send;

    /// The id of the most recently recorded event, or `None` if no event has been recorded yet.
    fn max_event_id(&self) -> impl Future<Output = anyhow::Result<Option<EventId>>> + Send;

    /// Records `event`, unless a message with the same id has already been recorded.
    fn insert_event(
        &self,
        event: &Event,
    ) -> impl Future<Output = anyhow::Result<InsertOutcome>> + Send;
}

impl<P> ChatPersistence for P
where
    P: Persistence + Send + Sync,
{
    async fn events_since(&self, last_event_id: EventId) -> anyhow::Result<Vec<Event>> {
        let query = "SELECT events.id, message_id, events.author_id, content, timestamp_ms \
            FROM events \
            WHERE events.id > ?1 ORDER BY events.id";

        let map = |row: &P::Row<'_>| {
            let event_id = row.get(0);
            let message_id = row.get(1);
            let author = row.get(2);
            let content = row.get(3);
            let timestamp_ms: i64 = row.get(4);
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

        self.rows_vec(query, last_event_id, map).await
    }

    async fn max_event_id(&self) -> anyhow::Result<Option<EventId>> {
        self.row("SELECT MAX(id) FROM events", (), |row| {
            let maybe_event_id: Option<EventId> = row.get(0);
            Ok(maybe_event_id)
        })
        .await
    }

    async fn insert_event(&self, event: &Event) -> anyhow::Result<InsertOutcome> {
        let event = event.clone();
        self.transaction(move |conn| insert_event(conn, &event))
            .await
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
            name TEXT NOT NULL UNIQUE,
            password_hash TEXT
        )",
        (),
    )?;
    let senders: Vec<String> = conn.rows_vec("SELECT DISTINCT sender FROM events", (), |row| {
        Ok(row.get(0))
    })?;
    for sender in &senders {
        let user_id = Uuid::new_v4();
        conn.execute(
            "INSERT INTO users (id, name) VALUES (?1, ?2)",
            (user_id, sender.as_str()),
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

fn insert_event<C>(conn: &C, event: &Event) -> Result<InsertOutcome, C::Error>
where
    C: ExecuteSql,
{
    let Err(err) = conn.execute(
        "INSERT INTO events (id, message_id, author_id, content, timestamp_ms) \
        VALUES (?1, ?2, ?3, ?4, ?5)",
        (
            event.id,
            event.message.id,
            event.message.author,
            event.message.content.as_str(),
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
        event.message.id,
        |row| {
            let author: UserId = row.get(0);
            let content: String = row.get(1);
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
    use std::time::SystemTime;

    use crate::{
        chat::{Event, EventId, Message, MessageId},
        persistence::SqlitePersistence,
        user::UserId,
    };

    use super::{ChatPersistence, InsertOutcome, migrate_chat_persistence};

    #[tokio::test]
    async fn events_since_excludes_events_up_to_last_event_id() {
        // Given three recorded events
        let persistence = persistence_fake().await;
        persistence
            .insert_event(&dummy_event(EventId(1), MessageId::ALPHA))
            .await
            .unwrap();
        persistence
            .insert_event(&dummy_event(EventId(2), MessageId::BETA))
            .await
            .unwrap();
        persistence
            .insert_event(&dummy_event(EventId(3), MessageId::GAMMA))
            .await
            .unwrap();

        // When retrieving events since event 1
        let events = persistence.events_since(EventId(1)).await.unwrap();

        // Then only events 2 and 3 are returned
        assert_eq!(events.len(), 2);
        assert_eq!(events[0].message.id, MessageId::BETA);
        assert_eq!(events[1].message.id, MessageId::GAMMA);
    }

    #[tokio::test]
    async fn events_since_beyond_all_events_returns_empty() {
        // Given a single recorded event
        let persistence = persistence_fake().await;
        persistence
            .insert_event(&dummy_event(EventId(1), MessageId::ALPHA))
            .await
            .unwrap();

        // When retrieving events since an id beyond the history
        let events = persistence.events_since(EventId(2)).await.unwrap();

        // Then no events are returned
        assert!(events.is_empty());
    }

    #[tokio::test]
    async fn insert_new_message() {
        // Given
        let persistence = persistence_fake().await;

        // When
        let outcome = persistence
            .insert_event(&dummy_event(EventId(1), MessageId::ALPHA))
            .await
            .unwrap();

        // Then
        assert!(matches!(outcome, InsertOutcome::New));
    }

    #[tokio::test]
    async fn insert_duplicate_message() {
        // Given a recorded event
        let persistence = persistence_fake().await;
        let message = Message {
            id: MessageId::ALPHA,
            author: UserId::ALICE,
            content: "Hello".to_owned(),
        };
        persistence
            .insert_event(&Event::with_timestamp(
                EventId(1),
                message.clone(),
                SystemTime::UNIX_EPOCH,
            ))
            .await
            .unwrap();

        // When recording the exact same message again under a new event id, as a retry would
        let outcome = persistence
            .insert_event(&Event::with_timestamp(
                EventId(2),
                message,
                SystemTime::UNIX_EPOCH,
            ))
            .await
            .unwrap();

        // Then it is reported as a duplicate
        assert!(matches!(outcome, InsertOutcome::Duplicate));
    }

    #[tokio::test]
    async fn insert_conflictig_message() {
        // Given a recorded event
        let persistence = persistence_fake().await;
        persistence
            .insert_event(&Event::with_timestamp(
                EventId(1),
                Message {
                    id: MessageId::ALPHA,
                    author: UserId::ALICE,
                    content: "Hello".to_owned(),
                },
                SystemTime::UNIX_EPOCH,
            ))
            .await
            .unwrap();

        // When recording a different message under the same message id
        let outcome = persistence
            .insert_event(&Event::with_timestamp(
                EventId(2),
                Message {
                    id: MessageId::ALPHA,
                    author: UserId::ALICE,
                    content: "Goodbye".to_owned(),
                },
                SystemTime::UNIX_EPOCH,
            ))
            .await
            .unwrap();

        // Then it is reported as a conflict
        assert!(matches!(outcome, InsertOutcome::Conflict));
    }

    fn dummy_event(id: EventId, message_id: MessageId) -> Event {
        Event::with_timestamp(
            id,
            Message {
                id: message_id,
                ..Message::dummy()
            },
            SystemTime::UNIX_EPOCH,
        )
    }

    async fn persistence_fake() -> impl ChatPersistence {
        SqlitePersistence::new(None, migrate_chat_persistence)
            .await
            .unwrap()
    }
}

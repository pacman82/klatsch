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

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
        let query = "SELECT id, message_id, sender, content, timestamp_ms \
            FROM events \
            WHERE id > ?1 ORDER BY id";

        let map = |row: &P::Row<'_>| {
            let event_id = row.get_i64(0);
            let event_id = EventId(event_id.try_into().unwrap());
            let message_id = row.get_blob(1);
            let message_id = Uuid::from_bytes(
                message_id
                    .try_into()
                    .expect("message_id must be a 16-byte BLOB column"),
            );
            let sender = row.get_text(2);
            let content = row.get_text(3);
            let timestamp_ms = row.get_i64(4);
            let timestamp_ms: u64 = timestamp_ms.try_into().unwrap();
            let message = Message {
                id: message_id,
                sender,
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

pub fn create_schema_chat<C>(conn: &C) -> Result<(), C::Error>
where
    C: ExecuteSql,
{
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

fn insert_event<C>(conn: &C, event: &Event) -> Result<InsertOutcome, C::Error>
where
    C: ExecuteSql,
{
    let event_id: i64 = event.id.0.try_into().unwrap();
    let Err(err) = conn.execute(
        "INSERT INTO events (id, message_id, sender, content, timestamp_ms) \
        VALUES (?1, ?2, ?3, ?4, ?5)",
        (
            event_id,
            event.message.id.as_bytes().as_slice(),
            &event.message.sender,
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
    let (sender, content) = conn.row(
        "SELECT sender, content FROM events WHERE message_id = ?1",
        event.message.id.as_bytes().as_slice(),
        |row| {
            let sender = row.get_text(0);
            let content = row.get_text(1);
            Ok((sender, content))
        },
    )?;
    if sender == event.message.sender && content == event.message.content {
        Ok(InsertOutcome::Duplicate)
    } else {
        Ok(InsertOutcome::Conflict)
    }
}

#[cfg(test)]
mod tests {
    use std::{
        sync::{Arc, Mutex},
        time::{SystemTime, UNIX_EPOCH},
    };

    use super::{Chat as _, PersistentChat, create_schema_chat};
    use crate::{
        chat::{ChatError, EventId, Message},
        persistence::{
            ExecuteSql, FieldAccess, Parameter, Parameters, Persistence, PersistenceError,
            SqlitePersistence,
        },
    };
    use uuid::Uuid;

    fn dummy_message(id: Uuid) -> Message {
        Message {
            id,
            sender: "dummy".to_owned(),
            content: "dummy".to_owned(),
        }
    }

    #[tokio::test]
    async fn recorded_message_is_broadcasted_in_event() {
        // Given a chat history
        let persistence = SqlitePersistence::new(None, create_schema_chat)
            .await
            .unwrap();
        let mut history = PersistentChat::new(persistence).await.unwrap();

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
        let persistence = SqlitePersistence::new(None, create_schema_chat)
            .await
            .unwrap();
        let mut history = PersistentChat::new(persistence).await.unwrap();

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
        let persistence = SqlitePersistence::new(None, create_schema_chat)
            .await
            .unwrap();
        let mut history = PersistentChat::new(persistence).await.unwrap();
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
    async fn no_new_event_is_emitted_for_duplicate_messages() {
        // Given a message id that already exists with the same content
        let id: Uuid = "019c0ab6-9d11-75ef-ab02-60f070b1582a".parse().unwrap();
        let persistence = PersistenceMock::new(vec![
            ExpectedQuery {
                sql: "SELECT MAX(id) FROM events",
                parameters: vec![],
                result: Ok(vec![vec![StubField::I64(1)]]),
            },
            ExpectedQuery {
                sql: "INSERT INTO events (id, message_id, sender, content, timestamp_ms) VALUES \
                (?1, ?2, ?3, ?4, ?5)",
                parameters: vec![
                    2i64.into(),
                    id.as_bytes().to_vec().into(),
                    "Bob".into(),
                    "Hello, World!".into(),
                    ParameterExpectation::recent_timestamp_ms(),
                ],
                result: Err(StubError::UniqueConstraintViolation),
            },
            ExpectedQuery {
                sql: "SELECT sender, content FROM events WHERE message_id = ?1",
                parameters: vec![id.as_bytes().to_vec().into()],
                result: Ok(vec![vec![
                    StubField::Text("Bob".to_owned()),
                    StubField::Text("Hello, World!".to_owned()),
                ]]),
            },
        ]);
        let mut history = PersistentChat::new(persistence).await.unwrap();

        // When recording a duplicate message
        let result = history
            .record_message(Message {
                id,
                sender: "Bob".to_owned(),
                content: "Hello, World!".to_owned(),
            })
            .await
            .unwrap();

        // Then no event is emitted
        assert!(result.is_none());
    }

    #[tokio::test]
    async fn different_message_with_same_id_is_a_conflict() {
        // Given a message id that already exists with different content
        let id: Uuid = "019c0ab6-9d11-75ef-ab02-60f070b1582a".parse().unwrap();
        let persistence = PersistenceMock::new(vec![
            ExpectedQuery {
                sql: "SELECT MAX(id) FROM events",
                parameters: vec![],
                result: Ok(vec![vec![StubField::I64(1)]]),
            },
            ExpectedQuery {
                sql: "INSERT INTO events (id, message_id, sender, content, timestamp_ms) VALUES \
                (?1, ?2, ?3, ?4, ?5)",
                parameters: vec![
                    2i64.into(),
                    id.as_bytes().to_vec().into(),
                    "Alice".into(),
                    "Goodbye".into(),
                    ParameterExpectation::recent_timestamp_ms(),
                ],
                // When we get a unique constraint violation
                result: Err(StubError::UniqueConstraintViolation),
            },
            // and see a message with the same id but different content
            ExpectedQuery {
                sql: "SELECT sender, content FROM events WHERE message_id = ?1",
                parameters: vec![id.as_bytes().to_vec().into()],
                result: Ok(vec![vec![
                    StubField::Text("Alice".to_owned()),
                    StubField::Text("Hello".to_owned()),
                ]]),
            },
        ]);
        let mut history = PersistentChat::new(persistence).await.unwrap();

        // When recording a message whose id already exists with different content
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
        let persistence = PersistenceMock::new(vec![
            ExpectedQuery {
                sql: "SELECT MAX(id) FROM events",
                parameters: vec![],
                result: Ok(vec![vec![StubField::I64(1)]]),
            },
            ExpectedQuery {
                sql: "SELECT id, message_id, sender, content, timestamp_ms \
                    FROM events \
                    WHERE id > ?1 ORDER BY id",
                parameters: vec![2i64.into()],
                result: Ok(vec![]),
            },
        ]);
        let history = PersistentChat::new(persistence).await.unwrap();

        // When retrieving events since an id beyond the history
        let events = history.events_since(EventId(2)).await.unwrap();

        // Then no events are returned
        assert!(events.is_empty());
    }

    /// Interaction with persistence during inserting a new row
    #[tokio::test]
    async fn insert_new_record() {
        // Given
        let message = Message {
            id: "019c0ab6-9d11-75ef-ab02-60f070b1582a".parse().unwrap(),
            sender: "Alice".to_owned(),
            content: "Hi".to_owned(),
        };
        let persistence = PersistenceMock::new(vec![
            ExpectedQuery {
                sql: "SELECT MAX(id) FROM events",
                parameters: vec![],
                result: Ok(vec![vec![StubField::I64(42)]]),
            },
            ExpectedQuery {
                sql: "INSERT INTO events (id, message_id, sender, content, timestamp_ms) VALUES \
                (?1, ?2, ?3, ?4, ?5)",
                parameters: vec![
                    43.into(),
                    message.id.as_bytes().to_vec().into(),
                    "Alice".into(),
                    "Hi".into(),
                    ParameterExpectation::recent_timestamp_ms(),
                ],
                result: Ok(vec![]),
            },
        ]);

        // When
        let mut chat = PersistentChat::new(persistence).await.unwrap();
        let maybe_event = chat.record_message(message.clone()).await.unwrap();

        // Then
        let event = maybe_event.expect("An event has been emitted");
        assert_eq!(event.id, EventId(43));
        assert_eq!(event.message, message);
    }

    trait PersistenceStub {
        fn query(
            &self,
            query: &str,
            params: impl Parameters,
        ) -> Result<Vec<Vec<StubField>>, StubError>;
    }

    impl<T> ExecuteSql for T
    where
        T: PersistenceStub,
    {
        type Row<'a> = Vec<StubField>;
        type Error = StubError;

        fn execute(&self, query: &str, params: impl Parameters) -> Result<(), Self::Error> {
            self.query(query, params)?;
            Ok(())
        }

        fn row<O>(
            &self,
            query: &'static str,
            params: impl Parameters,
            map: impl Fn(&Self::Row<'_>) -> Result<O, Self::Error>,
        ) -> Result<O, Self::Error> {
            let rows = self.query(query, params)?;
            map(&rows[0])
        }
    }

    impl<T> Persistence for T
    where
        T: PersistenceStub + Sync,
    {
        type Row<'a> = Vec<StubField>;
        type Error = StubError;
        type Connection = Self;

        async fn row<O>(
            &self,
            query: &'static str,
            params: impl Parameters + Send + Sync + 'static,
            map: impl Fn(&Self::Row<'_>) -> Result<O, Self::Error> + Send + 'static,
        ) -> anyhow::Result<O>
        where
            O: Send + 'static,
        {
            let rows = self.query(query, params)?;
            let row = map(&rows[0]).unwrap();
            Ok(row)
        }

        async fn rows_vec<O>(
            &self,
            query: &'static str,
            params: impl Parameters + Send + Sync + 'static,
            map: impl Fn(&Self::Row<'_>) -> Result<O, Self::Error> + Send + 'static,
        ) -> anyhow::Result<Vec<O>>
        where
            O: Send + 'static,
        {
            let rows = self.query(query, params)?;
            let result = rows.iter().map(|row| map(row).unwrap()).collect();
            Ok(result)
        }

        async fn transaction<O>(
            &self,
            execute: impl FnOnce(&Self::Connection) -> Result<O, Self::Error> + Send + 'static,
        ) -> anyhow::Result<O> {
            let value = execute(&self).unwrap();
            Ok(value)
        }
    }

    struct PersistenceMock(Arc<Mutex<Vec<ExpectedQuery>>>);

    impl PersistenceMock {
        fn new(mut expectations: Vec<ExpectedQuery>) -> Self {
            expectations.reverse();
            PersistenceMock(Arc::new(Mutex::new(expectations)))
        }
    }

    impl PersistenceStub for PersistenceMock {
        fn query(
            &self,
            query: &str,
            params: impl Parameters,
        ) -> Result<Vec<Vec<StubField>>, StubError> {
            let expected = self.0.lock().unwrap().pop().unwrap();
            expected.invoke(query, params)
        }
    }

    #[derive(Debug)]
    pub enum StubField {
        I64(i64),
        Text(String),
    }

    impl FieldAccess for Vec<StubField> {
        fn get_i64_opt(&self, index: usize) -> Option<i64> {
            match &self[index] {
                StubField::I64(value) => Some(*value),
                other => panic!("expected I64 at index {index}, got {other:?}"),
            }
        }

        fn get_text(&self, index: usize) -> String {
            match &self[index] {
                StubField::Text(value) => value.clone(),
                other => panic!("expected Text at index {index}, got {other:?}"),
            }
        }
    }

    #[derive(Debug, thiserror::Error)]
    pub enum StubError {
        #[error("test unique constraint violation")]
        UniqueConstraintViolation,
    }

    impl PersistenceError for StubError {
        fn is_unique_constraint_violation(&self) -> bool {
            match self {
                StubError::UniqueConstraintViolation => true,
            }
        }
    }

    struct ExpectedQuery {
        sql: &'static str,
        parameters: Vec<ParameterExpectation>,
        result: Result<Vec<Vec<StubField>>, StubError>,
    }

    impl ExpectedQuery {
        fn invoke(
            self,
            query: &str,
            params: impl Parameters,
        ) -> Result<Vec<Vec<StubField>>, StubError> {
            assert_eq!(self.sql, query);
            assert_eq!(self.parameters.len(), params.len());
            for (index, expectation) in self.parameters.into_iter().enumerate() {
                (expectation.0)(params.get(index));
            }
            self.result
        }
    }

    struct ParameterExpectation(Box<dyn FnOnce(Parameter<'_>) + Send>);

    impl ParameterExpectation {
        fn recent_timestamp_ms() -> Self {
            let before = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_millis() as i64;
            ParameterExpectation(Box::new(move |param| {
                let Parameter::I64(actual) = param else {
                    panic!("expected I64 parameter for timestamp, got {param:?}");
                };
                let after = SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .unwrap()
                    .as_millis() as i64;
                assert!(
                    actual >= before && actual <= after,
                    "timestamp {actual} not in expected range [{before}, {after}]"
                );
            }))
        }
    }

    impl From<i64> for ParameterExpectation {
        fn from(expected: i64) -> Self {
            ParameterExpectation(Box::new(move |actual| {
                assert_eq!(Parameter::I64(expected), actual);
            }))
        }
    }

    impl From<&str> for ParameterExpectation {
        fn from(expected: &str) -> Self {
            let expected = expected.to_owned();
            ParameterExpectation(Box::new(move |actual| {
                assert_eq!(Parameter::Text(expected.into()), actual);
            }))
        }
    }

    impl From<Vec<u8>> for ParameterExpectation {
        fn from(expected: Vec<u8>) -> Self {
            ParameterExpectation(Box::new(move |actual| {
                assert_eq!(Parameter::Blob(expected.into()), actual);
            }))
        }
    }
}

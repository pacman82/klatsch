use super::{
    chat_persistence::{ChatPersistence, InsertOutcome},
    event::{Event, EventId},
    message::Message,
};
use std::future::Future;

#[cfg_attr(test, double_trait::dummies)]
pub trait ChatStore {
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

impl<P> ChatStore for PersistentChat<P>
where
    P: ChatPersistence + Sync + Send,
{
    async fn events_since(&self, last_event_id: EventId) -> anyhow::Result<Vec<Event>> {
        self.persistence.events_since(last_event_id).await
    }

    async fn record_message(&mut self, message: Message) -> Result<Option<Event>, ChatError> {
        let event_id = self.last_event_id.successor();
        let event = Event::new(event_id, message);
        let result = self.persistence.insert_event(&event).await;
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
    P: ChatPersistence,
{
    pub async fn new(persistence: P) -> anyhow::Result<Self> {
        let last_event_id = persistence
            .max_event_id()
            .await?
            .unwrap_or_else(EventId::before_all);
        let new = PersistentChat {
            persistence,
            last_event_id,
        };
        Ok(new)
    }
}

#[cfg(test)]
mod tests {
    use std::time::{SystemTime, UNIX_EPOCH};

    use tempfile::tempdir;

    use super::{ChatStore as _, PersistentChat};
    use crate::{
        chat::{ChatError, EventId, Message, MessageId, migrate_chat_persistence},
        persistence::{ExecuteSql, Persistence, SqlitePersistence},
        user::UserId,
    };

    #[tokio::test]
    async fn events_since_excludes_events_up_to_last_event_id() {
        // Given a history with three events
        let persistence = persistence_fake().await;
        let mut history = PersistentChat::new(persistence).await.unwrap();
        history
            .record_message(Message {
                id: MessageId::ALPHA,
                author: UserId::nil(),
                content: "Dummy".to_owned(),
            })
            .await
            .unwrap();
        history
            .record_message(Message {
                id: MessageId::BETA,
                author: UserId::nil(),
                content: "Dummy".to_owned(),
            })
            .await
            .unwrap();
        history
            .record_message(Message {
                id: MessageId::GAMMA,
                author: UserId::nil(),
                content: "Dummy".to_owned(),
            })
            .await
            .unwrap();

        // When retrieving events since event 1
        let events = history.events_since(EventId(1)).await.unwrap();

        // Then only events 2 and 3 are returned
        assert_eq!(events.len(), 2);
        assert_eq!(events[0].message.id, MessageId::BETA);
        assert_eq!(events[1].message.id, MessageId::GAMMA);
    }

    #[tokio::test]
    async fn duplicate_same_id_same_message() {
        // Given a message id that already exists with the same content
        let persistence = persistence_fake().await;
        let mut history = PersistentChat::new(persistence).await.unwrap();
        let message = Message {
            id: MessageId::ALPHA,
            author: UserId::ALICE,
            content: "Hello".to_owned(),
        };
        history.record_message(message.clone()).await.unwrap();

        // When recording a duplicate message
        let maybe_event = history
            .record_message(Message {
                id: MessageId::ALPHA,
                author: UserId::ALICE,
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
        let persistence = persistence_fake().await;
        let mut history = PersistentChat::new(persistence).await.unwrap();
        let message = Message {
            id: MessageId::ALPHA,
            author: UserId::ALICE,
            content: "Hello".to_owned(),
        };
        history.record_message(message.clone()).await.unwrap();

        // When recording a message whose id already exists with different content
        let result = history
            .record_message(Message {
                id: MessageId::ALPHA,
                author: UserId::ALICE,
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
            author: UserId::nil(),
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
            author: UserId::ALICE,
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
            author: UserId::ALICE,
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
}

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

    use super::{ChatPersistence, ChatStore as _, Event, InsertOutcome, PersistentChat};
    use crate::{
        chat::{ChatError, EventId, Message, MessageId},
        user::UserId,
    };

    #[tokio::test]
    async fn events_since_forwards_to_persistence() {
        // Given a persistence layer that returns a canned event for a given last_event_id
        struct EventsSinceMock;
        impl ChatPersistence for EventsSinceMock {
            async fn events_since(&self, last_event_id: EventId) -> anyhow::Result<Vec<Event>> {
                // Expect being called with same arguments
                assert_eq!(last_event_id, EventId(7));

                let event = Event::with_timestamp(EventId(8), Message::dummy(), UNIX_EPOCH);
                Ok(vec![event])
            }
        }
        let history = PersistentChat::new(EventsSinceMock).await.unwrap();

        // When
        let events = history.events_since(EventId(7)).await.unwrap();

        // Then the persistence's response is forwarded unchanged
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].id, EventId(8));
    }

    #[tokio::test]
    async fn emit_no_events_for_duplicates() {
        // Given
        struct DuplicateStub;
        impl ChatPersistence for DuplicateStub {
            async fn insert_event(&self, _event: &Event) -> anyhow::Result<InsertOutcome> {
                Ok(InsertOutcome::Duplicate)
            }
        }
        let mut history = PersistentChat::new(DuplicateStub).await.unwrap();

        // When inserting a message reported to be a duplicate
        let maybe_event = history.record_message(Message::dummy()).await.unwrap();

        // Then no event is emitted
        assert!(maybe_event.is_none());
    }

    #[tokio::test]
    async fn conflicting_message_emits_error() {
        // Given a persistence layer reporting the message as conflicting
        struct ConflictStub;
        impl ChatPersistence for ConflictStub {
            async fn insert_event(&self, _event: &Event) -> anyhow::Result<InsertOutcome> {
                Ok(InsertOutcome::Conflict)
            }
        }
        let mut history = PersistentChat::new(ConflictStub).await.unwrap();

        // When recording the message which is reported as conflict
        let result = history.record_message(Message::dummy()).await;

        // Then a conflict error is returned
        assert!(matches!(result, Err(ChatError::Conflict)));
    }

    #[tokio::test]
    async fn inserting_new_message() {
        // Given
        struct NewStub;
        impl ChatPersistence for NewStub {
            async fn insert_event(&self, _event: &Event) -> anyhow::Result<InsertOutcome> {
                Ok(InsertOutcome::New)
            }
        }
        let mut history = PersistentChat::new(NewStub).await.unwrap();
        let start = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_millis() as u64;

        // When inserting a new record
        let message = Message {
            id: MessageId::ALPHA,
            author: UserId::ALICE,
            content: "Hello".to_owned(),
        };
        let event = history.record_message(message.clone()).await.unwrap();

        // Then it emits an event with a current timestamp
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
    async fn forward_messages_to_persistence() {
        // Given a persistence layer that asserts on the message it receives
        struct InsertEventMock;

        impl ChatPersistence for InsertEventMock {
            async fn insert_event(&self, event: &Event) -> anyhow::Result<InsertOutcome> {
                let expected = Message {
                    id: MessageId::ALPHA,
                    author: UserId::ALICE,
                    content: "Hello".to_owned(),
                };

                assert_eq!(event.message, expected);

                Ok(InsertOutcome::New)
            }
        }
        let mut history = PersistentChat::new(InsertEventMock).await.unwrap();

        // When recording a message
        history
            .record_message(Message {
                id: MessageId::ALPHA,
                author: UserId::ALICE,
                content: "Hello".to_owned(),
            })
            .await
            .unwrap();
    }
}

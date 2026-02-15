use std::{cmp::min, collections::HashMap, time::SystemTime};

use serde::Deserialize;
use uuid::Uuid;

#[cfg_attr(test, double_trait::dummies)]
pub trait Chat {
    /// All events since the event with the given `last_event_id` (exclusive).
    fn events_since(&self, last_event_id: u64) -> Vec<Event>;

    /// Record a message and return the corresponding event. `None` indiactes that no event should
    /// be emitted due to the message being a duplicate of an already recorded message.
    fn record_message(&mut self, message: Message) -> Result<Option<Event>, ChatError>;
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
    pub timestamp: SystemTime,
}

#[derive(Debug)]
pub enum ChatError {
    Conflict,
}

impl Chat for InMemoryChatHistory {
    fn events_since(&self, last_event_id: u64) -> Vec<Event> {
        let last_event_id = min(last_event_id as usize, self.events.len());
        self.events[last_event_id..].to_owned()
    }

    fn record_message(&mut self, message: Message) -> Result<Option<Event>, ChatError> {
        if let Some(existing) = self.seen_messages.get(&message.id) {
            if *existing == message {
                return Ok(None);
            }
            return Err(ChatError::Conflict);
        }
        self.seen_messages.insert(message.id, message.clone());
        let event = Event {
            id: self.events.len() as u64 + 1,
            message,
            timestamp: SystemTime::now(),
        };
        self.events.push(event.clone());
        Ok(Some(event))
    }
}

pub struct InMemoryChatHistory {
    events: Vec<Event>,
    seen_messages: HashMap<Uuid, Message>,
}

impl InMemoryChatHistory {
    pub fn new() -> Self {
        InMemoryChatHistory {
            events: Vec::new(),
            seen_messages: HashMap::new(),
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

    #[test]
    fn recorded_message_is_preserved_in_event() {
        // Given a chat history
        let mut history = InMemoryChatHistory::new();

        // When recording a message ...
        let msg = Message {
            id: "019c0ab6-9d11-75ef-ab02-60f070b1582a".parse().unwrap(),
            sender: "Alice".to_string(),
            content: "Hello".to_string(),
        };
        // ... and retrieving its corresponding event
        let event = history.record_message(msg.clone()).unwrap().unwrap();

        // Then the event contains the same message.
        assert_eq!(event.message, msg);
    }

    #[test]
    fn messages_are_retrieved_in_insertion_order() {
        // Given an empty chat history
        let mut history = InMemoryChatHistory::new();

        // When recording two messages after each other...
        let id_1 = "019c0ab6-9d11-75ef-ab02-60f070b1582a".parse().unwrap();
        let id_2 = "019c0ab6-9d11-7a5b-abde-cb349e5fd995".parse().unwrap();
        history.record_message(dummy_message(id_1)).unwrap();
        history.record_message(dummy_message(id_2)).unwrap();
        // ...and retrieving these messages after insertion
        let events = history.events_since(0);

        // Then the messages are retrieved in the order they were inserted.
        assert_eq!(events.len(), 2);
        assert_eq!(events[0].message.id, id_1);
        assert_eq!(events[1].message.id, id_2);
    }

    #[test]
    fn events_since_excludes_events_up_to_last_event_id() {
        // Given a history with three messages
        let mut history = InMemoryChatHistory::new();
        let id_1 = "019c0ab6-9d11-75ef-ab02-60f070b1582a".parse().unwrap();
        let id_2 = "019c0ab6-9d11-7a5b-abde-cb349e5fd995".parse().unwrap();
        let id_3 = "019c0ab6-9d11-7fff-abde-cb349e5fd996".parse().unwrap();
        for id in [id_1, id_2, id_3] {
            history.record_message(dummy_message(id)).unwrap();
        }

        // When retrieving events since event 1
        let events = history.events_since(1);

        // Then only events 2 and 3 are returned
        assert_eq!(events.len(), 2);
        assert_eq!(events[0].message.id, id_2);
        assert_eq!(events[1].message.id, id_3);
    }

    #[test]
    fn duplicate_message_id_is_not_stored() {
        // Given a history with one message
        let mut history = InMemoryChatHistory::new();
        let id = "019c0ab6-9d11-75ef-ab02-60f070b1582a".parse().unwrap();
        history
            .record_message(Message {
                id,
                sender: "Bob".to_owned(),
                content: "Hello, World!".to_owned(),
            })
            .unwrap();

        // When recording a duplicate message with the same id
        let result = history
            .record_message(Message {
                id,
                sender: "Bob".to_owned(),
                content: "Hello, World!".to_owned(),
            })
            .unwrap();

        // Then no event is emitted and the history remains unchanged
        assert!(result.is_none());
        assert_eq!(history.events_since(0).len(), 1);
    }

    #[test]
    fn different_message_with_same_id_is_a_conflict() {
        // Given a history with one message
        let mut history = InMemoryChatHistory::new();
        let id = "019c0ab6-9d11-75ef-ab02-60f070b1582a".parse().unwrap();
        history
            .record_message(Message {
                id,
                sender: "Alice".to_owned(),
                content: "Hello".to_owned(),
            })
            .unwrap();

        // When recording a different message with the same id
        let result = history.record_message(Message {
            id,
            sender: "Alice".to_owned(),
            content: "Goodbye".to_owned(),
        });

        // Then a conflict error is returned
        assert!(matches!(result, Err(ChatError::Conflict)));
    }

    #[test]
    fn last_event_id_exceeds_total_number_of_events() {
        // Given a history with one message
        let mut history = InMemoryChatHistory::new();
        history
            .record_message(dummy_message(
                "019c0ab6-9d11-75ef-ab02-60f070b1582a".parse().unwrap(),
            ))
            .unwrap();

        // When retrieving events since an id beyond the history
        let events = history.events_since(2);

        // Then no events are returned
        assert!(events.is_empty());
    }
}

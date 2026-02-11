use std::{cmp::min, time::SystemTime};

use super::Message;

pub trait ChatHistory {
    /// All events stored in the chat history since the event with the given `last_event_id`
    /// (exclusive).
    fn events_since(&self, last_event_id: u64) -> Vec<Event>;

    /// Add a message to the chat history and emit the corresponding event.
    fn record_message(&mut self, message: Message) -> Event;
}

impl ChatHistory for InMemoryChatHistory {
    fn events_since(&self, last_event_id: u64) -> Vec<Event> {
        let last_event_id = min(last_event_id as usize, self.events.len());
        self.events[last_event_id..].to_owned()
    }

    fn record_message(&mut self, message: Message) -> Event {
        let event = Event {
            id: self.events.len() as u64 + 1,
            message,
            timestamp: SystemTime::now(),
        };
        self.events.push(event.clone());
        event
    }
}

pub struct InMemoryChatHistory {
    events: Vec<Event>,
}

impl InMemoryChatHistory {
    pub fn new() -> Self {
        InMemoryChatHistory { events: Vec::new() }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
        let event = history.record_message(msg.clone());

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
        history.record_message(Message {
            id: id_1,
            sender: "dummy".to_string(),
            content: "dummy".to_string(),
        });
        history.record_message(Message {
            id: id_2,
            sender: "dummy".to_string(),
            content: "dummy".to_string(),
        });
        // ...and retrieving these messages after insertion
        let events = history.events_since(0);

        // Then the messages are retrieved in the order they were inserted.
        assert_eq!(events.len(), 2);
        assert_eq!(events[0].message.id, id_1);
        assert_eq!(events[1].message.id, id_2);
    }
}

/// A message as it is stored and represented as part of a chat.
#[derive(Clone)]
pub struct Event {
    /// One based ordered identifier of the events in the chat.
    pub id: u64,
    pub message: Message,
    pub timestamp: SystemTime,
}

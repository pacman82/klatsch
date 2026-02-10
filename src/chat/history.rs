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

/// A message as it is stored and represented as part of a chat.
#[derive(Clone)]
pub struct Event {
    /// One based ordered identifier of the events in the chat.
    pub id: u64,
    pub message: Message,
    pub timestamp: SystemTime,
}

use std::{cmp::min, time::SystemTime};

use super::Message;

pub struct ChatHistory {
    events: Vec<Event>,
}

impl ChatHistory {
    pub fn new() -> Self {
        ChatHistory { events: Vec::new() }
    }

    pub fn events_since(&self, last_event_id: u64) -> Vec<Event> {
        let last_event_id = min(last_event_id as usize, self.events.len());
        self.events[last_event_id..].to_owned()
    }

    pub fn record_message(&mut self, message: Message) -> Event {
        let event = Event {
            id: self.events.len() as u64 + 1,
            message,
            timestamp: SystemTime::now(),
        };
        self.events.push(event.clone());
        event
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

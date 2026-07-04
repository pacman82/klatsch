use uuid::Uuid;

#[cfg_attr(test, double_trait::dummies)]
pub trait Sessions {
    fn create(&mut self, user_id: Uuid) -> Uuid;
    fn lookup(&mut self, session_id: Uuid) -> Option<Uuid>;
    fn destroy(&mut self, session_id: Uuid);
}

use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
};

#[derive(Clone)]
pub struct InMemorySessions {
    sessions: Arc<Mutex<HashMap<Uuid, Uuid>>>,
}

impl InMemorySessions {
    pub fn new() -> Self {
        Self {
            sessions: Arc::new(Mutex::new(HashMap::new())),
        }
    }
}

impl Sessions for InMemorySessions {
    fn create(&mut self, user_id: Uuid) -> Uuid {
        let session_id = Uuid::new_v4();
        self.sessions
            .lock()
            .expect("sessions lock must not be poisoned")
            .insert(session_id, user_id);
        session_id
    }

    fn lookup(&mut self, session_id: Uuid) -> Option<Uuid> {
        self.sessions
            .lock()
            .expect("sessions lock must not be poisoned")
            .get(&session_id)
            .copied()
    }

    fn destroy(&mut self, session_id: Uuid) {
        self.sessions
            .lock()
            .expect("sessions lock must not be poisoned")
            .remove(&session_id);
    }
}

#[cfg(test)]
mod tests {
    use uuid::Uuid;

    use super::{InMemorySessions, Sessions as _};

    const ALICE_ID: Uuid = Uuid::from_bytes([
        0xab, 0x70, 0xb6, 0xca, 0x41, 0x39, 0x49, 0x9f, 0xa6, 0x6d, 0x15, 0xe8, 0x8f, 0x08, 0x1f,
        0xb1,
    ]);

    #[test]
    fn lookup_returns_user_id_session_was_created_for() {
        // Given
        let mut sessions = InMemorySessions::new();
        // When
        let session_id = sessions.create(ALICE_ID);
        // Then
        assert_eq!(sessions.lookup(session_id), Some(ALICE_ID));
    }

    #[test]
    fn destroyed_session_cannot_be_looked_up() {
        // Given
        let mut sessions = InMemorySessions::new();
        let session_id = sessions.create(ALICE_ID);
        // When
        sessions.destroy(session_id);
        // Then
        assert_eq!(sessions.lookup(session_id), None);
    }
}

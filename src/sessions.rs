use std::{
    collections::HashMap,
    fmt,
    str::FromStr,
    sync::{Arc, Mutex},
};

use uuid::Uuid;

use crate::user::UserId;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct SessionId(Uuid);

impl SessionId {
    pub const fn from_uuid(uuid: Uuid) -> Self {
        SessionId(uuid)
    }

    fn new() -> Self {
        Self::from_uuid(Uuid::new_v4())
    }
}

impl fmt::Display for SessionId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

impl FromStr for SessionId {
    type Err = uuid::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        s.parse().map(SessionId)
    }
}

#[cfg_attr(test, double_trait::dummies)]
pub trait Sessions {
    fn create(&mut self, user_id: UserId) -> SessionId;
    fn lookup(&mut self, session_id: SessionId) -> Option<UserId>;
    fn destroy(&mut self, session_id: SessionId);
}

#[derive(Clone)]
pub struct InMemorySessions {
    sessions: Arc<Mutex<HashMap<SessionId, UserId>>>,
}

impl InMemorySessions {
    pub fn new() -> Self {
        Self {
            sessions: Arc::new(Mutex::new(HashMap::new())),
        }
    }
}

impl Sessions for InMemorySessions {
    fn create(&mut self, user_id: UserId) -> SessionId {
        let session_id = SessionId::new();
        self.sessions
            .lock()
            .expect("sessions lock must not be poisoned")
            .insert(session_id, user_id);
        session_id
    }

    fn lookup(&mut self, session_id: SessionId) -> Option<UserId> {
        self.sessions
            .lock()
            .expect("sessions lock must not be poisoned")
            .get(&session_id)
            .copied()
    }

    fn destroy(&mut self, session_id: SessionId) {
        self.sessions
            .lock()
            .expect("sessions lock must not be poisoned")
            .remove(&session_id);
    }
}

#[cfg(test)]
mod tests {
    use uuid::Uuid;

    use crate::user::UserId;

    use super::{InMemorySessions, Sessions as _};

    const ALICE_ID: UserId = UserId::from_uuid(Uuid::from_bytes([
        0xab, 0x70, 0xb6, 0xca, 0x41, 0x39, 0x49, 0x9f, 0xa6, 0x6d, 0x15, 0xe8, 0x8f, 0x08, 0x1f,
        0xb1,
    ]));

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

use std::collections::HashMap;

use crate::user::UserId;

use super::SessionId;

pub trait SessionStore {
    fn create(&mut self, user_id: UserId) -> SessionId;
    fn lookup(&self, session_id: SessionId) -> Option<UserId>;
    fn destroy(&mut self, session_id: SessionId);
}

pub struct InMemorySessionStore {
    sessions: HashMap<SessionId, UserId>,
}

impl InMemorySessionStore {
    pub fn new() -> Self {
        Self {
            sessions: HashMap::new(),
        }
    }
}

impl SessionStore for InMemorySessionStore {
    fn create(&mut self, user_id: UserId) -> SessionId {
        let session_id = SessionId::new();
        self.sessions.insert(session_id, user_id);
        session_id
    }

    fn lookup(&self, session_id: SessionId) -> Option<UserId> {
        self.sessions.get(&session_id).copied()
    }

    fn destroy(&mut self, session_id: SessionId) {
        self.sessions.remove(&session_id);
    }
}

#[cfg(test)]
mod tests {
    use crate::user::UserId;

    use super::{InMemorySessionStore, SessionStore as _};

    #[test]
    fn lookup_returns_user_id_session_was_created_for() {
        // Given
        let mut store = InMemorySessionStore::new();
        // When
        let session_id = store.create(UserId::ALICE);
        let looked_up_session_id = store.lookup(session_id);
        // Then
        assert_eq!(looked_up_session_id, Some(UserId::ALICE));
    }

    #[test]
    fn destroyed_session_cannot_be_looked_up() {
        // Given
        let mut store = InMemorySessionStore::new();
        let session_id = store.create(UserId::ALICE);
        // When
        store.destroy(session_id);
        let looked_up_session_id = store.lookup(session_id);
        // Then
        assert_eq!(looked_up_session_id, None);
    }
}

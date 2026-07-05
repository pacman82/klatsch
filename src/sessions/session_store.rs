use std::collections::HashMap;

use tokio::time::Instant;

use crate::user::UserId;

use super::SessionId;

#[cfg_attr(test, double_trait::dummies)]
pub trait SessionStore {
    fn create(&mut self, user_id: UserId, now: Instant) -> SessionId;
    fn lookup(&mut self, session_id: SessionId) -> Option<UserId>;
    fn destroy(&mut self, session_id: SessionId);
    /// The earliest point in time at which a session will expire, or `None` if there are no
    /// active sessions.
    #[cfg(not(test))]
    fn next_expiry(&self) -> Option<Instant>;
    #[cfg(test)]
    fn next_expiry(&self) -> Option<Instant> {
        None
    }
    /// Remove all sessions whose lease has expired.
    fn remove_expired(&mut self, now: Instant);
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
    fn create(&mut self, user_id: UserId, now: Instant) -> SessionId {
        let session_id = SessionId::new();
        self.sessions.insert(session_id, user_id);
        session_id
    }

    fn lookup(&mut self, session_id: SessionId) -> Option<UserId> {
        self.sessions.get(&session_id).copied()
    }

    fn destroy(&mut self, session_id: SessionId) {
        self.sessions.remove(&session_id);
    }

    fn next_expiry(&self) -> Option<Instant> {
        None
    }

    fn remove_expired(&mut self, now: Instant) {}
}

#[cfg(test)]
mod tests {
    use crate::user::UserId;

    use tokio::time::Instant;

    use super::{InMemorySessionStore, SessionStore as _};

    #[test]
    fn lookup_returns_user_id_session_was_created_for() {
        // Given
        let mut store = InMemorySessionStore::new();
        // When
        let session_id = store.create(UserId::ALICE, Instant::now());
        let looked_up_session_id = store.lookup(session_id);
        // Then
        assert_eq!(looked_up_session_id, Some(UserId::ALICE));
    }

    #[test]
    fn destroyed_session_cannot_be_looked_up() {
        // Given
        let mut store = InMemorySessionStore::new();
        let session_id = store.create(UserId::ALICE, Instant::now());
        // When
        store.destroy(session_id);
        let looked_up_session_id = store.lookup(session_id);
        // Then
        assert_eq!(looked_up_session_id, None);
    }
}

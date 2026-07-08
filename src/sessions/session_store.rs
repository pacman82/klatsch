use std::{collections::HashMap, time::Duration};

/// Delay session expiration for this interval after each access.
const SLIDING_SESSION_TTL: Duration = Duration::from_hours(30 * 24);

use tokio::time::Instant;

use crate::user::UserId;

use super::SessionId;

#[cfg_attr(test, double_trait::dummies)]
pub trait SessionStore {
    fn create(&mut self, user_id: UserId, now: Instant) -> SessionId;
    fn lookup(&mut self, session_id: SessionId, now: Instant) -> Option<UserId>;
    fn destroy(&mut self, session_id: SessionId);
    /// The earliest point in time at which a session will expire, or `None` if there are no
    /// active sessions.
    fn next_expiry(&self) -> Option<Instant>;

    /// Remove all sessions whose lease has expired.
    fn remove_expired(&mut self, now: Instant);
}

pub struct InMemorySessionStore {
    sessions: HashMap<SessionId, (UserId, Instant)>,
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
        self.sessions
            .insert(session_id, (user_id, now + SLIDING_SESSION_TTL));
        session_id
    }

    fn lookup(&mut self, session_id: SessionId, now: Instant) -> Option<UserId> {
        self.sessions.get(&session_id).map(|(user_id, _)| *user_id)
    }

    fn destroy(&mut self, session_id: SessionId) {
        self.sessions.remove(&session_id);
    }

    fn next_expiry(&self) -> Option<Instant> {
        self.sessions.values().map(|(_, expiry)| *expiry).min()
    }

    fn remove_expired(&mut self, now: Instant) {}
}

#[cfg(test)]
mod tests {
    use crate::user::UserId;

    use std::time::Duration;

    use tokio::time::Instant;

    use super::{InMemorySessionStore, SLIDING_SESSION_TTL, SessionStore as _};

    #[test]
    fn session_expries_after_sliding_session_ttl() {
        // Given
        let now = Instant::now();
        let mut store = InMemorySessionStore::new();

        // When
        store.create(UserId::ALICE, now);
        let next_expiry = store.next_expiry();

        // Then
        assert_eq!(next_expiry, Some(now + SLIDING_SESSION_TTL));
    }

    #[test]
    fn next_expiry_reflects_earliest_session_only() {
        // Given
        let now = Instant::now();
        let mut store = InMemorySessionStore::new();
        store.create(UserId::ALICE, now);

        // When
        store.create(UserId::BOB, now + Duration::from_secs(1));

        // Then
        assert_eq!(store.next_expiry(), Some(now + SLIDING_SESSION_TTL));
    }

    #[test]
    fn lookup_returns_user_id_session_was_created_for() {
        // Given
        let mut store = InMemorySessionStore::new();
        // When
        let session_id = store.create(UserId::ALICE, Instant::now());
        let looked_up_session_id = store.lookup(session_id, Instant::now());
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
        let looked_up_session_id = store.lookup(session_id, Instant::now());
        // Then
        assert_eq!(looked_up_session_id, None);
    }
}

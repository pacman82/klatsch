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
    sessions: HashMap<SessionId, SessionInfo>,
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
        let session_info = SessionInfo::new(user_id, now);
        self.sessions.insert(session_id, session_info);
        session_id
    }

    fn lookup(&mut self, session_id: SessionId, now: Instant) -> Option<UserId> {
        let info = self.sessions.get_mut(&session_id)?;
        // Expired sessions linger until the next sweep; don't let them authenticate.
        if !info.is_valid(now) {
            self.sessions.remove(&session_id);
            return None;
        }
        info.last_activity = now;
        Some(info.user_id)
    }

    fn destroy(&mut self, session_id: SessionId) {
        self.sessions.remove(&session_id);
    }

    fn next_expiry(&self) -> Option<Instant> {
        self.sessions.values().map(|info| info.valid_until()).min()
    }

    fn remove_expired(&mut self, now: Instant) {
        self.sessions.retain(|_, info| info.is_valid(now));
    }
}

struct SessionInfo {
    user_id: UserId,
    last_activity: Instant,
}

impl SessionInfo {
    fn new(user_id: UserId, now: Instant) -> Self {
        Self {
            user_id,
            last_activity: now,
        }
    }

    fn valid_until(&self) -> Instant {
        self.last_activity + SLIDING_SESSION_TTL
    }

    fn is_valid(&self, now: Instant) -> bool {
        now <= self.valid_until()
    }
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
    fn lookup_extends_expiry() {
        // Given
        let now = Instant::now();
        let mut store = InMemorySessionStore::new();
        let session_id = store.create(UserId::ALICE, now);

        // When
        let one_day_later = now + Duration::from_hours(24);
        store.lookup(session_id, one_day_later);

        // Then
        assert_eq!(
            store.next_expiry(),
            Some(one_day_later + SLIDING_SESSION_TTL)
        );
    }

    #[test]
    fn remove_expired_removes_expired_sessions_leaving_active_ones() {
        // Given
        let now = Instant::now();
        let mut store = InMemorySessionStore::new();
        let expired = store.create(UserId::ALICE, now);
        let active = store.create(UserId::BOB, now + Duration::from_hours(24));

        // When — past Alice's expiry, but 24 hours before Bob's
        let sweep_time = now + SLIDING_SESSION_TTL + Duration::from_secs(1);
        store.remove_expired(sweep_time);

        // Then
        assert_eq!(store.lookup(expired, sweep_time), None);
        assert_eq!(store.lookup(active, sweep_time), Some(UserId::BOB));
    }

    #[test]
    fn lookup_rejects_expired_session_that_has_not_been_swept_yet() {
        // Given
        let now = Instant::now();
        let mut store = InMemorySessionStore::new();
        let session_id = store.create(UserId::ALICE, now);

        // When — past expiry, but no sweep has run
        let past_expiry = now + SLIDING_SESSION_TTL + Duration::from_secs(1);
        let looked_up = store.lookup(session_id, past_expiry);

        // Then
        assert_eq!(looked_up, None);
    }

    #[test]
    fn lookup_evicts_the_expired_session_it_rejects() {
        // Given
        let now = Instant::now();
        let mut store = InMemorySessionStore::new();
        let session_id = store.create(UserId::ALICE, now);

        // When
        let past_expiry = now + SLIDING_SESSION_TTL + Duration::from_secs(1);
        store.lookup(session_id, past_expiry);

        // Then — no session left to expire
        assert_eq!(store.next_expiry(), None);
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

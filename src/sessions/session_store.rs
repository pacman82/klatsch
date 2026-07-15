use std::{collections::HashMap, time::Duration};

use tokio::time::Instant;

use crate::user::UserId;

use super::SessionId;

/// When sessions expire. Static for the lifetime of the store.
#[derive(Clone, Copy)]
pub struct SessionExpiry {
    /// Delay session expiration for this interval after each access.
    pub idle_timeout: Duration,
    /// Hard cap on session lifetime; activity cannot extend a session beyond this.
    pub max_lifetime: Duration,
}

#[cfg_attr(test, double_trait::dummies)]
pub trait SessionStore {
    fn create(&mut self, user_id: UserId, now: Instant) -> SessionId;
    fn lookup(&mut self, session_id: SessionId, now: Instant) -> Option<UserId>;
    fn destroy(&mut self, session_id: SessionId);
    /// The earliest point in time at which any session may expire, or `None` if there are no
    /// active sessions. This is a conservative lower bound: no session expires before this
    /// instant, but the actual next expiry may be later.
    fn earliest_possible_expiry(&self) -> Option<Instant>;

    /// Remove all sessions whose lease has expired.
    fn remove_expired(&mut self, now: Instant);
}

pub struct InMemorySessionStore {
    expiry: SessionExpiry,
    sessions: HashMap<SessionId, SessionInfo>,
    /// Cached so answering it does not require a scan over all sessions. Lookups and removals
    /// only move true expiry later, so they leave the bound untouched and it goes stale early,
    /// never late. Only `create` lowers it; `remove_expired` restores it to the exact value.
    earliest_possible_expiry: Option<Instant>,
}

impl InMemorySessionStore {
    pub fn new(expiry: SessionExpiry) -> Self {
        Self {
            expiry,
            sessions: HashMap::new(),
            earliest_possible_expiry: None,
        }
    }
}

impl SessionStore for InMemorySessionStore {
    fn create(&mut self, user_id: UserId, now: Instant) -> SessionId {
        let session_id = SessionId::new();
        let session_info = SessionInfo::new(user_id, now);
        let valid_until = session_info.valid_until(&self.expiry);
        self.earliest_possible_expiry = Some(match self.earliest_possible_expiry {
            Some(bound) => bound.min(valid_until),
            None => valid_until,
        });
        self.sessions.insert(session_id, session_info);
        session_id
    }

    fn lookup(&mut self, session_id: SessionId, now: Instant) -> Option<UserId> {
        let info = self.sessions.get_mut(&session_id)?;
        // Expired sessions linger until the next sweep; don't let them authenticate.
        if !info.is_valid(now, &self.expiry) {
            self.sessions.remove(&session_id);
            return None;
        }
        info.last_activity = now;
        Some(info.user_id)
    }

    fn destroy(&mut self, session_id: SessionId) {
        self.sessions.remove(&session_id);
    }

    fn earliest_possible_expiry(&self) -> Option<Instant> {
        self.earliest_possible_expiry
    }

    fn remove_expired(&mut self, now: Instant) {
        let expiry = &self.expiry;
        self.sessions.retain(|_, info| info.is_valid(now, expiry));
        self.earliest_possible_expiry = self
            .sessions
            .values()
            .map(|info| info.valid_until(&self.expiry))
            .min();
    }
}

struct SessionInfo {
    user_id: UserId,
    created_at: Instant,
    last_activity: Instant,
}

impl SessionInfo {
    fn new(user_id: UserId, now: Instant) -> Self {
        Self {
            user_id,
            created_at: now,
            last_activity: now,
        }
    }

    fn valid_until(&self, expiry: &SessionExpiry) -> Instant {
        let idle = self.last_activity + expiry.idle_timeout;
        let absolute = self.created_at + expiry.max_lifetime;
        idle.min(absolute)
    }

    fn is_valid(&self, now: Instant, expiry: &SessionExpiry) -> bool {
        now <= self.valid_until(expiry)
    }
}

#[cfg(test)]
mod tests {
    use crate::user::UserId;

    use std::time::Duration;

    use tokio::time::Instant;

    use super::{InMemorySessionStore, SessionExpiry, SessionStore as _};

    /// For tests which are not concerned with expiry at all.
    const DEFAULT_SESSION_EXPIRY: SessionExpiry = SessionExpiry {
        idle_timeout: Duration::from_hours(30 * 24),
        max_lifetime: Duration::from_hours(90 * 24),
    };

    #[test]
    fn session_expires_after_idle_timeout() {
        // Given
        let now = Instant::now();
        let idle_timeout = Duration::from_hours(24);
        let mut store = InMemorySessionStore::new(SessionExpiry {
            idle_timeout,
            max_lifetime: Duration::from_hours(365 * 24),
        });

        // When
        store.create(UserId::ALICE, now);
        let earliest_possible_expiry = store.earliest_possible_expiry();

        // Then
        assert_eq!(earliest_possible_expiry, Some(now + idle_timeout));
    }

    #[test]
    fn next_expiry_reflects_earliest_session_only() {
        // Given
        let now = Instant::now();
        let idle_timeout = Duration::from_hours(24);
        let mut store = InMemorySessionStore::new(SessionExpiry {
            idle_timeout,
            max_lifetime: Duration::from_hours(365 * 24),
        });
        store.create(UserId::ALICE, now);

        // When
        store.create(UserId::BOB, now + Duration::from_secs(1));

        // Then
        assert_eq!(store.earliest_possible_expiry(), Some(now + idle_timeout));
    }

    #[test]
    fn activity_delays_expiry() {
        // Given
        let now = Instant::now();
        let idle_timeout = Duration::from_hours(48);
        let mut store = InMemorySessionStore::new(SessionExpiry {
            idle_timeout,
            max_lifetime: Duration::from_hours(365 * 24),
        });
        let session_id = store.create(UserId::ALICE, now);

        // When
        let one_day_later = now + Duration::from_hours(24);
        store.lookup(session_id, one_day_later);

        // Then — the session is still valid past its original deadline
        let past_original_deadline = now + Duration::from_hours(60);
        assert_eq!(
            store.lookup(session_id, past_original_deadline),
            Some(UserId::ALICE)
        );
    }

    #[test]
    fn sweep_at_the_expiry_bound_never_removes_live_sessions() {
        // Given — a session extended past the reported expiry bound
        let now = Instant::now();
        let idle_timeout = Duration::from_hours(48);
        let mut store = InMemorySessionStore::new(SessionExpiry {
            idle_timeout,
            max_lifetime: Duration::from_hours(365 * 24),
        });
        let session_id = store.create(UserId::ALICE, now);
        store.lookup(session_id, now + Duration::from_hours(24));

        // When
        let bound = store
            .earliest_possible_expiry()
            .expect("one session is active");
        store.remove_expired(bound);

        // Then
        assert_eq!(store.lookup(session_id, bound), Some(UserId::ALICE));
    }

    #[test]
    fn sweep_restores_exact_expiry_after_activity() {
        // Given — a session extended past the reported expiry bound
        let now = Instant::now();
        let idle_timeout = Duration::from_hours(48);
        let mut store = InMemorySessionStore::new(SessionExpiry {
            idle_timeout,
            max_lifetime: Duration::from_hours(365 * 24),
        });
        let session_id = store.create(UserId::ALICE, now);
        let one_day_later = now + Duration::from_hours(24);
        store.lookup(session_id, one_day_later);

        // When
        let bound = store
            .earliest_possible_expiry()
            .expect("one session is active");
        store.remove_expired(bound);

        // Then
        assert_eq!(
            store.earliest_possible_expiry(),
            Some(one_day_later + idle_timeout)
        );
    }

    #[test]
    fn remove_expired_removes_expired_sessions_leaving_active_ones() {
        // Given
        let now = Instant::now();
        let idle_timeout = Duration::from_hours(48);
        let mut store = InMemorySessionStore::new(SessionExpiry {
            idle_timeout,
            max_lifetime: Duration::from_hours(365 * 24),
        });
        let expired = store.create(UserId::ALICE, now);
        let active = store.create(UserId::BOB, now + Duration::from_hours(24));

        // When — past Alice's expiry, but 24 hours before Bob's
        let sweep_time = now + idle_timeout + Duration::from_secs(1);
        store.remove_expired(sweep_time);

        // Then
        assert_eq!(store.lookup(expired, sweep_time), None);
        assert_eq!(store.lookup(active, sweep_time), Some(UserId::BOB));
    }

    #[test]
    fn activity_cannot_extend_a_session_beyond_max_lifetime() {
        // Given
        let created_at = Instant::now();
        let max_lifetime = Duration::from_hours(7 * 24);
        let mut store = InMemorySessionStore::new(SessionExpiry {
            idle_timeout: Duration::from_hours(3 * 24),
            max_lifetime,
        });
        let session_id = store.create(UserId::ALICE, created_at);

        // When regular activity would keeps the sliding window open until past the absolute
        // deadline
        store.lookup(session_id, created_at + Duration::from_hours(2 * 24));
        store.lookup(session_id, created_at + Duration::from_hours(4 * 24));
        store.lookup(session_id, created_at + Duration::from_hours(6 * 24));

        // Then the session is unusable once the absolute deadline is hit.
        let past_deadline = created_at + max_lifetime + Duration::from_secs(1);
        assert_eq!(store.lookup(session_id, past_deadline), None);
    }

    #[test]
    fn lookup_rejects_expired_session_that_has_not_been_swept_yet() {
        // Given
        let now = Instant::now();
        let idle_timeout = Duration::from_hours(24);
        let mut store = InMemorySessionStore::new(SessionExpiry {
            idle_timeout,
            max_lifetime: Duration::from_hours(365 * 24),
        });
        let session_id = store.create(UserId::ALICE, now);

        // When — past expiry, but no sweep has run
        let past_expiry = now + idle_timeout + Duration::from_secs(1);
        let looked_up = store.lookup(session_id, past_expiry);

        // Then
        assert_eq!(looked_up, None);
    }

    #[test]
    fn lookup_returns_user_id_session_was_created_for() {
        // Given
        let mut store = InMemorySessionStore::new(DEFAULT_SESSION_EXPIRY);
        // When
        let session_id = store.create(UserId::ALICE, Instant::now());
        let looked_up_session_id = store.lookup(session_id, Instant::now());
        // Then
        assert_eq!(looked_up_session_id, Some(UserId::ALICE));
    }

    #[test]
    fn destroyed_session_cannot_be_looked_up() {
        // Given
        let mut store = InMemorySessionStore::new(DEFAULT_SESSION_EXPIRY);
        let session_id = store.create(UserId::ALICE, Instant::now());
        // When
        store.destroy(session_id);
        let looked_up_session_id = store.lookup(session_id, Instant::now());
        // Then
        assert_eq!(looked_up_session_id, None);
    }
}

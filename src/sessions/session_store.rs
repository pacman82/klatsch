use std::{
    collections::HashMap,
    time::{Duration, SystemTime},
};

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

/// A session as it crosses the store's boundary, e.g. when restored from persistence.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Session {
    /// Unique identifier for the session. Used for authentication and therfore security critical.
    pub id: SessionId,
    /// User associated with this session.
    pub user_id: UserId,
    /// The time at which this session was created. Used to track absolute expiry.
    pub created_at: SystemTime,
    /// The time at which this session last had activity. Used to track relative expiry using a
    /// sliding window.
    pub last_activity: SystemTime,
}

impl Session {
    fn to_info(&self) -> SessionInfo {
        SessionInfo {
            user_id: self.user_id,
            created_at: self.created_at,
            last_activity: self.last_activity,
        }
    }
}

#[cfg_attr(test, double_trait::dummies)]
pub trait SessionStore {
    /// Creates a new session associated with the given user. The timestamp is required to track
    /// expiry.
    fn create(&mut self, user_id: UserId, now: SystemTime) -> SessionId;
    /// Intended to restore previously persisted sessions.
    fn restore(&mut self, sessions: Vec<Session>, now: SystemTime);
    /// Returns the user ID if the session exists and is not expired, `None` otherwise.
    fn lookup(&mut self, session_id: SessionId, now: SystemTime) -> Option<UserId>;
    /// Revokes a session. This should happen if a user logs out of a client.
    fn destroy(&mut self, session_id: SessionId);
    /// The earliest point in time at which any session may expire, or `None` if there are no
    /// active sessions. This is a conservative lower bound: no session expires before this
    /// instant, but the actual next expiry may be later.
    fn earliest_possible_expiry(&self) -> Option<SystemTime>;
    /// Remove all expired sessions and report which ones were removed.
    ///
    /// Lookup already returns `None` for expired sessions which are still stored; calling this
    /// frees their resources and tells the caller which sessions ended, so revocation can be
    /// propagated (e.g. to persistence, or by closing streams the sessions kept open).
    fn remove_expired(&mut self, now: SystemTime) -> Vec<SessionId>;
}

pub struct ExpiringSessions {
    expiry: SessionExpiry,
    sessions: HashMap<SessionId, SessionInfo>,
    /// Cached so answering it does not require a scan over all sessions. Lookups and removals
    /// only move true expiry later, so they leave the bound untouched and it goes stale early,
    /// never late. Only `create` lowers it; `remove_expired` restores it to the exact value.
    earliest_possible_expiry: Option<SystemTime>,
}

impl ExpiringSessions {
    pub fn new(expiry: SessionExpiry) -> Self {
        Self {
            expiry,
            sessions: HashMap::new(),
            earliest_possible_expiry: None,
        }
    }

    fn update_earliest_possible_expiry(&mut self, valid_until: SystemTime) {
        let new_earliest = match self.earliest_possible_expiry {
            Some(bound) => bound.min(valid_until),
            None => valid_until,
        };
        self.earliest_possible_expiry.replace(new_earliest);
    }
}

impl SessionStore for ExpiringSessions {
    fn create(&mut self, user_id: UserId, now: SystemTime) -> SessionId {
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

    fn restore(&mut self, sessions: Vec<Session>, now: SystemTime) {
        for session in sessions {
            let info = session.to_info();
            let valid_until = info.valid_until(&self.expiry);
            if now < valid_until {
                self.update_earliest_possible_expiry(valid_until);
                self.sessions.insert(session.id, info);
            }
        }
    }

    fn lookup(&mut self, session_id: SessionId, now: SystemTime) -> Option<UserId> {
        let info = self.sessions.get_mut(&session_id)?;
        // Defensive, we runtime makes sure `remove_expired` is called on time. So outdated sessions
        // likely do not survive more than a few milliseconds (micro?). However, better safe than
        // sorry.
        if !info.is_valid(now, &self.expiry) {
            return None;
        }
        info.last_activity = now;
        Some(info.user_id)
    }

    fn destroy(&mut self, session_id: SessionId) {
        self.sessions.remove(&session_id);
    }

    fn earliest_possible_expiry(&self) -> Option<SystemTime> {
        self.earliest_possible_expiry
    }

    fn remove_expired(&mut self, now: SystemTime) -> Vec<SessionId> {
        let expiry = &self.expiry;
        // Earliest expiration of any remaining session already visited.
        let mut earliest_remaining: Option<SystemTime> = None;
        // Updates `earliest_remaining`. `true` for any expired session, false` for valid sessions.
        let is_expired = |_: &SessionId, info: &mut SessionInfo| {
            let valid_until = info.valid_until(expiry);
            if valid_until <= now {
                // Session is expired
                return true;
            }
            earliest_remaining = Some(match earliest_remaining {
                Some(bound) => bound.min(valid_until),
                None => valid_until,
            });
            false
        };
        // Remove all expired sesions.
        let expired = self
            .sessions
            .extract_if(is_expired)
            .map(|(session_id, _)| session_id)
            .collect();
        self.earliest_possible_expiry = earliest_remaining;
        // Return expired sessions, which have been removed from the store.
        expired
    }
}

struct SessionInfo {
    user_id: UserId,
    created_at: SystemTime,
    last_activity: SystemTime,
}

impl SessionInfo {
    fn new(user_id: UserId, now: SystemTime) -> Self {
        Self {
            user_id,
            created_at: now,
            last_activity: now,
        }
    }

    fn valid_until(&self, expiry: &SessionExpiry) -> SystemTime {
        let idle = self.last_activity + expiry.idle_timeout;
        let absolute = self.created_at + expiry.max_lifetime;
        idle.min(absolute)
    }

    fn is_valid(&self, now: SystemTime, expiry: &SessionExpiry) -> bool {
        now < self.valid_until(expiry)
    }
}

#[cfg(test)]
mod tests {
    use crate::user::UserId;

    use std::time::{Duration, SystemTime, UNIX_EPOCH};

    use super::{ExpiringSessions, Session, SessionExpiry, SessionId, SessionStore as _};

    /// For tests which are not concerned with expiry at all.
    const DEFAULT_SESSION_EXPIRY: SessionExpiry = SessionExpiry {
        idle_timeout: Duration::from_hours(30 * 24),
        max_lifetime: Duration::from_hours(90 * 24),
    };

    #[test]
    fn session_expires_after_idle_timeout() {
        // Given
        let now = SystemTime::now();
        let idle_timeout = Duration::from_hours(24);
        let mut store = ExpiringSessions::new(SessionExpiry {
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
        let now = SystemTime::now();
        let idle_timeout = Duration::from_hours(24);
        let mut store = ExpiringSessions::new(SessionExpiry {
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
        let now = SystemTime::now();
        let idle_timeout = Duration::from_hours(48);
        let mut store = ExpiringSessions::new(SessionExpiry {
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
        let now = SystemTime::now();
        let idle_timeout = Duration::from_hours(48);
        let mut store = ExpiringSessions::new(SessionExpiry {
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
        let now = SystemTime::now();
        let idle_timeout = Duration::from_hours(48);
        let mut store = ExpiringSessions::new(SessionExpiry {
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
        let now = SystemTime::now();
        let idle_timeout = Duration::from_hours(48);
        let mut store = ExpiringSessions::new(SessionExpiry {
            idle_timeout,
            max_lifetime: Duration::from_hours(365 * 24),
        });
        let one_day_later = now + Duration::from_hours(24);
        store.restore(
            vec![
                Session {
                    id: SessionId::ALICE,
                    user_id: UserId::ALICE,
                    created_at: now,
                    last_activity: now,
                },
                Session {
                    id: SessionId::BOB,
                    user_id: UserId::BOB,
                    created_at: one_day_later,
                    last_activity: one_day_later,
                },
            ],
            one_day_later,
        );

        // When — past Alice's expiry, but 24 hours before Bob's
        let sweep_time = now + idle_timeout + Duration::from_secs(1);
        store.remove_expired(sweep_time);

        // Then
        assert_eq!(store.lookup(SessionId::ALICE, sweep_time), None);
        assert_eq!(store.lookup(SessionId::BOB, sweep_time), Some(UserId::BOB));
    }

    #[test]
    fn remove_expired_reports_which_sessions_expired() {
        // Given
        let now = SystemTime::now();
        let two_day_idle_timeout = Duration::from_hours(48);
        let mut store = ExpiringSessions::new(SessionExpiry {
            idle_timeout: two_day_idle_timeout,
            max_lifetime: Duration::from_hours(365 * 24),
        });
        let one_day_later = now + Duration::from_hours(24);
        store.restore(
            vec![
                Session {
                    id: SessionId::ALICE,
                    user_id: UserId::ALICE,
                    created_at: now,
                    last_activity: now,
                },
                Session {
                    id: SessionId::BOB,
                    user_id: UserId::BOB,
                    created_at: one_day_later,
                    last_activity: one_day_later,
                },
            ],
            one_day_later,
        );

        // When — past Alice's expiry, but 24 hours before Bob's
        let sweep_time = now + two_day_idle_timeout + Duration::from_secs(1);
        let reported = store.remove_expired(sweep_time);

        // Then
        assert_eq!(reported, vec![SessionId::ALICE]);
    }

    #[test]
    fn activity_cannot_extend_a_session_beyond_max_lifetime() {
        // Given
        let created_at = SystemTime::now();
        let max_lifetime = Duration::from_hours(7 * 24);
        let mut store = ExpiringSessions::new(SessionExpiry {
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
        let now = SystemTime::now();
        let idle_timeout = Duration::from_hours(24);
        let mut store = ExpiringSessions::new(SessionExpiry {
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
    fn restore_sessions() {
        // Given
        let now = SystemTime::now();
        let mut store = ExpiringSessions::new(DEFAULT_SESSION_EXPIRY);
        let live = Session {
            id: SessionId::ALICE,
            user_id: UserId::ALICE,
            created_at: now,
            last_activity: now,
        };
        let long_gone = UNIX_EPOCH;
        let expired = Session {
            id: SessionId::BOB,
            user_id: UserId::BOB,
            created_at: long_gone,
            last_activity: long_gone,
        };

        // When
        store.restore(vec![live, expired], now);

        // Then
        assert_eq!(store.lookup(SessionId::BOB, now), None);
        assert_eq!(
            store.earliest_possible_expiry(),
            Some(now + DEFAULT_SESSION_EXPIRY.idle_timeout)
        );
        assert_eq!(store.lookup(SessionId::ALICE, now), Some(UserId::ALICE));
    }

    #[test]
    fn lookup_returns_user_id_session_was_created_for() {
        // Given
        let mut store = ExpiringSessions::new(DEFAULT_SESSION_EXPIRY);
        // When
        let session_id = store.create(UserId::ALICE, SystemTime::now());
        let looked_up_session_id = store.lookup(session_id, SystemTime::now());
        // Then
        assert_eq!(looked_up_session_id, Some(UserId::ALICE));
    }

    #[test]
    fn destroyed_session_cannot_be_looked_up() {
        // Given
        let mut store = ExpiringSessions::new(DEFAULT_SESSION_EXPIRY);
        let session_id = store.create(UserId::ALICE, SystemTime::now());
        // When
        store.destroy(session_id);
        let looked_up_session_id = store.lookup(session_id, SystemTime::now());
        // Then
        assert_eq!(looked_up_session_id, None);
    }
}

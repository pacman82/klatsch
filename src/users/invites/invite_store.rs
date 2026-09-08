use std::time::{Duration, SystemTime};

use super::{InvitePersistence, InviteToken};

/// Create, claim, and expire invites.
#[cfg_attr(test, double_trait::dummies)]
pub trait StoreInvites {
    /// Creates a new invite.
    fn new_invite(
        &mut self,
        now: SystemTime,
    ) -> impl Future<Output = anyhow::Result<InviteToken>> + Send;

    /// Attempts to claim the invite, succeeding only if it exists and is unexpired. A claimed
    /// invite is deleted — we do not remember used invites — so it can never be claimed a second
    /// time.
    fn claim(
        &mut self,
        token: InviteToken,
        now: SystemTime,
    ) -> impl Future<Output = anyhow::Result<bool>> + Send;

    /// Checks whether the invite exists and is unexpired, without claiming it.
    fn is_valid(
        &self,
        token: InviteToken,
        now: SystemTime,
    ) -> impl Future<Output = anyhow::Result<bool>> + Send;

    /// The instant the soonest-expiring outstanding invite becomes invalid, if any invites are
    /// outstanding. Used to schedule the next expiry sweep, e.g. after a restart.
    fn earliest_possible_expiry(&self) -> impl Future<Output = anyhow::Result<Option<SystemTime>>> + Send;

    /// Deletes all invites that have expired as of `now`, the same way an expired [`Self::claim`]
    /// would find them gone. Returns the instant the next remaining invite (if any) expires, so
    /// the sweep can be rescheduled.
    fn remove_expired(
        &self,
        now: SystemTime,
    ) -> impl Future<Output = anyhow::Result<Option<SystemTime>>> + Send;
}

/// Invite domain logic — expiry and single-use rules — backed by an [`InvitePersistence`].
#[derive(Clone)]
pub struct InviteStore<P> {
    persistence: P,
    /// How long an invite remains claimable after creation.
    expiry: Duration,
}

impl<P> InviteStore<P> {
    pub fn new(persistence: P, expiry: Duration) -> Self {
        InviteStore { persistence, expiry }
    }
}

impl<P> StoreInvites for InviteStore<P>
where
    P: InvitePersistence + Send + Sync,
{
    async fn new_invite(&mut self, now: SystemTime) -> anyhow::Result<InviteToken> {
        let token = InviteToken::new();
        self.persistence.insert(token, now).await?;
        Ok(token)
    }

    async fn claim(&mut self, token: InviteToken, now: SystemTime) -> anyhow::Result<bool> {
        if !self.is_valid(token, now).await? {
            return Ok(false);
        }
        self.persistence.delete(token).await?;
        Ok(true)
    }

    async fn is_valid(&self, token: InviteToken, now: SystemTime) -> anyhow::Result<bool> {
        let Some(created_at) = self.persistence.created_at(token).await? else {
            // Unknown token.
            return Ok(false);
        };
        let not_before = now.checked_sub(self.expiry).unwrap_or(SystemTime::UNIX_EPOCH);
        // An invite is valid strictly before `created_at + expiry`, i.e. invalid from that
        // instant on. Matches sessions' `valid_until <= now` convention.
        Ok(created_at > not_before)
    }

    async fn earliest_possible_expiry(&self) -> anyhow::Result<Option<SystemTime>> {
        let earliest_created_at = self.persistence.earliest_created_at().await?;
        Ok(earliest_created_at.map(|created_at| created_at + self.expiry))
    }

    async fn remove_expired(&self, now: SystemTime) -> anyhow::Result<Option<SystemTime>> {
        let not_before = now.checked_sub(self.expiry).unwrap_or(SystemTime::UNIX_EPOCH);
        self.persistence.delete_created_before(not_before).await?;
        let earliest_created_at = self.persistence.earliest_created_at().await?;
        Ok(earliest_created_at.map(|created_at| created_at + self.expiry))
    }
}

#[cfg(test)]
mod tests {
    use std::{
        collections::HashMap,
        sync::Mutex,
        time::{Duration, SystemTime},
    };

    use super::{InviteStore, InviteToken, StoreInvites};
    use crate::users::invites::InvitePersistence;

    const EXPIRY: Duration = Duration::from_hours(2 * 24);

    fn store() -> InviteStore<FakePersistence> {
        InviteStore::new(FakePersistence::default(), EXPIRY)
    }

    #[tokio::test]
    async fn new_invite_generates_distinct_tokens() {
        let mut store = store();

        let token_a = store.new_invite(SystemTime::now()).await.unwrap();
        let token_b = store.new_invite(SystemTime::now()).await.unwrap();

        assert_ne!(token_a, token_b);
    }

    #[tokio::test]
    async fn new_invite_can_be_claimed() {
        // Given
        let mut store = store();
        let token = store.new_invite(SystemTime::now()).await.unwrap();

        // When
        let claimed = store.claim(token, SystemTime::now()).await.unwrap();

        // Then
        assert!(claimed);
    }

    #[tokio::test]
    async fn unknown_invite_cannot_be_claimed() {
        let mut store = store();

        let claimed = store.claim(InviteToken::new(), SystemTime::now()).await.unwrap();

        assert!(!claimed);
    }

    #[tokio::test]
    async fn invite_can_only_be_claimed_once() {
        // Given a claimed invite
        let mut store = store();
        let token = store.new_invite(SystemTime::now()).await.unwrap();
        store.claim(token, SystemTime::now()).await.unwrap();

        // When claiming it again
        let claimed = store.claim(token, SystemTime::now()).await.unwrap();

        // Then
        assert!(!claimed);
    }

    #[tokio::test]
    async fn expired_invite_cannot_be_claimed() {
        // Given an invite older than the expiry
        let mut store = store();
        let created_at = SystemTime::now() - EXPIRY - Duration::from_secs(1);
        let token = store.new_invite(created_at).await.unwrap();

        // When
        let claimed = store.claim(token, SystemTime::now()).await.unwrap();

        // Then
        assert!(!claimed);
    }

    #[tokio::test]
    async fn invite_just_within_expiry_can_still_be_claimed() {
        // Given an invite created just under the expiry ago
        let mut store = store();
        let created_at = SystemTime::now() - EXPIRY + Duration::from_secs(60);
        let token = store.new_invite(created_at).await.unwrap();

        // When
        let claimed = store.claim(token, SystemTime::now()).await.unwrap();

        // Then
        assert!(claimed);
    }

    #[tokio::test]
    async fn new_invite_is_valid() {
        let mut store = store();
        let token = store.new_invite(SystemTime::now()).await.unwrap();

        let valid = store.is_valid(token, SystemTime::now()).await.unwrap();

        assert!(valid);
    }

    #[tokio::test]
    async fn unknown_invite_is_not_valid() {
        let store = store();

        let valid = store
            .is_valid(InviteToken::new(), SystemTime::now())
            .await
            .unwrap();

        assert!(!valid);
    }

    #[tokio::test]
    async fn expired_invite_is_not_valid() {
        // Given an invite older than the expiry
        let mut store = store();
        let created_at = SystemTime::now() - EXPIRY - Duration::from_secs(1);
        let token = store.new_invite(created_at).await.unwrap();

        // When
        let valid = store.is_valid(token, SystemTime::now()).await.unwrap();

        // Then
        assert!(!valid);
    }

    #[tokio::test]
    async fn checking_validity_does_not_consume_the_invite() {
        // Given
        let mut store = store();
        let token = store.new_invite(SystemTime::now()).await.unwrap();

        // When checking validity, possibly more than once
        store.is_valid(token, SystemTime::now()).await.unwrap();
        store.is_valid(token, SystemTime::now()).await.unwrap();

        // Then it can still be claimed afterwards
        let claimed = store.claim(token, SystemTime::now()).await.unwrap();
        assert!(claimed);
    }

    #[tokio::test]
    async fn earliest_possible_expiry_is_none_without_invites() {
        let store = store();

        let earliest = store.earliest_possible_expiry().await.unwrap();

        assert_eq!(earliest, None);
    }

    #[tokio::test]
    async fn earliest_possible_expiry_reflects_oldest_invite() {
        // Given two invites, created at different times
        let mut store = store();
        let older = SystemTime::now() - Duration::from_secs(60);
        let newer = SystemTime::now();
        store.new_invite(newer).await.unwrap();
        store.new_invite(older).await.unwrap();

        // When
        let earliest = store.earliest_possible_expiry().await.unwrap();

        // Then it is derived from the older invite
        assert_eq!(earliest, Some(older + EXPIRY));
    }

    #[tokio::test]
    async fn remove_expired_deletes_only_expired_invites() {
        // Given one expired and one still-valid invite
        let mut store = store();
        let expired = store
            .new_invite(SystemTime::now() - EXPIRY - Duration::from_secs(1))
            .await
            .unwrap();
        let valid = store.new_invite(SystemTime::now()).await.unwrap();

        // When
        store.remove_expired(SystemTime::now()).await.unwrap();

        // Then the expired invite is gone, but the valid one can still be claimed
        assert!(!store.claim(expired, SystemTime::now()).await.unwrap());
        assert!(store.claim(valid, SystemTime::now()).await.unwrap());
    }

    #[tokio::test]
    async fn remove_expired_reports_next_remaining_expiry() {
        // Given one expired and one still-valid invite
        let mut store = store();
        let still_valid_created_at = SystemTime::now();
        store
            .new_invite(SystemTime::now() - EXPIRY - Duration::from_secs(1))
            .await
            .unwrap();
        store.new_invite(still_valid_created_at).await.unwrap();

        // When
        let next_expiry = store.remove_expired(SystemTime::now()).await.unwrap();

        // Then
        assert_eq!(next_expiry, Some(still_valid_created_at + EXPIRY));
    }

    /// An in-memory `InvitePersistence`, so `InviteStore`'s expiry/single-use logic can be tested
    /// without touching a real database.
    #[derive(Default)]
    struct FakePersistence {
        invites: Mutex<HashMap<InviteToken, SystemTime>>,
    }

    impl InvitePersistence for FakePersistence {
        async fn insert(&self, token: InviteToken, created_at: SystemTime) -> anyhow::Result<()> {
            self.invites.lock().unwrap().insert(token, created_at);
            Ok(())
        }

        async fn created_at(&self, token: InviteToken) -> anyhow::Result<Option<SystemTime>> {
            Ok(self.invites.lock().unwrap().get(&token).copied())
        }

        async fn delete(&self, token: InviteToken) -> anyhow::Result<()> {
            self.invites.lock().unwrap().remove(&token);
            Ok(())
        }

        async fn earliest_created_at(&self) -> anyhow::Result<Option<SystemTime>> {
            Ok(self.invites.lock().unwrap().values().min().copied())
        }

        async fn delete_created_before(&self, cutoff: SystemTime) -> anyhow::Result<()> {
            self.invites
                .lock()
                .unwrap()
                .retain(|_, created_at| *created_at > cutoff);
            Ok(())
        }
    }
}

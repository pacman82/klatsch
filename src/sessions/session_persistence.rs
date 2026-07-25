use std::time::SystemTime;

use super::{Session, SessionId};

#[cfg_attr(test, double_trait::dummies)]
pub trait SessionPersistence {
    /// All persisted sessions. Used after a restart to restore the state of [`super::SessionStore`].
    fn all_sessions(&self) -> impl Future<Output = anyhow::Result<Vec<Session>>> + Send;

    /// To insert a session after creation, so it is remembered after a reboot.
    fn insert(&mut self, session: Session) -> impl Future<Output = anyhow::Result<()>> + Send;

    /// Free memory used to remember the session after it has been revoked.
    fn remove(&mut self, session: SessionId) -> impl Future<Output = anyhow::Result<()>> + Send;

    /// Update activity timestamp in persistence to prolong timeout window
    fn update_activity(
        &mut self,
        session: SessionId,
        last_activity: SystemTime,
    ) -> impl Future<Output = anyhow::Result<()>> + Send;
}

/// A [`SessionPersistence`] which does not persist anything. Sessions do not survive a restart.
pub struct NoPersistence;

impl SessionPersistence for NoPersistence {
    async fn all_sessions(&self) -> anyhow::Result<Vec<Session>> {
        Ok(Vec::new())
    }

    async fn insert(&mut self, _: Session) -> anyhow::Result<()> {
        Ok(())
    }

    async fn remove(&mut self, _: SessionId) -> anyhow::Result<()> {
        Ok(())
    }

    async fn update_activity(&mut self, _: SessionId, _: SystemTime) -> anyhow::Result<()> {
        Ok(())
    }
}

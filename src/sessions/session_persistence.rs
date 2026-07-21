use super::Session;

#[cfg_attr(test, double_trait::dummies)]
pub trait SessionPersistence {
    /// All persisted sessions. Used after a restart to restore the state of [`super::SessionStore`].
    fn all_sessions(&self) -> impl Future<Output = Vec<Session>> + Send;
}

/// A [`SessionPersistence`] which does not persist anything. Sessions do not survive a restart.
pub struct NoPersistence;

impl SessionPersistence for NoPersistence {
    async fn all_sessions(&self) -> Vec<Session> {
        Vec::new()
    }
}

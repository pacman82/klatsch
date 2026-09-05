mod session_id;
mod session_persistence;
mod session_store;
mod sessions_runtime;

use crate::persistence::ExecuteSqlAsync;

pub use self::{
    session_id::SessionId,
    session_persistence::migrate_session_persistence,
    session_store::SessionExpiry,
    sessions_runtime::{AuthenticateSession, SessionLifecycle, SessionsClient, SessionsRuntime},
};

use self::{
    session_id::SessionHash,
    session_persistence::SessionPersistence,
    session_store::{ExpiringSessions, Session, SessionStore},
};

impl SessionsRuntime {
    pub async fn new(
        expiry: SessionExpiry,
        persistence: impl ExecuteSqlAsync + Send + Sync + 'static,
    ) -> anyhow::Result<Self> {
        Self::start(ExpiringSessions::new(expiry), persistence).await
    }
}

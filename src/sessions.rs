mod session_id;
mod session_store;
mod sessions_runtime;

pub use self::{
    session_id::SessionId,
    session_store::SessionExpiry,
    sessions_runtime::{SessionLifecycle, SessionLookup, SessionsRuntime},
};

use self::session_store::{ExpiringSessions, SessionStore};

impl SessionsRuntime {
    pub fn new(expiry: SessionExpiry) -> Self {
        Self::with_session_store(ExpiringSessions::new(expiry))
    }
}

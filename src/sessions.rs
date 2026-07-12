mod session_id;
mod session_store;
mod sessions_runtime;

pub use self::{
    session_id::SessionId,
    sessions_runtime::{SessionLookup, SessionLifecycle, SessionsRuntime},
};

use self::session_store::{InMemorySessionStore, SessionStore};

impl SessionsRuntime {
    pub fn new() -> Self {
        Self::with_session_store(InMemorySessionStore::new())
    }
}

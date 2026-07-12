mod session_http;
mod session_id;
mod session_store;
mod sessions_runtime;

pub use self::{
    session_http::AuthenticatedUser,
    session_id::SessionId,
    sessions_runtime::{SessionLifecycle, SessionLookup, SessionsRuntime},
};

use self::session_store::{InMemorySessionStore, SessionStore};

impl SessionsRuntime {
    pub fn new() -> Self {
        Self::with_session_store(InMemorySessionStore::new())
    }
}

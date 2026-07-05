mod session_id;
mod session_store;
mod sessions_runtime;

pub use self::{
    session_id::SessionId,
    sessions_runtime::{Sessions, SessionsRuntime},
};

use self::session_store::InMemorySessionStore;

impl SessionsRuntime {
    pub fn new() -> Self {
        Self::with_session_store(InMemorySessionStore::new())
    }
}

mod sessions_id;

use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
};

use tokio::task::JoinHandle;

use crate::user::UserId;

pub use self::sessions_id::SessionId;

#[cfg_attr(test, double_trait::dummies)]
pub trait Sessions {
    fn create(&mut self, user_id: UserId) -> SessionId;
    fn lookup(&mut self, session_id: SessionId) -> Option<UserId>;
    fn destroy(&mut self, session_id: SessionId);
}

pub struct SessionsRuntime {
    sessions: Arc<Mutex<HashMap<SessionId, UserId>>>,
    handle: JoinHandle<()>,
}

impl SessionsRuntime {
    pub fn new() -> Self {
        let sessions = Arc::new(Mutex::new(HashMap::new()));
        let handle = tokio::spawn(async {});
        Self { sessions, handle }
    }
}

impl SessionsRuntime {
    pub async fn shutdown(self) {
        self.handle.await.unwrap();
    }

    pub fn client(&self) -> SessionsClient {
        SessionsClient {
            sessions: Arc::clone(&self.sessions),
        }
    }
}

#[derive(Clone)]
pub struct SessionsClient {
    sessions: Arc<Mutex<HashMap<SessionId, UserId>>>,
}

impl Sessions for SessionsClient {
    fn create(&mut self, user_id: UserId) -> SessionId {
        let session_id = SessionId::new();
        self.sessions
            .lock()
            .expect("sessions lock must not be poisoned")
            .insert(session_id, user_id);
        session_id
    }

    fn lookup(&mut self, session_id: SessionId) -> Option<UserId> {
        self.sessions
            .lock()
            .expect("sessions lock must not be poisoned")
            .get(&session_id)
            .copied()
    }

    fn destroy(&mut self, session_id: SessionId) {
        self.sessions
            .lock()
            .expect("sessions lock must not be poisoned")
            .remove(&session_id);
    }
}

#[cfg(test)]
mod tests {
    use crate::user::UserId;

    use super::{Sessions as _, SessionsRuntime};

    #[tokio::test]
    async fn lookup_returns_user_id_session_was_created_for() {
        // Given
        let runtime = SessionsRuntime::new();
        let mut sessions = runtime.client();
        // When
        let session_id = sessions.create(UserId::ALICE);
        let user_id_after_lookup = sessions.lookup(session_id);
        // Then
        assert_eq!(user_id_after_lookup, Some(UserId::ALICE));
        // Cleanup
        drop(sessions);
        runtime.shutdown().await
    }

    #[tokio::test]
    async fn destroyed_session_cannot_be_looked_up() {
        // Given
        let runtime = SessionsRuntime::new();
        let mut sessions = runtime.client();
        let session_id = sessions.create(UserId::ALICE);
        // When
        sessions.destroy(session_id);
        let user_id_after_lookup = sessions.lookup(session_id);
        // Then
        assert_eq!(user_id_after_lookup, None);
        // Cleanup
        drop(sessions);
        runtime.shutdown().await
    }
}

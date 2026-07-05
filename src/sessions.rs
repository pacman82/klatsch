mod sessions_id;
mod sessions_store;

use std::sync::{Arc, Mutex};

use tokio::task::JoinHandle;

use crate::user::UserId;

use self::sessions_store::SessionStore;

pub use self::sessions_id::SessionId;

#[cfg_attr(test, double_trait::dummies)]
pub trait Sessions {
    fn create(&mut self, user_id: UserId) -> impl Future<Output = SessionId> + Send;
    fn lookup(&mut self, session_id: SessionId) -> impl Future<Output = Option<UserId>> + Send;
    fn destroy(&mut self, session_id: SessionId) -> impl Future<Output = ()> + Send;
}

pub struct SessionsRuntime {
    sessions: Arc<Mutex<SessionStore>>,
    handle: JoinHandle<()>,
}

impl SessionsRuntime {
    pub fn new() -> Self {
        let sessions = Arc::new(Mutex::new(SessionStore::new()));
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
    sessions: Arc<Mutex<SessionStore>>,
}

impl Sessions for SessionsClient {
    async fn create(&mut self, user_id: UserId) -> SessionId {
        self.sessions
            .lock()
            .expect("sessions lock must not be poisoned")
            .create(user_id)
    }

    async fn lookup(&mut self, session_id: SessionId) -> Option<UserId> {
        self.sessions
            .lock()
            .expect("sessions lock must not be poisoned")
            .lookup(session_id)
    }

    async fn destroy(&mut self, session_id: SessionId) {
        self.sessions
            .lock()
            .expect("sessions lock must not be poisoned")
            .destroy(session_id);
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
        let session_id = sessions.create(UserId::ALICE).await;
        let user_id_after_lookup = sessions.lookup(session_id).await;
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
        let session_id = sessions.create(UserId::ALICE).await;
        // When
        sessions.destroy(session_id).await;
        let user_id_after_lookup = sessions.lookup(session_id).await;
        // Then
        assert_eq!(user_id_after_lookup, None);
        // Cleanup
        drop(sessions);
        runtime.shutdown().await
    }
}

mod sessions_id;

use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
};

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
}

impl SessionsRuntime {
    pub fn new() -> Self {
        Self {
            sessions: Arc::new(Mutex::new(HashMap::new())),
        }
    }
}

impl SessionsRuntime {
    pub async fn shutdown(self) {}

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

    #[test]
    fn lookup_returns_user_id_session_was_created_for() {
        // Given
        let mut sessions = SessionsRuntime::new().client();
        // When
        let session_id = sessions.create(UserId::ALICE);
        // Then
        assert_eq!(sessions.lookup(session_id), Some(UserId::ALICE));
    }

    #[test]
    fn destroyed_session_cannot_be_looked_up() {
        // Given
        let mut sessions = SessionsRuntime::new().client();
        let session_id = sessions.create(UserId::ALICE);
        // When
        sessions.destroy(session_id);
        // Then
        assert_eq!(sessions.lookup(session_id), None);
    }
}

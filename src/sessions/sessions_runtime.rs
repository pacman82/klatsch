use tokio::{
    sync::{mpsc, oneshot},
    task::JoinHandle,
};

use crate::user::UserId;

use super::{
    SessionId,
    session_store::{InMemorySessionStore, SessionStore},
};

#[cfg_attr(test, double_trait::dummies)]
pub trait Sessions {
    fn create(&mut self, user_id: UserId) -> impl Future<Output = SessionId> + Send;
    fn lookup(&mut self, session_id: SessionId) -> impl Future<Output = Option<UserId>> + Send;
    fn destroy(&mut self, session_id: SessionId) -> impl Future<Output = ()> + Send;
}

pub struct SessionsRuntime {
    sender: mpsc::Sender<SessionMsg>,
    handle: JoinHandle<()>,
}

impl SessionsRuntime {
    pub(super) fn with_session_store(store: impl SessionStore + Send + 'static) -> Self {
        let (sender, receiver) = mpsc::channel(16);
        let actor = SessionActor::new(store, receiver);
        let handle = tokio::spawn(async move { actor.run().await });
        Self { sender, handle }
    }

    pub fn new() -> Self {
        Self::with_session_store(InMemorySessionStore::new())
    }

    pub async fn shutdown(self) {
        drop(self.sender);
        self.handle.await.unwrap();
    }

    pub fn client(&self) -> SessionsClient {
        SessionsClient {
            sender: self.sender.clone(),
        }
    }
}

#[derive(Clone)]
pub struct SessionsClient {
    sender: mpsc::Sender<SessionMsg>,
}

impl Sessions for SessionsClient {
    async fn create(&mut self, user_id: UserId) -> SessionId {
        let (reply, response) = oneshot::channel();
        self.sender
            .send(SessionMsg::Create { user_id, reply })
            .await
            .expect("SessionsRuntime must outlive its clients.");
        response
            .await
            .expect("SessionsRuntime must outlive its clients.")
    }

    async fn lookup(&mut self, session_id: SessionId) -> Option<UserId> {
        let (reply, response) = oneshot::channel();
        self.sender
            .send(SessionMsg::Lookup { session_id, reply })
            .await
            .expect("SessionsRuntime must outlive its clients.");
        response
            .await
            .expect("SessionsRuntime must outlive its clients.")
    }

    async fn destroy(&mut self, session_id: SessionId) {
        self.sender
            .send(SessionMsg::Destroy { session_id })
            .await
            .expect("SessionsRuntime must outlive its clients.");
    }
}

struct SessionActor<S> {
    store: S,
    receiver: mpsc::Receiver<SessionMsg>,
}

impl<S: SessionStore> SessionActor<S> {
    fn new(store: S, receiver: mpsc::Receiver<SessionMsg>) -> Self {
        SessionActor { store, receiver }
    }

    async fn run(mut self) {
        while let Some(msg) = self.receiver.recv().await {
            self.handle(msg);
        }
    }

    fn handle(&mut self, msg: SessionMsg) {
        match msg {
            SessionMsg::Create { user_id, reply } => {
                let _ = reply.send(self.store.create(user_id));
            }
            SessionMsg::Lookup { session_id, reply } => {
                let _ = reply.send(self.store.lookup(session_id));
            }
            SessionMsg::Destroy { session_id } => {
                self.store.destroy(session_id);
            }
        }
    }
}

enum SessionMsg {
    Create {
        user_id: UserId,
        reply: oneshot::Sender<SessionId>,
    },
    Lookup {
        session_id: SessionId,
        reply: oneshot::Sender<Option<UserId>>,
    },
    Destroy {
        session_id: SessionId,
    },
}

#[cfg(test)]
mod tests {
    use tokio::time::timeout;

    use crate::user::UserId;

    use std::time::Duration;

    use super::{Sessions as _, SessionsRuntime};

    #[tokio::test]
    async fn shutdown_completes_within_one_second() {
        let runtime = SessionsRuntime::new();
        let result = timeout(Duration::from_secs(1), runtime.shutdown()).await;
        assert!(result.is_ok(), "Shutdown did not complete within 1 second");
    }

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

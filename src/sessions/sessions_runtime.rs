use tokio::{
    sync::{mpsc, oneshot},
    task::JoinHandle,
};

use crate::user::UserId;

use std::time::Instant;

use super::{SessionId, SessionStore};

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
                let _ = reply.send(self.store.create(user_id, Instant::now()));
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
    use std::{
        sync::{Arc, Mutex},
        time::{Duration, Instant},
    };

    use double_trait::Dummy;
    use tokio::time::timeout;

    use crate::user::UserId;

    use super::{SessionId, SessionStore, Sessions as _, SessionsRuntime};

    #[tokio::test]
    async fn shutdown_completes_within_one_second() {
        let runtime = SessionsRuntime::with_session_store(Dummy);
        let result = timeout(Duration::from_secs(1), runtime.shutdown()).await;
        assert!(result.is_ok(), "Shutdown did not complete within 1 second");
    }

    #[tokio::test]
    async fn forward_create_to_session_store() {
        // Given
        #[derive(Clone, Default)]
        struct Spy {
            created_with: Arc<Mutex<Option<UserId>>>,
        }
        impl SessionStore for Spy {
            fn create(&mut self, user_id: UserId, _now: Instant) -> SessionId {
                *self.created_with.lock().unwrap() = Some(user_id);
                SessionId::ALPHA
            }
        }
        let store = Spy::default();
        let runtime = SessionsRuntime::with_session_store(store.clone());
        let mut client = runtime.client();

        // When
        let session_id = client.create(UserId::ALICE).await;

        // Then
        assert_eq!(session_id, SessionId::ALPHA);
        assert_eq!(*store.created_with.lock().unwrap(), Some(UserId::ALICE));
        // Cleanup
        drop(client);
        runtime.shutdown().await;
    }

    #[tokio::test]
    async fn lookup_forwards_session_id_to_store_and_returns_user_id() {
        #[derive(Clone, Default)]
        struct Spy {
            looked_up: Arc<Mutex<Option<SessionId>>>,
        }
        impl SessionStore for Spy {
            fn lookup(&mut self, session_id: SessionId) -> Option<UserId> {
                *self.looked_up.lock().unwrap() = Some(session_id);
                Some(UserId::ALICE)
            }
        }
        let store = Spy::default();
        let runtime = SessionsRuntime::with_session_store(store.clone());
        let mut client = runtime.client();

        let returned = client.lookup(SessionId::ALPHA).await;

        assert_eq!(returned, Some(UserId::ALICE));
        assert_eq!(*store.looked_up.lock().unwrap(), Some(SessionId::ALPHA));
        drop(client);
        runtime.shutdown().await;
    }

    #[tokio::test]
    async fn destroy_forwards_session_id_to_store() {
        #[derive(Clone, Default)]
        struct Spy {
            destroyed: Arc<Mutex<Option<SessionId>>>,
        }
        impl SessionStore for Spy {
            fn destroy(&mut self, session_id: SessionId) {
                *self.destroyed.lock().unwrap() = Some(session_id);
            }
        }
        let store = Spy::default();
        let runtime = SessionsRuntime::with_session_store(store.clone());
        let mut client = runtime.client();

        client.destroy(SessionId::ALPHA).await;
        // Destroy has no reply channel; shutdown drains the actor's queue before returning.
        drop(client);
        runtime.shutdown().await;

        assert_eq!(*store.destroyed.lock().unwrap(), Some(SessionId::ALPHA));
    }
}

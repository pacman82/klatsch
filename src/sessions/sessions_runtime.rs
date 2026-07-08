use std::future::pending;

use tokio::{
    select,
    sync::{mpsc, oneshot},
    task::JoinHandle,
    time::sleep_until,
};

use crate::user::UserId;

use tokio::time::Instant;

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
        loop {
            let next_expiry = self.store.next_expiry();
            let sleep_until_sessions_expire = async {
                if let Some(next_expiry) = next_expiry {
                    sleep_until(next_expiry).await;
                } else {
                    pending().await
                }
            };
            select! {
                msg = self.receiver.recv() => match msg {
                    Some(msg) => self.handle(msg),
                    None => return,
                },
                () = sleep_until_sessions_expire => {
                    self.store.remove_expired(Instant::now());
                }
            }
        }
    }

    fn handle(&mut self, msg: SessionMsg) {
        match msg {
            SessionMsg::Create { user_id, reply } => {
                let _ = reply.send(self.store.create(user_id, Instant::now()));
            }
            SessionMsg::Lookup { session_id, reply } => {
                let _ = reply.send(self.store.lookup(session_id, Instant::now()));
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
        time::Duration,
    };

    use tokio::{
        sync::mpsc,
        time::{self, Instant},
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

    #[tokio::test(start_paused = true)]
    async fn forward_create_to_session_store() {
        // Given
        let now = Instant::now();
        #[derive(Clone, Default)]
        struct Spy {
            record: Arc<Mutex<Option<(UserId, Instant)>>>,
        }
        impl SessionStore for Spy {
            fn create(&mut self, user_id: UserId, now: Instant) -> SessionId {
                *self.record.lock().unwrap() = Some((user_id, now));
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
        assert_eq!(*store.record.lock().unwrap(), Some((UserId::ALICE, now)));
        // Cleanup
        drop(client);
        runtime.shutdown().await;
    }

    #[tokio::test(start_paused = true)]
    async fn lookup_forwards_session_id_to_store_and_returns_user_id() {
        // Given
        let now = Instant::now();
        #[derive(Clone, Default)]
        struct Spy {
            record: Arc<Mutex<Option<(SessionId, Instant)>>>,
        }
        impl SessionStore for Spy {
            fn lookup(&mut self, session_id: SessionId, now: Instant) -> Option<UserId> {
                *self.record.lock().unwrap() = Some((session_id, now));
                Some(UserId::ALICE)
            }
        }
        let store = Spy::default();
        let runtime = SessionsRuntime::with_session_store(store.clone());
        let mut client = runtime.client();

        // When
        let returned = client.lookup(SessionId::ALPHA).await;

        // Then
        assert_eq!(returned, Some(UserId::ALICE));
        assert_eq!(*store.record.lock().unwrap(), Some((SessionId::ALPHA, now)));
        // Cleanup
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

    #[tokio::test(start_paused = true)]
    async fn remove_expired_when_next_expiry_is_reached() {
        // Given
        const TTL: Duration = Duration::from_secs(10);
        let start = Instant::now();
        let (tx, mut rx) = mpsc::channel(1);
        #[derive(Clone)]
        struct SessionStoreDouble {
            start: Instant,
            tx: mpsc::Sender<Instant>,
        }
        impl SessionStore for SessionStoreDouble {
            fn next_expiry(&self) -> Option<Instant> {
                Some(self.start + TTL)
            }
            fn remove_expired(&mut self, now: Instant) {
                let _ = self.tx.try_send(now);
            }
        }
        let runtime = SessionsRuntime::with_session_store(SessionStoreDouble { start, tx });
        let client = runtime.client();

        // When
        time::advance(TTL).await;

        // Then
        let removed_at = timeout(Duration::from_secs(1), rx.recv())
            .await
            .expect("remove_expired was not called within one second")
            .unwrap();
        assert_eq!(removed_at, start + TTL);

        // Cleanup
        drop(client);
        runtime.shutdown().await;
    }
}

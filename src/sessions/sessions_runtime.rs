use std::{
    future::pending,
    time::{Duration, SystemTime},
};

use tokio::{
    select,
    sync::{mpsc, oneshot},
    task::JoinHandle,
    time::{Instant, Sleep, sleep_until},
};

use crate::user::UserId;

use super::{SessionId, SessionStore};

#[cfg_attr(test, double_trait::dummies)]
pub trait SessionLookup {
    fn lookup(&self, session_id: SessionId) -> impl Future<Output = Option<UserId>> + Send;
}

#[cfg_attr(test, double_trait::dummies)]
pub trait SessionLifecycle {
    fn create(&mut self, user_id: UserId) -> impl Future<Output = SessionId> + Send;
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

impl SessionLookup for SessionsClient {
    async fn lookup(&self, session_id: SessionId) -> Option<UserId> {
        let (reply, response) = oneshot::channel();
        self.sender
            .send(SessionMsg::Lookup { session_id, reply })
            .await
            .expect("SessionsRuntime must outlive its clients.");
        response
            .await
            .expect("SessionsRuntime must outlive its clients.")
    }
}

impl SessionLifecycle for SessionsClient {
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
    clock_anchor: ClockAnchor,
}

impl<S: SessionStore> SessionActor<S> {
    fn new(store: S, receiver: mpsc::Receiver<SessionMsg>) -> Self {
        SessionActor {
            store,
            receiver,
            clock_anchor: ClockAnchor::new(),
        }
    }

    async fn run(mut self) {
        loop {
            let earliest_possible_expiry = self.store.earliest_possible_expiry();
            let sleep_until_earliest_possible_expiry = async {
                if let Some(earliest_possible_expiry) = earliest_possible_expiry {
                    self.clock_anchor
                        .sleep_until(earliest_possible_expiry)
                        .await;
                } else {
                    pending().await
                }
            };
            select! {
                msg = self.receiver.recv() => match msg {
                    Some(msg) => self.handle(msg).await,
                    None => return,
                },
                () = sleep_until_earliest_possible_expiry => {
                    self.store.remove_expired(
                        earliest_possible_expiry
                            .expect("the timer only completes when a bound was armed"),
                    );
                }
            }
        }
    }

    async fn handle(&mut self, msg: SessionMsg) {
        match msg {
            SessionMsg::Create { user_id, reply } => {
                let _ = reply.send(self.store.create(user_id, SystemTime::now()));
            }
            SessionMsg::Lookup { session_id, reply } => {
                let _ = reply.send(self.store.lookup(session_id, SystemTime::now()));
            }
            SessionMsg::Destroy { session_id } => {
                self.store.destroy(session_id);
            }
        }
    }
}

/// Relates tokio's monotonic clock to the wall clock, so wall clock deadlines can drive tokio
/// timers. The mapping between the two clocks is fixed at construction.
struct ClockAnchor {
    tokio_origin: Instant,
    wall_origin: SystemTime,
}

impl ClockAnchor {
    fn new() -> Self {
        Self {
            tokio_origin: Instant::now(),
            wall_origin: SystemTime::now(),
        }
    }

    /// Completes once the wall clock reaches the deadline. Deadlines before the anchor complete
    /// immediately.
    fn sleep_until(&self, deadline: SystemTime) -> Sleep {
        let after_origin = deadline
            .duration_since(self.wall_origin)
            .unwrap_or(Duration::ZERO);
        sleep_until(self.tokio_origin + after_origin)
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
        time::{Duration, SystemTime},
    };

    use tokio::{sync::mpsc, time};

    use double_trait::Dummy;
    use tokio::time::timeout;

    use crate::user::UserId;

    use super::{
        SessionId, SessionLifecycle as _, SessionLookup as _, SessionStore, SessionsRuntime,
    };

    #[tokio::test]
    async fn shutdown_completes_within_one_second() {
        let runtime = SessionsRuntime::with_session_store(Dummy);
        let result = timeout(Duration::from_secs(1), runtime.shutdown()).await;
        assert!(result.is_ok(), "Shutdown did not complete within 1 second");
    }

    #[tokio::test(start_paused = true)]
    async fn forward_create_to_session_store() {
        // Given
        #[derive(Clone, Default)]
        struct Spy {
            record: Arc<Mutex<Option<(UserId, SystemTime)>>>,
        }
        impl SessionStore for Spy {
            fn create(&mut self, user_id: UserId, now: SystemTime) -> SessionId {
                *self.record.lock().unwrap() = Some((user_id, now));
                SessionId::ALICE
            }
        }
        let store = Spy::default();
        let runtime = SessionsRuntime::with_session_store(store.clone());
        let mut client = runtime.client();

        // When
        let before = SystemTime::now();
        let session_id = client.create(UserId::ALICE).await;
        let after = SystemTime::now();

        // Then
        assert_eq!(session_id, SessionId::ALICE);
        let (user_id, at) = (*store.record.lock().unwrap()).expect("create must reach the store");
        assert_eq!(user_id, UserId::ALICE);
        assert!(
            before <= at && at <= after,
            "store must see the current time"
        );
        // Cleanup
        drop(client);
        runtime.shutdown().await;
    }

    #[tokio::test(start_paused = true)]
    async fn lookup_forwards_session_id_to_store_and_returns_user_id() {
        // Given
        #[derive(Clone, Default)]
        struct Spy {
            record: Arc<Mutex<Option<(SessionId, SystemTime)>>>,
        }
        impl SessionStore for Spy {
            fn lookup(&mut self, session_id: SessionId, now: SystemTime) -> Option<UserId> {
                *self.record.lock().unwrap() = Some((session_id, now));
                Some(UserId::ALICE)
            }
        }
        let store = Spy::default();
        let runtime = SessionsRuntime::with_session_store(store.clone());
        let client = runtime.client();

        // When
        let before = SystemTime::now();
        let returned = client.lookup(SessionId::ALICE).await;
        let after = SystemTime::now();

        // Then
        assert_eq!(returned, Some(UserId::ALICE));
        let (session_id, at) =
            (*store.record.lock().unwrap()).expect("lookup must reach the store");
        assert_eq!(session_id, SessionId::ALICE);
        assert!(
            before <= at && at <= after,
            "store must see the current time"
        );
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

        client.destroy(SessionId::ALICE).await;
        // Destroy has no reply channel; shutdown drains the actor's queue before returning.
        drop(client);
        runtime.shutdown().await;

        assert_eq!(*store.destroyed.lock().unwrap(), Some(SessionId::ALICE));
    }

    #[tokio::test(start_paused = true)]
    async fn remove_expired_when_next_expiry_is_reached() {
        // Given
        const TTL: Duration = Duration::from_secs(10);
        let start = SystemTime::now();
        let (tx, mut rx) = mpsc::channel(1);
        #[derive(Clone)]
        struct SessionStoreDouble {
            start: SystemTime,
            tx: mpsc::Sender<SystemTime>,
        }
        impl SessionStore for SessionStoreDouble {
            fn earliest_possible_expiry(&self) -> Option<SystemTime> {
                Some(self.start + TTL)
            }
            fn remove_expired(&mut self, now: SystemTime) -> Vec<SessionId> {
                let _ = self.tx.try_send(now);
                Vec::new()
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

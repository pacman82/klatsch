use std::{
    future::pending,
    time::{Duration, SystemTime},
};

use anyhow::Context as _;

use tokio::{
    select,
    sync::{mpsc, oneshot},
    task::JoinHandle,
    time::{Instant, Sleep, sleep_until},
};

use crate::{sessions::session_store::Session, users::UserId};

use super::{SessionHash, SessionId, SessionPersistence, SessionStore};

/// Authenticate users using session ids.
#[cfg_attr(test, double_trait::dummies)]
pub trait AuthenticateSession {
    /// Id of the associated user if the session is valid, otherwise `None`.
    fn authenticate(&self, session_id: SessionId) -> impl Future<Output = Option<UserId>> + Send;
}

#[cfg_attr(test, double_trait::dummies)]
pub trait SessionLifecycle {
    #[cfg(not(test))]
    fn create(&mut self, user_id: UserId) -> impl Future<Output = SessionId> + Send;

    #[cfg(test)]
    fn create(&mut self, _user_id: UserId) -> impl Future<Output = SessionId> + Send {
        async { SessionId::new() }
    }

    fn revoke(&mut self, session_id: SessionId) -> impl Future<Output = ()> + Send;
}

pub struct SessionsRuntime {
    sender: mpsc::Sender<SessionMsg>,
    handle: JoinHandle<()>,
}

impl SessionsRuntime {
    pub(super) async fn start(
        mut store: impl SessionStore + Send + 'static,
        persistence: impl SessionPersistence + Send + 'static,
    ) -> anyhow::Result<Self> {
        let sessions = persistence
            .all_sessions()
            .await
            .context("Failed to restore sessions from persistence")?;
        store.restore(sessions, SystemTime::now());

        let (sender, receiver) = mpsc::channel(16);
        let actor = SessionActor::new(store, persistence, receiver);
        let handle = tokio::spawn(async move { actor.run().await });
        Ok(Self { sender, handle })
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

impl AuthenticateSession for SessionsClient {
    async fn authenticate(&self, session_id: SessionId) -> Option<UserId> {
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

    async fn revoke(&mut self, session_id: SessionId) {
        self.sender
            .send(SessionMsg::Destroy { session_id })
            .await
            .expect("SessionsRuntime must outlive its clients.");
    }
}

struct SessionActor<S, P> {
    store: S,
    persistence: P,
    receiver: mpsc::Receiver<SessionMsg>,
    clock_anchor: ClockAnchor,
    /// We hold a list of revoked sessions, which have not been removed from persistence, yet. This
    /// allows to retry their removal in case of persistence errors.
    revoked_session_hashes: Vec<SessionHash>,
}

impl<S: SessionStore, P: SessionPersistence> SessionActor<S, P> {
    fn new(store: S, persistence: P, receiver: mpsc::Receiver<SessionMsg>) -> Self {
        SessionActor {
            store,
            persistence,
            receiver,
            clock_anchor: ClockAnchor::new(),
            revoked_session_hashes: Vec::new(),
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
                    let revoked_sessions = self.store.remove_expired(
                        earliest_possible_expiry
                            .expect("the timer only completes when a bound was armed"),
                    );
                    self.revoked_session_hashes.extend(revoked_sessions);
                    self.remove_revoked_session().await;
                }
            }
        }
    }

    async fn handle(&mut self, msg: SessionMsg) {
        match msg {
            SessionMsg::Create { user_id, reply } => {
                let now = SystemTime::now();
                let (session_id, session_hash) = self.store.create(user_id, now);
                let session = Session {
                    id: session_hash,
                    user_id,
                    created_at: now,
                    last_activity: now,
                };
                let _ = self.persistence.insert(session).await;
                let _ = reply.send(session_id);
            }
            SessionMsg::Lookup { session_id, reply } => {
                let now = SystemTime::now();
                let (session_hash, user_id) = self.store.lookup(session_id, now);
                let _result = self.persistence.update_activity(session_hash, now).await;
                let _ = reply.send(user_id);
            }
            SessionMsg::Destroy { session_id } => {
                let session_hash = self.store.destroy(session_id);
                self.revoked_session_hashes.push(session_hash);
                self.remove_revoked_session().await;
            }
        }
    }

    async fn remove_revoked_session(&mut self) {
        for session_hash in &self.revoked_session_hashes {
            match self.persistence.remove(*session_hash).await {
                Ok(_) => {}
                // In case of error, we stop, do not clear `revoked_session_hashes` and try again
                // the next time.
                Err(_) => {
                    return;
                }
            }
        }
        self.revoked_session_hashes.clear();
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
        mem::take,
        sync::{Arc, Mutex},
        time::{Duration, SystemTime},
    };

    use tokio::{sync::mpsc, time};

    use double_trait::Dummy;
    use tokio::time::timeout;

    use crate::{
        sessions::{session_persistence::SessionPersistence, session_store::Session},
        users::UserId,
    };

    use super::{
        AuthenticateSession as _, SessionHash, SessionId, SessionLifecycle as _, SessionStore,
        SessionsRuntime,
    };

    #[tokio::test]
    async fn shutdown_completes_within_one_second() {
        let runtime = SessionsRuntime::start(Dummy, Dummy).await.unwrap();
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
            fn create(&mut self, user_id: UserId, now: SystemTime) -> (SessionId, SessionHash) {
                *self.record.lock().unwrap() = Some((user_id, now));
                (
                    SessionId::ALICE,
                    SessionHash::from_session_id(SessionId::nil()),
                )
            }
        }
        let store = Spy::default();
        let runtime = SessionsRuntime::start(store.clone(), Dummy).await.unwrap();
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
            fn lookup(
                &mut self,
                session_id: SessionId,
                now: SystemTime,
            ) -> (SessionHash, Option<UserId>) {
                *self.record.lock().unwrap() = Some((session_id, now));
                (
                    SessionHash::from_session_id(SessionId::nil()),
                    Some(UserId::ALICE),
                )
            }
        }
        let store = Spy::default();
        let runtime = SessionsRuntime::start(store.clone(), Dummy).await.unwrap();
        let client = runtime.client();

        // When
        let before = SystemTime::now();
        let returned = client.authenticate(SessionId::ALICE).await;
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
    async fn revoke_forwards_session_id_to_store() {
        #[derive(Clone, Default)]
        struct Spy {
            destroyed: Arc<Mutex<Option<SessionId>>>,
        }
        impl SessionStore for Spy {
            fn destroy(&mut self, session_id: SessionId) -> SessionHash {
                *self.destroyed.lock().unwrap() = Some(session_id);
                SessionHash::from_session_id(SessionId::nil())
            }
        }
        let store = Spy::default();
        let runtime = SessionsRuntime::start(store.clone(), Dummy).await.unwrap();
        let mut client = runtime.client();

        client.revoke(SessionId::ALICE).await;
        // Revoke has no reply channel; shutdown drains the actor's queue before returning.
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
            fn remove_expired(&mut self, now: SystemTime) -> Vec<SessionHash> {
                let _ = self.tx.try_send(now);
                Vec::new()
            }
        }
        let runtime = SessionsRuntime::start(SessionStoreDouble { start, tx }, Dummy)
            .await
            .unwrap();
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

    #[tokio::test]
    async fn session_are_restored_at_start() {
        // Given a persisted sessions for Alice and Bob
        fn persisted_sessions() -> Vec<Session> {
            vec![
                Session {
                    id: SessionHash::from_session_id(SessionId::ALICE),
                    user_id: UserId::ALICE,
                    created_at: SystemTime::UNIX_EPOCH,
                    last_activity: SystemTime::UNIX_EPOCH,
                },
                Session {
                    id: SessionHash::from_session_id(SessionId::BOB),
                    user_id: UserId::BOB,
                    created_at: SystemTime::UNIX_EPOCH,
                    last_activity: SystemTime::UNIX_EPOCH,
                },
            ]
        }
        struct PersistenceStub;
        impl SessionPersistence for PersistenceStub {
            async fn all_sessions(&self) -> anyhow::Result<Vec<Session>> {
                Ok(persisted_sessions())
            }
        }
        struct SessionStoreMock;

        // When starting the runtime
        let runtime = SessionsRuntime::start(SessionStoreMock, PersistenceStub)
            .await
            .unwrap();

        // Then the runtime should restore the sessions
        impl SessionStore for SessionStoreMock {
            fn restore(&mut self, sessions: Vec<Session>, _now: SystemTime) {
                assert_eq!(sessions, persisted_sessions());
            }
        }

        // Cleanup
        runtime.shutdown().await;
    }

    #[tokio::test]
    async fn boot_aborts_when_persisted_sessions_cannot_be_loaded() {
        // Given persistence which fails to report its sessions
        struct FailingPersistence;
        impl SessionPersistence for FailingPersistence {
            async fn all_sessions(&self) -> anyhow::Result<Vec<Session>> {
                anyhow::bail!("simulated persistence failure")
            }
        }

        // When starting the runtime
        let result = SessionsRuntime::start(Dummy, FailingPersistence).await;

        // Then starting fails instead of silently continuing with an empty session store
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn persist_new_sessions() {
        // Given
        struct StubSessionStore;
        impl SessionStore for StubSessionStore {
            fn create(&mut self, _: UserId, _: SystemTime) -> (SessionId, SessionHash) {
                (
                    SessionId::ALICE,
                    SessionHash::from_session_id(SessionId::ALICE),
                )
            }
        }
        let spy = SessionPersistenceSpy::default();
        let runtime = SessionsRuntime::start(StubSessionStore, spy.clone())
            .await
            .unwrap();
        let mut client = runtime.client();

        // When
        client.create(UserId::ALICE).await;

        // Then
        let inserted_sessions = spy.take_insert_record();
        assert_eq!(inserted_sessions.len(), 1);
        assert_eq!(
            inserted_sessions[0].id,
            SessionHash::from_session_id(SessionId::ALICE)
        );
        assert_eq!(inserted_sessions[0].user_id, UserId::ALICE);
    }

    #[tokio::test]
    async fn session_creation_succeeds_despite_persistence_failure() {
        // Given a session store that succeeds, but persistence that fails to insert
        struct StubSessionStore;
        impl SessionStore for StubSessionStore {
            fn create(&mut self, _: UserId, _: SystemTime) -> (SessionId, SessionHash) {
                (
                    SessionId::ALICE,
                    SessionHash::from_session_id(SessionId::nil()),
                )
            }
        }
        struct SessionInsertSaboteur;
        impl SessionPersistence for SessionInsertSaboteur {
            async fn insert(&mut self, _: Session) -> anyhow::Result<()> {
                anyhow::bail!("simulated persistence failure")
            }
        }
        let runtime = SessionsRuntime::start(StubSessionStore, SessionInsertSaboteur)
            .await
            .unwrap();
        let mut client = runtime.client();

        // When creating a session while persistence is broken
        let session_id = client.create(UserId::ALICE).await;

        // Then the store still creates the session and reports it back to the caller
        assert_eq!(session_id, SessionId::ALICE);

        // Cleanup
        drop(client);
        runtime.shutdown().await;
    }

    #[tokio::test]
    async fn remove_revoked_sessions_from_persistence() {
        // Given
        struct StubSessionStore;
        impl SessionStore for StubSessionStore {
            fn destroy(&mut self, session_id: SessionId) -> SessionHash {
                SessionHash::from_session_id(session_id)
            }
        }
        let persistence = SessionPersistenceSpy::default();
        let runtime = SessionsRuntime::start(StubSessionStore, persistence.clone())
            .await
            .unwrap();

        // When revoking a session
        runtime.client().revoke(SessionId::ALICE).await;

        // Then it is also removed from persistence
        runtime.shutdown().await; // Revoke has no reply channel; shutdown drains the actor's queue.
        assert_eq!(
            persistence.take_remove_record(),
            [SessionHash::from_session_id(SessionId::ALICE)]
        );
    }

    #[tokio::test(start_paused = true)]
    async fn remove_expired_sessions_from_persistence() {
        // Given a session which expires in less than a day
        struct PersistenceSpy(mpsc::UnboundedSender<SessionHash>);
        impl SessionPersistence for PersistenceSpy {
            async fn remove(&mut self, session_hash: SessionHash) -> anyhow::Result<()> {
                self.0.send(session_hash).unwrap();
                Ok(())
            }
        }
        let (sender, mut removed_session) = mpsc::unbounded_channel();
        let persistence = PersistenceSpy(sender);
        struct SessionStoreStub;
        impl SessionStore for SessionStoreStub {
            fn earliest_possible_expiry(&self) -> Option<SystemTime> {
                Some(SystemTime::now() + Duration::from_hours(23))
            }

            fn remove_expired(&mut self, _: SystemTime) -> Vec<SessionHash> {
                vec![SessionHash::from_session_id(SessionId::ALICE)]
            }
        }
        let runtime = SessionsRuntime::start(SessionStoreStub, persistence)
            .await
            .unwrap();

        // When a day passes
        time::advance(Duration::from_hours(24)).await;

        // Then it is also removed from persistence
        let session_hash = timeout(Duration::from_secs(1), removed_session.recv())
            .await
            .expect("Session must be removed, timely after expiration");
        assert_eq!(
            session_hash,
            Some(SessionHash::from_session_id(SessionId::ALICE))
        );

        // Cleanup
        runtime.shutdown().await;
    }

    #[tokio::test]
    async fn update_activity_in_persistence() {
        // Given a session which
        struct SessionStoreStub;
        impl SessionStore for SessionStoreStub {
            fn lookup(
                &mut self,
                session_id: SessionId,
                _: SystemTime,
            ) -> (SessionHash, Option<UserId>) {
                (
                    SessionHash::from_session_id(session_id),
                    Some(UserId::nil()),
                )
            }
        }
        let start = SystemTime::now();
        let persistence = SessionPersistenceSpy::default();
        let runtime = SessionsRuntime::start(SessionStoreStub, persistence.clone())
            .await
            .unwrap();

        // When the session is looked up
        runtime.client().authenticate(SessionId::ALICE).await;

        // Then its timestamp is updated
        let record = persistence.take_updated_record();
        assert_eq!(1, record.len());
        let (session_hash, last_activity) = record[0];
        assert_eq!(SessionHash::from_session_id(SessionId::ALICE), session_hash);
        assert!(last_activity >= start);

        // Cleanup
        runtime.shutdown().await;
    }

    #[derive(Clone, Default)]
    struct SessionPersistenceSpy {
        inserted: Arc<Mutex<Vec<Session>>>,
        removed: Arc<Mutex<Vec<SessionHash>>>,
        updated: Arc<Mutex<Vec<(SessionHash, SystemTime)>>>,
    }

    impl SessionPersistenceSpy {
        fn take_insert_record(&self) -> Vec<Session> {
            take(&mut *self.inserted.lock().unwrap())
        }

        fn take_remove_record(&self) -> Vec<SessionHash> {
            take(&mut *self.removed.lock().unwrap())
        }

        fn take_updated_record(&self) -> Vec<(SessionHash, SystemTime)> {
            take(&mut *self.updated.lock().unwrap())
        }
    }

    impl SessionPersistence for SessionPersistenceSpy {
        async fn insert(&mut self, session: Session) -> anyhow::Result<()> {
            self.inserted.lock().unwrap().push(session);
            Ok(())
        }

        async fn remove(&mut self, session_hash: SessionHash) -> anyhow::Result<()> {
            self.removed.lock().unwrap().push(session_hash);
            Ok(())
        }

        async fn update_activity(
            &mut self,
            id: SessionHash,
            last_activity: SystemTime,
        ) -> anyhow::Result<()> {
            self.updated.lock().unwrap().push((id, last_activity));
            Ok(())
        }
    }
}

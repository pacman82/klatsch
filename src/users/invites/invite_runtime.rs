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

use crate::users::{CreateUser, UserId, UsersError};

use super::{InviteToken, StoreInvites};

pub struct InviteRuntime {
    sender: mpsc::Sender<ActorMsg>,
    join_handle: JoinHandle<()>,
}

impl InviteRuntime {
    /// Construct runtime with any invite store and user-creating collaborator.
    ///
    /// This flexibility makes it well testable and enforces the implementation of runtime aspects
    /// to be independent of `StoreInvites`'s implementation. The visibility is super since the
    /// decision which store to use in production belongs to the `invites` parent module.
    pub(super) fn with<U>(store: impl StoreInvites + Send + 'static, users: U) -> Self
    where
        U: CreateUser + Send + 'static,
    {
        let (sender, receiver) = mpsc::channel(5);
        let actor = Actor::new(store, users, receiver);
        let join_handle = tokio::spawn(async move { actor.run().await });
        InviteRuntime {
            sender,
            join_handle,
        }
    }

    /// A client which implements the [`Invite`] trait.
    pub fn client(&self) -> InviteClient {
        InviteClient {
            sender: self.sender.clone(),
        }
    }

    /// Shuts down the invite runtime. In order for this to complete, all clients must have been
    /// dropped.
    pub async fn shutdown(self) {
        // At this point we should be the only owner of the sender, since all clients should have
        // been dropped. This might be unecessary restrictive if we want to shutdown things in
        // parallel. Right now however the invariant holds. The panic might save us some time if we
        // forget to clean up all senders in a test.
        debug_assert_eq!(self.sender.strong_count(), 1);
        // We drop the sender, to signal to the actor thread that it can no longer receive messages
        // and should stop.
        drop(self.sender);
        self.join_handle.await.unwrap();
    }
}

/// Create invites and claim them.
#[cfg_attr(test, double_trait::dummies)]
pub trait Invite {
    /// Creates a new invite, valid for the configured expiry.
    fn new_invite(&mut self) -> impl Future<Output = anyhow::Result<InviteToken>> + Send;

    /// Claims an invite and creates the account it authorizes
    fn claim(
        &mut self,
        invitation: InviteToken,
        name: String,
        password: String,
    ) -> impl Future<Output = Result<UserId, UsersError>> + Send;

    /// Checks whether an invite exists and has not yet expired, without claiming it.
    fn is_valid(
        &mut self,
        invitation: InviteToken,
    ) -> impl Future<Output = anyhow::Result<bool>> + Send;
}

#[derive(Clone)]
pub struct InviteClient {
    sender: mpsc::Sender<ActorMsg>,
}

impl Invite for InviteClient {
    async fn new_invite(&mut self) -> anyhow::Result<InviteToken> {
        let (responder, response) = oneshot::channel();
        self.sender
            .send(ActorMsg::NewInvite { responder })
            .await
            .expect("Actor must outlive client.");
        response.await.unwrap()
    }

    async fn claim(
        &mut self,
        invitation: InviteToken,
        name: String,
        password: String,
    ) -> Result<UserId, UsersError> {
        let (responder, response) = oneshot::channel();
        self.sender
            .send(ActorMsg::Claim {
                invitation,
                name,
                password,
                responder,
            })
            .await
            .expect("Actor must outlive client.");
        response.await.unwrap()
    }

    async fn is_valid(&mut self, invitation: InviteToken) -> anyhow::Result<bool> {
        let (responder, response) = oneshot::channel();
        self.sender
            .send(ActorMsg::IsValid {
                invitation,
                responder,
            })
            .await
            .expect("Actor must outlive client.");
        response.await.unwrap()
    }
}

enum ActorMsg {
    NewInvite {
        responder: oneshot::Sender<anyhow::Result<InviteToken>>,
    },
    Claim {
        invitation: InviteToken,
        name: String,
        password: String,
        responder: oneshot::Sender<Result<UserId, UsersError>>,
    },
    IsValid {
        invitation: InviteToken,
        responder: oneshot::Sender<anyhow::Result<bool>>,
    },
}

struct Actor<S, U> {
    /// The invites' domain state.
    store: S,
    /// Used to create the account once an invite is claimed.
    users: U,
    receiver: mpsc::Receiver<ActorMsg>,
    clock_anchor: ClockAnchor,
}

impl<S: StoreInvites, U: CreateUser> Actor<S, U> {
    fn new(store: S, users: U, receiver: mpsc::Receiver<ActorMsg>) -> Self {
        Actor {
            store,
            users,
            receiver,
            clock_anchor: ClockAnchor::new(),
        }
    }

    async fn run(mut self) {
        loop {
            let earliest_possible_expiry = self
                .store
                .earliest_possible_expiry()
                .await
                .unwrap_or_default();
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
                    Some(msg) => self.handle_message(msg).await,
                    None => return,
                },
                () = sleep_until_earliest_possible_expiry => {
                    // Errors are not fatal — we simply try again once the next event re-enters
                    // the loop, or on the next scheduled sweep once the store recovers. We sweep
                    // using the deadline we woke up for, rather than a freshly read `now`, to stay
                    // deterministic under a paused clock in tests.
                    let deadline = earliest_possible_expiry
                        .expect("the timer only completes when a bound was armed");
                    let _ = self.store.remove_expired(deadline).await;
                }
            }
        }
    }

    async fn handle_message(&mut self, msg: ActorMsg) {
        match msg {
            ActorMsg::NewInvite { responder } => {
                let result = self.store.new_invite(SystemTime::now()).await;
                // We ignore send errors, since it only happens if the receiver has been dropped.
                // In that case the receiver is no longer interested in the response, anyway.
                let _ = responder.send(result);
            }
            ActorMsg::Claim {
                invitation,
                name,
                password,
                responder,
            } => {
                let result = self.claim(invitation, name, password).await;
                let _ = responder.send(result);
            }
            ActorMsg::IsValid {
                invitation,
                responder,
            } => {
                let result = self.store.is_valid(invitation, SystemTime::now()).await;
                let _ = responder.send(result);
            }
        }
    }

    async fn claim(
        &mut self,
        invitation: InviteToken,
        name: String,
        password: String,
    ) -> Result<UserId, UsersError> {
        let claimed = self
            .store
            .claim(invitation, SystemTime::now())
            .await
            .map_err(|_| UsersError::Internal)?;
        if !claimed {
            return Err(UsersError::InvalidInvite);
        }
        self.users.create_user(name, password).await
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

#[cfg(test)]
mod tests {
    use std::{
        sync::{Arc, Mutex},
        time::{Duration, SystemTime},
    };

    use double_trait::Dummy;
    use tokio::sync::mpsc;
    use tokio::time::{self, timeout};

    use super::{CreateUser, Invite, InviteRuntime, InviteToken, StoreInvites, UserId, UsersError};

    #[tokio::test]
    async fn new_invite_forwards_to_the_store() {
        // Given
        #[derive(Clone, Default)]
        struct NewInviteSpy {
            observed: Arc<Mutex<Vec<SystemTime>>>,
        }
        impl StoreInvites for NewInviteSpy {
            async fn new_invite(&mut self, now: SystemTime) -> anyhow::Result<InviteToken> {
                self.observed.lock().unwrap().push(now);
                Ok(InviteToken::ALPHA)
            }
        }
        let spy = NewInviteSpy::default();
        let invites = InviteRuntime::with(spy.clone(), Dummy);

        // When
        let before = SystemTime::now();
        let token = invites.client().new_invite().await.unwrap();
        let after = SystemTime::now();

        // Then
        assert_eq!(token, InviteToken::ALPHA);
        let observed = spy.observed.lock().unwrap();
        assert_eq!(observed.len(), 1);
        assert!(before <= observed[0] && observed[0] <= after);

        // Cleanup
        drop(observed);
        invites.shutdown().await;
    }

    #[tokio::test]
    async fn claim_forwards_the_token_to_the_store() {
        // Given
        #[derive(Clone, Default)]
        struct ClaimSpy {
            observed: Arc<Mutex<Vec<InviteToken>>>,
        }
        impl StoreInvites for ClaimSpy {
            async fn claim(
                &mut self,
                token: InviteToken,
                _now: SystemTime,
            ) -> anyhow::Result<bool> {
                self.observed.lock().unwrap().push(token);
                Ok(true)
            }
        }
        let spy = ClaimSpy::default();
        #[derive(Clone)]
        struct CreateUserStub;
        impl CreateUser for CreateUserStub {
            async fn create_user(
                &mut self,
                _name: String,
                _password: String,
            ) -> Result<UserId, UsersError> {
                Ok(UserId::ALICE)
            }
        }
        let invites = InviteRuntime::with(spy.clone(), CreateUserStub);

        // When
        let user_id = invites
            .client()
            .claim(InviteToken::ALPHA, "Alice".into(), "secret".into())
            .await
            .unwrap();

        // Then
        assert_eq!(user_id, UserId::ALICE);
        assert_eq!(*spy.observed.lock().unwrap(), vec![InviteToken::ALPHA]);

        // Cleanup
        invites.shutdown().await;
    }

    #[tokio::test]
    async fn claim_creates_the_user_with_the_given_name_and_password() {
        // Given a store that always claims successfully
        #[derive(Clone)]
        struct AcceptingStore;
        impl StoreInvites for AcceptingStore {
            async fn claim(
                &mut self,
                _token: InviteToken,
                _now: SystemTime,
            ) -> anyhow::Result<bool> {
                Ok(true)
            }
        }
        #[derive(Clone, Default)]
        struct CreateUserSpy {
            observed: Arc<Mutex<Vec<(String, String)>>>,
        }
        impl CreateUser for CreateUserSpy {
            async fn create_user(
                &mut self,
                name: String,
                password: String,
            ) -> Result<UserId, UsersError> {
                self.observed.lock().unwrap().push((name, password));
                Ok(UserId::ALICE)
            }
        }
        let spy = CreateUserSpy::default();
        let invites = InviteRuntime::with(AcceptingStore, spy.clone());

        // When
        invites
            .client()
            .claim(InviteToken::ALPHA, "Alice".into(), "secret".into())
            .await
            .unwrap();

        // Then
        assert_eq!(
            *spy.observed.lock().unwrap(),
            vec![("Alice".to_owned(), "secret".to_owned())]
        );

        // Cleanup
        invites.shutdown().await;
    }

    #[tokio::test]
    async fn claim_does_not_create_a_user_when_the_invite_is_invalid() {
        // Given a store that never successfully claims
        #[derive(Clone)]
        struct RejectingStore;
        impl StoreInvites for RejectingStore {
            async fn claim(
                &mut self,
                _token: InviteToken,
                _now: SystemTime,
            ) -> anyhow::Result<bool> {
                Ok(false)
            }
        }
        #[derive(Clone)]
        struct RejectsCreateUser;
        impl CreateUser for RejectsCreateUser {
            async fn create_user(
                &mut self,
                _name: String,
                _password: String,
            ) -> Result<UserId, UsersError> {
                panic!("an invalid invite must not create a user");
            }
        }
        let invites = InviteRuntime::with(RejectingStore, RejectsCreateUser);

        // When
        let result = invites
            .client()
            .claim(InviteToken::nil(), "Alice".into(), "secret".into())
            .await;

        // Then no panic occurred, i.e. create_user was never called
        assert!(matches!(result, Err(UsersError::InvalidInvite)));

        // Cleanup
        invites.shutdown().await;
    }

    #[tokio::test]
    async fn is_valid_forwards_to_the_store() {
        // Given
        #[derive(Clone, Default)]
        struct IsValidSpy {
            observed: Arc<Mutex<Vec<InviteToken>>>,
        }
        impl StoreInvites for IsValidSpy {
            async fn is_valid(&self, token: InviteToken, _now: SystemTime) -> anyhow::Result<bool> {
                self.observed.lock().unwrap().push(token);
                Ok(true)
            }
        }
        let spy = IsValidSpy::default();
        let invites = InviteRuntime::with(spy.clone(), Dummy);

        // When
        let valid = invites.client().is_valid(InviteToken::ALPHA).await.unwrap();

        // Then
        assert!(valid);
        assert_eq!(*spy.observed.lock().unwrap(), vec![InviteToken::ALPHA]);

        // Cleanup
        invites.shutdown().await;
    }

    #[tokio::test]
    async fn is_valid_does_not_send_a_claim() {
        // Given a store that fails the test if claim is ever called
        #[derive(Clone, Default)]
        struct RejectsClaim;
        impl StoreInvites for RejectsClaim {
            async fn claim(
                &mut self,
                _token: InviteToken,
                _now: SystemTime,
            ) -> anyhow::Result<bool> {
                panic!("is_valid must not claim the invite");
            }

            async fn is_valid(
                &self,
                _token: InviteToken,
                _now: SystemTime,
            ) -> anyhow::Result<bool> {
                Ok(true)
            }
        }
        let invites = InviteRuntime::with(RejectsClaim, Dummy);

        // When
        let valid = invites.client().is_valid(InviteToken::ALPHA).await.unwrap();

        // Then no panic occurred, i.e. claim was never called
        assert!(valid);

        // Cleanup
        invites.shutdown().await;
    }

    #[tokio::test(start_paused = true)]
    async fn sweeps_expired_invites_once_the_next_expiry_is_reached() {
        // Given a store reporting a single invite that expires in 10 seconds
        const TTL: Duration = Duration::from_secs(10);
        let start = SystemTime::now();
        let (tx, mut rx) = mpsc::channel(1);
        #[derive(Clone)]
        struct StoreDouble {
            start: SystemTime,
            tx: mpsc::Sender<SystemTime>,
        }
        impl StoreInvites for StoreDouble {
            async fn earliest_possible_expiry(&self) -> anyhow::Result<Option<SystemTime>> {
                Ok(Some(self.start + TTL))
            }

            async fn remove_expired(&self, now: SystemTime) -> anyhow::Result<Option<SystemTime>> {
                let _ = self.tx.try_send(now);
                Ok(None)
            }
        }
        let invites = InviteRuntime::with(StoreDouble { start, tx }, Dummy);

        // When 10 seconds pass
        time::advance(TTL).await;

        // Then the store is swept
        let swept_at = timeout(Duration::from_secs(1), rx.recv())
            .await
            .expect("remove_expired was not called within one second")
            .unwrap();
        assert_eq!(swept_at, start + TTL);

        // Cleanup
        invites.shutdown().await;
    }

    #[tokio::test]
    async fn shutdown_completes_within_one_second() {
        // Given
        let invites = InviteRuntime::with(Dummy, Dummy);

        // When
        let result = timeout(Duration::from_secs(1), invites.shutdown()).await;

        // Then
        assert!(result.is_ok(), "Shutdown did not complete within 1 second");
    }
}

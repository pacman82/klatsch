use std::time::Duration;

use axum::http::request;
use tokio::sync::watch;

use crate::{http::HttpError, persistence::ExecuteSqlAsync, server::Routes};

use super::{
    AuthenticateRequest, AuthenticateSession, ChangeUsers, CreateUser, InviteClient, InviteRuntime,
    SessionExpiry, SessionId, SessionLifecycle, SessionsClient, SessionsRuntime, UserId, UserStore,
    UsersError, VerifyCredentials, VerifyCredentialsError, invite_routes, login_routes,
    user_routes,
};

/// Configuration required by [`UsersRuntime`], bundling what each of its sub-runtimes needs.
#[derive(Clone, Copy)]
pub struct UsersConfiguration {
    /// When sessions expire.
    pub session_expiry: SessionExpiry,
    /// How long an invite remains claimable after creation.
    pub invite_expiry: Duration,
}

pub struct UsersRuntime<P> {
    users: UserStore<P>,
    sessions: SessionsRuntime,
    invites: InviteRuntime,
}

impl<P> UsersRuntime<P> {
    pub async fn new<F>(
        cfg: UsersConfiguration,
        open_connection: impl Fn() -> F,
    ) -> anyhow::Result<Self>
    where
        F: Future<Output = anyhow::Result<P>>,
        P: ExecuteSqlAsync + Send + Sync + Clone + 'static,
    {
        let (users, sessions, invite_connection) = tokio::try_join!(
            async {
                let conn = open_connection().await?;
                Ok(UserStore::new(conn))
            },
            async { SessionsRuntime::new(cfg.session_expiry, open_connection().await?).await },
            open_connection(),
        )?;
        let invites = InviteRuntime::new(cfg.invite_expiry, invite_connection, users.clone());
        Ok(Self {
            users,
            sessions,
            invites,
        })
    }

    pub async fn shutdown(self) {
        tokio::join!(self.sessions.shutdown(), self.invites.shutdown());
    }

    pub fn client(&self) -> UsersClient<UserStore<P>, SessionsClient>
    where
        P: Clone,
    {
        UsersClient::new(
            self.users.clone(),
            self.sessions.client(),
            self.invites.client(),
        )
    }
}

/// Signup users, log them in and out.
#[derive(Clone)]
pub struct UsersClient<U, S, I = InviteClient> {
    /// Used to validatate credentials and create new users during signup
    users: U,
    /// Creates and revokes session during login and logout
    sessions: S,
    /// Used to verify and claim invites during signup
    invites: I,
}

impl<U, S, I> UsersClient<U, S, I> {
    pub fn new(users: U, sessions: S, invites: I) -> Self {
        Self {
            users,
            sessions,
            invites,
        }
    }
}

#[cfg_attr(test, double_trait::dummies)]
pub trait Login {
    /// Creates a session if credentials are correct
    fn login(
        &mut self,
        name: String,
        password: String,
    ) -> impl Future<Output = Result<(SessionId, UserId), VerifyCredentialsError>> + Send;

    /// Revokes a session
    fn logout(&mut self, session_id: SessionId) -> impl Future<Output = ()> + Send;

    /// Creates the very first user in the system, and a session for them. Only succeeds while the
    /// system has no users yet; every other account is created by claiming an invite instead (see
    /// the `invites` module), a different process entirely.
    fn create_initial_user(
        &mut self,
        name: String,
        password: String,
    ) -> impl Future<Output = Result<(SessionId, UserId), UsersError>> + Send;
}

impl<U, S, I> Login for UsersClient<U, S, I>
where
    U: VerifyCredentials + ChangeUsers + CreateUser + Send,
    S: SessionLifecycle + Send,
    I: Send,
{
    async fn login(
        &mut self,
        name: String,
        password: String,
    ) -> Result<(SessionId, UserId), VerifyCredentialsError> {
        let user_id = self.users.authenticate(name, password).await?;
        let session_id = self.sessions.create(user_id).await;
        Ok((session_id, user_id))
    }

    async fn logout(&mut self, session_id: SessionId) {
        self.sessions.revoke(session_id).await;
    }

    async fn create_initial_user(
        &mut self,
        name: String,
        password: String,
    ) -> Result<(SessionId, UserId), UsersError> {
        if !self.users.is_empty().await? {
            return Err(UsersError::AlreadyBootstrapped);
        }
        let user_id = self.users.create_user(name, password).await?;
        let session_id = self.sessions.create(user_id).await;
        Ok((session_id, user_id))
    }
}

impl<U, S, I> AuthenticateRequest for UsersClient<U, S, I>
where
    S: AuthenticateRequest,
{
    fn authenticate_request(
        &self,
        parts: &request::Parts,
    ) -> impl Future<Output = Result<UserId, HttpError>> + Send {
        self.sessions.authenticate_request(parts)
    }
}

impl<U, S> Routes for UsersClient<U, S, InviteClient>
where
    U: Send + Sync + Clone + VerifyCredentials + ChangeUsers + CreateUser + 'static,
    S: Send + Sync + Clone + SessionLifecycle + AuthenticateSession + 'static,
{
    fn routes(
        self,
        _auth: impl AuthenticateRequest + Send + Sync + Clone + 'static,
        _shutting_down: watch::Receiver<bool>,
        encrypted: bool,
    ) -> axum::Router<()> {
        login_routes(self.clone(), encrypted)
            .merge(user_routes(self.users, self.sessions.clone()))
            .merge(invite_routes(self.invites, self.sessions, encrypted))
    }
}

#[cfg(test)]
mod tests {
    use std::assert_matches;

    use double_trait::Dummy;

    use super::{
        ChangeUsers, CreateUser, Login, UserId, UsersClient, UsersError, VerifyCredentials,
    };

    #[tokio::test]
    async fn create_initial_user_succeeds_when_users_are_empty() {
        // Given no users exist yet
        #[derive(Clone)]
        struct EmptyUsers;
        impl VerifyCredentials for EmptyUsers {}
        impl ChangeUsers for EmptyUsers {
            async fn is_empty(&mut self) -> Result<bool, UsersError> {
                Ok(true)
            }
        }
        impl CreateUser for EmptyUsers {
            async fn create_user(
                &mut self,
                _name: String,
                _password: String,
            ) -> Result<UserId, UsersError> {
                Ok(UserId::ALICE)
            }
        }
        let mut client = UsersClient::new(EmptyUsers, Dummy, Dummy);

        // When creating the initial user
        let result = client
            .create_initial_user("Alice".into(), "secret".into())
            .await;

        // Then
        assert_matches!(result, Ok((_, UserId::ALICE)));
    }

    #[tokio::test]
    async fn create_initial_user_rejects_once_the_system_is_no_longer_empty() {
        // Given a user already exists
        #[derive(Clone)]
        struct ExistingUsers;
        impl VerifyCredentials for ExistingUsers {}
        impl ChangeUsers for ExistingUsers {
            async fn is_empty(&mut self) -> Result<bool, UsersError> {
                Ok(false)
            }
        }
        impl CreateUser for ExistingUsers {}
        let mut client = UsersClient::new(ExistingUsers, Dummy, Dummy);

        // When attempting to create the initial user again
        let result = client
            .create_initial_user("Alice".into(), "secret".into())
            .await;

        // Then
        assert_matches!(result, Err(UsersError::AlreadyBootstrapped));
    }
}

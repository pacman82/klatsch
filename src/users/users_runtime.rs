use axum::http::request;
use tokio::sync::watch;

use crate::{
    http::HttpError,
    persistence::ExecuteSqlAsync,
    server::Routes,
    sessions::{
        AuthenticateSession, SessionExpiry, SessionId, SessionLifecycle, SessionsClient,
        SessionsRuntime,
    },
};

use super::{
    AuthenticateRequest, ChangeUsers, UserId, UserStore, UsersError, VerifyCredentials,
    VerifyCredentialsError,
    invites::{InviteClient, InviteRuntime},
    login_routes, user_routes,
};

pub struct UsersRuntime<P> {
    users: UserStore<P>,
    sessions: SessionsRuntime,
    invites: InviteRuntime,
}

impl<P> UsersRuntime<P> {
    pub async fn new<F>(
        session_expiry: SessionExpiry,
        open_connection: impl Fn() -> F,
    ) -> anyhow::Result<Self>
    where
        F: Future<Output = anyhow::Result<P>>,
        P: ExecuteSqlAsync + Send + Sync + 'static,
    {
        let (users, sessions) = tokio::try_join!(
            async {
                let conn = open_connection().await?;
                Ok(UserStore::new(conn))
            },
            async { SessionsRuntime::new(session_expiry, open_connection().await?).await },
        )?;
        let invites = InviteRuntime::new();
        Ok(Self {
            users,
            sessions,
            invites,
        })
    }

    pub async fn shutdown(self) {
        self.sessions.shutdown().await;
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
pub struct UsersClient<U, S> {
    /// Used to validatate credentials and create new users during signup
    users: U,
    /// Creates and revokes session during login and logout
    sessions: S,
    invites: InviteClient,
}

impl<U, S> UsersClient<U, S> {
    pub fn new(users: U, sessions: S, invites: InviteClient) -> Self {
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

    /// Creates a user and a session
    fn signup(
        &mut self,
        name: String,
        password: String,
    ) -> impl Future<Output = Result<(SessionId, UserId), UsersError>> + Send;

    /// Whether the system has no users at all yet. Used to allow the very first user to sign up
    /// without an invite.
    fn is_empty(&mut self) -> impl Future<Output = Result<bool, UsersError>> + Send;
}

impl<U, S> Login for UsersClient<U, S>
where
    U: VerifyCredentials + ChangeUsers + Send,
    S: SessionLifecycle + Send,
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

    async fn signup(
        &mut self,
        name: String,
        password: String,
    ) -> Result<(SessionId, UserId), UsersError> {
        let user_id = self.users.signup(name, password).await?;
        let session_id = self.sessions.create(user_id).await;
        Ok((session_id, user_id))
    }

    async fn is_empty(&mut self) -> Result<bool, UsersError> {
        self.users.is_empty().await
    }
}

impl<U, S> AuthenticateRequest for UsersClient<U, S>
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

impl<U, S> Routes for UsersClient<U, S>
where
    U: Send + Sync + Clone + VerifyCredentials + ChangeUsers + 'static,
    S: Send + Sync + Clone + SessionLifecycle + AuthenticateSession + 'static,
{
    fn routes(
        self,
        _auth: impl AuthenticateRequest + Send + Sync + Clone + 'static,
        shutting_down: watch::Receiver<bool>,
        encrypted: bool,
    ) -> axum::Router<()> {
        login_routes(self.clone(), self.invites.clone(), encrypted)
            .merge(user_routes(self.users, self.sessions.clone()))
            .merge(self.invites.routes(self.sessions, shutting_down, encrypted))
    }
}

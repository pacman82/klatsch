use axum::http::request;

use crate::{
    authentication::AuthenticateRequest,
    http::HttpError,
    sessions::{SessionId, SessionLifecycle},
    user::{AuthenticateUser, AuthenticationError, UserId, Users, UsersError},
};

/// Signup users, log them in and out.
#[derive(Clone)]
pub struct AuthService<U, S> {
    /// Used to validatate credentials and create new users during signup
    users: U,
    /// Creates and revokes session during login and logout
    sessions: S,
}

impl<U, S> AuthService<U, S> {
    pub fn new(users: U, sessions: S) -> Self {
        Self { users, sessions }
    }
}

#[cfg_attr(test, double_trait::dummies)]
pub trait Login {
    /// Creates a session if credentials are correct
    fn login(
        &mut self,
        name: String,
        password: String,
    ) -> impl Future<Output = Result<(SessionId, UserId), AuthenticationError>> + Send;

    /// Revokes a session
    fn logout(&mut self, session_id: SessionId) -> impl Future<Output = ()> + Send;

    /// Creates a user and a session
    fn signup(
        &mut self,
        name: String,
        password: String,
    ) -> impl Future<Output = Result<(SessionId, UserId), UsersError>> + Send;
}

impl<U, S> Login for AuthService<U, S>
where
    U: AuthenticateUser + Users + Send,
    S: SessionLifecycle + Send,
{
    async fn login(
        &mut self,
        name: String,
        password: String,
    ) -> Result<(SessionId, UserId), AuthenticationError> {
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
}

impl<U, S> AuthenticateRequest for AuthService<U, S>
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

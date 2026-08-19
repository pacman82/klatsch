use crate::{
    sessions::{SessionId, SessionLifecycle},
    user::{AuthenticateUser, AuthenticationError, UserId, Users, UsersError},
};

/// Signup users, log them in and out.
#[derive(Clone)]
pub struct AuthService<U, S> {
    users: U,
    sessions: S,
}

impl<U, S> AuthService<U, S> {
    pub fn new(users: U, sessions: S) -> Self {
        Self { users, sessions }
    }
}

#[cfg_attr(test, double_trait::dummies)]
pub trait Login {
    fn login(
        &mut self,
        name: String,
        password: String,
    ) -> impl Future<Output = Result<(SessionId, UserId), AuthenticationError>> + Send;

    fn logout(&mut self, session_id: SessionId) -> impl Future<Output = ()> + Send;

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

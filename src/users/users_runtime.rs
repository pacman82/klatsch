use axum::http::request;
use tokio::sync::watch;

use crate::{http::HttpError, persistence::ExecuteSqlAsync, server::Routes};

use super::{
    AuthenticateRequest, AuthenticateSession, ChangeUsers, SessionExpiry, SessionId,
    SessionLifecycle, SessionsClient, SessionsRuntime, UserId, UserStore, UsersError,
    VerifyCredentials, VerifyCredentialsError,
    invites::{Invite, InviteClient, InviteRuntime, InviteToken},
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

    /// Creates a user and a session. Unless this is the very first user in the system, a valid,
    /// claimed invite is required.
    fn signup(
        &mut self,
        name: String,
        password: String,
        invite: Option<InviteToken>,
    ) -> impl Future<Output = Result<(SessionId, UserId), UsersError>> + Send;
}

impl<U, S, I> Login for UsersClient<U, S, I>
where
    U: VerifyCredentials + ChangeUsers + Send,
    S: SessionLifecycle + Send,
    I: Invite + Send,
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
        invite: Option<InviteToken>,
    ) -> Result<(SessionId, UserId), UsersError> {
        // The very first user bootstraps the system and does not need an invite.
        if !self.users.is_empty().await? {
            let token = invite.ok_or(UsersError::MissingInvite)?;
            let claimed = self
                .invites
                .claim(token)
                .map_err(|_| UsersError::Internal)?;
            if !claimed {
                return Err(UsersError::InvalidInvite);
            }
        }

        let user_id = self.users.signup(name, password).await?;
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
    U: Send + Sync + Clone + VerifyCredentials + ChangeUsers + 'static,
    S: Send + Sync + Clone + SessionLifecycle + AuthenticateSession + 'static,
{
    fn routes(
        self,
        _auth: impl AuthenticateRequest + Send + Sync + Clone + 'static,
        shutting_down: watch::Receiver<bool>,
        encrypted: bool,
    ) -> axum::Router<()> {
        login_routes(self.clone(), encrypted)
            .merge(user_routes(self.users, self.sessions.clone()))
            .merge(self.invites.routes(self.sessions, shutting_down, encrypted))
    }
}

#[cfg(test)]
mod tests {
    use std::assert_matches;

    use double_trait::Dummy;

    use super::{ChangeUsers, Login, UserId, UsersClient, UsersError, VerifyCredentials};
    use crate::users::invites::{Invite, InviteToken};

    #[tokio::test]
    async fn signup_does_not_require_invite_when_users_are_empty() {
        // Given no users exist yet
        #[derive(Clone)]
        struct EmptyUsers;
        impl VerifyCredentials for EmptyUsers {}
        impl ChangeUsers for EmptyUsers {
            async fn is_empty(&mut self) -> Result<bool, UsersError> {
                Ok(true)
            }

            async fn signup(
                &mut self,
                _name: String,
                _password: String,
            ) -> Result<UserId, UsersError> {
                Ok(UserId::ALICE)
            }
        }
        let mut client = UsersClient::new(EmptyUsers, Dummy, Dummy);

        // When signing up without an invite
        let result = client.signup("Alice".into(), "secret".into(), None).await;

        // Then
        assert_matches!(result, Ok((_, UserId::ALICE)));
    }

    #[tokio::test]
    async fn signup_rejects_missing_invite() {
        // Given existing users
        #[derive(Clone)]
        struct ExistingUsers;
        impl VerifyCredentials for ExistingUsers {}
        impl ChangeUsers for ExistingUsers {
            async fn is_empty(&mut self) -> Result<bool, UsersError> {
                Ok(false)
            }
        }
        let mut client = UsersClient::new(ExistingUsers, Dummy, Dummy);

        // When signing up without an invite
        let result = client.signup("Alice".into(), "secret".into(), None).await;

        // Then
        assert_matches!(result, Err(UsersError::MissingInvite));
    }

    #[tokio::test]
    async fn signup_rejects_invalid_invite() {
        // Given existing users and an invite that fails to claim
        #[derive(Clone)]
        struct ExistingUsers;
        impl VerifyCredentials for ExistingUsers {}
        impl ChangeUsers for ExistingUsers {
            async fn is_empty(&mut self) -> Result<bool, UsersError> {
                Ok(false)
            }
        }
        #[derive(Clone)]
        struct InvalidInvite;
        impl Invite for InvalidInvite {
            fn claim(&mut self, _invitation: InviteToken) -> anyhow::Result<bool> {
                Ok(false)
            }
        }
        let mut client = UsersClient::new(ExistingUsers, Dummy, InvalidInvite);

        // When signing up with the invalid invite
        let result = client
            .signup("Alice".into(), "secret".into(), Some(InviteToken::nil()))
            .await;

        // Then
        assert_matches!(result, Err(UsersError::InvalidInvite));
    }

    #[tokio::test]
    async fn signup_claims_invite() {
        // Given existing users and a valid invite
        #[derive(Clone)]
        struct ExistingUsers;
        impl VerifyCredentials for ExistingUsers {}
        impl ChangeUsers for ExistingUsers {
            async fn is_empty(&mut self) -> Result<bool, UsersError> {
                Ok(false)
            }

            async fn signup(
                &mut self,
                _name: String,
                _password: String,
            ) -> Result<UserId, UsersError> {
                Ok(UserId::ALICE)
            }
        }
        #[derive(Clone)]
        struct ValidInvite;
        impl Invite for ValidInvite {
            fn claim(&mut self, invitation: InviteToken) -> anyhow::Result<bool> {
                assert_eq!(invitation, InviteToken::ALPHA);
                Ok(true)
            }
        }
        let mut client = UsersClient::new(ExistingUsers, Dummy, ValidInvite);

        // When signing up with the valid invite
        let result = client
            .signup("Alice".into(), "secret".into(), Some(InviteToken::ALPHA))
            .await;

        // Then
        assert_matches!(result, Ok((_, UserId::ALICE)));
    }
}

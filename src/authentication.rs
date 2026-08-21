mod auth_service;
mod authenticate_request;
mod session_cookie;

use tokio::sync::watch;

use crate::{
    server::Routes,
    sessions::SessionLifecycle,
    user::{AuthenticateUser, Users},
};

use self::{auth_service::Login, session_cookie::login_routes};

pub use self::{
    auth_service::AuthService,
    authenticate_request::{AuthenticateRequest, AuthenticatedUser},
};

impl<U, S> Routes for AuthService<U, S>
where
    U: Send + Sync + Clone + AuthenticateUser + Users + 'static,
    S: Send + Sync + Clone + SessionLifecycle + 'static,
{
    fn routes(
        self,
        _auth: impl AuthenticateRequest + Send + Sync + Clone + 'static,
        _shutting_down: watch::Receiver<bool>,
        encrypted: bool,
    ) -> axum::Router<()> {
        login_routes(self, encrypted)
    }
}

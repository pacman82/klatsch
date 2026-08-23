mod authenticate_request;
mod login_routes;
mod users_runtime;

use tokio::sync::watch;

use crate::{
    server::Routes,
    sessions::SessionLifecycle,
    user::{AuthenticateUser, Users},
};

use self::{login_routes::login_routes, users_runtime::Login};

pub use self::{
    authenticate_request::{AuthenticateRequest, AuthenticatedUser},
    users_runtime::{UsersClient, UsersRuntime},
};

impl<U, S> Routes for UsersClient<U, S>
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

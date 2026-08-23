mod password_hash;
mod user_http;
mod user_id;
mod user_persistence;
mod user_store;

use tokio::sync::watch;

use crate::{persistence::ExecuteSqlAsync, server::Routes, users::AuthenticateRequest};

pub use self::{
    user_id::UserId,
    user_persistence::{UserPersistence, migrate_users_persistence},
    user_store::{AuthenticateUser, AuthenticationError, User, UserStore, Users, UsersError},
};

use self::{user_http::user_routes, user_persistence::UserCreateOutcome};

impl<P> Routes for UserStore<P>
where
    P: ExecuteSqlAsync + Send + Sync + Clone + 'static,
{
    fn routes(
        self,
        auth: impl AuthenticateRequest + Send + Sync + Clone + 'static,
        _shutting_down: watch::Receiver<bool>,
        _encrypted: bool,
    ) -> axum::Router<()> {
        user_routes(self, auth)
    }
}

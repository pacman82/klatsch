mod invite_http;
mod invite_runtime;
mod invite_store;
mod invite_token;

use tokio::sync::watch;

use crate::{server::Routes, users::AuthenticateRequest};

use self::{invite_http::invite_routes, invite_store::InviteStore, invite_token::InviteToken};

pub use self::invite_runtime::{Invite, InviteClient, InviteRuntime};

impl Routes for InviteClient {
    fn routes(
        self,
        _auth: impl AuthenticateRequest + Send + Sync + Clone + 'static,
        _shutting_down: watch::Receiver<bool>,
        encrypted: bool,
    ) -> axum::Router<()> {
        invite_routes(self, encrypted)
    }
}

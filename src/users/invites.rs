mod invite_http;
mod invite_runtime;
mod invite_store;
mod invite_token;

use tokio::sync::watch;

use crate::{server::Routes, users::AuthenticateRequest};

use self::{invite_http::invite_routes, invite_store::InviteStore};

pub use self::{
    invite_runtime::{Invite, InviteClient, InviteRuntime},
    invite_token::InviteToken,
};

impl Routes for InviteClient {
    fn routes(
        self,
        auth: impl AuthenticateRequest + Send + Sync + Clone + 'static,
        _shutting_down: watch::Receiver<bool>,
        encrypted: bool,
    ) -> axum::Router<()> {
        invite_routes(self, auth, encrypted)
    }
}

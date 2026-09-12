mod invite_http;
mod invite_persistence;
mod invite_runtime;
mod invite_store;
mod invite_token;

use std::time::Duration;

use crate::{persistence::ExecuteSqlAsync, users::CreateUser};

use self::{invite_persistence::InvitePersistence, invite_store::InviteStore};

pub use self::{
    invite_http::invite_routes,
    invite_persistence::migrate_invite_persistence,
    invite_runtime::{Invite, InviteClient, InviteRuntime},
    invite_store::StoreInvites,
    invite_token::InviteToken,
};

// Integrate invite store with invite runtime. We do it here, because we want the submodules to be
// independent from each other. Yet, the decision still belongs to the invites module.
impl InviteRuntime {
    pub fn new<U>(
        expiry: Duration,
        persistence: impl ExecuteSqlAsync + Send + Sync + 'static,
        users: U,
    ) -> Self
    where
        U: CreateUser + Send + 'static,
    {
        Self::with(InviteStore::new(persistence, expiry), users)
    }
}

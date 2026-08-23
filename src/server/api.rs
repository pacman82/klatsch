use crate::{invites::invite_routes, server::Routes, users::AuthenticateRequest};
use axum::Router;
use tokio::sync::watch;

pub fn api_router<C, U>(
    chat: C,
    users: U,
    shutting_down: watch::Receiver<bool>,
    encrypted: bool,
) -> Router
where
    C: Routes,
    U: AuthenticateRequest + Routes + Send + Sync + Clone + 'static,
{
    let auth = users.clone();

    Router::new()
        .merge(chat.routes(auth.clone(), shutting_down.clone(), encrypted))
        .merge(users.routes(auth, shutting_down, encrypted))
        .merge(invite_routes())
}

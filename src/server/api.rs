use super::session_cookie::session_routes;
use crate::{
    chat::{Chat, chat_routes},
    http::AuthenticateRequest,
    invites::invite_routes,
    sessions::SessionLifecycle,
    user::{AuthenticateUser, Users, user_routes},
};
use axum::Router;
use tokio::sync::watch;

pub fn api_router<C, U, S>(
    chat: C,
    users: U,
    sessions: S,
    shutting_down: watch::Receiver<bool>,
    encrypted: bool,
) -> Router
where
    C: Chat + Send + Sync + Clone + 'static,
    U: Users + AuthenticateUser + Send + Sync + Clone + 'static,
    S: SessionLifecycle + AuthenticateRequest + Send + Sync + Clone + 'static,
{
    Router::new()
        .merge(chat_routes(chat, sessions.clone(), shutting_down))
        .merge(session_routes(users.clone(), sessions.clone(), encrypted))
        .merge(user_routes(users, sessions))
        .merge(invite_routes())
}

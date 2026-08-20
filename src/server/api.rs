use super::session_cookie::login_routes;
use crate::{
    authentication::{AuthService, AuthenticateRequest},
    chat::{Chat, chat_routes},
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
    let auth_service = AuthService::new(users.clone(), sessions.clone());

    Router::new()
        .merge(chat_routes(chat, sessions.clone(), shutting_down))
        .merge(login_routes(auth_service, encrypted))
        .merge(user_routes(users, sessions))
        .merge(invite_routes())
}

use crate::{
    chat::{Chat, chat_routes},
    sessions::{SessionLifecycle, SessionLookup},
    user::{Users, user_routes},
};
use axum::Router;
use tokio::sync::watch;

pub fn api_router<C, U, S>(
    chat: C,
    users: U,
    sessions: S,
    shutting_down: watch::Receiver<bool>,
) -> Router
where
    C: Chat + Send + Sync + Clone + 'static,
    U: Users + Send + Sync + Clone + 'static,
    S: SessionLifecycle + SessionLookup + Send + Sync + Clone + 'static,
{
    let router = Router::new();
    let router = router
        .merge(chat_routes(chat, sessions.clone(), shutting_down))
        .merge(user_routes(users, sessions));

    router
}

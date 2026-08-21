use crate::{
    authentication::{AuthService, AuthenticateRequest},
    invites::invite_routes,
    server::Routes,
    sessions::SessionLifecycle,
    user::{AuthenticateUser, Users},
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
    C: Routes,
    U: Users + AuthenticateUser + Routes + Send + Sync + Clone + 'static,
    S: SessionLifecycle + AuthenticateRequest + Send + Sync + Clone + 'static,
{
    let auth_service = AuthService::new(users.clone(), sessions.clone());

    Router::new()
        .merge(
            auth_service
                .clone()
                .routes(auth_service.clone(), shutting_down.clone(), encrypted),
        )
        .merge(chat.routes(auth_service.clone(), shutting_down.clone(), encrypted))
        .merge(users.routes(auth_service, shutting_down, encrypted))
        .merge(invite_routes())
}

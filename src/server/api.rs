use crate::{
    authentication::AuthenticateRequest,
    invites::invite_routes,
    server::Routes,
    user::{AuthenticateUser, Users},
};
use axum::Router;
use tokio::sync::watch;

pub fn api_router<C, U, A>(
    chat: C,
    users: U,
    auth_service: A,
    shutting_down: watch::Receiver<bool>,
    encrypted: bool,
) -> Router
where
    C: Routes,
    U: Users + AuthenticateUser + Routes + Send + Sync + Clone + 'static,
    A: AuthenticateRequest + Routes + Send + Sync + Clone + 'static,
{
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

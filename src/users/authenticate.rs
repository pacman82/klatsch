use axum::{
    extract::FromRequestParts,
    http::{StatusCode, request::Parts},
};
use axum_extra::extract::CookieJar;

use crate::http::HttpError;

use super::{AuthenticateSession, SessionId, UserId};

pub trait AuthenticateRequest {
    fn authenticate_request(
        &self,
        parts: &Parts,
    ) -> impl Future<Output = Result<UserId, HttpError>> + Send;
}

pub struct AuthenticatedUser(pub UserId);

impl<S> FromRequestParts<S> for AuthenticatedUser
where
    S: AuthenticateRequest + Send + Sync,
{
    type Rejection = HttpError;

    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, HttpError> {
        state
            .authenticate_request(parts)
            .await
            .map(AuthenticatedUser)
    }
}

// We authenticate requests, using sessions.
impl<T> AuthenticateRequest for T
where
    T: AuthenticateSession + Sync,
{
    fn authenticate_request(
        &self,
        parts: &Parts,
    ) -> impl Future<Output = Result<UserId, HttpError>> + Send {
        let jar = CookieJar::from_headers(&parts.headers);
        let session_id = jar
            .get("session")
            .ok_or(HttpError {
                status_code: StatusCode::UNAUTHORIZED,
                message: "Missing session".into(),
            })
            .and_then(|c| {
                c.value().parse::<SessionId>().map_err(|_| HttpError {
                    status_code: StatusCode::UNAUTHORIZED,
                    message: "Invalid session".into(),
                })
            });
        async move {
            let session_id = session_id?;
            self.authenticate(session_id).await.ok_or(HttpError {
                status_code: StatusCode::UNAUTHORIZED,
                message: "Unknown session".into(),
            })
        }
    }
}

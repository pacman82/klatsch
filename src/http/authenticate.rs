use crate::user::UserId;
use axum::{extract::FromRequestParts, http::request::Parts};

use super::HttpError;

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

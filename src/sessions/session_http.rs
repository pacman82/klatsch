use super::{SessionId, SessionLookup};
use crate::{http::HttpError, user::UserId};
use axum::{
    extract::FromRequestParts,
    http::{StatusCode, request::Parts},
};
use axum_extra::extract::CookieJar;

/// Extractor for Axum route handlers. Extracts User Id from Session copy given that the the router
/// state is [`super::SessionLookup`].
pub struct AuthenticatedUser(pub UserId);

impl<S> FromRequestParts<S> for AuthenticatedUser
where
    S: SessionLookup + Clone + Send + Sync,
{
    type Rejection = HttpError;

    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, HttpError> {
        let jar = CookieJar::from_headers(&parts.headers);
        let session_id = jar
            .get("session")
            .ok_or(HttpError {
                status_code: StatusCode::UNAUTHORIZED,
                message: "Missing session".into(),
            })?
            .value()
            .parse::<SessionId>()
            .map_err(|_| HttpError {
                status_code: StatusCode::UNAUTHORIZED,
                message: "Invalid session".into(),
            })?;
        let user_id = state.clone().lookup(session_id).await.ok_or(HttpError {
            status_code: StatusCode::UNAUTHORIZED,
            message: "Unknown session".into(),
        })?;
        Ok(AuthenticatedUser(user_id))
    }
}

#[cfg(test)]
mod tests {
    use axum::{Router, body::Body, http::Request, http::StatusCode, routing::post};
    use http_body_util::BodyExt as _;
    use tower::ServiceExt as _;
    use uuid::Uuid;

    use crate::{
        sessions::{SessionId, SessionLookup},
        user::UserId,
    };

    use super::AuthenticatedUser;

    const SOME_SESSION_ID: SessionId = SessionId::from_uuid(Uuid::from_u128(1));

    fn authenticated_user_app(
        sessions: impl SessionLookup + Clone + Send + Sync + 'static,
    ) -> Router {
        Router::new()
            .route(
                "/test",
                post(|AuthenticatedUser(_): AuthenticatedUser| async {}),
            )
            .with_state(sessions)
    }

    #[tokio::test]
    async fn rejects_missing_session() {
        // Given
        let app = authenticated_user_app(double_trait::Dummy);

        // When
        let response = app
            .oneshot(Request::post("/test").body(Body::empty()).unwrap())
            .await
            .unwrap();

        // Then
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn rejects_unknown_session() {
        // Given
        #[derive(Clone)]
        struct EmptySessionsStub;
        impl SessionLookup for EmptySessionsStub {
            async fn lookup(&self, _session_id: SessionId) -> Option<UserId> {
                None
            }
        }
        let app = authenticated_user_app(EmptySessionsStub);

        // When
        let response = app
            .oneshot(
                Request::post("/test")
                    .header("cookie", format!("session={SOME_SESSION_ID}"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        // Then
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn resolves_user_id_from_session() {
        // Given
        #[derive(Clone)]
        struct SessionsStub;
        impl SessionLookup for SessionsStub {
            async fn lookup(&self, _session_id: SessionId) -> Option<UserId> {
                Some(UserId::ALICE)
            }
        }
        let app =
            Router::new()
                .route(
                    "/test",
                    post(|AuthenticatedUser(user_id): AuthenticatedUser| async move {
                        user_id.to_string()
                    }),
                )
                .with_state(SessionsStub);

        // When
        let response = app
            .oneshot(
                Request::post("/test")
                    .header("cookie", format!("session={SOME_SESSION_ID}"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        // Then
        assert_eq!(response.status(), StatusCode::OK);
        let body = response.into_body().collect().await.unwrap().to_bytes();
        assert_eq!(body, UserId::ALICE.to_string().as_bytes());
    }
}

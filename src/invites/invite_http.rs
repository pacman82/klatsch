use axum::{
    Json, Router,
    extract::Path,
    response::{IntoResponse, Redirect},
    routing::{get, post},
};
use uuid::Uuid;

use super::InviteToken;

pub fn invite_routes() -> Router {
    Router::new()
        .route("/api/v0/invites", post(create_invite))
        .route("/invite/{token}", get(claim_invite))
}

async fn create_invite() -> Json<InviteToken> {
    // Hardcoded for now — real per-request token generation is a later increment.
    Json(InviteToken::from_uuid(Uuid::nil()))
}

/// Forwards to the signup page
async fn claim_invite(Path(_token): Path<InviteToken>) -> impl IntoResponse {
    Redirect::to("/signup")
}

#[cfg(test)]
mod tests {
    use axum::{body::Body, http::Request};
    use http_body_util::BodyExt as _;
    use tower::ServiceExt as _;
    use uuid::Uuid;

    use super::{InviteToken, invite_routes};

    #[tokio::test]
    async fn create_invite_returns_hardcoded_nil_token() {
        // When
        let response = invite_routes()
            .oneshot(
                Request::post("/api/v0/invites")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        // Then
        let body = response.into_body().collect().await.unwrap().to_bytes();
        let token: InviteToken = serde_json::from_slice(&body).unwrap();
        assert_eq!(token, InviteToken::from_uuid(Uuid::nil()));
    }

    #[tokio::test]
    async fn claiming_invite_redirects_to_signup() {
        // Given
        let token = InviteToken::from_uuid(Uuid::nil());

        // When
        let response = invite_routes()
            .oneshot(
                Request::get(format!("/invite/{token}"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        // Then
        assert_eq!(response.status(), axum::http::StatusCode::SEE_OTHER);
        assert_eq!(response.headers().get("location").unwrap(), "/signup");
    }
}

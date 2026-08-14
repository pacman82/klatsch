use axum::{Json, Router, routing::post};
use uuid::Uuid;

use super::InviteToken;

pub fn invite_routes() -> Router {
    Router::new().route("/api/v0/invites", post(create_invite))
}

async fn create_invite() -> Json<InviteToken> {
    // Hardcoded for now — real per-request token generation is a later increment.
    Json(InviteToken::from_uuid(Uuid::nil()))
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
}

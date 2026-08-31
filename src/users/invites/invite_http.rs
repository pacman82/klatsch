use axum::{
    Json, Router,
    extract::{Path, State},
    http::StatusCode,
    response::{IntoResponse, Redirect},
    routing::{get, post},
};

use crate::http::HttpError;

use super::{Invite, InviteToken};

pub fn invite_routes<I>(invitations_api: I) -> Router
where
    I: Invite + Clone + Send + Sync + 'static,
{
    Router::new()
        .route("/api/v0/invites", post(create_invite::<I>))
        .with_state(invitations_api)
        .route("/invite/{token}", get(claim_invite))
}

async fn create_invite<I>(State(mut invite): State<I>) -> Result<Json<InviteToken>, HttpError>
where
    I: Invite,
{
    let invitation = invite.new_invite().map_err(|_| HttpError {
        status_code: StatusCode::INTERNAL_SERVER_ERROR,
        message: "Internal Error".into(),
    })?;
    Ok(Json(invitation))
}

/// Forwards to the signup page
async fn claim_invite(Path(_token): Path<InviteToken>) -> impl IntoResponse {
    Redirect::to("/signup")
}

#[cfg(test)]
mod tests {
    use axum::{body::Body, http::Request};
    use double_trait::Dummy;
    use http_body_util::BodyExt as _;
    use reqwest::StatusCode;
    use tower::ServiceExt as _;

    use super::{Invite, InviteToken, invite_routes};

    #[tokio::test]
    async fn create_invite() {
        // Given
        #[derive(Clone)]
        struct InviteStub;
        impl Invite for InviteStub {
            fn new_invite(&mut self) -> anyhow::Result<InviteToken> {
                Ok(InviteToken::ALPHA)
            }
        }

        // When
        let response = invite_routes(InviteStub)
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
        assert_eq!(token, InviteToken::ALPHA);
    }

    #[tokio::test]
    async fn claiming_invite_redirects_to_signup() {
        // Given
        let token = InviteToken::nil();

        // When
        let response = invite_routes(Dummy)
            .oneshot(
                Request::get(format!("/invite/{token}"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        // Then
        assert_eq!(response.status(), StatusCode::SEE_OTHER);
        assert_eq!(response.headers().get("location").unwrap(), "/signup");
    }
}

use axum::{
    Json, Router,
    extract::{Path, State},
    http::StatusCode,
    response::Redirect,
    routing::{get, post},
};
use axum_extra::extract::{
    CookieJar,
    cookie::{Cookie, SameSite},
};

use crate::http::HttpError;

use super::{Invite, InviteToken};

pub fn invite_routes<I>(invitations_api: I, encrypted: bool) -> Router
where
    I: Invite + Clone + Send + Sync + 'static,
{
    let create_invite_route = Router::new()
        .route("/api/v0/invites", post(create_invite::<I>))
        .with_state(invitations_api.clone());
    let claim_invite_route = Router::new()
        .route("/invite/{token}", get(claim_invite::<I>))
        .with_state(ClaimInviteState {
            invite: invitations_api,
            encrypted,
        });
    create_invite_route.merge(claim_invite_route)
}

/// Distinct from the "session" cookie: it authenticates a claimed invite, not a user, since no
/// user exists yet at this point in the signup flow.
fn invite_cookie(token: InviteToken, encrypted: bool) -> Cookie<'static> {
    Cookie::build(("invite", token.to_string()))
        .http_only(true)
        .same_site(SameSite::Strict)
        .secure(encrypted)
        .build()
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

#[derive(Clone)]
struct ClaimInviteState<I> {
    invite: I,
    /// Wether we only send the invite cookie exclusively over https.
    encrypted: bool,
}

async fn claim_invite<I>(
    jar: CookieJar,
    State(ClaimInviteState {
        mut invite,
        encrypted,
    }): State<ClaimInviteState<I>>,
    Path(token): Path<InviteToken>,
) -> Result<(CookieJar, Redirect), HttpError>
where
    I: Invite,
{
    let claimed = invite.claim(token).map_err(|_| HttpError {
        status_code: StatusCode::INTERNAL_SERVER_ERROR,
        message: "Internal Error".into(),
    })?;
    if claimed {
        Ok((
            jar.add(invite_cookie(token, encrypted)),
            Redirect::to("/signup"),
        ))
    } else {
        Ok((jar, Redirect::to("/invite-invalid")))
    }
}

#[cfg(test)]
mod tests {
    use axum::{body::Body, http::Request};
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
        let response = invite_routes(InviteStub, true)
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
    async fn claim_invite() {
        // Given a valid invite
        #[derive(Clone)]
        struct InviteMock;
        impl Invite for InviteMock {
            fn claim(&mut self, invitation: InviteToken) -> anyhow::Result<bool> {
                assert_eq!(invitation, InviteToken::ALPHA);
                Ok(true)
            }
        }
        let token = InviteToken::ALPHA;

        // When claiming the invite
        let response = invite_routes(InviteMock, true)
            .oneshot(
                Request::get(format!("/invite/{token}"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        // Then it is forwaret to the signup page and an invite cookie is set
        assert_eq!(response.status(), StatusCode::SEE_OTHER);
        assert_eq!(response.headers().get("location").unwrap(), "/signup");
        let cookie = response
            .headers()
            .get("set-cookie")
            .unwrap()
            .to_str()
            .unwrap();
        assert!(cookie.contains(&format!("invite={token}")));
        assert!(cookie.contains("HttpOnly"));
        assert!(cookie.contains("SameSite=Strict"));
    }

    #[tokio::test]
    async fn claiming_invalid_invite() {
        // Given
        #[derive(Clone)]
        struct InviteStub;
        impl Invite for InviteStub {
            fn claim(&mut self, _invitation: InviteToken) -> anyhow::Result<bool> {
                Ok(false)
            }
        }
        let token = InviteToken::nil();

        // When claiming an invalid invite
        let response = invite_routes(InviteStub, true)
            .oneshot(
                Request::get(format!("/invite/{token}"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        // Then redirect to the invalid-invite page without setting an invite cookie
        assert_eq!(response.status(), StatusCode::SEE_OTHER);
        assert_eq!(
            response.headers().get("location").unwrap(),
            "/invite-invalid"
        );
        assert!(response.headers().get("set-cookie").is_none());
    }
}

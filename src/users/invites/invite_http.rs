use axum::{
    Json, Router,
    extract::{Path, State},
    http::{StatusCode, request::Parts},
    response::Redirect,
    routing::{get, post},
};
use axum_extra::extract::{
    CookieJar,
    cookie::{Cookie, SameSite},
};
use serde::Deserialize;

use crate::{
    http::HttpError,
    users::{
        AuthenticateRequest, AuthenticatedUser, SessionLifecycle, UserId, UsersError,
        login_routes::session_cookie,
    },
};

use super::{Invite, InviteToken};

pub fn invite_routes<I, S>(invitations_api: I, sessions: S, encrypted: bool) -> Router
where
    I: Invite + Clone + Send + Sync + 'static,
    S: AuthenticateRequest + SessionLifecycle + Send + Sync + Clone + 'static,
{
    let create_invite_route = Router::new()
        .route("/api/v0/invites", post(create_invite::<I, S>))
        .with_state(CreateInviteState {
            invite: invitations_api.clone(),
            sessions: sessions.clone(),
        });
    let claim_invite_route = Router::new()
        .route("/invite/{token}", get(claim_invite::<I>))
        .with_state(ClaimInviteState {
            invite: invitations_api.clone(),
            encrypted,
        });
    let signup_route = Router::new()
        .route("/api/v0/signup", post(signup::<I, S>))
        .with_state(SignupState {
            invite: invitations_api,
            sessions,
            encrypted,
        });
    create_invite_route
        .merge(claim_invite_route)
        .merge(signup_route)
}

/// Distinct from the "session" cookie: it authenticates a claimed invite, not a user, since no
/// user exists yet at this point in the signup flow.
fn invite_cookie(token: InviteToken, encrypted: bool) -> Cookie<'static> {
    Cookie::build(("invite", token.to_string()))
        .http_only(true)
        .same_site(SameSite::Strict)
        .secure(encrypted)
        // Without this, browsers default the path to the directory of the request that set the
        // cookie (`/invite`), which would keep it from being sent along with `/api/v0/signup`.
        .path("/")
        .build()
}

#[derive(Clone)]
struct CreateInviteState<I, S> {
    invite: I,
    sessions: S,
}

impl<I: Send + Sync, S: AuthenticateRequest + Sync> AuthenticateRequest
    for CreateInviteState<I, S>
{
    fn authenticate_request(
        &self,
        parts: &Parts,
    ) -> impl Future<Output = Result<UserId, HttpError>> + Send {
        self.sessions.authenticate_request(parts)
    }
}

async fn create_invite<I, S>(
    AuthenticatedUser(_): AuthenticatedUser,
    State(CreateInviteState { mut invite, .. }): State<CreateInviteState<I, S>>,
) -> Result<Json<InviteToken>, HttpError>
where
    I: Invite,
    S: AuthenticateRequest + Sync,
{
    let invitation = invite.new_invite().await.map_err(|_| HttpError {
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
    // Only checked here, not claimed — merely following the link should not consume the invite.
    // It is actually claimed once the signup form is submitted.
    let valid = invite.is_valid(token).await.map_err(|_| HttpError {
        status_code: StatusCode::INTERNAL_SERVER_ERROR,
        message: "Internal Error".into(),
    })?;
    if valid {
        Ok((
            jar.add(invite_cookie(token, encrypted)),
            Redirect::to("/signup"),
        ))
    } else {
        Ok((jar, Redirect::to("/invite-invalid")))
    }
}

#[derive(Clone)]
struct SignupState<I, S> {
    invite: I,
    sessions: S,
    encrypted: bool,
}

#[derive(Deserialize)]
struct SignupBody {
    name: String,
    password: String,
}

async fn signup<I, S>(
    jar: CookieJar,
    State(SignupState {
        mut invite,
        mut sessions,
        encrypted,
    }): State<SignupState<I, S>>,
    Json(body): Json<SignupBody>,
) -> Result<(CookieJar, Json<UserId>), HttpError>
where
    I: Invite,
    S: SessionLifecycle,
{
    let token = jar
        .get("invite")
        .and_then(|c| c.value().parse().ok())
        .ok_or(UsersError::MissingInvite)?;
    let user_id = invite.claim(token, body.name, body.password).await?;
    let session_id = sessions.create(user_id).await;
    Ok((
        jar.add(session_cookie(session_id, encrypted)),
        Json(user_id),
    ))
}

#[cfg(test)]
mod tests {
    use axum::{body::Body, http::Request, http::request::Parts};
    use http_body_util::BodyExt as _;
    use reqwest::StatusCode;
    use tower::ServiceExt as _;

    use crate::users::{AuthenticateRequest, SessionId, SessionLifecycle, UserId, UsersError};

    use super::{HttpError, Invite, InviteToken, invite_routes};

    #[derive(Clone)]
    struct AuthStub;
    impl AuthenticateRequest for AuthStub {
        async fn authenticate_request(&self, _parts: &Parts) -> Result<UserId, HttpError> {
            Ok(UserId::ALICE)
        }
    }
    impl SessionLifecycle for AuthStub {}

    #[tokio::test]
    async fn create_invite() {
        // Given
        #[derive(Clone)]
        struct InviteStub;
        impl Invite for InviteStub {
            async fn new_invite(&mut self) -> anyhow::Result<InviteToken> {
                Ok(InviteToken::ALPHA)
            }
        }

        // When
        let response = invite_routes(InviteStub, AuthStub, true)
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
    async fn create_invite_requires_authentication() {
        // Given
        #[derive(Clone)]
        struct RejectingAuth;
        impl AuthenticateRequest for RejectingAuth {
            async fn authenticate_request(&self, _parts: &Parts) -> Result<UserId, HttpError> {
                Err(HttpError {
                    status_code: reqwest::StatusCode::UNAUTHORIZED,
                    message: "Missing session".into(),
                })
            }
        }
        impl SessionLifecycle for RejectingAuth {}

        // When creating an invite without a valid session
        let response = invite_routes(double_trait::Dummy, RejectingAuth, true)
            .oneshot(
                Request::post("/api/v0/invites")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        // Then it is rejected
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn invite_link_redirects_to_signup() {
        // Given a valid invite
        #[derive(Clone)]
        struct InviteMock;
        impl Invite for InviteMock {
            async fn is_valid(&mut self, invitation: InviteToken) -> anyhow::Result<bool> {
                assert_eq!(invitation, InviteToken::ALPHA);
                Ok(true)
            }
        }
        let token = InviteToken::ALPHA;

        // When claiming the invite
        let response = invite_routes(InviteMock, AuthStub, true)
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
        // Without an explicit path, browsers default to the directory of the request that set the
        // cookie (`/invite`), which would keep it from being sent along with `/api/v0/signup`.
        assert!(cookie.contains("Path=/"));
    }

    #[tokio::test]
    async fn claiming_invalid_invite() {
        // Given
        #[derive(Clone)]
        struct InviteStub;
        impl Invite for InviteStub {
            async fn is_valid(&mut self, _invitation: InviteToken) -> anyhow::Result<bool> {
                Ok(false)
            }
        }
        let token = InviteToken::nil();

        // When claiming an invalid invite
        let response = invite_routes(InviteStub, AuthStub, true)
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

    #[tokio::test]
    async fn signup_rejects_missing_invite_cookie() {
        // Given no invite cookie on the request
        let response = invite_routes(double_trait::Dummy, AuthStub, true)
            .oneshot(
                Request::post("/api/v0/signup")
                    .header("content-type", "application/json")
                    .body(Body::from(r#"{"name": "Alice", "password": "secret"}"#))
                    .unwrap(),
            )
            .await
            .unwrap();

        // Then
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn claim_invite_via_signup() {
        // Given a valid invite
        #[derive(Clone)]
        struct InviteMock;
        impl Invite for InviteMock {
            async fn claim(
                &mut self,
                invitation: InviteToken,
                name: String,
                password: String,
            ) -> Result<UserId, UsersError> {
                assert_eq!(invitation, InviteToken::ALPHA);
                assert_eq!(name, "Alice");
                assert_eq!(password, "secret");
                Ok(UserId::ALICE)
            }
        }
        #[derive(Clone)]
        struct SessionsStub;
        impl AuthenticateRequest for SessionsStub {
            async fn authenticate_request(&self, _parts: &Parts) -> Result<UserId, HttpError> {
                Ok(UserId::ALICE)
            }
        }
        impl SessionLifecycle for SessionsStub {
            async fn create(&mut self, user_id: UserId) -> SessionId {
                assert_eq!(user_id, UserId::ALICE);
                SessionId::ALICE
            }
        }

        // When
        let response = invite_routes(InviteMock, SessionsStub, true)
            .oneshot(
                Request::post("/api/v0/signup")
                    .header("content-type", "application/json")
                    .header("cookie", format!("invite={}", InviteToken::ALPHA))
                    .body(Body::from(r#"{"name": "Alice", "password": "secret"}"#))
                    .unwrap(),
            )
            .await
            .unwrap();

        // Then a session cookie is set and the new user id is returned
        assert_eq!(response.status(), StatusCode::OK);
        let cookie = response
            .headers()
            .get("set-cookie")
            .unwrap()
            .to_str()
            .unwrap();
        assert!(cookie.contains(&format!("session={}", SessionId::ALICE)));
        let body = response.into_body().collect().await.unwrap().to_bytes();
        let id: UserId = serde_json::from_slice(&body).unwrap();
        assert_eq!(id, UserId::ALICE);
    }

    #[tokio::test]
    async fn signup_rejection_is_forwarded_as_http_error() {
        // Given an invite that fails to claim
        #[derive(Clone)]
        struct InvalidInvite;
        impl Invite for InvalidInvite {
            async fn claim(
                &mut self,
                _invitation: InviteToken,
                _name: String,
                _password: String,
            ) -> Result<UserId, UsersError> {
                Err(UsersError::InvalidInvite)
            }
        }

        // When
        let response = invite_routes(InvalidInvite, AuthStub, true)
            .oneshot(
                Request::post("/api/v0/signup")
                    .header("content-type", "application/json")
                    .header("cookie", format!("invite={}", InviteToken::ALPHA))
                    .body(Body::from(r#"{"name": "Alice", "password": "secret"}"#))
                    .unwrap(),
            )
            .await
            .unwrap();

        // Then
        assert_eq!(response.status(), StatusCode::FORBIDDEN);
    }
}

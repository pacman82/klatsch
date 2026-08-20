use axum::{Json, Router, extract::State, routing::post};
use axum_extra::extract::{
    CookieJar,
    cookie::{Cookie, SameSite},
};
use serde::Deserialize;

use crate::{authentication::Login, http::HttpError, sessions::SessionId, user::UserId};

/// State for the routes that create a session cookie. `encrypted` reflects whether the connection
/// to the client is encrypted, be it terminated by Klatsch itself or by a reverse proxy in front
/// of it, and controls the cookie's `Secure` attribute.
#[derive(Clone)]
struct SessionState<L> {
    auth_service: L,
    encrypted: bool,
}

pub fn login_routes<L>(auth_service: L, encrypted: bool) -> Router
where
    L: Login + Send + Sync + Clone + 'static,
{
    let state = SessionState {
        auth_service: auth_service.clone(),
        encrypted,
    };
    Router::new()
        .route("/api/v0/login", post(login::<L>))
        .route("/api/v0/signup", post(signup::<L>))
        .route("/api/v0/logout", post(logout::<L>))
        .with_state(state.clone())
}

fn session_cookie(session_id: SessionId, encrypted: bool) -> Cookie<'static> {
    Cookie::build(("session", session_id.to_string()))
        // Http only prevents JavaScript from interacting with the session cookie. Hardening against
        // Cross site scripting attacks
        .http_only(true)
        // Hardening against cross site request forgery. Prevents other sites from abusing the trust
        // we put in the users browser.
        .same_site(SameSite::Strict)
        // Secure `true` would prevent this cookie from being transported via http instead of https.
        .secure(encrypted)
        .build()
}

async fn logout<L>(
    jar: CookieJar,
    State(SessionState {
        mut auth_service,
        encrypted: _,
    }): State<SessionState<L>>,
) -> CookieJar
where
    L: Login,
{
    if let Some(session_id) = jar
        .get("session")
        .and_then(|c| c.value().parse::<SessionId>().ok())
    {
        auth_service.logout(session_id).await;
    }
    jar.remove(
        Cookie::build("session")
            .http_only(true)
            .same_site(SameSite::Strict)
            .build(),
    )
}

#[derive(Deserialize)]
struct LoginBody {
    name: String,
    password: String,
}

async fn signup<L>(
    jar: CookieJar,
    State(SessionState {
        mut auth_service,
        encrypted,
    }): State<SessionState<L>>,
    Json(body): Json<LoginBody>,
) -> Result<(CookieJar, Json<UserId>), HttpError>
where
    L: Login,
{
    let (session_id, user_id) = auth_service.signup(body.name, body.password).await?;
    Ok((
        jar.add(session_cookie(session_id, encrypted)),
        Json(user_id),
    ))
}

async fn login<L>(
    jar: CookieJar,
    State(SessionState {
        mut auth_service,
        encrypted,
    }): State<SessionState<L>>,
    Json(body): Json<LoginBody>,
) -> Result<(CookieJar, Json<UserId>), HttpError>
where
    L: Login,
{
    let (session_id, user_id) = auth_service.login(body.name, body.password).await?;
    Ok((
        jar.add(session_cookie(session_id, encrypted)),
        Json(user_id),
    ))
}

#[cfg(test)]
mod tests {
    use std::{
        mem::take,
        sync::{Arc, Mutex},
    };

    use axum::{Router, body::Body, http::Request, http::StatusCode, routing::post};
    use double_trait::Dummy;
    use http_body_util::BodyExt as _;
    use serde_json::from_slice;
    use tower::ServiceExt as _;
    use uuid::Uuid;

    use crate::{
        authentication::{AuthenticatedUser, Login},
        server::session_cookie::login_routes,
        sessions::{AuthenticateSession, SessionId},
        user::{AuthenticationError, UserId, UsersError},
    };

    const SOME_SESSION_ID: SessionId = SessionId::from_uuid(Uuid::from_u128(1));

    fn authenticated_user_app(
        sessions: impl AuthenticateSession + Clone + Send + Sync + 'static,
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
        let app = authenticated_user_app(Dummy);

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
        impl AuthenticateSession for EmptySessionsStub {
            async fn authenticate(&self, _session_id: SessionId) -> Option<UserId> {
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
        impl AuthenticateSession for SessionsStub {
            async fn authenticate(&self, _session_id: SessionId) -> Option<UserId> {
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

    #[tokio::test]
    async fn signup_forwards_credentials() {
        // Given
        let spy = LoginSpy::default();
        let app = login_routes(spy.clone(), true);

        // When
        let response = app
            .oneshot(
                Request::post("/api/v0/signup")
                    .header("content-type", "application/json")
                    .body(Body::from(r#"{"name": "Alice", "password": "secret"}"#))
                    .unwrap(),
            )
            .await
            .unwrap();

        // Then
        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(
            spy.take_signup_record(),
            [("Alice".to_owned(), "secret".to_owned())]
        );
    }

    #[tokio::test]
    async fn successful_signup() {
        // Given
        #[derive(Clone)]
        struct SignupStub;
        impl Login for SignupStub {
            async fn signup(
                &mut self,
                _name: String,
                _password: String,
            ) -> Result<(SessionId, UserId), UsersError> {
                Ok((SessionId::ALICE, UserId::ALICE))
            }
        }
        let app = login_routes(SignupStub, true);

        // When
        let response = app
            .oneshot(
                Request::post("/api/v0/signup")
                    .header("content-type", "application/json")
                    .body(Body::from(r#"{"name": "Alice", "password": "secret"}"#))
                    .unwrap(),
            )
            .await
            .unwrap();

        // Then a session cookie is set and the new user ID is returned
        let cookie = response
            .headers()
            .get("set-cookie")
            .unwrap()
            .to_str()
            .unwrap();
        assert!(cookie.contains(&format!("session={}", SessionId::ALICE)));
        assert!(cookie.contains("HttpOnly"));
        assert!(cookie.contains("SameSite=Strict"));

        let body = response.into_body().collect().await.unwrap().to_bytes();
        let id: UserId = from_slice(&body).unwrap();
        assert_eq!(id, UserId::ALICE);
    }

    #[tokio::test]
    async fn login_marks_cookie_secure_when_connection_is_encrypted() {
        // Given
        #[derive(Clone)]
        struct LoginStub;
        impl Login for LoginStub {
            async fn login(
                &mut self,
                _name: String,
                _password: String,
            ) -> Result<(SessionId, UserId), AuthenticationError> {
                Ok((SessionId::nil(), UserId::nil()))
            }
        }
        let app = login_routes(LoginStub, true);

        // When
        let response = app
            .oneshot(
                Request::post("/api/v0/login")
                    .header("content-type", "application/json")
                    .body(Body::from(r#"{"name": "dummy", "password": "dummy"}"#))
                    .unwrap(),
            )
            .await
            .unwrap();

        // Then
        let cookie = response
            .headers()
            .get("set-cookie")
            .unwrap()
            .to_str()
            .unwrap();
        assert!(cookie.contains("Secure"));
    }

    #[tokio::test]
    async fn login_omits_secure_flag_when_connection_is_not_encrypted() {
        // Given
        // Given
        #[derive(Clone)]
        struct LoginStub;
        impl Login for LoginStub {
            async fn login(
                &mut self,
                _name: String,
                _password: String,
            ) -> Result<(SessionId, UserId), AuthenticationError> {
                Ok((SessionId::nil(), UserId::nil()))
            }
        }
        let app = login_routes(LoginStub, false);

        // When
        let response = app
            .oneshot(
                Request::post("/api/v0/login")
                    .header("content-type", "application/json")
                    .body(Body::from(r#"{"name": "dummy", "password": "dummy"}"#))
                    .unwrap(),
            )
            .await
            .unwrap();

        // Then
        let cookie = response
            .headers()
            .get("set-cookie")
            .unwrap()
            .to_str()
            .unwrap();
        assert!(!cookie.contains("Secure"));
    }

    #[tokio::test]
    async fn logout_clears_session_cookie() {
        // Given
        let app = login_routes(Dummy, true);

        // When
        let response = app
            .oneshot(
                Request::post("/api/v0/logout")
                    .header("cookie", format!("session={SOME_SESSION_ID}"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        // Then
        assert_eq!(response.status(), StatusCode::OK);
        let cookie = response
            .headers()
            .get("set-cookie")
            .unwrap()
            .to_str()
            .unwrap();
        assert!(cookie.contains("session="));
        assert!(cookie.contains("Max-Age=0"));
        assert!(cookie.contains("HttpOnly"));
        assert!(cookie.contains("SameSite=Strict"));
    }

    #[tokio::test]
    async fn logout_destroys_session() {
        // Given
        #[derive(Clone, Default)]
        struct LogoutSpy {
            destroyed: Arc<Mutex<Vec<SessionId>>>,
        }
        impl Login for LogoutSpy {
            async fn logout(&mut self, session_id: SessionId) {
                self.destroyed.lock().unwrap().push(session_id);
            }
        }
        let spy = LogoutSpy::default();
        let app = login_routes(spy.clone(), true);

        // When
        app.oneshot(
            Request::post("/api/v0/logout")
                .header("cookie", format!("session={SOME_SESSION_ID}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

        // Then
        assert_eq!(*spy.destroyed.lock().unwrap(), [SOME_SESSION_ID]);
    }

    #[tokio::test]
    async fn login_forwards_credentials() {
        // Given
        let spy = LoginSpy::default();
        let app = login_routes(spy.clone(), true);

        // When
        let response = app
            .oneshot(
                Request::post("/api/v0/login")
                    .header("content-type", "application/json")
                    .body(Body::from(r#"{"name": "Alice", "password": "secret"}"#))
                    .unwrap(),
            )
            .await
            .unwrap();

        // Then
        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(
            spy.take_login_record(),
            [("Alice".to_owned(), "secret".to_owned())]
        );
    }

    #[tokio::test]
    async fn successful_login() {
        // Given a user Alice
        #[derive(Clone)]
        struct AuthenticateUserStub;
        impl Login for AuthenticateUserStub {
            async fn login(
                &mut self,
                _name: String,
                _password: String,
            ) -> Result<(SessionId, UserId), AuthenticationError> {
                Ok((SessionId::ALICE, UserId::ALICE))
            }
        }
        let app = login_routes(AuthenticateUserStub, true);

        // When she successfully logs in
        let response = app
            .oneshot(
                Request::post("/api/v0/login")
                    .header("content-type", "application/json")
                    .body(Body::from(r#"{"name": "dummy", "password": "dummy"}"#))
                    .unwrap(),
            )
            .await
            .unwrap();

        // Then a session cookie is set and her user ID is returned
        let cookie = response
            .headers()
            .get("set-cookie")
            .unwrap()
            .to_str()
            .unwrap();
        assert!(cookie.contains(&format!("session={}", SessionId::ALICE)));
        assert!(cookie.contains("HttpOnly"));
        assert!(cookie.contains("SameSite=Strict"));

        let body = response.into_body().collect().await.unwrap().to_bytes();
        let id: UserId = from_slice(&body).unwrap();
        assert_eq!(id, UserId::ALICE);
    }

    #[tokio::test]
    async fn wrong_credentials_returns_401() {
        // Given
        #[derive(Clone)]
        struct UsersSaboteur;
        impl Login for UsersSaboteur {
            async fn login(
                &mut self,
                _name: String,
                _password: String,
            ) -> Result<(SessionId, UserId), AuthenticationError> {
                Err(AuthenticationError::WrongCredentials)
            }
        }
        let app = login_routes(UsersSaboteur, true);

        // When
        let response = app
            .oneshot(
                Request::post("/api/v0/login")
                    .header("content-type", "application/json")
                    .body(Body::from(r#"{"name": "dummy", "password": "dummy"}"#))
                    .unwrap(),
            )
            .await
            .unwrap();

        // Then
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    }

    #[derive(Clone, Default)]
    struct LoginSpy {
        signup_record: Arc<Mutex<Vec<(String, String)>>>,
        login_record: Arc<Mutex<Vec<(String, String)>>>,
    }

    impl LoginSpy {
        fn take_signup_record(&self) -> Vec<(String, String)> {
            take(&mut *self.signup_record.lock().unwrap())
        }

        fn take_login_record(&self) -> Vec<(String, String)> {
            take(&mut *self.login_record.lock().unwrap())
        }
    }

    impl Login for LoginSpy {
        async fn signup(
            &mut self,
            name: String,
            password: String,
        ) -> Result<(SessionId, UserId), UsersError> {
            self.signup_record.lock().unwrap().push((name, password));
            Ok((SessionId::nil(), UserId::nil()))
        }

        async fn login(
            &mut self,
            name: String,
            password: String,
        ) -> Result<(SessionId, UserId), AuthenticationError> {
            self.login_record.lock().unwrap().push((name, password));
            Ok((SessionId::nil(), UserId::nil()))
        }
    }
}

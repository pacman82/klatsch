use axum::{
    Json, Router,
    extract::State,
    http::{StatusCode, request::Parts},
    routing::post,
};
use axum_extra::extract::{
    CookieJar,
    cookie::{Cookie, SameSite},
};
use serde::Deserialize;

use crate::{
    http::{AuthenticateRequest, HttpError},
    sessions::{SessionId, SessionLifecycle, SessionLookup},
    user::{UserId, Users},
};

impl<T: SessionLookup + Sync> AuthenticateRequest for T {
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
            self.lookup(session_id).await.ok_or(HttpError {
                status_code: StatusCode::UNAUTHORIZED,
                message: "Unknown session".into(),
            })
        }
    }
}

pub fn session_routes<U, S>(users: U, sessions: S) -> Router
where
    U: Users + Send + Sync + Clone + 'static,
    S: SessionLifecycle + Send + Sync + Clone + 'static,
{
    Router::new()
        .route("/api/v0/signup", post(signup::<U, S>))
        .route("/api/v0/login", post(login::<U, S>))
        .with_state((users, sessions.clone()))
        .route("/api/v0/logout", post(logout::<S>))
        .with_state(sessions)
}

fn session_cookie(session_id: SessionId) -> Cookie<'static> {
    Cookie::build(("session", session_id.to_string()))
        // Http only prevents JavaScript from interacting with the session cookie. Hardening against
        // Cross site scripting attacks
        .http_only(true)
        // Hardening against cross site request forgery. Prevents other sites from abusing the trust
        // we put in the users browser.
        .same_site(SameSite::Strict)
        // Secure `true` would prevent this cookie to be transported via http instead of https. This
        // is great, **but**, we currently do not support https. So this stays `false` for now.
        .secure(false)
        .build()
}

async fn logout<S>(jar: CookieJar, State(mut sessions): State<S>) -> CookieJar
where
    S: SessionLifecycle,
{
    if let Some(session_id) = jar
        .get("session")
        .and_then(|c| c.value().parse::<SessionId>().ok())
    {
        sessions.destroy(session_id).await;
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

async fn signup<U, S>(
    jar: CookieJar,
    State((mut users, mut sessions)): State<(U, S)>,
    Json(body): Json<LoginBody>,
) -> Result<(CookieJar, Json<UserId>), HttpError>
where
    U: Users,
    S: SessionLifecycle,
{
    let user_id = users.signup(body.name, body.password).await?;
    let session_id = sessions.create(user_id).await;
    Ok((jar.add(session_cookie(session_id)), Json(user_id)))
}

async fn login<U, S>(
    jar: CookieJar,
    State((mut users, mut sessions)): State<(U, S)>,
    Json(body): Json<LoginBody>,
) -> Result<(CookieJar, Json<UserId>), HttpError>
where
    U: Users,
    S: SessionLifecycle,
{
    let user_id = users.login(body.name, body.password).await?;
    let session_id = sessions.create(user_id).await;
    Ok((jar.add(session_cookie(session_id)), Json(user_id)))
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
        http::AuthenticatedUser,
        sessions::{SessionId, SessionLifecycle, SessionLookup},
        user::{UserId, Users, UsersError},
    };

    use super::session_routes;

    const SOME_SESSION_ID: SessionId = SessionId::from_uuid(Uuid::from_u128(1));

    // --- AuthenticateRequest tests ---

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

    // --- session_cookie_routes tests ---

    #[tokio::test]
    async fn signup_forwards_to_users() {
        // Given
        let spy = UsersSpy::default();
        let app = session_routes(spy.clone(), Dummy);

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
    async fn signup_returns_user_id() {
        // Given
        #[derive(Clone)]
        struct UsersStub;
        impl Users for UsersStub {
            async fn signup(
                &mut self,
                _name: String,
                _password: String,
            ) -> Result<UserId, UsersError> {
                Ok(UserId::ALICE)
            }
        }
        let app = session_routes(UsersStub, Dummy);

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
        let body = response.into_body().collect().await.unwrap().to_bytes();
        let id: UserId = from_slice(&body).unwrap();
        assert_eq!(id, UserId::ALICE);
    }

    #[tokio::test]
    async fn login_sets_session_cookie() {
        // Given
        #[derive(Clone)]
        struct SessionsStub;
        impl SessionLifecycle for SessionsStub {
            async fn create(&mut self, _user_id: UserId) -> SessionId {
                SOME_SESSION_ID
            }
        }
        let app = session_routes(Dummy, SessionsStub);

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
        assert!(cookie.contains(&format!("session={SOME_SESSION_ID}")));
        assert!(cookie.contains("HttpOnly"));
        assert!(cookie.contains("SameSite=Strict"));
    }

    #[tokio::test]
    async fn signup_sets_session_cookie() {
        // Given
        #[derive(Clone)]
        struct SessionsStub;
        impl SessionLifecycle for SessionsStub {
            async fn create(&mut self, _user_id: UserId) -> SessionId {
                SOME_SESSION_ID
            }
        }
        let app = session_routes(Dummy, SessionsStub);

        // When
        let response = app
            .oneshot(
                Request::post("/api/v0/signup")
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
        assert!(cookie.contains(&format!("session={SOME_SESSION_ID}")));
        assert!(cookie.contains("HttpOnly"));
        assert!(cookie.contains("SameSite=Strict"));
    }

    #[tokio::test]
    async fn logout_clears_session_cookie() {
        // Given
        let app = session_routes(Dummy, Dummy);

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
        struct SessionsSpy {
            destroyed: Arc<Mutex<Vec<SessionId>>>,
        }
        impl SessionLifecycle for SessionsSpy {
            async fn destroy(&mut self, session_id: SessionId) {
                self.destroyed.lock().unwrap().push(session_id);
            }
        }
        let spy = SessionsSpy::default();
        let app = session_routes(Dummy, spy.clone());

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
    async fn login_forwards_to_users() {
        // Given
        let spy = UsersSpy::default();
        let app = session_routes(spy.clone(), Dummy);

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
    async fn logging_in_returns_user_id() {
        // Given
        #[derive(Clone)]
        struct UsersStub;
        impl Users for UsersStub {
            async fn login(
                &mut self,
                _name: String,
                _password: String,
            ) -> Result<UserId, UsersError> {
                Ok(UserId::ALICE)
            }
        }
        let app = session_routes(UsersStub, Dummy);

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
        let body = response.into_body().collect().await.unwrap().to_bytes();
        let id: UserId = from_slice(&body).unwrap();
        assert_eq!(id, UserId::ALICE);
    }

    #[tokio::test]
    async fn wrong_credentials_returns_401() {
        // Given
        #[derive(Clone)]
        struct UsersSaboteur;
        impl Users for UsersSaboteur {
            async fn login(
                &mut self,
                _name: String,
                _password: String,
            ) -> Result<UserId, UsersError> {
                Err(UsersError::Unauthenticated)
            }
        }
        let app = session_routes(UsersSaboteur, Dummy);

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
    struct UsersSpy {
        signup_record: Arc<Mutex<Vec<(String, String)>>>,
        login_record: Arc<Mutex<Vec<(String, String)>>>,
    }

    impl UsersSpy {
        fn take_signup_record(&self) -> Vec<(String, String)> {
            take(&mut *self.signup_record.lock().unwrap())
        }

        fn take_login_record(&self) -> Vec<(String, String)> {
            take(&mut *self.login_record.lock().unwrap())
        }
    }

    impl Users for UsersSpy {
        async fn signup(&mut self, name: String, password: String) -> Result<UserId, UsersError> {
            self.signup_record.lock().unwrap().push((name, password));
            Ok(UserId::nil())
        }

        async fn login(&mut self, name: String, password: String) -> Result<UserId, UsersError> {
            self.login_record.lock().unwrap().push((name, password));
            Ok(UserId::nil())
        }
    }
}

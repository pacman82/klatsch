use crate::{
    chat::{Chat, chat_routes},
    http::HttpError,
    sessions::{SessionId, Sessions},
    user::{User, UserId, Users, UsersError},
};
use axum::{
    Json, Router,
    extract::{Path, State},
    http::StatusCode,
    routing::{get, post},
};
use axum_extra::extract::{
    CookieJar,
    cookie::{Cookie, SameSite},
};
use serde::Deserialize;
use tokio::sync::watch;

impl From<UsersError> for HttpError {
    fn from(err: UsersError) -> Self {
        match err {
            UsersError::Internal => HttpError {
                status_code: StatusCode::INTERNAL_SERVER_ERROR,
                message: "Internal server error".into(),
            },
            UsersError::UnknownUser => HttpError {
                status_code: StatusCode::NOT_FOUND,
                message: "Unknown user".into(),
            },
            UsersError::Unauthenticated => HttpError {
                status_code: StatusCode::UNAUTHORIZED,
                message: "Either user name or password is incorrect".into(),
            },
        }
    }
}

pub fn api_router<C, U, S>(
    chat: C,
    users: U,
    sessions: S,
    shutting_down: watch::Receiver<bool>,
) -> Router
where
    C: Chat + Send + Sync + Clone + 'static,
    U: Users + Send + Sync + Clone + 'static,
    S: Sessions + Send + Sync + Clone + 'static,
{
    let router = Router::new()
        .route("/api/v0/users/{id}", get(user_info::<U>))
        .with_state(users.clone())
        .route("/api/v0/signup", post(signup::<U, S>))
        .route("/api/v0/login", post(login::<U, S>))
        .with_state((users, sessions.clone()))
        .route("/api/v0/logout", post(logout::<S>))
        .with_state(sessions.clone());

    let router = router.merge(chat_routes(chat, sessions, shutting_down));

    router
}

async fn logout<S>(jar: CookieJar, State(mut sessions): State<S>) -> CookieJar
where
    S: Sessions,
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

async fn signup<U, S>(
    jar: CookieJar,
    State((mut users, mut sessions)): State<(U, S)>,
    Json(body): Json<LoginBody>,
) -> Result<(CookieJar, Json<UserId>), HttpError>
where
    U: Users,
    S: Sessions,
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
    S: Sessions,
{
    let user_id = users.login(body.name, body.password).await?;
    let session_id = sessions.create(user_id).await;
    Ok((jar.add(session_cookie(session_id)), Json(user_id)))
}

async fn user_info<U>(
    State(mut users): State<U>,
    Path(id): Path<UserId>,
) -> Result<Json<User>, HttpError>
where
    U: Users,
{
    let user = users.user_by_id(id).await?;
    Ok(Json(user))
}

#[cfg(test)]
mod tests {
    use std::{
        mem::take,
        sync::{Arc, Mutex},
    };

    use uuid::Uuid;

    use crate::user::{User, UsersError};

    use super::*;
    use axum::{
        body::Body,
        http::{Request, StatusCode},
    };
    use double_trait::Dummy;
    use http_body_util::BodyExt as _;
    use serde_json::{Value, json};
    use tower::ServiceExt; // for `oneshot`

    const SOME_SESSION_ID: SessionId = SessionId::from_uuid(Uuid::from_u128(1));

    #[tokio::test]
    async fn messages_should_return_content_type_event_stream() {
        // Given
        let (_, shutting_down) = watch::channel(false);
        let app = api_router(Dummy, Dummy, Dummy, shutting_down);

        // When
        let response = app
            .oneshot(
                Request::builder()
                    .uri("/api/v0/events")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        // Then
        let content_type = response
            .headers()
            .get("content-type")
            .and_then(|v| v.to_str().ok())
            .unwrap_or("");
        assert!(
            content_type.starts_with("text/event-stream"),
            "Expected SSE content-type, got: {}",
            content_type
        );
    }

    #[tokio::test]
    async fn signup_forwards_to_users() {
        // Given
        let spy = UsersSpy::default();
        let (_, shutting_down) = watch::channel(false);
        let app = api_router(Dummy, spy.clone(), SessionsDummy, shutting_down);

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
    async fn login_in_route_forwards_to_users() {
        // Given
        let (_, shutting_down) = watch::channel(false);
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
        let app = api_router(Dummy, UsersStub, SessionsDummy, shutting_down);

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
        let id: UserId = serde_json::from_slice(&body).unwrap();
        assert_eq!(id, UserId::ALICE);
    }

    #[tokio::test]
    async fn login_sets_session_cookie() {
        // Given
        #[derive(Clone)]
        struct SessionsStub;
        impl Sessions for SessionsStub {
            async fn create(&mut self, _user_id: UserId) -> SessionId {
                SOME_SESSION_ID
            }
        }
        let (_, shutting_down) = watch::channel(false);
        let app = api_router(Dummy, UserDummy, SessionsStub, shutting_down);

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
        impl Sessions for SessionsStub {
            async fn create(&mut self, _user_id: UserId) -> SessionId {
                SOME_SESSION_ID
            }
        }
        let (_, shutting_down) = watch::channel(false);
        let app = api_router(Dummy, UserDummy, SessionsStub, shutting_down);

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
        let (_, shutting_down) = watch::channel(false);
        let app = api_router(Dummy, UserDummy, SessionsDummy, shutting_down);

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
        use std::sync::{Arc, Mutex};

        // Given
        #[derive(Clone, Default)]
        struct SessionsSpy {
            destroyed: Arc<Mutex<Vec<SessionId>>>,
        }
        impl Sessions for SessionsSpy {
            async fn destroy(&mut self, session_id: SessionId) {
                self.destroyed.lock().unwrap().push(session_id);
            }
        }
        let spy = SessionsSpy::default();
        let (_, shutting_down) = watch::channel(false);
        let app = api_router(Dummy, UserDummy, spy.clone(), shutting_down);

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
        let (_, shutting_down) = watch::channel(false);
        let app = api_router(Dummy, spy.clone(), SessionsDummy, shutting_down);

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
        let (_, shutting_down) = watch::channel(false);
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
        let app = api_router(Dummy, UsersStub, SessionsDummy, shutting_down);

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
        let id: UserId = serde_json::from_slice(&body).unwrap();
        assert_eq!(id, UserId::ALICE);
    }

    #[tokio::test]
    async fn wrong_credentials_returns_401() {
        // Given
        let (_, shutting_down) = watch::channel(false);
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
        let app = api_router(Dummy, UsersSaboteur, Dummy, shutting_down);

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

    #[tokio::test]
    async fn user_info() {
        // Given
        let (_, shutting_down) = watch::channel(false);
        #[derive(Clone)]
        struct UsersStub;

        impl Users for UsersStub {
            async fn user_by_id(&mut self, _: UserId) -> Result<User, UsersError> {
                Ok(User {
                    name: "Alice".to_owned(),
                })
            }
        }
        let app = api_router(Dummy, UsersStub, Dummy, shutting_down);

        // When
        let response = app
            .oneshot(
                Request::builder()
                    .uri("/api/v0/users/f9108910-9f1d-4a9e-85dd-f768472298d7")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        // Then
        let content_type = response
            .headers()
            .get("content-type")
            .and_then(|v| v.to_str().ok())
            .unwrap_or("");
        assert!(
            content_type.starts_with("application/json"),
            "Expected application/json, got: {}",
            content_type
        );
        let body = response.into_body().collect().await.unwrap();
        let body = body.to_bytes().to_vec();
        let body: Value = serde_json::from_slice(&body).unwrap();

        assert_eq!(json!({"name": "Alice"}), body)
    }

    #[tokio::test]
    async fn user_info_for_unknown_user() {
        // Given
        let (_, shutting_down) = watch::channel(false);
        #[derive(Clone)]
        struct UsersStub;

        impl Users for UsersStub {
            async fn user_by_id(&mut self, _: UserId) -> Result<User, UsersError> {
                Err(UsersError::UnknownUser)
            }
        }
        let app = api_router(Dummy, UsersStub, Dummy, shutting_down);

        // When
        let response = app
            .oneshot(
                Request::builder()
                    .uri("/api/v0/users/f9108910-9f1d-4a9e-85dd-f768472298d7")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        // Then
        assert_eq!(response.status(), StatusCode::NOT_FOUND);
        let body = response.into_body().collect().await.unwrap();
        let body = String::from_utf8(body.to_bytes().to_vec()).unwrap();

        assert_eq!("Unknown user", body)
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

        async fn user_by_id(&mut self, _id: UserId) -> Result<User, UsersError> {
            Ok(User {
                name: "dummy".to_owned(),
            })
        }
    }

    #[derive(Clone)]
    struct SessionsDummy;

    impl Sessions for SessionsDummy {
        async fn create(&mut self, _user_id: UserId) -> SessionId {
            SessionId::from_uuid(Uuid::nil())
        }

        async fn lookup(&mut self, _session_id: SessionId) -> Option<UserId> {
            Some(UserId::nil())
        }
    }

    #[derive(Clone)]
    struct UserDummy;

    impl Users for UserDummy {
        async fn signup(&mut self, _name: String, _password: String) -> Result<UserId, UsersError> {
            Ok(UserId::nil())
        }

        async fn login(&mut self, _name: String, _password: String) -> Result<UserId, UsersError> {
            Ok(UserId::nil())
        }

        async fn user_by_id(&mut self, _id: UserId) -> Result<User, UsersError> {
            Ok(User {
                name: "dummy".to_owned(),
            })
        }
    }
}

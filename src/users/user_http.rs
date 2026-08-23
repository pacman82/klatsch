use crate::http::HttpError;

use super::{
    AuthenticateRequest, AuthenticatedUser, AuthenticationError, User, UserId, Users, UsersError,
};

use axum::{
    Json, Router,
    extract::{Path, State},
    http::{StatusCode, request::Parts},
    routing::{get, post},
};
use serde::Deserialize;

pub fn user_routes<U, S>(users: U, sessions: S) -> Router
where
    U: Users + Send + Sync + Clone + 'static,
    S: AuthenticateRequest + Send + Sync + Clone + 'static,
{
    Router::new()
        .route("/api/v0/users/{id}", get(user_info::<U, S>))
        .route(
            "/api/v0/users/me/change_password",
            post(change_password::<U, S>),
        )
        .route("/api/v0/users/is_empty", get(is_empty::<U, S>))
        .with_state(UserState { users, sessions })
}

/// Used to check if the system is new and does not have any users setup yet. This is useful for
/// bootstrapping the first user of the system. This route does not require authentication. If there
/// would be an authenticated user, this would imply the answer to be `true`.
async fn is_empty<U, S>(State(state): State<UserState<U, S>>) -> Result<Json<bool>, HttpError>
where
    U: Users + Send + Sync,
    S: AuthenticateRequest + Send + Sync,
{
    let mut users = state.users;
    let is_empty = users.is_empty().await?;
    Ok(Json(is_empty))
}

#[derive(Clone)]
struct UserState<U, S> {
    users: U,
    sessions: S,
}

impl<U: Send + Sync, S: AuthenticateRequest + Sync> AuthenticateRequest for UserState<U, S> {
    fn authenticate_request(
        &self,
        parts: &Parts,
    ) -> impl Future<Output = Result<UserId, HttpError>> + Send {
        self.sessions.authenticate_request(parts)
    }
}

async fn user_info<U, S>(
    AuthenticatedUser(_): AuthenticatedUser,
    State(state): State<UserState<U, S>>,
    Path(id): Path<UserId>,
) -> Result<Json<User>, HttpError>
where
    U: Users + Send + Sync,
    S: AuthenticateRequest + Send + Sync,
{
    let mut users = state.users;
    let user = users.user_by_id(id).await?;
    Ok(Json(user))
}

#[derive(Deserialize)]
struct ChangePasswordBody {
    current_password: String,
    new_password: String,
}

async fn change_password<U, S>(
    AuthenticatedUser(id): AuthenticatedUser,
    State(state): State<UserState<U, S>>,
    Json(body): Json<ChangePasswordBody>,
) -> Result<(), HttpError>
where
    U: Users + Send + Sync,
    S: AuthenticateRequest + Send + Sync,
{
    let mut users = state.users;
    users
        .change_password(id, body.current_password, body.new_password)
        .await?;
    Ok(())
}

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
            UsersError::WrongCurrentPassword => HttpError {
                status_code: StatusCode::BAD_REQUEST,
                message: "Current password is incorrect".into(),
            },
            UsersError::NameTaken => HttpError {
                status_code: StatusCode::BAD_REQUEST,
                message: "Username is already taken".into(),
            },
        }
    }
}

impl From<AuthenticationError> for HttpError {
    fn from(err: AuthenticationError) -> Self {
        match err {
            AuthenticationError::Internal => HttpError {
                status_code: StatusCode::INTERNAL_SERVER_ERROR,
                message: "Internal server error".into(),
            },
            AuthenticationError::WrongCredentials => HttpError {
                status_code: StatusCode::UNAUTHORIZED,
                message: "Either user name or password is incorrect".into(),
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};

    use super::*;
    use axum::{
        body::Body,
        http::{Request, StatusCode},
    };
    use http_body_util::BodyExt as _;
    use serde_json::{Value, json};
    use tower::ServiceExt;

    #[tokio::test]
    async fn user_info() {
        // Given
        #[derive(Clone)]
        struct UsersStub;

        impl Users for UsersStub {
            async fn user_by_id(&mut self, _: UserId) -> Result<User, UsersError> {
                Ok(User {
                    name: "Alice".to_owned(),
                })
            }
        }
        let app = user_routes(UsersStub, AuthDummy);

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
    async fn is_empty_route_forwards_to_users() {
        // Given
        #[derive(Clone)]
        struct UsersStub;
        impl Users for UsersStub {
            async fn is_empty(&mut self) -> Result<bool, UsersError> {
                Ok(true)
            }
        }
        let app = user_routes(UsersStub, AuthDummy);

        // When
        let response = app
            .oneshot(
                Request::builder()
                    .uri("/api/v0/users/is_empty")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        // Then
        assert_eq!(response.status(), StatusCode::OK);
        let body = response.into_body().collect().await.unwrap().to_bytes();
        let is_empty: bool = serde_json::from_slice(&body).unwrap();
        assert!(is_empty);
    }

    #[tokio::test]
    async fn user_info_for_unknown_user() {
        // Given
        #[derive(Clone)]
        struct UsersStub;

        impl Users for UsersStub {
            async fn user_by_id(&mut self, _: UserId) -> Result<User, UsersError> {
                Err(UsersError::UnknownUser)
            }
        }
        let app = user_routes(UsersStub, AuthDummy);

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

    #[tokio::test]
    async fn change_password_forwards_current_and_new_password() {
        // Given
        #[derive(Clone, Default)]
        struct UsersSpy {
            calls: Arc<Mutex<Vec<(UserId, String, String)>>>,
        }
        impl Users for UsersSpy {
            async fn change_password(
                &mut self,
                id: UserId,
                current_password: String,
                new_password: String,
            ) -> Result<(), UsersError> {
                self.calls
                    .lock()
                    .unwrap()
                    .push((id, current_password, new_password));
                Ok(())
            }
        }
        let spy = UsersSpy::default();
        let app = user_routes(spy.clone(), AuthDummy);

        // When
        let response = app
            .oneshot(
                Request::post("/api/v0/users/me/change_password")
                    .header("content-type", "application/json")
                    .body(Body::from(
                        r#"{"current_password": "old-secret", "new_password": "new-secret"}"#,
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();

        // Then
        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(
            *spy.calls.lock().unwrap(),
            [(
                UserId::nil(),
                "old-secret".to_owned(),
                "new-secret".to_owned()
            )]
        );
    }

    #[tokio::test]
    async fn change_password_returns_bad_request_for_wrong_password() {
        // Given

        // Rejects any current password, simulating a mismatch against the stored hash.
        #[derive(Clone)]
        struct WrongPasswordSaboteur;
        impl Users for WrongPasswordSaboteur {
            async fn change_password(
                &mut self,
                _id: UserId,
                _current_password: String,
                _new_password: String,
            ) -> Result<(), UsersError> {
                Err(UsersError::WrongCurrentPassword)
            }
        }
        let app = user_routes(WrongPasswordSaboteur, AuthDummy);

        // When
        let response = app
            .oneshot(
                Request::post("/api/v0/users/me/change_password")
                    .header("content-type", "application/json")
                    .body(Body::from(
                        r#"{"current_password": "wrong-secret", "new_password": "new-secret"}"#,
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();

        // Then

        // Distinct from the 401 an expired/missing session produces, so the frontend can tell
        // "wrong password" apart from "please log in again".
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    }

    #[derive(Clone)]
    struct AuthDummy;

    impl AuthenticateRequest for AuthDummy {
        async fn authenticate_request(&self, _parts: &Parts) -> Result<UserId, HttpError> {
            Ok(UserId::nil())
        }
    }
}

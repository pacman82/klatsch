use crate::http::HttpError;

use super::{User, UserId, Users, UsersError};

use axum::{
    Json, Router,
    extract::{Path, State},
    http::StatusCode,
    routing::get,
};

pub fn user_routes<U>(users: U) -> Router
where
    U: Users + Send + Sync + Clone + 'static,
{
    Router::new()
        .route("/api/v0/users/{id}", get(user_info::<U>))
        .with_state(users)
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

#[cfg(test)]
mod tests {
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
        let app = user_routes(UsersStub);

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
        #[derive(Clone)]
        struct UsersStub;

        impl Users for UsersStub {
            async fn user_by_id(&mut self, _: UserId) -> Result<User, UsersError> {
                Err(UsersError::UnknownUser)
            }
        }
        let app = user_routes(UsersStub);

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
}

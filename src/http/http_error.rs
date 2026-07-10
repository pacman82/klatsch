use std::borrow::Cow;

use axum::{
    http::StatusCode,
    response::{IntoResponse, Response},
};

pub struct HttpError {
    pub status_code: StatusCode,
    pub message: Cow<'static, str>,
}

impl IntoResponse for HttpError {
    fn into_response(self) -> Response {
        (self.status_code, self.message).into_response()
    }
}

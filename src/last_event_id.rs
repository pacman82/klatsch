use std::convert::Infallible;

use axum::{extract::FromRequestParts, http::request::Parts};

/// Extractor for the `Last-Event-ID` header used by EventSource clients.
#[derive(Clone, Copy, Debug)]
pub struct LastEventId(pub u64);

impl<S> FromRequestParts<S> for LastEventId
where
    S: Send + Sync,
{
    type Rejection = Infallible;

    async fn from_request_parts(parts: &mut Parts, _state: &S) -> Result<Self, Self::Rejection> {
        let id = parts
            .headers
            .get("last-event-id")
            .and_then(|v| v.to_str().ok())
            .and_then(|s| s.parse::<u64>().ok())
            .unwrap_or_default();
        Ok(LastEventId(id))
    }
}

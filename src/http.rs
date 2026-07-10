//! Utilities for writing http handlers.

mod http_error;
mod last_event_id;

pub use self::{http_error::HttpError, last_event_id::LastEventId};

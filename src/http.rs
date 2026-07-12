//! Utilities for writing http handlers.

mod authenticate;
mod http_error;
mod last_event_id;

pub use self::{
    authenticate::{AuthenticateRequest, AuthenticatedUser},
    http_error::HttpError,
    last_event_id::LastEventId,
};

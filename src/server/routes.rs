use tokio::sync::watch;

use crate::users::AuthenticateRequest;

/// Provide routes for the HTTP API
pub trait Routes {
    /// Used to register routes in the HTTP API
    ///
    /// # Parameters
    ///
    /// * `auth`: Used to extract authenticated users from requests
    /// * `shutting_down`: Long lived requests use this to abort, so server shutdon is fast.
    /// * `encrypted`: Whether https is used or not. Used to decide how to handle confidential
    ///   information in cookies.
    fn routes(
        self,
        auth: impl AuthenticateRequest + Send + Sync + Clone + 'static,
        shutting_down: watch::Receiver<bool>,
        encrypted: bool,
    ) -> axum::Router<()>;
}

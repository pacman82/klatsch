//! Module for statically hosting the UI assets

use axum::Router;

pub fn ui_router() -> Router {
    memory_serve::load!()
        .index_file(Some("/index.html"))
        .into_router()
}

// Memory serve does not work correctly with windows in debug.
#[cfg(not(windows))]
#[cfg(test)]
mod tests {
    use axum::{body::Body, http::Request};
    use tower::ServiceExt;

    use super::ui_router;

    #[tokio::test]
    async fn static_ui_serves_index_page() {
        // Given a running server
        let app = ui_router();

        // When requesting the root path
        let response = app
            .oneshot(Request::builder().uri("/").body(Body::empty()).unwrap())
            .await
            .unwrap();

        // Then it should return 200 with HTML content
        assert_eq!(response.status(), 200);
        assert!(
            response.headers()["content-type"]
                .to_str()
                .unwrap()
                .contains("text/html")
        );
    }
}

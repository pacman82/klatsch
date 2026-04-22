//! Module for statically hosting the UI assets

use axum::Router;
use static_serve::embed_assets;

pub fn ui_router() -> Router {
    embed_assets!(
        // Populated by `build.rs`, which stages `ui/` into `target/ui/` and runs npm there so the
        // build output stays inside cargo's `target/` instead of polluting the source tree.
        "./target/ui/build",
        // Match `/index.html to `/`
        strip_html_ext = true,
        compress = true,
        cache_busted_paths = ["_app/immutable"]
    );
    static_router()
}

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

use std::convert::Infallible;

use axum::{
    Router,
    response::{Sse, sse::Event},
    routing::get,
};
use futures_util::Stream;
use memory_serve::{MemoryServe, load_assets};
use tokio::{
    net::{TcpListener, ToSocketAddrs},
    sync::oneshot,
    task::JoinHandle,
};

pub struct Server {
    trigger_shutdown: oneshot::Sender<()>,
    join_handle: JoinHandle<()>,
}

impl Server {
    pub async fn new(socket_address: impl ToSocketAddrs) -> anyhow::Result<Server> {
        let listener = TcpListener::bind(socket_address).await?;
        let router = router();
        let (trigger_shutdown, shutdown_triggered) = oneshot::channel();
        let join_handle = tokio::spawn(async move {
            axum::serve(listener, router)
                .with_graceful_shutdown(async move {
                    shutdown_triggered
                        .await
                        .expect("Sendor for shutdown trigger must not be dropped before used.")
                })
                .await
                .expect("axum::serve must not return an error");
        });
        let server = Server {
            trigger_shutdown,
            join_handle,
        };
        Ok(server)
    }

    pub async fn shutdown(self) {
        self.trigger_shutdown.send(()).expect("Receiver must exist");
        self.join_handle.await.unwrap();
    }
}

fn router() -> Router {
    let client_ui_router = MemoryServe::new(load_assets!("./ui/build"))
        .index_file(Some("/index.html"))
        .into_router();

    Router::new()
        .merge(client_ui_router)
        .route("/health", get(|| async { "OK" }))
        .route("/messages", get(messages))
}

async fn messages() -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
    Sse::new(futures_util::stream::empty())
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{
        body::Body,
        http::{Request, StatusCode},
    };
    use http_body_util::BodyExt as _;
    use tower::ServiceExt; // for `oneshot`

    #[tokio::test]
    async fn test_messages_route_returns_empty_sse_stream() {
        let app = router();

        let response = app
            .oneshot(
                Request::builder()
                    .uri("/messages")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        // Check status code
        assert_eq!(response.status(), StatusCode::OK);

        // Check content-type header
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

        // Check that the body is empty (no SSE events)
        let bytes = response.into_body().collect().await.unwrap().to_bytes();
        assert_eq!(
            "", bytes,
            "Expected empty SSE stream body, got: {:?}",
            bytes
        );
    }
}

use std::convert::Infallible;

use axum::{
    Router,
    extract::State,
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

use crate::conversation::Conversation;

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
    let conversation = Conversation::new();

    let client_ui_router = MemoryServe::new(load_assets!("./ui/build"))
        .index_file(Some("/index.html"))
        .into_router();

    Router::new()
        .merge(client_ui_router)
        .route("/health", get(|| async { "OK" }))
        .route("/messages", get(messages))
        .with_state(conversation)
}

async fn messages(
    State(conversation): State<Conversation>,
) -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
    let messages = conversation
        .messages()
        .into_iter()
        .enumerate()
        .map(|(id, msg)| {
            let event = Event::default()
                .id(id.to_string())
                .json_data(msg)
                .expect("Deserializing message must not fail");
            Ok(event)
        });
    Sse::new(tokio_stream::iter(messages))
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
    async fn messages_route_returns_hardcoded_messages_stream() {
        // Given
        let app = router();

        // When
        let response = app
            .oneshot(
                Request::builder()
                    .uri("/messages")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        // Then
        assert_eq!(response.status(), StatusCode::OK);

        let bytes = response.into_body().collect().await.unwrap().to_bytes();
        let expected_body = "id: 0\n\
            data: {\"id\":\"019c0050-e4d7-7447-9d8f-81cde690f4a1\",\"sender\":\"Alice\",\
            \"content\":\"Hey there! 👋\",\"timestamp\":1704531600}\n\
            \n\
            id: 1\n\
            data: {\"id\":\"019c0051-c29d-7968-b953-4adc898b1360\",\"sender\":\"Bob\",\"content\":\
            \"Hi Alice! How are you?\",\"timestamp\":1704532600}\n\
            \n\
            id: 2\ndata: {\"id\":\"019c0051-e50d-7ea7-8a0e-f7df4176dd93\",\"sender\":\"Alice\",\
            \"content\":\"I'm good, thanks! Working on the chat server project.\",\"timestamp\":\
            1704533600}\n\
            \n\
            id: 3\ndata: {\"id\":\"019c0052-09b0-73be-a145-3767cb10cdf6\",\"sender\":\"Bob\",\
            \"content\":\"That's awesome! Let me know if you need any help.\",\"timestamp\":\
            1704534600}\n\
            \n";
        assert_eq!(expected_body, bytes);
    }

    #[tokio::test]
    async fn messages_should_return_content_type_event_stream() {
        // Given
        let app = router();

        // When
        let response = app
            .oneshot(
                Request::builder()
                    .uri("/messages")
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
            content_type.starts_with("text/event-stream"),
            "Expected SSE content-type, got: {}",
            content_type
        );
    }
}

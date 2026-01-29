use std::convert::Infallible;

use axum::{
    Router,
    extract::State,
    response::{Sse, sse::Event},
    routing::get,
};
use futures_util::{Stream, StreamExt as _};
use memory_serve::{MemoryServe, load_assets};
use tokio::{
    net::{TcpListener, ToSocketAddrs},
    sync::oneshot,
    task::JoinHandle,
};

use crate::conversation::ConversationApi;

pub struct Server {
    trigger_shutdown: oneshot::Sender<()>,
    join_handle: JoinHandle<()>,
}

impl Server {
    pub async fn new<C>(
        socket_address: impl ToSocketAddrs,
        conversation: C,
    ) -> anyhow::Result<Server>
    where
        C: ConversationApi + Send + Sync + Clone + 'static,
    {
        let listener = TcpListener::bind(socket_address).await?;
        let router = router(conversation);
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

fn router<C>(conversation: C) -> Router
where
    C: ConversationApi + Send + Sync + Clone + 'static,
{
    let client_ui_router = MemoryServe::new(load_assets!("./ui/build"))
        .index_file(Some("/index.html"))
        .into_router();

    Router::new()
        .merge(client_ui_router)
        .route("/health", get(|| async { "OK" }))
        .route("/api/v0/messages", get(messages::<C>))
        .with_state(conversation)
}

async fn messages<C>(
    State(conversation): State<C>,
) -> Sse<impl Stream<Item = Result<Event, Infallible>>>
where
    C: ConversationApi + Send,
{
    let messages = conversation.messages().enumerate().map(|(id, msg)| {
        let event = Event::default()
            .id(id.to_string())
            .json_data(msg)
            .expect("Deserializing message must not fail");
        Ok(event)
    });
    Sse::new(messages)
}

#[cfg(test)]
mod tests {
    use crate::conversation::Message;

    use super::*;
    use axum::{
        body::Body,
        http::{Request, StatusCode},
    };
    use double_trait::Dummy;
    use http_body_util::BodyExt as _;
    use tower::ServiceExt; // for `oneshot`

    #[tokio::test]
    async fn messages_route_returns_hardcoded_messages_stream() {
        // Given
        #[derive(Clone)]
        struct ConversationStub;

        impl ConversationApi for ConversationStub {
            fn messages(self) -> impl Stream<Item = Message> + Send {
                let messages = vec![
                    Message {
                        id: "019c0050-e4d7-7447-9d8f-81cde690f4a1".parse().unwrap(),
                        sender: "Alice".to_owned(),
                        content: "One".to_owned(),
                        timestamp_ms: 1704531600000,
                    },
                    Message {
                        id: "019c0051-c29d-7968-b953-4adc898b1360".parse().unwrap(),
                        sender: "Bob".to_owned(),
                        content: "Two".to_owned(),
                        timestamp_ms: 1704531601000,
                    },
                    Message {
                        id: "019c0051-e50d-7ea7-8a0e-f7df4176dd93".parse().unwrap(),
                        sender: "Alice".to_string(),
                        content: "Three".to_owned(),
                        timestamp_ms: 1704531602000,
                    },
                    Message {
                        id: "019c0052-09b0-73be-a145-3767cb10cdf6".parse().unwrap(),
                        sender: "Bob".to_owned(),
                        content: "Four".to_owned(),
                        timestamp_ms: 1704531603000,
                    },
                ];
                tokio_stream::iter(messages)
            }
        }
        let app = router(ConversationStub);

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

        let bytes = response
            .into_body()
            .collect()
            .await
            .unwrap()
            .to_bytes()
            .to_vec();
        let expected_body = "id: 0\n\
            data: {\"id\":\"019c0050-e4d7-7447-9d8f-81cde690f4a1\",\"sender\":\"Alice\",\
            \"content\":\"One\",\"timestamp_ms\":1704531600000}\n\
            \n\
            id: 1\n\
            data: {\"id\":\"019c0051-c29d-7968-b953-4adc898b1360\",\"sender\":\"Bob\",\"content\":\
            \"Two\",\"timestamp_ms\":1704531601000}\n\
            \n\
            id: 2\ndata: {\"id\":\"019c0051-e50d-7ea7-8a0e-f7df4176dd93\",\"sender\":\"Alice\",\
            \"content\":\"Three\",\"timestamp_ms\":1704531602000}\n\
            \n\
            id: 3\ndata: {\"id\":\"019c0052-09b0-73be-a145-3767cb10cdf6\",\"sender\":\"Bob\",\
            \"content\":\"Four\",\"timestamp_ms\":1704531603000}\n\
            \n";
        assert_eq!(expected_body, String::from_utf8(bytes).unwrap());
    }

    #[tokio::test]
    async fn messages_should_return_content_type_event_stream() {
        // Given
        let app = router(Dummy);

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

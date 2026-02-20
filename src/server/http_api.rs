use std::{borrow::Cow, convert::Infallible, time::UNIX_EPOCH};

use axum::{
    Json, Router,
    extract::State,
    http::StatusCode,
    response::{IntoResponse, Response, Sse, sse::Event as SseEvent},
    routing::{get, post},
};
use futures_util::{Stream, StreamExt as _};
use serde::Serialize;
use tokio::sync::watch;
use uuid::Uuid;

use crate::chat::{ChatError, Event, Message, SharedChat};

struct HttpError {
    status_code: StatusCode,
    message: Cow<'static, str>,
}

impl IntoResponse for HttpError {
    fn into_response(self) -> Response {
        (self.status_code, self.message).into_response()
    }
}

impl From<ChatError> for HttpError {
    fn from(err: ChatError) -> Self {
        match err {
            ChatError::Conflict => HttpError {
                status_code: StatusCode::CONFLICT,
                message: "A different message with this ID already exists".into(),
            },
        }
    }
}

use super::{last_event_id::LastEventId, terminate_on_shutdown::terminate_on_shutdown};

pub fn api_router<C>(chat: C, shutting_down: watch::Receiver<bool>) -> Router
where
    C: SharedChat + Send + Sync + Clone + 'static,
{
    Router::new()
        .route("/api/v0/events", get(events::<C>))
        .with_state((chat.clone(), shutting_down))
        .route("/api/v0/add_message", post(add_message::<C>))
        .with_state(chat)
}

async fn events<C>(
    State((chat, shutting_down)): State<(C, watch::Receiver<bool>)>,
    last_event_id: LastEventId,
) -> Sse<impl Stream<Item = Result<SseEvent, Infallible>> + Send + 'static>
where
    C: SharedChat + Send + 'static,
{
    let last_event_id = last_event_id.0;
    // Convert chat events into SSE events
    let events = chat.events(last_event_id).map(|chat_event| {
        let sse_event: SseEvent = chat_event.into();
        Ok(sse_event)
    });

    let events = terminate_on_shutdown(events, shutting_down);

    Sse::new(events)
}

/// A message as represented by the `events` route.
#[derive(Serialize)]
pub struct HttpMessage {
    /// Sender generated unique identifier for the message. It is used to recover from errors
    /// sending messages. It also a key for the UI to efficiently update data structures then
    /// rendering messages.
    pub id: Uuid,
    /// Author of the message
    pub sender: String,
    /// Text content of the message. I.e. the actual message
    pub content: String,
    /// Unix timestamp of that message being received by the server. Milliseconds since epoch.
    pub timestamp_ms: u64,
}

impl From<Event> for SseEvent {
    fn from(source: Event) -> Self {
        // Destructure source event
        let Event {
            id: event_id,
            message:
                Message {
                    id: message_id,
                    sender,
                    content,
                },
            timestamp,
        } = source;
        // `u64` covers ~584 million years since epoch, so we can afford to downcast the ms from
        // `u128` to u64 without fear.
        let timestamp_ms = timestamp.duration_since(UNIX_EPOCH).unwrap().as_millis() as u64;
        SseEvent::default()
            .id(event_id.to_string())
            .json_data(HttpMessage {
                id: message_id,
                sender,
                content,
                timestamp_ms,
            })
            .expect("Deserializing message must not fail")
    }
}

async fn add_message<C>(
    State(mut chat): State<C>,
    Json(msg): Json<Message>,
) -> Result<(), HttpError>
where
    C: SharedChat,
{
    chat.add_message(msg).await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::{
        mem::swap,
        sync::{Arc, Mutex},
        time::Duration,
    };

    use crate::chat::Event;

    use super::*;
    use axum::{
        body::Body,
        http::{Request, StatusCode},
    };
    use double_trait::Dummy;
    use http_body_util::BodyExt as _;
    use serde_json::json;
    use tokio::time::timeout;
    use tokio_stream::pending;
    use tower::ServiceExt; // for `oneshot`

    #[tokio::test]
    async fn messages_route_returns_hardcoded_messages_stream() {
        // Given
        #[derive(Clone)]
        struct ChatStub;

        impl SharedChat for ChatStub {
            fn events(self, _last_event_id: u64) -> impl Stream<Item = Event> + Send {
                let messages = vec![
                    Event::with_timestamp(
                        1,
                        Message {
                            id: "019c0050-e4d7-7447-9d8f-81cde690f4a1".parse().unwrap(),
                            sender: "Alice".to_owned(),
                            content: "One".to_owned(),
                        },
                        UNIX_EPOCH + Duration::from_millis(1704531600000),
                    ),
                    Event::with_timestamp(
                        2,
                        Message {
                            id: "019c0051-c29d-7968-b953-4adc898b1360".parse().unwrap(),
                            sender: "Bob".to_owned(),
                            content: "Two".to_owned(),
                        },
                        UNIX_EPOCH + Duration::from_millis(1704531601000),
                    ),
                    Event::with_timestamp(
                        3,
                        Message {
                            id: "019c0051-e50d-7ea7-8a0e-f7df4176dd93".parse().unwrap(),
                            sender: "Alice".to_string(),
                            content: "Three".to_owned(),
                        },
                        UNIX_EPOCH + Duration::from_millis(1704531602000),
                    ),
                    Event::with_timestamp(
                        4,
                        Message {
                            id: "019c0052-09b0-73be-a145-3767cb10cdf6".parse().unwrap(),
                            sender: "Bob".to_owned(),
                            content: "Four".to_owned(),
                        },
                        UNIX_EPOCH + Duration::from_millis(1704531603000),
                    ),
                ];
                tokio_stream::iter(messages)
            }
        }
        let (_send_shutdown_trigger, shutting_down) = watch::channel(false);
        let app = api_router(ChatStub, shutting_down);

        // When
        let response = app
            .oneshot(
                Request::builder()
                    .uri("/api/v0/events")
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
        let expected_body = "id: 1\n\
            data: {\"id\":\"019c0050-e4d7-7447-9d8f-81cde690f4a1\",\"sender\":\"Alice\",\
            \"content\":\"One\",\"timestamp_ms\":1704531600000}\n\
            \n\
            id: 2\n\
            data: {\"id\":\"019c0051-c29d-7968-b953-4adc898b1360\",\"sender\":\"Bob\",\"content\":\
            \"Two\",\"timestamp_ms\":1704531601000}\n\
            \n\
            id: 3\ndata: {\"id\":\"019c0051-e50d-7ea7-8a0e-f7df4176dd93\",\"sender\":\"Alice\",\
            \"content\":\"Three\",\"timestamp_ms\":1704531602000}\n\
            \n\
            id: 4\ndata: {\"id\":\"019c0052-09b0-73be-a145-3767cb10cdf6\",\"sender\":\"Bob\",\
            \"content\":\"Four\",\"timestamp_ms\":1704531603000}\n\
            \n";
        assert_eq!(expected_body, String::from_utf8(bytes).unwrap());
    }

    #[tokio::test]
    async fn messages_should_return_content_type_event_stream() {
        // Given
        let (_, shutting_down) = watch::channel(false);
        let app = api_router(Dummy, shutting_down);

        // When
        let response = app
            .oneshot(
                Request::builder()
                    .uri("/api/v0/events")
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
    #[tokio::test]
    async fn add_message_route_forwards_arguments_to_chat_api() {
        // Given
        let spy = ChatSpy::default();
        let (_, shutting_down) = watch::channel(false);
        let app = api_router(spy.clone(), shutting_down);
        let new_message = json!({
            "id": "019c0a7f-3d8e-7cf8-bea4-3a8614c8da09",
            "sender": "Bob",
            "content": "Hello, Alice!"
        });

        // When
        let _response = app
            .oneshot(
                Request::post("/api/v0/add_message")
                    .header("content-type", "application/json")
                    .body(Body::from(new_message.to_string()))
                    .unwrap(),
            )
            .await
            .unwrap();

        // Then
        let expected_msg = Message {
            id: "019c0a7f-3d8e-7cf8-bea4-3a8614c8da09"
                .parse::<Uuid>()
                .unwrap(),
            sender: "Bob".to_owned(),
            content: "Hello, Alice!".to_owned(),
        };
        assert_eq!(spy.take_add_message_record(), &[expected_msg],);
    }

    #[tokio::test]
    async fn last_event_id_forwarded_to_chat_runtime_then_fetching_events() {
        // Given
        let spy = ChatSpy::default();
        let (_, shutting_down) = watch::channel(false);
        let app = api_router(spy.clone(), shutting_down);

        // When: request with Last-Event-ID = 7
        let _response = app
            .oneshot(
                Request::builder()
                    .uri("/api/v0/events")
                    .header("Last-Event-ID", "7")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        // Then: the chat should have been asked for events since id 7
        assert_eq!(spy.take_events_record(), vec![7]);
    }

    #[tokio::test]
    async fn shutdown_terminates_event_stream() {
        // Given a pending chat and an open request to events
        #[derive(Clone)]
        struct PendingChatStub;
        impl SharedChat for PendingChatStub {
            fn events(self, _last_event_id: u64) -> impl futures_util::Stream<Item = Event> + Send {
                pending()
            }
        }

        let (shutdown_tx, shutdown_rx) = watch::channel(false);
        let app = api_router(PendingChatStub, shutdown_rx);

        let response_body = app
            .oneshot(
                Request::builder()
                    .uri("/api/v0/events")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap()
            .into_body()
            .collect();

        // When the shutdown is initiated
        shutdown_tx.send(true).unwrap();

        // Then the request to events stops waiting for new events and terminates immediately
        let result = timeout(std::time::Duration::from_millis(500), response_body).await;
        assert!(
            result.is_ok(),
            "SSE stream should terminate after shutdown, but timed out"
        );
    }

    #[tokio::test]
    async fn conflict_error_translates_to_409() {
        // Given a chat that reports any message as a conflict
        #[derive(Clone)]
        struct ChatSaboteur;
        impl SharedChat for ChatSaboteur {
            async fn add_message(&mut self, _: Message) -> Result<(), ChatError> {
                Err(ChatError::Conflict)
            }
        }
        let (_, shutting_down) = watch::channel(false);
        let app = api_router(ChatSaboteur, shutting_down);

        // When a message is sent
        let response = app
            .oneshot(
                Request::post("/api/v0/add_message")
                    .header("content-type", "application/json")
                    .body(Body::from(
                        json!({
                            "id": "019c0a7f-3d8e-7cf8-bea4-3a8614c8da09",
                            "sender": "dummy",
                            "content": "dummy"
                        })
                        .to_string(),
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();

        // Then the response is 409 Conflict
        assert_eq!(response.status(), StatusCode::CONFLICT);
    }

    // Spy that records calls to add_message and events for later inspection
    #[derive(Clone, Default)]
    struct ChatSpy {
        add_message_record: Arc<Mutex<Vec<Message>>>,
        events_record: Arc<Mutex<Vec<u64>>>,
    }

    impl SharedChat for ChatSpy {
        fn events(self, last_event_id: u64) -> impl Stream<Item = Event> + Send {
            self.events_record.lock().unwrap().push(last_event_id);
            tokio_stream::iter(Vec::new())
        }

        async fn add_message(&mut self, message: Message) -> Result<(), ChatError> {
            self.add_message_record.lock().unwrap().push(message);
            Ok(())
        }
    }

    impl ChatSpy {
        fn take_add_message_record(&self) -> Vec<Message> {
            let mut tmp = Vec::new();
            swap(&mut tmp, &mut *self.add_message_record.lock().unwrap());
            tmp
        }

        fn take_events_record(&self) -> Vec<u64> {
            let mut tmp = Vec::new();
            swap(&mut tmp, &mut *self.events_record.lock().unwrap());
            tmp
        }
    }
}

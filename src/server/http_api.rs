use std::{borrow::Cow, convert::Infallible};

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

// Additional imports needed for sabatoge mode, which is only available in debug builds
#[cfg(debug_assertions)]
use axum::routing::put;
#[cfg(debug_assertions)]
use std::sync::Arc;

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
            ChatError::Internal => HttpError {
                status_code: StatusCode::INTERNAL_SERVER_ERROR,
                message: "Internal server error".into(),
            },
        }
    }
}

use super::{last_event_id::LastEventId, terminate_if::terminate_if};

pub fn api_router<C>(chat: C, shutting_down: watch::Receiver<bool>) -> Router
where
    C: SharedChat + Send + Sync + Clone + 'static,
{
    #[cfg(debug_assertions)]
    let (sabotage_tx, sabotage_rx) = watch::channel(false);

    let events_state = EventsState {
        chat: chat.clone(),
        shutting_down,
        #[cfg(debug_assertions)]
        sabotaged: sabotage_rx,
    };

    let router = Router::new()
        .route("/api/v0/events", get(events::<C>))
        .with_state(events_state)
        .route("/api/v0/add_message", post(add_message::<C>))
        .with_state(chat);

    #[cfg(debug_assertions)]
    let router = router
        .route("/sabotage", put(set_sabotage))
        .with_state(Arc::new(sabotage_tx));

    router
}

/// State for the events route.
#[derive(Clone)]
struct EventsState<C> {
    /// The chat which provides the events we want to stream to our client
    chat: C,
    /// We terminate the events stream in case of a shutdown. So the request finishes cleanly for
    /// clients. Also graceful shutdown in Axum waits for requests to finish, yet events never
    /// finish on their own (as there could always be a new message), so graceful shutdown would use
    /// the entire grace period if even one client is still connected.
    shutting_down: watch::Receiver<bool>,
    /// We insert a sabotage error and close the event stream in case sabotage mode is enabled. This
    /// helps testing the UI in error states, without needing to cause disc i/o errors and messing
    /// with persistence.
    #[cfg(debug_assertions)]
    sabotaged: watch::Receiver<bool>,
}

async fn events<C>(
    state: State<EventsState<C>>,
    last_event_id: LastEventId,
) -> Sse<impl Stream<Item = Result<SseEvent, Infallible>> + Send + 'static>
where
    C: SharedChat + Send + 'static,
{
    let EventsState {
        chat,
        shutting_down,
        #[cfg(debug_assertions)]
        sabotaged,
    } = state.0;
    let last_event_id = last_event_id.0;

    // Convert chat events into SSE events
    let events = chat.events(last_event_id).map(|chat_event| {
        let sse_event = match chat_event {
            Ok(event) => event.into(),
            Err(_) => SseEvent::default()
                .event("error")
                .data("Internal server error"),
        };
        Ok(sse_event)
    });

    #[cfg(debug_assertions)]
    let events = maybe_sabotage(sabotaged, events);

    let events = terminate_if(events, shutting_down);

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
            timestamp_ms,
        } = source;
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

/// Developer only endpoint. Enables or disables sabotage mode. Helps with testing the UI behavior
/// in error states.
#[cfg(debug_assertions)]
async fn set_sabotage(
    State(sabotaged): State<Arc<watch::Sender<bool>>>,
    Json(enabled): Json<bool>,
) {
    let _ = sabotaged.send(enabled);
}

#[cfg(debug_assertions)]
fn maybe_sabotage<S>(
    sabotaged: watch::Receiver<bool>,
    events: S,
) -> impl Stream<Item = Result<SseEvent, Infallible>> + Send + 'static
where
    S: Stream<Item = Result<SseEvent, Infallible>> + Send + 'static,
{
    use std::pin::pin;

    let events = terminate_if(events, sabotaged.clone());
    async_stream::stream! {
        let mut events = pin!(events);
        while let Some(event) = futures_util::StreamExt::next(&mut events).await {
            yield event;
        }
        if *sabotaged.borrow() {
            yield Ok(SseEvent::default().event("error").data("Sabotage"));
        }
    }
}

#[cfg(test)]
mod tests {
    use std::{
        mem::take,
        sync::{Arc, Mutex},
        time::{Duration, UNIX_EPOCH},
    };

    use crate::chat::{Event, EventId};

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
            fn events(
                self,
                _last_event_id: EventId,
            ) -> impl Stream<Item = anyhow::Result<Event>> + Send {
                let messages = vec![
                    Event::with_timestamp(
                        EventId(1),
                        Message {
                            id: "019c0050-e4d7-7447-9d8f-81cde690f4a1".parse().unwrap(),
                            sender: "Alice".to_owned(),
                            content: "One".to_owned(),
                        },
                        UNIX_EPOCH + Duration::from_millis(1704531600000),
                    ),
                    Event::with_timestamp(
                        EventId(2),
                        Message {
                            id: "019c0051-c29d-7968-b953-4adc898b1360".parse().unwrap(),
                            sender: "Bob".to_owned(),
                            content: "Two".to_owned(),
                        },
                        UNIX_EPOCH + Duration::from_millis(1704531601000),
                    ),
                    Event::with_timestamp(
                        EventId(3),
                        Message {
                            id: "019c0051-e50d-7ea7-8a0e-f7df4176dd93".parse().unwrap(),
                            sender: "Alice".to_string(),
                            content: "Three".to_owned(),
                        },
                        UNIX_EPOCH + Duration::from_millis(1704531602000),
                    ),
                    Event::with_timestamp(
                        EventId(4),
                        Message {
                            id: "019c0052-09b0-73be-a145-3767cb10cdf6".parse().unwrap(),
                            sender: "Bob".to_owned(),
                            content: "Four".to_owned(),
                        },
                        UNIX_EPOCH + Duration::from_millis(1704531603000),
                    ),
                ];
                tokio_stream::iter(messages).map(Ok)
            }
        }
        let (_, shutting_down) = watch::channel(false);
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
    async fn events_stream_forwards_error_as_sse_error_event() {
        // Given a chat that fails immediately
        #[derive(Clone)]
        struct ChatSaboteur;
        impl SharedChat for ChatSaboteur {
            fn events(self, _: EventId) -> impl Stream<Item = anyhow::Result<Event>> + Send {
                tokio_stream::iter(vec![Err(anyhow::anyhow!("test error"))])
            }
        }
        let (_, shutting_down) = watch::channel(false);
        let app = api_router(ChatSaboteur, shutting_down);

        // When requesting events
        let response = app
            .oneshot(
                Request::builder()
                    .uri("/api/v0/events")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        // Then the response contains:
        // - An "error" event type so the UI can distinguish it from normal events.
        // - A generic error message, not the internal cause.
        // - No id field — the client's Last-Event-ID must not advance past the last successful event.
        let bytes = response
            .into_body()
            .collect()
            .await
            .unwrap()
            .to_bytes()
            .to_vec();
        let expected_body = "\
            event: error\n\
            data: Internal server error\n\
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
        assert_eq!(spy.take_events_record(), vec![EventId(7)]);
    }

    #[tokio::test]
    async fn shutdown_terminates_event_stream() {
        // Given a pending chat and an open request to events
        #[derive(Clone)]
        struct PendingChatStub;
        impl SharedChat for PendingChatStub {
            fn events(
                self,
                _last_event_id: EventId,
            ) -> impl futures_util::Stream<Item = anyhow::Result<Event>> + Send {
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

    #[cfg(debug_assertions)]
    #[tokio::test]
    async fn sabotaged_events_stream_receives_error_event() {
        // Given a server
        let (_, shutting_down) = watch::channel(false);
        let app = api_router(Dummy, shutting_down);

        // When sabotage is enabled and events are requested
        let _ = app
            .clone()
            .oneshot(
                Request::put("/sabotage")
                    .header("content-type", "application/json")
                    .body(Body::from("true"))
                    .unwrap(),
            )
            .await
            .unwrap();

        let response = app
            .oneshot(
                Request::builder()
                    .uri("/api/v0/events")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        // Then the stream contains an error event identifying the saboteur
        let bytes = response
            .into_body()
            .collect()
            .await
            .unwrap()
            .to_bytes()
            .to_vec();
        let expected_body = "\
            event: error\n\
            data: Sabotage\n\
            \n";
        assert_eq!(expected_body, String::from_utf8(bytes).unwrap());
    }

    #[cfg(debug_assertions)]
    #[tokio::test]
    async fn sabotage_interrupts_open_events_stream() {
        // Given a client receiving events from a server
        #[derive(Clone)]
        struct OneEventThenPendingStub;
        impl SharedChat for OneEventThenPendingStub {
            fn events(self, _: EventId) -> impl Stream<Item = anyhow::Result<Event>> + Send {
                tokio_stream::iter(vec![Ok(Event::with_timestamp(
                    EventId(1),
                    Message {
                        id: "019c0050-e4d7-7447-9d8f-81cde690f4a1".parse().unwrap(),
                        sender: "dummy".to_owned(),
                        content: "dummy".to_owned(),
                    },
                    UNIX_EPOCH,
                ))])
                .chain(pending())
            }
        }
        let (_, shutting_down) = watch::channel(false);
        let app = api_router(OneEventThenPendingStub, shutting_down);
        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/api/v0/events")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        let mut body = response.into_body();
        let _first_event = body.frame().await;

        // When sabotage is enabled
        app.oneshot(
            Request::put("/sabotage")
                .header("content-type", "application/json")
                .body(Body::from("true"))
                .unwrap(),
        )
        .await
        .unwrap();

        // Then the stream delivers the sabotage error
        let frame = timeout(Duration::from_secs(1), body.frame())
            .await
            .expect("timed out: sabotage did not interrupt the stream")
            .unwrap()
            .unwrap();
        let bytes = frame.into_data().unwrap();
        assert_eq!(
            "event: error\ndata: Sabotage\n\n",
            String::from_utf8(bytes.to_vec()).unwrap()
        );
    }

    // Spy that records calls to add_message and events for later inspection
    #[derive(Clone, Default)]
    struct ChatSpy {
        add_message_record: Arc<Mutex<Vec<Message>>>,
        events_record: Arc<Mutex<Vec<EventId>>>,
    }

    impl SharedChat for ChatSpy {
        fn events(
            self,
            last_event_id: EventId,
        ) -> impl Stream<Item = anyhow::Result<Event>> + Send {
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
            take(&mut *self.add_message_record.lock().unwrap())
        }

        fn take_events_record(&self) -> Vec<EventId> {
            take(&mut *self.events_record.lock().unwrap())
        }
    }
}

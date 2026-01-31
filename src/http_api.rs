use std::{convert::Infallible, future::ready, time::UNIX_EPOCH};

use axum::{
    Json, Router,
    extract::State,
    response::{Sse, sse::Event as SseEvent},
    routing::{get, post},
};
use futures_util::{Stream, StreamExt as _};
use serde::Serialize;
use uuid::Uuid;

use crate::{
    conversation::{Conversation, Event, Message},
    last_event_id::LastEventId,
};

pub fn api_router<C>(conversation: C) -> Router
where
    C: Conversation + Send + Sync + Clone + 'static,
{
    Router::new()
        .route("/api/v0/messages", get(messages::<C>))
        .route("/api/v0/add_message", post(add_message::<C>))
        .with_state(conversation)
}

async fn messages<C>(
    State(conversation): State<C>,
    last_event_id: LastEventId,
) -> Sse<impl Stream<Item = Result<SseEvent, Infallible>> + Send + 'static>
where
    C: Conversation + Send + 'static,
{
    let last_event_id = last_event_id.0;

    let events = conversation
        .events()
        .filter(move |e| ready(e.id > last_event_id))
        .map(|conversation_event| {
            let sse_event: SseEvent = conversation_event.into();
            Ok(sse_event)
        });
    Sse::new(events)
}

/// A message as represented by the `messages` route.
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
    pub timestamp_ms: u128,
}

impl From<Event> for HttpMessage {
    fn from(source: Event) -> Self {
        let Event {
            id: _,
            message:
                Message {
                    id,
                    sender,
                    content,
                },
            timestamp,
        } = source;
        let timestamp_ms = timestamp.duration_since(UNIX_EPOCH).unwrap().as_millis();
        HttpMessage {
            id,
            sender,
            content,
            timestamp_ms,
        }
    }
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
        // Convert timestamp to milliseconds since epoch
        let timestamp_ms = timestamp.duration_since(UNIX_EPOCH).unwrap().as_millis();
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

async fn add_message<C>(State(mut conversation): State<C>, Json(msg): Json<Message>)
where
    C: Conversation,
{
    conversation.add_message(msg).await;
}

#[cfg(test)]
mod tests {
    use std::{
        mem::swap,
        sync::{Arc, Mutex},
        time::Duration,
    };

    use crate::conversation::Event;

    use super::*;
    use axum::{
        body::Body,
        http::{Request, StatusCode},
    };
    use double_trait::Dummy;
    use http_body_util::BodyExt as _;
    use serde_json::json;
    use tower::ServiceExt; // for `oneshot`

    #[tokio::test]
    async fn messages_route_returns_hardcoded_messages_stream() {
        // Given
        #[derive(Clone)]
        struct ConversationStub;

        impl Conversation for ConversationStub {
            fn events(self) -> impl Stream<Item = Event> + Send {
                let messages = vec![
                    Event {
                        id: 1,
                        message: Message {
                            id: "019c0050-e4d7-7447-9d8f-81cde690f4a1".parse().unwrap(),
                            sender: "Alice".to_owned(),
                            content: "One".to_owned(),
                        },
                        timestamp: UNIX_EPOCH + Duration::from_millis(1704531600000),
                    },
                    Event {
                        id: 2,
                        message: Message {
                            id: "019c0051-c29d-7968-b953-4adc898b1360".parse().unwrap(),
                            sender: "Bob".to_owned(),
                            content: "Two".to_owned(),
                        },
                        timestamp: UNIX_EPOCH + Duration::from_millis(1704531601000),
                    },
                    Event {
                        id: 3,
                        message: Message {
                            id: "019c0051-e50d-7ea7-8a0e-f7df4176dd93".parse().unwrap(),
                            sender: "Alice".to_string(),
                            content: "Three".to_owned(),
                        },
                        timestamp: UNIX_EPOCH + Duration::from_millis(1704531602000),
                    },
                    Event {
                        id: 4,
                        message: Message {
                            id: "019c0052-09b0-73be-a145-3767cb10cdf6".parse().unwrap(),
                            sender: "Bob".to_owned(),
                            content: "Four".to_owned(),
                        },
                        timestamp: UNIX_EPOCH + Duration::from_millis(1704531603000),
                    },
                ];
                tokio_stream::iter(messages)
            }
        }
        let app = api_router(ConversationStub);

        // When
        let response = app
            .oneshot(
                Request::builder()
                    .uri("/api/v0/messages")
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
    async fn events_filtered_based_on_last_event_id() {
        // Given: a conversation with historic events 1..4
        #[derive(Clone)]
        struct ConversationStub;

        impl Conversation for ConversationStub {
            fn events(self) -> impl Stream<Item = Event> + Send {
                let messages = vec![
                    Event { id: 1, message: Message { id: "019c0050-e4d7-7447-9d8f-81cde690f4a1".parse().unwrap(), sender: "Alice".to_owned(), content: "One".to_owned() }, timestamp: UNIX_EPOCH + Duration::from_millis(1704531600000) },
                    Event { id: 2, message: Message { id: "019c0051-c29d-7968-b953-4adc898b1360".parse().unwrap(), sender: "Bob".to_owned(), content: "Two".to_owned() }, timestamp: UNIX_EPOCH + Duration::from_millis(1704531601000) },
                    Event { id: 3, message: Message { id: "019c0051-e50d-7ea7-8a0e-f7df4176dd93".parse().unwrap(), sender: "Alice".to_string(), content: "Three".to_owned() }, timestamp: UNIX_EPOCH + Duration::from_millis(1704531602000) },
                    Event { id: 4, message: Message { id: "019c0052-09b0-73be-a145-3767cb10cdf6".parse().unwrap(), sender: "Bob".to_owned(), content: "Four".to_owned() }, timestamp: UNIX_EPOCH + Duration::from_millis(1704531603000) },
                ];
                tokio_stream::iter(messages)
            }
        }
        let app = api_router(ConversationStub);

        // When: request with Last-Event-ID = 2
        let response = app
            .oneshot(
                Request::builder()
                    .uri("/api/v0/messages")
                    .header("Last-Event-ID", "2")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        // Then: only events with id > 2 are present
        assert_eq!(response.status(), StatusCode::OK);
        let bytes = response
            .into_body()
            .collect()
            .await
            .unwrap()
            .to_bytes()
            .to_vec();
        let expected_body = "id: 3\n\
            data: {\"id\":\"019c0051-e50d-7ea7-8a0e-f7df4176dd93\",\"sender\":\"Alice\",\
            \"content\":\"Three\",\"timestamp_ms\":1704531602000}\n\
            \n\
            id: 4\n\
            data: {\"id\":\"019c0052-09b0-73be-a145-3767cb10cdf6\",\"sender\":\"Bob\",\"content\":\
            \"Four\",\"timestamp_ms\":1704531603000}\n\
            \n";
        assert_eq!(expected_body, String::from_utf8(bytes).unwrap());
    }

    #[tokio::test]
    async fn messages_should_return_content_type_event_stream() {
        // Given
        let app = api_router(Dummy);

        // When
        let response = app
            .oneshot(
                Request::builder()
                    .uri("/api/v0/messages")
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
    async fn add_message_route_forwards_arguments_to_conversation_api() {
        // Given
        let spy = ConversationSpy::default();
        let app = api_router(spy.clone());
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

    // Spy that records calls to add_message for later inspection
    #[derive(Clone, Default)]
    struct ConversationSpy {
        add_message_record: Arc<Mutex<Vec<Message>>>,
    }

    impl Conversation for ConversationSpy {
        async fn add_message(&mut self, message: Message) {
            self.add_message_record.lock().unwrap().push(message);
        }
    }

    impl ConversationSpy {
        fn take_add_message_record(&self) -> Vec<Message> {
            let mut tmp = Vec::new();
            swap(&mut tmp, &mut *self.add_message_record.lock().unwrap());
            tmp
        }
    }
}

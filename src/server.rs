mod http_api;
mod last_event_id;
mod terminate_if;
mod ui;

use std::time::Duration;

use axum::{
    Router,
    http::{HeaderMap, Request, Response},
    routing::get,
};

use tokio::{
    net::{TcpListener, ToSocketAddrs},
    sync::watch,
    task::JoinHandle,
};
use tower_http::{classify::ServerErrorsFailureClass, trace::TraceLayer};
use tracing::{Span, debug, debug_span, error, info};

use crate::chat::SharedChat;

use self::{http_api::api_router, ui::ui_router};

pub struct Server {
    /// Indicates whether the server is about to shut down. Long-lived requests like event streams
    /// watch this in order to short circut and allow the the graceful shutdown to complete faster.
    shutting_down: watch::Sender<bool>,
    join_handle: JoinHandle<()>,
}

impl Server {
    /// Starts the HTTP server providing both the API and UI to clients. While the server runs in
    /// its own thread, the TCP socket is already opened and listened to once this function returns.
    pub async fn new<C>(socket_address: impl ToSocketAddrs, chat: C) -> anyhow::Result<Server>
    where
        C: SharedChat + Send + Sync + Clone + 'static,
    {
        let listener = TcpListener::bind(socket_address).await?;

        // The "Listening" in the event log would indicate to operators that we can do accept
        // incoming connections. Before creating the listener they would have been refused with a
        // "transport endpoint not connect" error. This information is however also implied by the
        // "Ready" message emitted from main. More importantly we provide the port we bind to. In
        // case our input socket address was telling us to bind to port `0` the operation system
        // chooses a free port for us. Only through this log message then the operator will learn
        // on which port the server listens. The integration tests utilize binding to port `0` in
        // order to run in parallel without clashing on ports.
        info!(
            target: "server",
            port = listener
                .local_addr()
                .expect("Listener must have local address after binding")
                .port(),
            "Listening"
        );
        let (shutting_down_sender, mut shutting_down_receiver) = watch::channel(false);
        let join_handle = tokio::spawn(async move {
            let router = router(chat, shutting_down_receiver.clone());
            axum::serve(listener, router)
                .with_graceful_shutdown(async move {
                    shutting_down_receiver
                        .wait_for(|&is_shutting_down| is_shutting_down)
                        .await
                        .expect("Sender for shutdown sender must not be dropped before used.");
                })
                .await
                .expect("axum::serve must not return an error");
        });
        let server = Server {
            shutting_down: shutting_down_sender,
            join_handle,
        };
        Ok(server)
    }

    pub async fn shutdown(self) {
        self.shutting_down.send(true).expect("Receiver must exist");
        self.join_handle.await.unwrap();
    }
}

fn router<C>(chat: C, shutting_down: watch::Receiver<bool>) -> Router
where
    C: SharedChat + Send + Sync + Clone + 'static,
{
    let router = Router::new()
        .route("/health", get(|| async { "OK" }))
        .merge(api_router(chat, shutting_down))
        .merge(ui_router());

    add_tracing_layer(router)
}

/// Extends the router with a tracing layer. We want to log request spans as part of the http
/// target. Function operates on `Router` as the types for Tracing layers or the constraints on
/// Layer traits are rather verbose.
fn add_tracing_layer(router: Router) -> Router {
    // Mostly we want to replace targets like tower_http::trace::on_request with our own "http"
    // target. We imagine not only developers operating klatsch. Therfore what modules and libraries
    // we use should be an implementation detail.
    router.layer(
        TraceLayer::new_for_http()
            .make_span_with(|request: &Request<_>| {
                debug_span!(
                    target: "http",
                    "request",
                    method = %request.method(),
                    uri = %request.uri(),
                )
            })
            .on_request(|_: &Request<_>, _: &Span| {
                debug!(target: "http", "Started");
            })
            .on_response(|response: &Response<_>, latency: Duration, _: &Span| {
                debug!(
                    target: "http",
                    status = response.status().as_u16(),
                    latency_ms = latency.as_millis(),
                    "Finished"
                );
            })
            .on_eos(|_trailers: Option<&HeaderMap>, stream_duration: Duration, _: &Span| {
                  debug!(target: "http", stream_duration_ms = stream_duration.as_millis(), "End of stream");
            })
            .on_failure(
                |error: ServerErrorsFailureClass, latency: Duration, _: &Span| {
                    error!(
                        target: "http",
                        %error,
                        latency_ms = latency.as_millis(),
                        "Failed"
                    );
                },
            ),
    )
}

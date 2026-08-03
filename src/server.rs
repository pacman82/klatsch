mod api;
mod session_cookie;
mod ui;

use std::{net::SocketAddr, path::PathBuf, time::Duration};

use axum::{
    Router,
    http::{HeaderMap, Request, Response},
    routing::get,
};
use axum_server::Handle;
use tokio::{net::TcpListener, sync::watch, task::JoinHandle};
use tower_http::{classify::ServerErrorsFailureClass, trace::TraceLayer};
use tracing::{Span, debug, debug_span, error, info};

use crate::{chat::Chat, http::AuthenticateRequest, sessions::SessionLifecycle, user::Users};

use self::{api::api_router, ui::ui_router};

/// Configuration for the HTTP server interface.
pub struct ServerConfiguration {
    pub host: String,
    pub port: u16,
    pub tls: Option<TlsConfig>,
}

/// Paths to a certificate and private key file, to serve HTTPS directly without a reverse proxy
/// in front of Klatsch.
#[derive(Clone)]
pub struct TlsConfig {
    pub cert_file: PathBuf,
    pub key_file: PathBuf,
}

pub struct Server {
    /// Indicates whether the server is about to shut down. Long-lived requests like event streams
    /// watch this in order to short circut and allow the the graceful shutdown to complete faster.
    shutting_down: watch::Sender<bool>,
    /// Handle to the axum-server instance, used to trigger its graceful shutdown.
    server_handle: Handle<SocketAddr>,
    join_handle: JoinHandle<()>,
}

impl Server {
    /// Starts the HTTP server providing both the API and UI to clients. While the server runs in
    /// its own thread, the TCP socket is already opened and listened to once this function returns.
    pub async fn new(
        config: ServerConfiguration,
        chat: impl Chat + Send + Sync + Clone + 'static,
        users: impl Users + Send + Sync + Clone + 'static,
        sessions: impl SessionLifecycle + AuthenticateRequest + Send + Sync + Clone + 'static,
    ) -> anyhow::Result<Server> {
        // TLS is not wired up yet; config.tls is ignored for now.
        let listener = TcpListener::bind((config.host.as_str(), config.port)).await?;

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
        let listener = listener.into_std()?;

        let (shutting_down_sender, shutting_down_receiver) = watch::channel(false);
        let server_handle = Handle::new();
        let join_handle = tokio::spawn({
            let server_handle = server_handle.clone();
            async move {
                let router = router(chat, users, sessions, shutting_down_receiver);
                axum_server::from_tcp(listener)
                    .expect("Listener must be convertible to a tokio listener")
                    .handle(server_handle)
                    .serve(router.into_make_service())
                    .await
                    .expect("axum-server must not return an error");
            }
        });
        let server = Server {
            shutting_down: shutting_down_sender,
            server_handle,
            join_handle,
        };
        Ok(server)
    }

    pub async fn shutdown(self) {
        self.shutting_down.send(true).expect("Receiver must exist");
        self.server_handle.graceful_shutdown(None);
        self.join_handle.await.unwrap();
    }
}

fn router<C, U, S>(chat: C, users: U, sessions: S, shutting_down: watch::Receiver<bool>) -> Router
where
    C: Chat + Send + Sync + Clone + 'static,
    U: Users + Send + Sync + Clone + 'static,
    S: SessionLifecycle + AuthenticateRequest + Send + Sync + Clone + 'static,
{
    let router = Router::new()
        .route("/health", get(|| async { "OK" }))
        .merge(api_router(chat, users, sessions, shutting_down))
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
    //
    // We could also change the target in formatting, however I do like the terser messageing we've
    // chosen here.
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

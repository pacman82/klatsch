mod api;
mod routes;
mod ui;

use std::{net::SocketAddr, path::PathBuf, time::Duration};

use anyhow::Context;
use axum::{
    Router,
    http::{HeaderMap, Request, Response},
    routing::get,
};
use axum_server::{
    Handle,
    tls_rustls::{RustlsAcceptor, RustlsConfig},
};
use tokio::{net::TcpListener, sync::watch, task::JoinHandle};
use tower_http::{classify::ServerErrorsFailureClass, trace::TraceLayer};
use tracing::{Span, debug, debug_span, error, info};

use crate::{
    authentication::{AuthService, AuthenticateRequest},
    sessions::SessionLifecycle,
    user::{AuthenticateUser, Users},
};

use self::{api::api_router, ui::ui_router};

pub use self::routes::Routes;

/// Configuration for the HTTP server interface.
pub struct ServerConfiguration {
    pub host: String,
    pub port: u16,
    pub tls: TlsConfig,
}

/// How Klatsch handles TLS termination.
#[derive(Clone)]
pub enum TlsConfig {
    /// No encryption. Suitable for local development only.
    Off,
    /// Encryption is used, but a reverse proxy terminates TLS, not Klatsch itself.
    Proxy,
    /// Klatsch terminates TLS itself, using the given certificate and private key.
    Static {
        cert_file: PathBuf,
        key_file: PathBuf,
    },
}

impl TlsConfig {
    async fn to_rustls(&self) -> anyhow::Result<Option<RustlsConfig>> {
        match self {
            TlsConfig::Static {
                cert_file,
                key_file,
            } => RustlsConfig::from_pem_file(cert_file, key_file)
                .await
                .with_context(|| {
                    format!(
                        "Failed to load TLS certificate ({}) or key ({})",
                        cert_file.display(),
                        key_file.display()
                    )
                })
                .map(Some),
            TlsConfig::Off | TlsConfig::Proxy => Ok(None),
        }
    }

    /// Whether the connection between client and Klatsch is encrypted, be it terminated by
    /// Klatsch itself or by a reverse proxy in front of it.
    fn is_encrypted(&self) -> bool {
        match self {
            TlsConfig::Off => false,
            TlsConfig::Proxy | TlsConfig::Static { .. } => true,
        }
    }
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
        chat: impl Routes + Send + 'static,
        users: impl Users + Routes + AuthenticateUser + Send + Sync + Clone + 'static,
        sessions: impl SessionLifecycle + AuthenticateRequest + Send + Sync + Clone + 'static,
    ) -> anyhow::Result<Server> {
        let ServerConfiguration { host, port, tls } = config;

        let listener = TcpListener::bind((host.as_str(), port)).await?;
        // Fail early in case tls configuration is invalid
        let encrypted = tls.is_encrypted();
        let maybe_rustls_config = tls.to_rustls().await?;

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

        let (shutting_down_sender, shutting_down_receiver) = watch::channel(false);
        let server_handle = Handle::new();
        let join_handle = tokio::spawn({
            let server_handle = server_handle.clone();
            async move {
                let router = router(chat, users, sessions, shutting_down_receiver, encrypted);
                let server = axum_server::Server::from_listener(listener).handle(server_handle);
                let result = match maybe_rustls_config {
                    Some(rustls_config) => {
                        server
                            .acceptor(RustlsAcceptor::new(rustls_config))
                            .serve(router.into_make_service())
                            .await
                    }
                    None => server.serve(router.into_make_service()).await,
                };
                result.expect("axum-server must not return an error");
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

fn router<C, U, S>(
    chat: C,
    users: U,
    sessions: S,
    shutting_down: watch::Receiver<bool>,
    encrypted: bool,
) -> Router
where
    C: Routes + 'static,
    U: Users + Routes + AuthenticateUser + Send + Sync + Clone + 'static,
    S: SessionLifecycle + AuthenticateRequest + Send + Sync + Clone + 'static,
{
    let auth_service = AuthService::new(users.clone(), sessions.clone());

    let router = Router::new()
        .route("/health", get(|| async { "OK" }))
        .merge(api_router(
            chat,
            users,
            auth_service,
            shutting_down,
            encrypted,
        ))
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

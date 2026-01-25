use axum::{Router, routing::get};
use tokio::net::{TcpListener, ToSocketAddrs};

pub struct Server {}

impl Server {
    pub async fn new(
        socket_address: impl ToSocketAddrs,
        shutdown: impl Future<Output = ()> + Send + 'static,
    ) -> anyhow::Result<()> {
        let app = Router::new()
            .route("/health", get(|| async { "OK" }))
            .route("/", get(|| async { "Hello, World!" }));
        let listener = TcpListener::bind(socket_address).await?;
        axum::serve(listener, app)
            .with_graceful_shutdown(shutdown)
            .await
            .expect("axum::serve never fails");
        Ok(())
    }
}

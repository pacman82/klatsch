mod configuration;
mod shutdown;

use axum::{Router, routing::get};
use dotenv::dotenv;
use tokio::net::TcpListener;

use shutdown::shutdown_signal;

use crate::configuration::Configuration;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Register shutdown signal handler
    let shutdown = shutdown_signal().await;

    // Source environment from .env file and load configuration. Errors during sourcing the .env
    // file are ignored. In case of it not existing we intend to use the plain environment.
    dotenv().ok();
    let cfg = Configuration::from_env()?;

    let app = Router::new()
        .route("/health", get(|| async { "OK" }))
        .route("/", get(|| async { "Hello, World!" }));
    let listener = TcpListener::bind(cfg.socket_addr()).await?;
    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown)
        .await
        .expect("axum::serve never fails");

    Ok(())
}

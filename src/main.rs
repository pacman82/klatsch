mod configuration;
mod conversation;
mod http_api;
mod last_event_id;
mod server;
mod shutdown;
mod terminate_on_shutdown;
mod ui;

use dotenv::dotenv;
use tracing::info;
use tracing_subscriber::{EnvFilter, FmtSubscriber};

use crate::{
    configuration::Configuration, conversation::ConversationRuntime, server::Server,
    shutdown::shutdown_signal,
};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Register shutdown signal handler
    let shutdown = shutdown_signal().await;

    // Source environment from .env file and load configuration. Errors during sourcing the .env
    // file are ignored. In case of it not existing we intend to use the plain environment.
    dotenv().ok();
    let cfg = Configuration::from_env()?;

    let subscriber = FmtSubscriber::builder()
        .with_env_filter(
            EnvFilter::default()
                .add_directive("memory_serve=off".parse().unwrap())
                .add_directive("klatsch=info".parse().unwrap()),
        )
        .finish();
    tracing::subscriber::set_global_default(subscriber)
        .expect("Setting global default provider must not fail.");

    info!("Starting");

    // Forward messages between peers in the conversation
    let conversation = ConversationRuntime::new();

    // Answer incoming HTTP requests
    let server = Server::new(cfg.socket_addr(), conversation.api()).await?;
    info!("Ready");

    // Run our application until a shutdown signal is received
    shutdown.await;
    info!("Shutdown signal received, shutting down...");

    // Gracefully shutdown the http server.
    server.shutdown().await;

    // Let's shutdown the conversation runtime as well. After the http interface, since the http
    // interface relies on it.
    conversation.shutdown().await;

    info!("Shutdown complete, exiting.");
    Ok(())
}

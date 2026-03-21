mod chat;
mod configuration;
mod persistence;
mod server;
mod shutdown;
mod tracing;

use ::tracing::info;
use dotenv::dotenv;

use crate::{
    chat::{ChatRuntime, PersistentChat},
    configuration::Configuration,
    server::Server,
    shutdown::shutdown_signal,
    tracing::init_tracing,
};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Register shutdown signal handler
    let shutdown = shutdown_signal().await;

    // Source environment from .env file and load configuration. Errors during sourcing the .env
    // file are ignored. In case of it not existing we intend to use the plain environment.
    dotenv().ok();
    let cfg = Configuration::from_env()?;

    init_tracing();

    info!(target: "app", "Starting");

    // Initialize persistence for chat
    let history = PersistentChat::new(cfg.persistence_dir()).await?;

    // Forward messages between peers in the chat
    let chat = ChatRuntime::new(history);

    // Answer incoming HTTP requests
    let server = Server::new(cfg.socket_addr(), chat.client()).await?;
    info!(target: "app", "Ready");

    // Run our application until a shutdown signal is received
    shutdown.await;
    info!(target: "app", "Shutdown signal received");

    // Gracefully shutdown the http server.
    server.shutdown().await;

    // Let's shutdown the chat runtime as well. After the http interface, since the http interface
    // relies on it.
    chat.shutdown().await;

    info!(target: "app", "Shutdown complete");
    Ok(())
}

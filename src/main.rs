mod chat;
mod configuration;
mod server;
mod shutdown;

use std::io::stderr;

use dotenv::dotenv;
use tracing::info;
use tracing_subscriber::{EnvFilter, FmtSubscriber};

use crate::{
    chat::{ChatRuntime, SqLiteChatHistory},
    configuration::Configuration,
    server::Server,
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
        // Surpress rendering of module path. We do not want to bother our operators with our
        // internal module structure.
        .with_target(false)
        .with_writer(stderr)
        .with_env_filter(
            EnvFilter::default()
                .add_directive("memory_serve=off".parse().unwrap())
                .add_directive("klatsch=info".parse().unwrap()),
        )
        .finish();
    tracing::subscriber::set_global_default(subscriber)
        .expect("Setting global default provider must not fail.");

    info!("Starting");

    // Initialize persistence for chat
    let history = SqLiteChatHistory::new().await?;

    // Forward messages between peers in the chat
    let chat = ChatRuntime::new(history);

    // Answer incoming HTTP requests
    let server = Server::new(cfg.socket_addr(), chat.client()).await?;
    info!("Ready");

    // Run our application until a shutdown signal is received
    shutdown.await;
    info!("Shutdown signal received");

    // Gracefully shutdown the http server.
    server.shutdown().await;

    // Let's shutdown the chat runtime as well. After the http interface, since the http interface
    // relies on it.
    chat.shutdown().await;

    info!("Shutdown complete");
    Ok(())
}

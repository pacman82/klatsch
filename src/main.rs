mod configuration;
mod conversation;
mod server;
mod shutdown;
mod ui;

use dotenv::dotenv;
use tracing::Level;
use tracing_subscriber::FmtSubscriber;

use crate::{
    configuration::Configuration, conversation::Conversation, server::Server,
    shutdown::shutdown_signal,
};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Register shutdown signal handler
    let shutdown = shutdown_signal().await;

    let subscriber = FmtSubscriber::builder()
        .with_max_level(Level::INFO)
        .finish();
    tracing::subscriber::set_global_default(subscriber)
        .expect("Setting global default provider must not fail.");

    // Source environment from .env file and load configuration. Errors during sourcing the .env
    // file are ignored. In case of it not existing we intend to use the plain environment.
    dotenv().ok();
    let cfg = Configuration::from_env()?;

    // Forward messages between peers in the conversation
    let conversation = Conversation::new();

    // Answer incoming HTTP requests
    let server = Server::new(cfg.socket_addr(), conversation).await?;

    // Run our application until a shutdown signal is received
    shutdown.await;

    // Gracefully shutdown the http server
    server.shutdown().await;

    Ok(())
}

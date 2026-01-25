mod configuration;
mod server;
mod shutdown;

use dotenv::dotenv;
use shutdown::shutdown_signal;

use crate::{configuration::Configuration, server::Server};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Register shutdown signal handler
    let shutdown = shutdown_signal().await;

    // Source environment from .env file and load configuration. Errors during sourcing the .env
    // file are ignored. In case of it not existing we intend to use the plain environment.
    dotenv().ok();
    let cfg = Configuration::from_env()?;

    Server::new(cfg.socket_addr(), shutdown).await?;

    Ok(())
}

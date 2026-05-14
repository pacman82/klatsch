mod chat;
mod configuration;
mod klatsch;
mod persistence;
mod server;
mod shutdown;
mod tracing;

use dotenvy::dotenv;

use crate::{
    configuration::Configuration, klatsch::Klatsch, shutdown::shutdown_signal,
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

    let app = Klatsch::new(&cfg).await?;

    // Run our application until a shutdown signal is received
    shutdown.await;

    app.shutdown().await;
    Ok(())
}

use std::io::stderr;

use tracing::level_filters::LevelFilter;
use tracing_subscriber::{EnvFilter, FmtSubscriber};

pub fn init_tracing() {
    let subscriber = FmtSubscriber::builder()
        .with_writer(stderr)
        .with_env_filter(
            EnvFilter::builder()
                .with_env_var("LOG_LEVEL")
                .with_default_directive(LevelFilter::INFO.into())
                .from_env_lossy()
                .add_directive("memory_serve=off".parse().unwrap()),
        )
        .finish();
    tracing::subscriber::set_global_default(subscriber)
        .expect("Setting global default provider must not fail.");
}

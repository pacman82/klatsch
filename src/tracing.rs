use std::io::stderr;

use tracing_subscriber::{EnvFilter, FmtSubscriber};

pub fn init_tracing() {
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
}

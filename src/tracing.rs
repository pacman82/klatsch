mod format;

use std::io::stderr;

use tracing::level_filters::LevelFilter;
use tracing_subscriber::{EnvFilter, FmtSubscriber};

use self::format::OperatorFormat;

pub fn init_tracing() {
    let subscriber = FmtSubscriber::builder()
        .with_writer(stderr)
        .event_format(OperatorFormat)
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

/// Maps the module string to an operator friendly target.
///
/// The rust tracinig ecosystem uses the module path as the default target for log messages. This
/// function allows us to map `axum::serve` to `server` or `tower_http::trace::on_request` to
/// `http`. I.e. we can hide our internal module structure and dependencies and translate it into
/// meaningful categories for an operator.
fn operator_target(target: &'static str) -> &'static str {
    match target {
        "axum::serve" => "server",
        other => other,
    }
}

use futures_util::FutureExt;
use tokio::signal::ctrl_c;

/// Registers signal handlers for termination and interrupt signals. I.e. this application will
/// gracefully shutdown with Ctrl+C as well as container stop. The returned future completes as soon
/// as a signal is received.
pub fn shutdown_signal() -> impl Future<Output = ()> {
    // We are careful to install the the ctrl+c handler synchronously. We achieve this by wrapping
    // the resulting future using `map`, rather than `await`ing it in an async block and wrapping
    // the result.
    let ctrl_c = ctrl_c().map(|result| result.expect("failed to install Ctrl+C handler"));

    #[cfg(unix)]
    use tokio::signal::unix;

    #[cfg(unix)]
    let terminate = {
        let mut signal =
            unix::signal(unix::SignalKind::terminate()).expect("failed to install signal handler");

        async move { signal.recv().map(|_| ()).await }
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    async move {
        tokio::select! {
            () = ctrl_c => {},
            () = terminate => {},
        }
    }
}

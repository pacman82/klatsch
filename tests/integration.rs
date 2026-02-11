use std::{
    process::Stdio,
    time::{Duration, Instant},
};

use reqwest::Client;
use tokio::{
    io::{AsyncBufReadExt, BufReader},
    process::{Child, Command},
    sync::watch,
    task::JoinHandle,
    time::timeout,
};

#[cfg(unix)]
use nix::{
    sys::signal::{self, Signal},
    unistd::Pid,
};

// On Windows, signal handling is different and SIGTERM is not supported in the same way. We can use
// Ctrl-C to initiate a graceful shutdown in Windows, however this is tricky from a test suite.
// Amongst other things, Ctrl-C is send to the entire process group and would also interrupt the
// test runner itself. Therefore, we skip tests using SIGTERM on Windows. We could run them on
// windows if we provide an alternative way to trigger graceful shutdown in the server.
#[cfg(not(windows))]
#[tokio::test]
async fn server_shuts_down_within_1_sec() {
    // Given a running server
    let mut child = TestServer::new().await;

    // When sending SIGTERM to the server process
    child.send_sigterm();
    // And measuring the time it takes to shut down
    let start = Instant::now();
    child
        .wait_for_termination(Duration::from_secs(5))
        .await
        .unwrap();
    let end = Instant::now();

    // Then it should have taken less than 1 second to shut down
    let max_duration = Duration::from_secs(1);
    assert!(end - start <= max_duration)
}

// On Windows, signal handling is different and SIGTERM is not supported in the same way. We can use
// Ctrl-C to initiate a graceful shutdown in Windows, however this is tricky from a test suite.
// Amongst other things, Ctrl-C is send to the entire process group and would also interrupt the
// test runner itself. Therefore, we skip tests using SIGTERM on Windows. We could run them on
// windows if we provide an alternative way to trigger graceful shutdown in the server.
#[cfg(not(windows))]
#[tokio::test]
async fn server_finished_with_success_status_code_after_terminate() {
    // Given a runninng server process
    let mut child = TestServer::new().await;

    // When sending SIGTERM to the server process
    child.send_sigterm();
    // And waiting for it to finish
    let output = child
        .wait_for_termination(Duration::from_secs(5))
        .await
        .unwrap();

    // Then it should have finished with a success status code (`0`)
    assert!(output.success())
}

#[tokio::test]
async fn server_boots_within_one_sec() {
    // Given a start time
    let start = Instant::now();

    // When measuring the time it takes to boot
    let _child = TestServer::new().await;
    let end = Instant::now();

    // Then it should have taken less than 1 second to boot up
    let max_duration = Duration::from_secs(1);
    assert!(end - start <= max_duration)
}

#[tokio::test]
async fn health_check_returns_200_ok() {
    // Given a running server
    let server = TestServer::new().await;

    // When requesting the health check endpoint
    let response = server.health_check().await;

    // Then it should return 200 OK
    assert_eq!(response.status(), 200);
    assert_eq!(response.text().await.unwrap(), "OK");
}

#[cfg(not(windows))]
#[tokio::test]
async fn shutdown_within_1_sec_with_active_events_stream_client() {
    // Given a running server
    let mut child = TestServer::new().await;

    // and a client connected to the events stream
    let _event_stream_body = child
        .request_event_stream()
        .await
        .error_for_status()
        .unwrap()
        .bytes();

    // When sending SIGTERM to the server process
    child.send_sigterm();
    // And measuring the time it takes to shut down
    let start = Instant::now();
    child
        .wait_for_termination(Duration::from_secs(2))
        .await
        .unwrap();
    let end = Instant::now();

    // Then it should have taken less than 1 second to shut down
    let max_duration = Duration::from_secs(1);
    assert!(
        end - start <= max_duration,
        "Shutdown took longer than 1 second with an active events stream client"
    );
}

/// Allows to interact with a Klatsch Server Running in its own process.
struct TestServer {
    // Process member is currently unused in windows. This might change if we can have test helpers
    // for graceful shutdown on windows as well.
    #[cfg_attr(windows, allow(dead_code))]
    process: ServerProcess,
    _log_observer: LogObserver,
    port: u16,
    client: Client,
}

impl TestServer {
    async fn new() -> Self {
        let mut child = Command::new(env!("CARGO_BIN_EXE_klatsch"))
            // Let the OS assign a free port so tests can run in parallel without clashing. The
            // actual port is learned later from the server's log output.
            .env("PORT", "0")
            // Suppress ANSI escape codes so the log observer can parse log lines as plain text.
            .env("NO_COLOR", "1")
            // We do not want the log output of the process to clutter the output of our test
            // runner.
            .stdout(Stdio::null())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        let stderr = child.stderr.take().unwrap();
        let process = ServerProcess::new(child);
        let mut log_observer = LogObserver::new(stderr);
        timeout(Duration::from_secs(5), log_observer.wait_for_ready())
            .await
            .expect("Server did not become ready within 5 seconds");
        let port = log_observer.port().await;
        let client = Client::new();
        Self {
            process,
            _log_observer: log_observer,
            port,
            client,
        }
    }

    async fn health_check(&self) -> reqwest::Response {
        self.client
            .get(format!("http://localhost:{}/health", self.port))
            .send()
            .await
            .expect("Failed to send health check request")
    }

    // Supported on every platform, but so far only used in unix-specific tests.
    #[cfg(unix)]
    async fn request_event_stream(&mut self) -> reqwest::Response {
        self.client
            .get(format!("http://localhost:{}/api/v0/events", self.port))
            .send()
            .await
            .expect("Failed to connect to events stream")
    }

    #[cfg(unix)]
    fn send_sigterm(&mut self) {
        self.process.send_sigterm();
    }

    #[cfg(unix)]
    async fn wait_for_termination(
        &mut self,
        timeout: Duration,
    ) -> std::io::Result<std::process::ExitStatus> {
        self.process.wait_for_termination(timeout).await
    }
}

/// Observes the server's log output on stderr and communicates observations (like "Ready" and the
/// listening port) back via watch channels.
struct LogObserver {
    _task: JoinHandle<()>,
    ready: watch::Receiver<bool>,
    port: watch::Receiver<Option<u16>>,
}

impl LogObserver {
    fn new(stderr: tokio::process::ChildStderr) -> Self {
        let (ready_tx, ready) = watch::channel(false);
        let (port_tx, port) = watch::channel(None);
        let _task = tokio::spawn(async move {
            let mut lines = BufReader::new(stderr).lines();
            while let Ok(Some(line)) = lines.next_line().await {
                if let Some(port) = parse_port(&line) {
                    let _ = port_tx.send(Some(port));
                }
                if line.contains("Ready") {
                    let _ = ready_tx.send(true);
                }
                // Continue reading even after all observations have been made, so the pipe
                // buffer does not fill up and block the server process.
            }
        });
        Self { _task, ready, port }
    }

    /// Waits for the server process to emit "Ready" to standard error. This indicates that the
    /// server has been successfully booted and is ready to receive requests.
    async fn wait_for_ready(&mut self) {
        self.ready
            .wait_for(|&ready| ready)
            .await
            .expect("Server process exited before becoming ready");
    }

    /// Waits for the server to log the port it is listening on.
    async fn port(&mut self) -> u16 {
        self.port
            .wait_for(|port| port.is_some())
            .await
            .expect("Server process exited before logging its port")
            .unwrap()
    }
}

/// RAII wrapper around child process. Takes care of killing the process once the helper goes out of
/// scope.
struct ServerProcess {
    child: Child,
}

impl ServerProcess {
    fn new(child: Child) -> Self {
        Self { child }
    }

    #[cfg(unix)]
    fn send_sigterm(&mut self) {
        let pid = Pid::from_raw(self.child.id().expect("Test process must be running") as i32);
        signal::kill(pid, Signal::SIGTERM).unwrap();
    }

    #[cfg(unix)]
    async fn wait_for_termination(
        &mut self,
        timeout: Duration,
    ) -> std::io::Result<std::process::ExitStatus> {
        tokio::time::timeout(timeout, self.child.wait()).await?
    }
}

/// Extracts the port number from a log line like `... Listening port=3000`.
fn parse_port(line: &str) -> Option<u16> {
    let suffix = line.split("port=").nth(1)?;
    suffix.split_whitespace().next()?.parse().ok()
}

// Try to make sure, none of the processes we spawn are left after finishing the tests.
impl Drop for ServerProcess {
    fn drop(&mut self) {
        let _ = self.child.start_kill();
    }
}

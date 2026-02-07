use std::{
    process::Stdio,
    time::{Duration, Instant},
};

use reqwest::Client;
use tokio::{
    process::{Child, Command},
    time::sleep,
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
    let mut child = TestServer::new(3001).await;
    child.wait_for_health_check().await;

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
    let mut child = TestServer::new(3002).await;
    child.wait_for_health_check().await;

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
    let mut child = TestServer::new(3003).await;
    child.wait_for_health_check().await;
    let end = Instant::now();
    // Then it should have taken less than 1 second to boot up
    let max_duration = Duration::from_secs(1);
    assert!(end - start <= max_duration)
}

#[cfg(not(windows))]
#[tokio::test]
async fn shutdown_within_1_sec_with_active_events_stream_client() {
    // Given a running server
    let mut child = TestServer::new(3004).await;
    child.wait_for_health_check().await;

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
    process: ServerProcess,
    port: u16,
    client: Client,
}

impl TestServer {
    async fn new(port: u16) -> Self {
        let process = ServerProcess::new(port);
        let client = Client::new();
        Self {
            process,
            port,
            client,
        }
    }

    async fn wait_for_health_check(&mut self) {
        tokio::time::timeout(Duration::from_secs(5), async {
            while let Err(_) = self
                .client
                .get(format!("http://localhost:{}/health", self.port))
                .send()
                .await
            {
                sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .expect("Server did not become healthy within 5 seconds");
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

/// RAII wrapper around child process. Takes care of killing the process ones the helper goes out of
/// scope.
struct ServerProcess {
    child: Child,
}

impl ServerProcess {
    pub fn new(port: u16) -> Self {
        let binary_path = env!("CARGO_BIN_EXE_klatsch");
        let child = Command::new(binary_path)
            .env("PORT", port.to_string())
            // We do not want the log output of the process to clutter the output of our test
            // runner.
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .unwrap();

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

// Try to make sure, none of the processes we spawn are left after finishing the tests.
impl Drop for ServerProcess {
    fn drop(&mut self) {
        let _ = self.child.start_kill();
    }
}

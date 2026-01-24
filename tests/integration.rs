use std::{
    io,
    process::ExitStatus,
    time::{Duration, Instant},
};

use reqwest::Client;
use tokio::{
    process::{Child, Command},
    time::{self, sleep},
};

#[cfg(unix)]
use nix::{
    sys::signal::{self, Signal},
    unistd::Pid,
};

#[tokio::test]
async fn server_shuts_down_within_1_sec() {
    // Given a running server
    let mut child = TestServerProcess::new(3001);
    child.wait_for_health_check().await;

    // // When sending SIGTERM to the server process
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

#[tokio::test]
async fn server_finished_with_success_status_code_after_terminate() {
    // Given a runninng server process
    let mut child = TestServerProcess::new(3002);
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

/// Runs the server process as a command from the binary. Useful for testing stuff affecting the
/// entire process. E.g. signal handling, shutdown, etc.
struct TestServerProcess {
    child: Child,
    port: u16,
    client: Client,
}

impl TestServerProcess {
    fn new(port: u16) -> Self {
        let binary_path = env!("CARGO_BIN_EXE_tattle");
        let child = Command::new(binary_path)
            .env("PORT", port.to_string())
            .spawn()
            .unwrap();
        let client = Client::new();
        Self {
            child,
            port,
            client,
        }
    }

    async fn wait_for_health_check(&mut self) {
        while let Err(_) = self
            .client
            .get(format!("http://localhost:{}/health", self.port))
            .send()
            .await
        {
            sleep(Duration::from_millis(10)).await;
        }
    }

    #[cfg(unix)]
    fn send_sigterm(&mut self) {
        let pid = Pid::from_raw(self.child.id().expect("Test process must be running") as i32);
        signal::kill(pid, Signal::SIGTERM).unwrap();
    }

    #[cfg(windows)]
    fn send_sigterm(&mut self) {
        use windows::Win32::System::Console::{CTRL_C_EVENT, GenerateConsoleCtrlEvent};
        let id = self.child.id().expect("Test process must be running");
        unsafe { GenerateConsoleCtrlEvent(CTRL_C_EVENT, id).unwrap() }
    }

    async fn wait_for_termination(&mut self, timeout: Duration) -> io::Result<ExitStatus> {
        time::timeout(timeout, self.child.wait()).await?
    }
}

// Try to make sure, none of the processes we spawn are left after finishing the tests.
impl Drop for TestServerProcess {
    fn drop(&mut self) {
        let _ = self.child.start_kill();
    }
}

use std::{
    path::Path,
    process::Stdio,
    time::{Duration, Instant},
};

use eventsource_stream::Eventsource as _;
use futures_util::{Stream, StreamExt as _};
use reqwest::Client;
use serde_json::json;
use tokio::{
    fs,
    io::{AsyncBufReadExt, BufReader},
    process::{Child, Command},
    sync::watch,
    task::JoinHandle,
    time::timeout,
};
use uuid::Uuid;

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
    let mut child = TestEnv::new(None, false).await;

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
    let mut child = TestEnv::new(None, false).await;

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
    let _child = TestEnv::new(None, false).await;
    let end = Instant::now();

    // Then it should have taken less than 1 second to boot up
    let time_to_boot = end - start;
    eprintln!("Time to boot: {} ms", time_to_boot.as_millis());
    let max_duration = Duration::from_secs(1);
    assert!(time_to_boot <= max_duration)
}

#[tokio::test]
async fn health_check_returns_200_ok() {
    // Given a running server
    let server = TestEnv::new(None, false).await;

    // When requesting the health check endpoint
    let response = server.health_check().await;

    // Then it should return 200 OK
    assert_eq!(response.status(), 200);
    assert_eq!(response.text().await.unwrap(), "OK");
}

#[tokio::test]
async fn https() {
    // Given a server configured to use TLS
    let server = TestEnv::new(None, true).await;

    // When requesting the health check endpoint
    let response = server.health_check().await;

    // Then it should return 200 OK
    assert_eq!(response.status(), 200);
    assert_eq!(response.text().await.unwrap(), "OK");
}

#[tokio::test]
async fn sent_messages_appear_in_event_stream() {
    // Given a server with two messages
    let server = TestEnv::new(None, false).await;
    let alice_id = server.register_alice().await;
    let bob_id = server.register_bob().await;
    let alice_session = server.login_alice().await;
    let bob_session = server.login_bob().await;
    let msg = json!({ "id": "019c0ab6-9d11-75ef-ab02-60f070b1582a", "content": "Hello" });
    server.send_message(msg, &alice_session).await;
    let msg = json!({ "id": "019c0ab6-9d11-7a5b-abde-cb349e5fd995", "content": "Hi there" });
    server.send_message(msg, &bob_session).await;

    // When requesting the events stream
    let mut sse = server.events(&alice_session).await;

    // Then both messages appear in order
    let event_1 = timeout(Duration::from_secs(1), sse.next())
        .await
        .expect("timed out waiting for first event")
        .unwrap();
    let event_2 = timeout(Duration::from_secs(1), sse.next())
        .await
        .expect("timed out waiting for second event")
        .unwrap();
    let data_1: serde_json::Value = serde_json::from_str(&event_1.data).unwrap();
    let data_2: serde_json::Value = serde_json::from_str(&event_2.data).unwrap();
    assert_eq!(data_1["sender_id"], alice_id.to_string());
    assert_eq!(data_1["content"], "Hello");
    assert_eq!(data_2["sender_id"], bob_id.to_string());
    assert_eq!(data_2["content"], "Hi there");
}

#[cfg(not(windows))]
#[tokio::test]
async fn persistence() {
    // Given a server that accepted two messages
    let persistence_dir = tempfile::tempdir().unwrap();

    let mut server = TestEnv::new(Some(persistence_dir.path()), false).await;
    let alice_id = server.register_alice().await;
    let bob_id = server.register_bob().await;
    let alice_session = server.login_alice().await;
    let bob_session = server.login_bob().await;
    let msg = json!({ "id": "019c0ab6-9d11-75ef-ab02-60f070b1582a", "content": "Hello" });
    server.send_message(msg, &alice_session).await;
    let msg = json!({ "id": "019c0ab6-9d11-7a5b-abde-cb349e5fd995", "content": "Hi there" });
    server.send_message(msg, &bob_session).await;

    // When restarting with the same database
    server.send_sigterm();
    server
        .wait_for_termination(Duration::from_secs(5))
        .await
        .unwrap();
    let server = TestEnv::new(Some(persistence_dir.path()), false).await;
    let alice_session = server.login_alice().await;

    // Then the messages are still available
    let mut sse = server.events(&alice_session).await;
    let event_1 = timeout(Duration::from_secs(1), sse.next())
        .await
        .expect("timed out waiting for first event")
        .unwrap();
    let event_2 = timeout(Duration::from_secs(1), sse.next())
        .await
        .expect("timed out waiting for second event")
        .unwrap();
    let data_1: serde_json::Value = serde_json::from_str(&event_1.data).unwrap();
    assert_eq!(data_1["sender_id"], alice_id.to_string());
    assert_eq!(data_1["content"], "Hello");
    let data_2: serde_json::Value = serde_json::from_str(&event_2.data).unwrap();
    assert_eq!(data_2["sender_id"], bob_id.to_string());
    assert_eq!(data_2["content"], "Hi there");
}

#[tokio::test]
async fn second_server_on_same_persistence_directory_is_rejected() {
    // Given a running server on a persistence directory
    let persistence_dir = tempfile::tempdir().unwrap();
    let _running = TestEnv::new(Some(persistence_dir.path()), false).await;

    // When starting a second server on the same directory
    let stderr = TestEnv::spawn_expecting_termination(Some(persistence_dir.path())).await;

    // Then it exits with an error identifying the locked directory
    assert!(
        stderr.contains("Another instance is already using the same persistence directory"),
        "unexpected stderr: {stderr}"
    );
}

#[tokio::test]
async fn load_v1_persistence() {
    // Given
    let persistence_dir = tempfile::tempdir().unwrap();
    fs::copy("tests/v1.db", persistence_dir.path().join("klatsch.db"))
        .await
        .unwrap();

    // When restarting with the same database
    let server = TestEnv::new(Some(persistence_dir.path()), false).await;
    let alice_session = server.login_alice().await;

    // Then the messages are still available
    let mut sse = server.events(&alice_session).await;
    let event_1 = timeout(Duration::from_secs(1), sse.next())
        .await
        .expect("timed out waiting for first event")
        .unwrap();
    let event_2 = timeout(Duration::from_secs(1), sse.next())
        .await
        .expect("timed out waiting for second event")
        .unwrap();
    let data_1: serde_json::Value = serde_json::from_str(&event_1.data).unwrap();
    let sender_1: Uuid = serde_json::from_value(data_1["sender_id"].clone()).unwrap();
    assert_eq!(server.user(sender_1, &alice_session).await["name"], "Bob");
    assert_eq!(data_1["content"], "Hi Alice");
    let data_2: serde_json::Value = serde_json::from_str(&event_2.data).unwrap();
    let sender_2: Uuid = serde_json::from_value(data_2["sender_id"].clone()).unwrap();
    assert_eq!(server.user(sender_2, &alice_session).await["name"], "Alice");
    assert_eq!(data_2["content"], "Hi Bob");
}

#[cfg(not(windows))]
#[tokio::test]
async fn shutdown_within_1_sec_with_active_events_stream_client() {
    // Given a running server
    let mut child = TestEnv::new(None, false).await;

    // and a client connected to the events stream
    child.register_alice().await;
    let alice_session = child.login_alice().await;
    let _event_stream_body = child
        .request_event_stream(&alice_session)
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
struct TestEnv {
    process: TestServer,
    // Empty working directory so the server's dotenv() doesn't pick up the developer's .env file.
    _working_dir: tempfile::TempDir,
    client: Client,
}

impl TestEnv {
    async fn new(db_path: Option<&Path>, tls: bool) -> Self {
        let working_dir = tempfile::tempdir().unwrap();
        let process = TestServer::new(working_dir.path(), db_path, tls)
            .await
            .unwrap();
        let client = if tls {
            // The fixture certificate is self-signed, so it is not in any trust store the client
            // would otherwise validate against.
            Client::builder()
                .danger_accept_invalid_certs(true)
                .build()
                .expect("Failed to build https client")
        } else {
            Client::new()
        };
        Self {
            process,
            _working_dir: working_dir,
            client,
        }
    }

    /// Spawns a server expected to exit before becoming ready, and returns its stderr.
    async fn spawn_expecting_termination(db_path: Option<&Path>) -> String {
        let working_dir = tempfile::tempdir().unwrap();
        TestServer::new(working_dir.path(), db_path, false)
            .await
            .err()
            .expect("server became ready, but had been expected to not start up")
    }

    async fn health_check(&self) -> reqwest::Response {
        self.client
            .get(format!("{}/health", self.process.base_url()))
            .send()
            .await
            .expect("Failed to send health check request")
    }

    async fn request_event_stream(&self, session: &str) -> reqwest::Response {
        self.client
            .get(format!("{}/api/v0/events", self.process.base_url()))
            .header("cookie", format!("session={session}"))
            .send()
            .await
            .expect("Failed to connect to events stream")
    }

    async fn events(&self, session: &str) -> impl Stream<Item = eventsource_stream::Event> {
        self.request_event_stream(session)
            .await
            .bytes_stream()
            .eventsource()
            .map(|r| r.expect("SSE event must be parseable"))
    }

    async fn user(&self, id: Uuid, session: &str) -> serde_json::Value {
        self.client
            .get(format!("{}/api/v0/users/{}", self.process.base_url(), id))
            .header("cookie", format!("session={session}"))
            .send()
            .await
            .expect("Failed to fetch user")
            .error_for_status()
            .expect("Server returned error for user fetch")
            .json()
            .await
            .expect("Failed to parse user")
    }

    async fn register_user(&self, name: &str, password: &str) -> Uuid {
        self.client
            .post(format!("{}/api/v0/signup", self.process.base_url()))
            .json(&json!({ "name": name, "password": password }))
            .send()
            .await
            .expect("Failed to register user")
            .error_for_status()
            .expect("Server rejected user registration")
            .json::<Uuid>()
            .await
            .expect("Failed to parse user id")
    }

    async fn register_alice(&self) -> Uuid {
        self.register_user("Alice", "alice_password").await
    }

    async fn register_bob(&self) -> Uuid {
        self.register_user("Bob", "bob_password").await
    }

    async fn login_alice(&self) -> String {
        self.login("Alice", "alice_password").await
    }

    async fn login_bob(&self) -> String {
        self.login("Bob", "bob_password").await
    }

    async fn login(&self, name: &str, password: &str) -> String {
        let response = self
            .client
            .post(format!("{}/api/v0/login", self.process.base_url()))
            .json(&json!({ "name": name, "password": password }))
            .send()
            .await
            .expect("Failed to login");
        response
            .cookies()
            .find(|c| c.name() == "session")
            .expect("Login response must set session cookie")
            .value()
            .to_owned()
    }

    async fn send_message(&self, message: serde_json::Value, session: &str) {
        self.client
            .post(format!("{}/api/v0/add_message", self.process.base_url()))
            .header("cookie", format!("session={session}"))
            .json(&message)
            .send()
            .await
            .expect("Failed to send message")
            .error_for_status()
            .expect("Server rejected message");
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

fn server_command(db_path: Option<&Path>, working_dir: &Path) -> Command {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_klatsch"));
    cmd.current_dir(working_dir)
        // Let the OS assign a free port so tests can run in parallel without clashing. The
        // actual port is learned later from the server's log output.
        .env("PORT", "0")
        // Suppress ANSI escape codes so the log observer can parse log lines as plain text.
        .env("NO_COLOR", "1")
        // We do not want the log output of the process to clutter the output of our test
        // runner.
        .stdout(Stdio::null())
        .stderr(Stdio::piped());
    match db_path {
        Some(path) => {
            cmd.env("PERSISTENCE_DIRECTORY", path);
        }
        None => {
            cmd.env("PERSISTENCE", "false");
        }
    }
    cmd
}

/// A klatsch server process ready to receive requests
struct TestServer {
    _log_observer: LogObserver,
    port: u16,
    child: Child,
    tls: bool,
}

impl TestServer {
    async fn new(working_dir: &Path, db_path: Option<&Path>, tls: bool) -> Result<Self, String> {
        let mut cmd = server_command(db_path, working_dir);
        let cmd = cmd.kill_on_drop(true);
        if tls {
            let tests_folder = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests");
            cmd.env("TLS", "static")
                .env("TLS_CERT_FILE", tests_folder.join("cert.pem"))
                .env("TLS_KEY_FILE", tests_folder.join("key.pem"));
        }
        let mut child = cmd.spawn().unwrap();
        let stderr = child.stderr.take().unwrap();
        let mut log_observer = LogObserver::new(stderr);
        let success = timeout(Duration::from_secs(5), log_observer.wait_for_ready())
            .await
            .expect("Server process did not become ready, nor did it fail fast");

        if !success {
            return Err(log_observer.wait_for_eof().await);
        }

        let port = log_observer.port().await;
        Ok(Self {
            _log_observer: log_observer,
            port,
            child,
            tls,
        })
    }

    fn base_url(&self) -> String {
        let scheme = if self.tls { "https" } else { "http" };
        format!("{scheme}://localhost:{}", self.port)
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

/// Observes the server's log output on stderr and communicates observations (like "Ready" and the
/// listening port) back via watch channels.
struct LogObserver {
    task: JoinHandle<String>,
    ready: watch::Receiver<bool>,
    port: watch::Receiver<Option<u16>>,
}

impl LogObserver {
    fn new(stderr: tokio::process::ChildStderr) -> Self {
        let (ready_tx, ready) = watch::channel(false);
        let (port_tx, port) = watch::channel(None);
        let task = tokio::spawn(async move {
            let mut stderr_complete = String::new();
            let mut lines = BufReader::new(stderr).lines();
            while let Ok(Some(line)) = lines.next_line().await {
                stderr_complete.push_str(&line);
                stderr_complete.push('\n');
                if let Some(port) = parse_port(&line) {
                    let _ = port_tx.send(Some(port));
                }
                if line.contains("Ready") {
                    let _ = ready_tx.send(true);
                }
                // Continue reading even after all observations have been made, so the pipe
                // buffer does not fill up and block the server process.
            }
            stderr_complete
        });
        Self { task, ready, port }
    }

    /// Waits for the server process to emit "Ready" to standard error. This indicates that the
    /// server has been successfully booted and is ready to receive requests.
    ///
    /// Returns `true` if the server is ready, `false` if it terminated before becoming ready
    async fn wait_for_ready(&mut self) -> bool {
        self.ready.wait_for(|&ready| ready).await.is_ok()
    }

    /// Waits for the server to log the port it is listening on.
    async fn port(&mut self) -> u16 {
        self.port
            .wait_for(|port| port.is_some())
            .await
            .expect("Server process exited before logging its port")
            .unwrap()
    }

    async fn wait_for_eof(self) -> String {
        self.task
            .await
            .expect("Thread observing server logs must be joinable")
    }
}

/// Extracts the port number from a log line like `... Listening port=3000`.
fn parse_port(line: &str) -> Option<u16> {
    let suffix = line.split("port=").nth(1)?;
    suffix.split_whitespace().next()?.parse().ok()
}

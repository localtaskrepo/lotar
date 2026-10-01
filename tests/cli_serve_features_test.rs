//! Serve command lifecycle and option contracts.
//!
//! Every test owns the `lotar serve` child it spawns: readiness is the
//! bind-then-banner contract from DEV-76 (`--port 0` binds an OS-assigned
//! port and the startup banner reports the actual bound port), teardown goes
//! through the test stop endpoint plus one wake-up connection, and the child
//! is always reaped with a bounded wait and a kill fallback. No test touches
//! processes it did not spawn.

mod common;

use common::TestFixtures;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::process::{Child, Command, Stdio};
use std::sync::mpsc;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

const READY_TIMEOUT: Duration = Duration::from_secs(15);
const STOP_TIMEOUT: Duration = Duration::from_secs(5);

struct ServeChild {
    child: Option<Child>,
    host: String,
    port: u16,
    lines: mpsc::Receiver<String>,
    stdout_dump: Arc<Mutex<String>>,
    stderr_dump: Arc<Mutex<String>>,
}

/// Outcome of starting a serve child that may legitimately fail to start.
enum ServeStart {
    Ready(ServeChild),
    Failed {
        status: std::process::ExitStatus,
        stdout: String,
        stderr: String,
    },
}

/// Spawn a serve child that is expected to reach readiness; panics with the
/// captured output otherwise.
fn spawn_serve(cwd: &std::path::Path, extra_args: &[&str]) -> ServeChild {
    match try_spawn_serve(cwd, extra_args) {
        ServeStart::Ready(serve) => serve,
        ServeStart::Failed {
            status,
            stdout,
            stderr,
        } => panic!(
            "serve exited before a readiness banner ({status}); stdout:\n{stdout}\nstderr:\n{stderr}"
        ),
    }
}

/// Spawn a serve child and report either readiness or the exact early-exit
/// output, so tests can assert real startup contracts without assuming any
/// foreign listener stays alive.
fn try_spawn_serve(cwd: &std::path::Path, extra_args: &[&str]) -> ServeStart {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_lotar"));
    cmd.current_dir(cwd)
        .env_remove("LOTAR_TASKS_DIR")
        .env_remove("LOTAR_PORT")
        .env_remove("LOTAR_SERVER_PORT")
        .env("LOTAR_IGNORE_HOME_CONFIG", "1")
        .env("LOTAR_TEST_SILENT", "1")
        .env("LOTAR_ALLOW_TEST_STOP", "1")
        .args(extra_args)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let mut child = cmd.spawn().expect("spawn lotar serve");

    let stdout = child.stdout.take().expect("piped stdout");
    let stderr = child.stderr.take().expect("piped stderr");
    let (tx, rx) = mpsc::channel::<String>();
    let stdout_dump = Arc::new(Mutex::new(String::new()));
    let stdout_sink = Arc::clone(&stdout_dump);
    thread::spawn(move || {
        for line in BufReader::new(stdout).lines() {
            match line {
                Ok(line) => {
                    if let Ok(mut sink) = stdout_sink.lock() {
                        sink.push_str(&line);
                        sink.push('\n');
                    }
                    if tx.send(line).is_err() {
                        break;
                    }
                }
                Err(_) => break,
            }
        }
    });
    let stderr_dump = Arc::new(Mutex::new(String::new()));
    let stderr_sink = Arc::clone(&stderr_dump);
    thread::spawn(move || {
        let mut reader = BufReader::new(stderr);
        let mut chunk = String::new();
        while let Ok(n) = reader.read_line(&mut chunk) {
            if n == 0 {
                break;
            }
            if let Ok(mut sink) = stderr_sink.lock() {
                sink.push_str(&chunk);
            }
            chunk.clear();
        }
    });

    let mut serve = ServeChild {
        child: Some(child),
        host: String::new(),
        port: 0,
        lines: rx,
        stdout_dump,
        stderr_dump,
    };
    match serve.wait_for_banner() {
        Ok(()) => ServeStart::Ready(serve),
        Err(_) => match serve.child.take() {
            Some(mut child) => ServeStart::Failed {
                status: child.wait().expect("reap failed serve child"),
                stdout: serve.stdout_snapshot(),
                stderr: serve.stderr_snapshot(),
            },
            None => panic!("serve child ownership lost"),
        },
    }
}

impl ServeChild {
    fn wait_for_banner(&mut self) -> Result<(), ()> {
        let deadline = Instant::now() + READY_TIMEOUT;
        let mut seen = String::new();
        loop {
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                self.fail_with_output(format!(
                    "serve did not advertise readiness within {READY_TIMEOUT:?}; stdout so far:\n{seen}"
                ));
            }
            match self.lines.recv_timeout(remaining) {
                Ok(line) => {
                    seen.push_str(&line);
                    seen.push('\n');
                    if let Some(rest) = line.strip_prefix("   URL: http://") {
                        let (host, port) = rest
                            .rsplit_once(':')
                            .unwrap_or_else(|| panic!("malformed readiness banner: {line}"));
                        let port: u16 = port.trim().parse().unwrap_or_else(|_| {
                            panic!("non-numeric port in readiness banner: {line}")
                        });
                        self.host = host.to_string();
                        self.port = port;
                        return Ok(());
                    }
                }
                Err(mpsc::RecvTimeoutError::Timeout) => {
                    self.fail_with_output(format!(
                        "serve did not advertise readiness within {READY_TIMEOUT:?}; stdout so far:\n{seen}"
                    ));
                }
                Err(mpsc::RecvTimeoutError::Disconnected) => {
                    return Err(());
                }
            }
        }
    }

    fn stdout_snapshot(&self) -> String {
        self.stdout_dump
            .lock()
            .map(|s| s.clone())
            .unwrap_or_default()
    }

    fn stderr_snapshot(&self) -> String {
        self.stderr_dump
            .lock()
            .map(|s| s.clone())
            .unwrap_or_default()
    }

    fn fail_with_output(&self, message: String) -> ! {
        panic!("{message}\nstderr:\n{}", self.stderr_snapshot());
    }

    fn connect(&self) -> TcpStream {
        TcpStream::connect((self.host.as_str(), self.port))
            .unwrap_or_else(|e| panic!("connect to {}:{} failed: {e}", self.host, self.port))
    }

    fn http_request(&self, method: &str, path: &str, body: Option<&str>) -> (u16, Vec<u8>) {
        let mut stream = self.connect();
        stream
            .set_read_timeout(Some(Duration::from_millis(1_500)))
            .unwrap();
        let payload = body.unwrap_or("");
        let req = format!(
            "{method} {path} HTTP/1.1\r\nHost: {}\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{payload}",
            self.host,
            payload.len()
        );
        stream.write_all(req.as_bytes()).unwrap();
        stream.flush().unwrap();

        let mut header_buf = Vec::new();
        let mut tmp = [0u8; 1024];
        loop {
            let n = stream.read(&mut tmp).unwrap_or(0);
            if n == 0 {
                break;
            }
            header_buf.extend_from_slice(&tmp[..n]);
            if let Some(pos) = header_buf.windows(4).position(|w| w == b"\r\n\r\n") {
                let body_leftover = header_buf.split_off(pos + 4);
                let headers_text = String::from_utf8_lossy(&header_buf);
                let status = headers_text
                    .lines()
                    .next()
                    .and_then(|line| line.split_whitespace().nth(1))
                    .and_then(|value| value.parse::<u16>().ok())
                    .unwrap_or(0);
                let content_length = headers_text
                    .lines()
                    .filter_map(|line| {
                        line.split_once(':')
                            .map(|(key, value)| (key.trim(), value.trim()))
                    })
                    .find(|(key, _)| key.eq_ignore_ascii_case("Content-Length"))
                    .and_then(|(_, value)| value.parse::<usize>().ok())
                    .unwrap_or(0);

                let mut body_bytes = body_leftover;
                while body_bytes.len() < content_length {
                    let n = stream.read(&mut tmp).unwrap_or(0);
                    if n == 0 {
                        break;
                    }
                    body_bytes.extend_from_slice(&tmp[..n]);
                }
                body_bytes.truncate(content_length);
                return (status, body_bytes);
            }
        }
        (0, Vec::new())
    }

    fn get(&self, path: &str) -> (u16, Vec<u8>) {
        self.http_request("GET", path, None)
    }

    fn post_json(&self, path: &str, body: &str) -> (u16, Vec<u8>) {
        self.http_request("POST", path, Some(body))
    }

    /// Stop the owned child through the test stop endpoint, wake the accept
    /// loop once, and reap the process with a bounded wait (kill fallback).
    /// Returns the exit code the process had.
    fn stop(&mut self) -> std::process::ExitStatus {
        let _ = self.get("/__test/stop");
        let _ = TcpStream::connect((self.host.as_str(), self.port));

        let child = self.child.as_mut().expect("serve child ownership");
        let deadline = Instant::now() + STOP_TIMEOUT;
        loop {
            match child.try_wait().expect("poll serve child") {
                Some(status) => return status,
                None if Instant::now() >= deadline => {
                    child.kill().expect("kill serve child after stop timeout");
                    return child.wait().expect("reap killed serve child");
                }
                None => thread::sleep(Duration::from_millis(25)),
            }
        }
    }

    fn reap(&mut self) {
        if let Some(mut child) = self.child.take() {
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}

impl Drop for ServeChild {
    fn drop(&mut self) {
        self.reap();
    }
}

fn reserve_ephemeral_port() -> u16 {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    drop(listener);
    port
}

fn sorted_titles(payload: &serde_json::Value) -> Vec<String> {
    let mut titles: Vec<String> = payload["data"]["tasks"]
        .as_array()
        .expect("data.tasks array")
        .iter()
        .map(|task| task["title"].as_str().expect("title").to_string())
        .collect();
    titles.sort();
    titles
}

#[test]
fn serve_ephemeral_port_is_advertised_and_api_answers() {
    let fixtures = TestFixtures::new();
    let mut serve = spawn_serve(
        fixtures.temp_dir.path(),
        &["serve", "--host", "127.0.0.1", "--port", "0"],
    );

    assert!(serve.port != 0, "port 0 must be replaced by the bound port");
    assert_eq!(serve.host, "127.0.0.1");

    let (status, body) = serve.get("/api/tasks/list");
    assert_eq!(status, 200, "body: {}", String::from_utf8_lossy(&body));
    let payload: serde_json::Value = serde_json::from_slice(&body).expect("task list JSON payload");
    assert!(payload["data"].is_object(), "payload: {payload}");
    assert_eq!(
        payload["data"]["tasks"].as_array().map_or(0, Vec::len),
        0,
        "empty workspace must serve an empty task list: {payload}"
    );

    let exit = serve.stop();
    assert!(
        exit.success(),
        "serve must exit cleanly after the stop endpoint, got {exit}"
    );
}

#[test]
fn serve_reports_requested_port_exactly() {
    let fixtures = TestFixtures::new();
    let port = reserve_ephemeral_port();
    let mut serve = spawn_serve(
        fixtures.temp_dir.path(),
        &["serve", "--host", "127.0.0.1", "--port", &port.to_string()],
    );

    assert_eq!(serve.port, port, "banner must report the requested port");
    let (status, _) = serve.get("/api/tasks/list");
    assert_eq!(status, 200);

    assert!(serve.stop().success());
}

#[test]
fn serve_short_port_flag_sets_requested_port() {
    let fixtures = TestFixtures::new();
    let port = reserve_ephemeral_port();
    let mut serve = spawn_serve(
        fixtures.temp_dir.path(),
        &["serve", "-p", &port.to_string()],
    );

    assert_eq!(serve.port, port, "-p must set the serve port");
    let (status, _) = serve.get("/api/tasks/list");
    assert_eq!(status, 200);

    assert!(serve.stop().success());
}

#[test]
fn serve_wildcard_host_accepts_loopback_connections() {
    let fixtures = TestFixtures::new();
    let mut serve = spawn_serve(
        fixtures.temp_dir.path(),
        &["serve", "--host", "0.0.0.0", "--port", "0"],
    );

    assert_eq!(serve.host, "0.0.0.0");
    let (status, _) = serve.get("/api/tasks/list");
    assert_eq!(status, 200, "wildcard bind must accept loopback traffic");

    assert!(serve.stop().success());
}

#[test]
fn serve_localhost_host_answers_on_advertised_host() {
    let fixtures = TestFixtures::new();
    let mut serve = spawn_serve(
        fixtures.temp_dir.path(),
        &["serve", "--host", "localhost", "--port", "0"],
    );

    assert_eq!(serve.host, "localhost");
    let (status, _) = serve.get("/api/tasks/list");
    assert_eq!(status, 200, "connections must work on the advertised host");

    assert!(serve.stop().success());
}

#[test]
fn serve_verbose_and_json_format_flags_keep_server_ready() {
    let fixtures = TestFixtures::new();
    let mut serve = spawn_serve(
        fixtures.temp_dir.path(),
        &[
            "--format=json",
            "serve",
            "--host",
            "127.0.0.1",
            "--port",
            "0",
            "--verbose",
        ],
    );

    let (status, _) = serve.get("/api/tasks/list");
    assert_eq!(status, 200);

    assert!(serve.stop().success());
}

#[test]
fn serve_creates_missing_tasks_dir_and_serves_empty_list() {
    let fixtures = TestFixtures::new();
    let missing_dir = fixtures.temp_dir.path().join("created-by-serve");
    assert!(!missing_dir.exists());

    let mut serve = spawn_serve(
        fixtures.temp_dir.path(),
        &[
            "--tasks-dir",
            &missing_dir.to_string_lossy(),
            "serve",
            "--host",
            "127.0.0.1",
            "--port",
            "0",
        ],
    );

    let (status, body) = serve.get("/api/tasks/list");
    assert_eq!(status, 200, "body: {}", String::from_utf8_lossy(&body));

    assert!(serve.stop().success());
    assert!(
        missing_dir.is_dir(),
        "serve must materialize the requested tasks directory"
    );
}

#[test]
fn serve_invalid_port_value_fails_with_argument_error() {
    let fixtures = TestFixtures::new();
    let output = Command::new(env!("CARGO_BIN_EXE_lotar"))
        .current_dir(fixtures.temp_dir.path())
        .env_remove("LOTAR_TASKS_DIR")
        .env("LOTAR_IGNORE_HOME_CONFIG", "1")
        .env("LOTAR_TEST_SILENT", "1")
        .args(["serve", "--port=99999"])
        .output()
        .expect("run serve with invalid port");

    assert!(!output.status.success());
    assert!(
        String::from_utf8_lossy(&output.stderr)
            .to_lowercase()
            .contains("error"),
        "stderr should report the argument error: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn serve_held_port_fails_before_readiness_banner() {
    let fixtures = TestFixtures::new();
    let holder = TcpListener::bind("127.0.0.1:0").unwrap();
    let held_port = holder.local_addr().unwrap().port();

    let output = Command::new(env!("CARGO_BIN_EXE_lotar"))
        .current_dir(fixtures.temp_dir.path())
        .env_remove("LOTAR_TASKS_DIR")
        .env("LOTAR_IGNORE_HOME_CONFIG", "1")
        .env("LOTAR_TEST_SILENT", "1")
        .args([
            "serve",
            "--host",
            "127.0.0.1",
            "--port",
            &held_port.to_string(),
        ])
        .output()
        .expect("run serve against a held port");

    assert!(!output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        !stdout.contains("URL: http"),
        "no readiness banner may precede a failed bind: {stdout}"
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("Failed to bind"),
        "bind failure must be reported on stderr: {stderr}"
    );
}

#[test]
fn serve_help_documents_bind_options() {
    let fixtures = TestFixtures::new();
    let output = Command::new(env!("CARGO_BIN_EXE_lotar"))
        .current_dir(fixtures.temp_dir.path())
        .env_remove("LOTAR_TASKS_DIR")
        .env("LOTAR_IGNORE_HOME_CONFIG", "1")
        .env("LOTAR_TEST_SILENT", "1")
        .args(["serve", "--help"])
        .output()
        .expect("run serve --help");

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("--port"), "help must document --port");
    assert!(stdout.contains("--host"), "help must document --host");
    assert!(
        stdout.contains("8080"),
        "help must document the default port"
    );
}

#[test]
fn serve_with_project_data_lists_exact_tasks() {
    let fixtures = TestFixtures::new();
    let temp_dir = fixtures.temp_dir.path();

    for args in [
        vec![
            "task",
            "add",
            "Web UI Test Task",
            "--project=test-project",
            "--type=feature",
            "--priority=high",
            "--assignee=test@example.com",
        ],
        vec![
            "task",
            "add",
            "API Test Task",
            "--project=test-project",
            "--type=bug",
            "--priority=high",
        ],
    ] {
        let out = Command::new(env!("CARGO_BIN_EXE_lotar"))
            .current_dir(temp_dir)
            .env_remove("LOTAR_TASKS_DIR")
            .env("LOTAR_IGNORE_HOME_CONFIG", "1")
            .env("LOTAR_TEST_SILENT", "1")
            .args(&args)
            .output()
            .expect("seed task");
        assert!(
            out.status.success(),
            "seed failed: {}",
            String::from_utf8_lossy(&out.stderr)
        );
    }
    let out = Command::new(env!("CARGO_BIN_EXE_lotar"))
        .current_dir(temp_dir)
        .env_remove("LOTAR_TASKS_DIR")
        .env("LOTAR_IGNORE_HOME_CONFIG", "1")
        .env("LOTAR_TEST_SILENT", "1")
        .args(["status", "2", "in_progress", "--project=test-project"])
        .output()
        .expect("set status");
    assert!(
        out.status.success(),
        "status update failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );

    let mut serve = spawn_serve(temp_dir, &["serve", "--host", "127.0.0.1", "--port", "0"]);
    let (status, body) = serve.get("/api/tasks/list");
    assert_eq!(status, 200, "body: {}", String::from_utf8_lossy(&body));
    let payload: serde_json::Value = serde_json::from_slice(&body).expect("task list JSON payload");

    assert_eq!(
        sorted_titles(&payload),
        vec!["API Test Task", "Web UI Test Task"],
        "served task list must match the seeded data exactly"
    );
    let statuses: Vec<(String, String)> = payload["data"]["tasks"]
        .as_array()
        .expect("data.tasks array")
        .iter()
        .map(|task| {
            (
                task["title"].as_str().unwrap_or_default().to_string(),
                task["status"].as_str().unwrap_or_default().to_string(),
            )
        })
        .collect();
    assert_eq!(
        statuses,
        vec![
            ("API Test Task".to_string(), "InProgress".to_string()),
            ("Web UI Test Task".to_string(), "Todo".to_string()),
        ],
        "served statuses must match the seeded transitions exactly"
    );

    assert!(serve.stop().success());
}

#[test]
fn serve_honors_explicit_tasks_dir_for_api_requests() {
    let fixtures = TestFixtures::new();
    let temp_dir = fixtures.temp_dir.path();
    let custom_tasks_dir = temp_dir.join("sandbox-data").join(".tasks");
    std::fs::create_dir_all(&custom_tasks_dir).unwrap();
    std::fs::write(
        custom_tasks_dir.join("config.yml"),
        "default:\n  project: SAN\nissue:\n  states: [Todo, InProgress, Done]\n  priorities: [Low, Medium, High]\n  types: [Feature, Bug]\n",
    )
    .unwrap();

    let mut serve = spawn_serve(
        temp_dir,
        &[
            "--tasks-dir",
            &custom_tasks_dir.to_string_lossy(),
            "serve",
            "--host",
            "127.0.0.1",
            "--port",
            "0",
        ],
    );

    let (status, body) =
        serve.post_json("/api/tasks/add?project=SAN", r#"{"title":"Sandbox task"}"#);
    let exit = serve.stop();

    assert_eq!(
        status,
        201,
        "unexpected response: {}",
        String::from_utf8_lossy(&body)
    );
    assert!(
        custom_tasks_dir.join("SAN").join("1.yml").exists(),
        "task should be created in explicit tasks dir"
    );
    assert!(
        !temp_dir.join(".tasks").join("SAN").join("1.yml").exists(),
        "task should not be created under cwd/.tasks when --tasks-dir is set"
    );
    assert!(exit.success());
}

#[test]
fn serve_implicit_default_port_falls_back_when_busy_and_serves_workspace() {
    let fixtures = TestFixtures::new();
    let temp_dir = fixtures.temp_dir.path();

    // Seed one task so the fallback port provably serves this workspace.
    let out = Command::new(env!("CARGO_BIN_EXE_lotar"))
        .current_dir(temp_dir)
        .env_remove("LOTAR_TASKS_DIR")
        .env_remove("LOTAR_PORT")
        .env_remove("LOTAR_SERVER_PORT")
        .env("LOTAR_IGNORE_HOME_CONFIG", "1")
        .env("LOTAR_TEST_SILENT", "1")
        .args([
            "task",
            "add",
            "Fallback Serves This Workspace",
            "--project=fall",
        ])
        .output()
        .expect("seed task");
    assert!(
        out.status.success(),
        "seed failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );

    // This case owns the default port when available. nextest runs every
    // test as its own process, so the two 8080-touching tests in this
    // binary may race each other for it; both use own-or-foreign tolerant
    // branches that assert the same real contract either way.
    let holder = TcpListener::bind(("127.0.0.1", 8080));
    let owned = holder.is_ok();

    let mut serve = spawn_serve(temp_dir, &["serve", "--host", "127.0.0.1"]);
    let stderr = serve.stderr_snapshot();
    let stdout = serve.stdout_snapshot();

    if owned {
        // Deterministic contract: the untouched built-in default moves once
        // to an OS-assigned port and says so.
        assert_ne!(
            serve.port, 8080,
            "implicit default must fall back when 8080 is held"
        );
        assert!(
            stderr.contains("8080") && stderr.contains(&serve.port.to_string()),
            "fallback warning must name preferred 8080 and actual {}: {stderr}",
            serve.port
        );
    } else {
        // Foreign holder on 8080: assert the consistent startup contract
        // without assuming that server stays alive — either we started on
        // the default itself (holder released; no move) or we moved away
        // with a warning naming both ports.
        if serve.port == 8080 {
            assert!(
                !stderr.contains("serving on port"),
                "no fallback warning when the default was free: {stderr}"
            );
        } else {
            assert!(
                stderr.contains("8080") && stderr.contains(&serve.port.to_string()),
                "fallback warning must name preferred 8080 and actual {}: {stderr}",
                serve.port
            );
        }
    }

    // Banner integrity: the Port and URL lines agree, stay plain (piped
    // stdout must never carry ANSI), and the URL line keeps its exact
    // machine-parseable shape.
    assert!(
        stdout.contains(&format!("   Port: {}", serve.port)),
        "Port line must report the actual port plainly: {stdout}"
    );
    assert!(
        stdout.contains(&format!("   URL: http://127.0.0.1:{}", serve.port)),
        "URL line must stay plain and match the actual port: {stdout}"
    );
    assert!(
        !stdout.contains('\u{1b}'),
        "piped stdout must not contain ANSI escapes: {stdout}"
    );

    let (status, body) = serve.get("/api/tasks/list");
    assert_eq!(status, 200, "body: {}", String::from_utf8_lossy(&body));
    let payload: serde_json::Value = serde_json::from_slice(&body).expect("task list JSON payload");
    assert_eq!(
        sorted_titles(&payload),
        vec!["Fallback Serves This Workspace"],
        "server on the fallback port must serve this workspace's tasks: {payload}"
    );

    assert!(serve.stop().success());
    drop(holder);
}

#[test]
fn serve_explicit_default_port_is_strict_when_held() {
    let fixtures = TestFixtures::new();

    let holder = TcpListener::bind(("127.0.0.1", 8080));
    if let Ok(_holder) = holder {
        // Deterministic: an explicit --port 8080 is a configured port, not
        // the implicit default, so it must fail exactly like any other held
        // port instead of moving.
        let output = Command::new(env!("CARGO_BIN_EXE_lotar"))
            .current_dir(fixtures.temp_dir.path())
            .env_remove("LOTAR_TASKS_DIR")
            .env_remove("LOTAR_PORT")
            .env_remove("LOTAR_SERVER_PORT")
            .env("LOTAR_IGNORE_HOME_CONFIG", "1")
            .env("LOTAR_TEST_SILENT", "1")
            .args(["serve", "--host", "127.0.0.1", "--port", "8080"])
            .output()
            .expect("run serve with explicit 8080 against a held port");

        assert!(
            !output.status.success(),
            "explicit --port 8080 must not fall back"
        );
        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(
            !stdout.contains("URL: http"),
            "no readiness banner may precede a failed bind: {stdout}"
        );
        assert!(
            stderr.contains("Failed to bind"),
            "bind failure must be reported: {stderr}"
        );
        assert!(
            !stderr.contains("serving on port"),
            "no fallback notice for an explicit port: {stderr}"
        );
    } else {
        // Foreign holder: keep the assertion honest — the explicit port must
        // either fail strictly or bind 8080 itself (holder released). It
        // must never silently move to another port.
        match try_spawn_serve(
            fixtures.temp_dir.path(),
            &["serve", "--host", "127.0.0.1", "--port", "8080"],
        ) {
            ServeStart::Ready(mut serve) => {
                assert_eq!(
                    serve.port, 8080,
                    "explicit 8080 must bind exactly 8080, never move"
                );
                assert!(
                    !serve.stderr_snapshot().contains("serving on port"),
                    "no fallback notice for an explicit port"
                );
                assert!(serve.stop().success());
            }
            ServeStart::Failed { stderr, stdout, .. } => {
                assert!(
                    stderr.contains("Failed to bind"),
                    "strict failure must report the bind error: {stderr}"
                );
                assert!(
                    !stdout.contains("URL: http") && !stderr.contains("serving on port"),
                    "no banner, no fallback notice for an explicit port: {stdout} / {stderr}"
                );
            }
        }
    }
}

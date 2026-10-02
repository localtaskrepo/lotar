//! Serve port resolution contracts (DEV-72): `--port` beats env and config,
//! env beats config files, and every explicitly configured port — env var,
//! config key, or flag — is honored strictly (no fallback). Only the
//! untouched built-in default may move to an OS-assigned port.
//!
//! Every spawned `lotar serve` child is owned by the test that started it
//! and is reaped before assertions end; all ports used are either held by
//! the test itself or OS-assigned. No foreign process is ever touched.

mod common;

use common::TestFixtures;
use std::io::{BufRead, BufReader, Read};
use std::net::TcpListener;
use std::process::{Child, Command, Stdio};
use std::sync::mpsc;
use std::thread;
use std::time::{Duration, Instant};

const READY_TIMEOUT: Duration = Duration::from_secs(15);
const STOP_TIMEOUT: Duration = Duration::from_secs(5);

enum Start {
    Ready(ServeChild),
    Failed {
        status: std::process::ExitStatus,
        stdout: String,
        stderr: String,
    },
}

enum OutputLine {
    Stdout(String),
    Stderr(String),
}

struct ServeChild {
    child: Option<Child>,
    host: String,
    port: u16,
    lines: mpsc::Receiver<OutputLine>,
    stdout_dump: std::sync::Arc<std::sync::Mutex<String>>,
    stderr_dump: std::sync::Arc<std::sync::Mutex<String>>,
    readers: Vec<thread::JoinHandle<()>>,
}

fn prepare_command(cwd: &std::path::Path, args: &[&str]) -> Command {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_lotar"));
    cmd.current_dir(cwd)
        .env_remove("LOTAR_TASKS_DIR")
        .env_remove("LOTAR_PORT")
        .env_remove("LOTAR_SERVER_PORT")
        .env("LOTAR_IGNORE_HOME_CONFIG", "1")
        .env("LOTAR_TEST_SILENT", "1")
        .env("LOTAR_ALLOW_TEST_STOP", "1")
        .args(args);
    cmd
}

/// Spawn `lotar serve` with extra env vars applied after the hygiene clears.
fn try_spawn_serve_env(cwd: &std::path::Path, args: &[&str], env_sets: &[(&str, &str)]) -> Start {
    let mut cmd = prepare_command(cwd, args);
    for (key, value) in env_sets {
        cmd.env(key, value);
    }
    cmd.stdout(Stdio::piped()).stderr(Stdio::piped());
    let mut serve = capture_serve_child(cmd.spawn().expect("spawn lotar serve"));
    if serve.wait_for_banner().is_ok() {
        Start::Ready(serve)
    } else if let Some(mut child) = serve.child.take() {
        Start::Failed {
            status: child.wait().expect("reap failed serve child"),
            stdout: serve.stdout_snapshot(),
            stderr: serve.stderr_snapshot(),
        }
    } else {
        panic!("serve child ownership lost");
    }
}

fn capture_stream(
    stream: impl Read + Send + 'static,
    dump: std::sync::Arc<std::sync::Mutex<String>>,
    tx: mpsc::Sender<OutputLine>,
    event: fn(String) -> OutputLine,
) -> thread::JoinHandle<()> {
    thread::spawn(move || {
        for line in BufReader::new(stream).lines() {
            let Ok(line) = line else { break };
            if let Ok(mut sink) = dump.lock() {
                sink.push_str(&line);
                sink.push('\n');
            }
            // The snapshot is updated before its stream's readiness event.
            if tx.send(event(line)).is_err() {
                break;
            }
        }
    })
}

fn capture_serve_child(mut child: Child) -> ServeChild {
    let stdout = child.stdout.take().expect("piped stdout");
    let stderr = child.stderr.take().expect("piped stderr");
    let (tx, rx) = mpsc::channel();
    let stdout_dump = std::sync::Arc::new(std::sync::Mutex::new(String::new()));
    let stderr_dump = std::sync::Arc::new(std::sync::Mutex::new(String::new()));
    let readers = vec![
        capture_stream(stdout, stdout_dump.clone(), tx.clone(), OutputLine::Stdout),
        capture_stream(stderr, stderr_dump.clone(), tx, OutputLine::Stderr),
    ];
    ServeChild {
        child: Some(child),
        host: String::new(),
        port: 0,
        lines: rx,
        stdout_dump,
        stderr_dump,
        readers,
    }
}

fn spawn_serve_env(cwd: &std::path::Path, args: &[&str], env_sets: &[(&str, &str)]) -> ServeChild {
    match try_spawn_serve_env(cwd, args, env_sets) {
        Start::Ready(serve) => serve,
        Start::Failed {
            status,
            stdout,
            stderr,
        } => {
            panic!("serve exited before readiness ({status}); stdout:\n{stdout}\nstderr:\n{stderr}")
        }
    }
}

/// Run a serve invocation that is expected to fail; returns its output.
fn run_serve_env(
    cwd: &std::path::Path,
    args: &[&str],
    env_sets: &[(&str, &str)],
) -> (std::process::ExitStatus, String, String) {
    let mut cmd = prepare_command(cwd, args);
    for (key, value) in env_sets {
        cmd.env(key, value);
    }
    let output = cmd.output().expect("run lotar serve");
    (
        output.status,
        String::from_utf8_lossy(&output.stdout).into_owned(),
        String::from_utf8_lossy(&output.stderr).into_owned(),
    )
}

impl ServeChild {
    fn wait_for_banner(&mut self) -> Result<(), ()> {
        let deadline = Instant::now() + READY_TIMEOUT;
        let mut seen = String::new();
        let mut stdout_ready = false;
        let mut stderr_ready = false;
        loop {
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                panic!(
                    "serve did not advertise readiness within {READY_TIMEOUT:?}; stdout so far:\n{seen}\nstderr:\n{}",
                    self.stderr_snapshot()
                );
            }
            match self.lines.recv_timeout(remaining) {
                Ok(OutputLine::Stdout(line)) => {
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
                        stdout_ready = true;
                    }
                }
                Ok(OutputLine::Stderr(line)) => {
                    // This final stderr startup line follows the fallback
                    // warning in the same pipe. The URL alone cannot prove
                    // that the independent stderr reader has caught up.
                    if line.contains("Press Ctrl+C to stop the server") {
                        stderr_ready = true;
                    }
                }
                Err(mpsc::RecvTimeoutError::Timeout) => {
                    panic!(
                        "serve did not advertise readiness within {READY_TIMEOUT:?}; stdout so far:\n{seen}\nstderr:\n{}",
                        self.stderr_snapshot()
                    );
                }
                Err(mpsc::RecvTimeoutError::Disconnected) => return Err(()),
            }
            if stdout_ready && stderr_ready {
                return Ok(());
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

    fn stop(&mut self) -> std::process::ExitStatus {
        let _ = self.request_stop();
        let _ = std::net::TcpStream::connect((self.host.as_str(), self.port));
        let child = self.child.as_mut().expect("serve child ownership");
        let deadline = Instant::now() + STOP_TIMEOUT;
        let status = loop {
            match child.try_wait().expect("poll serve child") {
                Some(status) => break status,
                None if Instant::now() >= deadline => {
                    child.kill().expect("kill serve child after stop timeout");
                    break child.wait().expect("reap killed serve child");
                }
                None => thread::sleep(Duration::from_millis(25)),
            }
        };
        self.join_readers();
        status
    }

    fn join_readers(&mut self) {
        for reader in self.readers.drain(..) {
            let _ = reader.join();
        }
    }

    fn request_stop(&self) -> std::io::Result<()> {
        use std::io::{Read, Write};
        let mut stream = std::net::TcpStream::connect((self.host.as_str(), self.port))?;
        stream.set_read_timeout(Some(Duration::from_millis(500)))?;
        stream.write_all(
            format!("GET /__test/stop HTTP/1.1\r\nHost: {}\r\n\r\n", self.host).as_bytes(),
        )?;
        stream.flush()?;
        let mut scratch = [0u8; 128];
        let _ = stream.read(&mut scratch);
        Ok(())
    }
}

impl Drop for ServeChild {
    fn drop(&mut self) {
        if let Some(mut child) = self.child.take() {
            let _ = child.kill();
            let _ = child.wait();
        }
        self.join_readers();
    }
}

fn reserve_ephemeral_port() -> u16 {
    let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
    let port = listener.local_addr().unwrap().port();
    drop(listener);
    port
}

#[test]
fn stdout_banner_alone_is_not_complete_startup_output() {
    let (tx, rx) = mpsc::channel();
    tx.send(OutputLine::Stdout(
        "   URL: http://127.0.0.1:12345".to_string(),
    ))
    .unwrap();
    drop(tx);
    let mut serve = ServeChild {
        child: None,
        host: String::new(),
        port: 0,
        lines: rx,
        stdout_dump: Default::default(),
        stderr_dump: Default::default(),
        readers: Vec::new(),
    };
    assert!(
        serve.wait_for_banner().is_err(),
        "stdout readiness must not expose an unread stderr snapshot"
    );
}

#[test]
fn startup_capture_accepts_either_stream_order() {
    for stderr_first in [false, true] {
        let (tx, rx) = mpsc::channel();
        let stdout = OutputLine::Stdout("   URL: http://127.0.0.1:12345".to_string());
        let stderr = OutputLine::Stderr("Press Ctrl+C to stop the server".to_string());
        let events = if stderr_first {
            [stderr, stdout]
        } else {
            [stdout, stderr]
        };
        for event in events {
            tx.send(event).unwrap();
        }
        drop(tx);
        let mut serve = ServeChild {
            child: None,
            host: String::new(),
            port: 0,
            lines: rx,
            stdout_dump: Default::default(),
            stderr_dump: Default::default(),
            readers: Vec::new(),
        };
        assert!(serve.wait_for_banner().is_ok());
        assert_eq!(serve.host, "127.0.0.1");
        assert_eq!(serve.port, 12345);
    }
}

#[test]
fn stderr_startup_marker_alone_is_not_complete_startup_output() {
    let (tx, rx) = mpsc::channel();
    tx.send(OutputLine::Stderr(
        "Press Ctrl+C to stop the server".to_string(),
    ))
    .unwrap();
    drop(tx);
    let mut serve = ServeChild {
        child: None,
        host: String::new(),
        port: 0,
        lines: rx,
        stdout_dump: Default::default(),
        stderr_dump: Default::default(),
        readers: Vec::new(),
    };
    assert!(
        serve.wait_for_banner().is_err(),
        "stderr startup must not substitute for a bound URL"
    );
}

fn write_global_config(fixtures: &TestFixtures, content: &str) {
    std::fs::write(fixtures.tasks_root.join("config.yml"), content)
        .expect("write global config.yml");
}

fn assert_strict_failure(status: &std::process::ExitStatus, stdout: &str, stderr: &str, port: u16) {
    assert!(!status.success(), "configured port must fail, not move");
    assert!(
        !stdout.contains("URL: http"),
        "no readiness banner may precede a failed bind: {stdout}"
    );
    assert!(
        stderr.contains("Failed to bind"),
        "bind failure must be reported: {stderr}"
    );
    assert!(
        stderr.contains(&port.to_string()),
        "error must name the configured port: {stderr}"
    );
    assert!(
        !stderr.contains("serving on port"),
        "no fallback notice for a configured port: {stderr}"
    );
}

#[test]
fn env_port_selects_free_port_exactly() {
    let fixtures = TestFixtures::new();
    let port = reserve_ephemeral_port();
    let mut serve = spawn_serve_env(
        fixtures.temp_dir.path(),
        &["serve", "--host", "127.0.0.1"],
        &[("LOTAR_PORT", &port.to_string())],
    );

    assert_eq!(serve.port, port, "env-configured port must be used exactly");
    let stdout = serve.stdout_snapshot();
    assert!(
        stdout.contains(&format!("   URL: http://127.0.0.1:{port}")),
        "banner must advertise the env port: {stdout}"
    );
    assert!(
        !serve.stderr_snapshot().contains("serving on port"),
        "free env port must not warn"
    );
    assert!(serve.stop().success());
}

#[test]
fn env_server_port_alias_selects_free_port_exactly() {
    let fixtures = TestFixtures::new();
    let port = reserve_ephemeral_port();
    let mut serve = spawn_serve_env(
        fixtures.temp_dir.path(),
        &["serve", "--host", "127.0.0.1"],
        &[("LOTAR_SERVER_PORT", &port.to_string())],
    );

    assert_eq!(
        serve.port, port,
        "LOTAR_SERVER_PORT must configure the port"
    );
    assert!(serve.stop().success());
}

#[test]
fn env_port_on_held_port_fails_without_fallback() {
    let fixtures = TestFixtures::new();
    let holder = TcpListener::bind(("127.0.0.1", 0)).unwrap();
    let held = holder.local_addr().unwrap().port();

    let (status, stdout, stderr) = run_serve_env(
        fixtures.temp_dir.path(),
        &["serve", "--host", "127.0.0.1"],
        &[("LOTAR_PORT", &held.to_string())],
    );
    assert_strict_failure(&status, &stdout, &stderr, held);
    assert!(
        stderr.contains("LOTAR_PORT"),
        "env lock is documented: error should hint the requester, got: {stderr}"
    );
}

#[test]
fn config_file_port_selects_free_port_exactly() {
    let fixtures = TestFixtures::new();
    let port = reserve_ephemeral_port();
    write_global_config(&fixtures, &format!("server:\n  port: {port}\n"));

    let mut serve = spawn_serve_env(
        fixtures.temp_dir.path(),
        &["serve", "--host", "127.0.0.1"],
        &[],
    );
    assert_eq!(
        serve.port, port,
        "config-configured port must be used exactly"
    );
    assert!(
        !serve.stderr_snapshot().contains("serving on port"),
        "free config port must not warn"
    );
    assert!(serve.stop().success());
}

#[test]
fn config_file_port_on_held_port_fails_without_fallback() {
    let fixtures = TestFixtures::new();
    let holder = TcpListener::bind(("127.0.0.1", 0)).unwrap();
    let held = holder.local_addr().unwrap().port();
    write_global_config(&fixtures, &format!("server.port: {held}\n"));

    let (status, stdout, stderr) = run_serve_env(
        fixtures.temp_dir.path(),
        &["serve", "--host", "127.0.0.1"],
        &[],
    );
    assert_strict_failure(&status, &stdout, &stderr, held);
    assert!(
        stderr.contains("server.port"),
        "config lock is documented: error should hint the requester, got: {stderr}"
    );
}

#[test]
fn cli_port_beats_env_port() {
    let fixtures = TestFixtures::new();
    let env_port = reserve_ephemeral_port();
    let cli_port = reserve_ephemeral_port();
    let mut serve = spawn_serve_env(
        fixtures.temp_dir.path(),
        &[
            "serve",
            "--host",
            "127.0.0.1",
            "--port",
            &cli_port.to_string(),
        ],
        &[("LOTAR_PORT", &env_port.to_string())],
    );

    assert_eq!(
        serve.port, cli_port,
        "--port must win over the resolved env port"
    );
    assert!(serve.stop().success());
}

#[test]
fn cli_port_beats_config_file_port() {
    let fixtures = TestFixtures::new();
    let config_port = reserve_ephemeral_port();
    let cli_port = reserve_ephemeral_port();
    write_global_config(&fixtures, &format!("server:\n  port: {config_port}\n"));

    let mut serve = spawn_serve_env(
        fixtures.temp_dir.path(),
        &[
            "serve",
            "--host",
            "127.0.0.1",
            "--port",
            &cli_port.to_string(),
        ],
        &[],
    );
    assert_eq!(
        serve.port, cli_port,
        "--port must win over the config file port"
    );
    assert!(serve.stop().success());
}

#[test]
fn env_port_beats_config_file_port() {
    let fixtures = TestFixtures::new();
    let config_port = reserve_ephemeral_port();
    let env_port = reserve_ephemeral_port();
    write_global_config(&fixtures, &format!("server:\n  port: {config_port}\n"));

    let mut serve = spawn_serve_env(
        fixtures.temp_dir.path(),
        &["serve", "--host", "127.0.0.1"],
        &[("LOTAR_PORT", &env_port.to_string())],
    );
    assert_eq!(
        serve.port, env_port,
        "env must win over the config file port"
    );
    assert!(serve.stop().success());
}

#[test]
fn json_format_keeps_fallback_notice_plain_and_machine_readable() {
    let fixtures = TestFixtures::new();

    // Own the default port when possible so the implicit fallback fires
    // deterministically; a foreign holder triggers the same path.
    let holder = TcpListener::bind(("127.0.0.1", 8080));
    let mut serve = spawn_serve_env(
        fixtures.temp_dir.path(),
        &["--format=json", "serve", "--host", "127.0.0.1"],
        &[],
    );
    let stdout = serve.stdout_snapshot();
    let stderr = serve.stderr_snapshot();

    if serve.port == 8080 {
        // Foreign holder released first; the default was free.
        assert!(
            !stderr.contains("serving on port"),
            "no fallback notice when the default was free: {stderr}"
        );
    } else {
        assert!(
            stderr.contains("8080") && stderr.contains(&serve.port.to_string()),
            "JSON mode must still carry the explicit fallback notice: {stderr}"
        );
        assert!(
            stderr.contains("warning"),
            "machine formats keep the warning envelope: {stderr}"
        );
    }
    assert!(
        !stdout.contains('\u{1b}'),
        "machine format stdout must never carry ANSI: {stdout}"
    );
    assert!(serve.stop().success());
    drop(holder);
}

/// Spawn `lotar serve` with default hygiene (home config ignored) but both
/// home-dir variables (`HOME`/`USERPROFILE`) pointed at an isolated fixture,
/// proving the ignored layer is never read on any platform.
fn spawn_serve_home_disabled(cwd: &std::path::Path, home_dir: &std::path::Path) -> ServeChild {
    spawn_serve_home_args(cwd, &["serve", "--host", "127.0.0.1"], home_dir, true)
}

/// Spawn `lotar serve` with the home config layer honored and both home-dir
/// variables (`HOME`/`USERPROFILE`) pointing at an isolated fixture
/// directory (never the host user's data on any platform).
fn spawn_serve_home(
    cwd: &std::path::Path,
    args: &[&str],
    home_dir: &std::path::Path,
) -> ServeChild {
    spawn_serve_home_args(cwd, args, home_dir, false)
}

fn spawn_serve_home_args(
    cwd: &std::path::Path,
    args: &[&str],
    home_dir: &std::path::Path,
    ignore_home: bool,
) -> ServeChild {
    let mut cmd = prepare_command(cwd, args);
    cmd.env_remove("RUST_TEST_THREADS")
        .env_remove("LOTAR_TEST_MODE")
        .env(
            "LOTAR_IGNORE_HOME_CONFIG",
            if ignore_home { "1" } else { "0" },
        )
        // Point every home-dir lookup the child can make at the isolated
        // fixture: POSIX reads HOME, Windows reads USERPROFILE, and the
        // test never mutates parent-process (global) environment state.
        .env("HOME", home_dir)
        .env("USERPROFILE", home_dir)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    spawn_from_command(cmd)
}

fn spawn_from_command(mut cmd: Command) -> ServeChild {
    let mut serve = capture_serve_child(cmd.spawn().expect("spawn lotar serve"));
    if serve.wait_for_banner().is_err() {
        if let Some(mut child) = serve.child.take() {
            let _ = child.kill();
            let _ = child.wait();
        }
        panic!(
            "serve exited before readiness; stdout:\n{}\nstderr:\n{}",
            serve.stdout_snapshot(),
            serve.stderr_snapshot()
        );
    }
    serve
}

#[test]
fn config_override_port_wins_over_env_port() {
    let fixtures = TestFixtures::new();
    let env_port = reserve_ephemeral_port();
    let override_port = reserve_ephemeral_port();
    let mut serve = spawn_serve_env(
        fixtures.temp_dir.path(),
        &[
            "--config",
            &format!("server.port={override_port}"),
            "serve",
            "--host",
            "127.0.0.1",
        ],
        &[("LOTAR_PORT", &env_port.to_string())],
    );
    assert_eq!(
        serve.port, override_port,
        "--config server.port must win over LOTAR_PORT"
    );
    assert!(
        !serve.stderr_snapshot().contains("serving on port"),
        "explicit override port must not warn"
    );
    assert!(serve.stop().success());
}

#[test]
fn config_override_explicit_default_number_is_strict_when_busy() {
    let fixtures = TestFixtures::new();
    let args = [
        "--config",
        "server.port=8080",
        "serve",
        "--host",
        "127.0.0.1",
    ];

    let holder = TcpListener::bind(("127.0.0.1", 8080));
    if let Ok(_holder) = holder {
        // Deterministic: an explicit --config request for the default port
        // number is a configured port and must fail exactly like any other
        // held port — it may never silently move (the round-1 bug).
        let (status, stdout, stderr) = run_serve_env(fixtures.temp_dir.path(), &args, &[]);
        assert_strict_failure(&status, &stdout, &stderr, 8080);
        assert!(
            stderr.contains("(port set by --config server.port)"),
            "error must attribute the --config layer: {stderr}"
        );
    } else {
        // Foreign holder: contract stays consistent — either a strict bind
        // failure naming --config, or 8080 itself after the holder released.
        match try_spawn_serve_env(fixtures.temp_dir.path(), &args, &[]) {
            Start::Ready(mut serve) => {
                assert_eq!(serve.port, 8080, "explicit 8080 must bind exactly");
                assert!(
                    !serve.stderr_snapshot().contains("serving on port"),
                    "no fallback notice for an explicit port"
                );
                assert!(serve.stop().success());
            }
            Start::Failed { stderr, stdout, .. } => {
                assert!(
                    stderr.contains("Failed to bind")
                        && stderr.contains("(port set by --config server.port)"),
                    "strict failure must attribute --config: {stderr}"
                );
                assert!(
                    !stdout.contains("URL: http") && !stderr.contains("serving on port"),
                    "no banner, no fallback notice: {stdout} / {stderr}"
                );
            }
        }
    }
}

#[test]
fn config_override_port_zero_is_rejected_by_config_validation() {
    // The existing DEV-71 validation contract rejects port 0 through
    // --config ("must be between 1 and 65535") before serve starts; port 0
    // as an OS-assignment request remains a --port-only feature, and a
    // config FILE may still spell it (see the file-layer test below).
    let fixtures = TestFixtures::new();
    let (status, stdout, stderr) = run_serve_env(
        fixtures.temp_dir.path(),
        &["--config", "server.port=0", "serve", "--host", "127.0.0.1"],
        &[],
    );
    assert!(
        !status.success(),
        "invalid --config port must fail before serve starts"
    );
    assert!(
        stderr.contains("Invalid --config override"),
        "must surface the config validation error: {stderr}"
    );
    assert!(
        stderr.contains("1 and 65535"),
        "documents the valid range: {stderr}"
    );
    assert!(
        !stdout.contains("URL: http"),
        "no readiness banner may appear: {stdout}"
    );
}

#[test]
fn env_explicit_default_number_beats_config_file_port() {
    let fixtures = TestFixtures::new();
    let file_port = reserve_ephemeral_port();
    write_global_config(&fixtures, &format!("server:\n  port: {file_port}\n"));

    let holder = TcpListener::bind(("127.0.0.1", 8080));
    if let Ok(_holder) = holder {
        // Deterministic: env asked for exactly 8080 while it is held -> the
        // env request must fail strictly, never fall back and never let the
        // lower config-file port win by value-merge accident.
        let (status, stdout, stderr) = run_serve_env(
            fixtures.temp_dir.path(),
            &["serve", "--host", "127.0.0.1"],
            &[("LOTAR_PORT", "8080")],
        );
        assert_strict_failure(&status, &stdout, &stderr, 8080);
        assert!(
            stderr.contains("LOTAR_PORT"),
            "error must attribute the env layer: {stderr}"
        );
    } else {
        match try_spawn_serve_env(
            fixtures.temp_dir.path(),
            &["serve", "--host", "127.0.0.1"],
            &[("LOTAR_PORT", "8080")],
        ) {
            Start::Ready(mut serve) => {
                assert_eq!(
                    serve.port, 8080,
                    "env-requested 8080 must bind exactly, never the file port"
                );
                assert!(!serve.stderr_snapshot().contains("serving on port"));
                assert!(serve.stop().success());
            }
            Start::Failed { stderr, stdout, .. } => {
                assert!(
                    stderr.contains("Failed to bind") && stderr.contains("LOTAR_PORT"),
                    "strict failure must attribute the env layer: {stderr}"
                );
                assert!(
                    !stdout.contains("URL: http") && !stderr.contains("serving on port"),
                    "no banner, no fallback notice: {stdout} / {stderr}"
                );
            }
        }
    }
}

#[test]
fn home_config_port_is_honored_when_home_config_enabled() {
    let fixtures = TestFixtures::new();
    let home_port = reserve_ephemeral_port();
    let home_fixture = tempfile::tempdir().expect("home fixture");
    std::fs::write(
        home_fixture.path().join(".lotar"),
        format!("server:\n  port: {home_port}\n"),
    )
    .expect("write fixture home config");

    let mut serve = spawn_serve_home(
        fixtures.temp_dir.path(),
        &["serve", "--host", "127.0.0.1"],
        home_fixture.path(),
    );
    assert_eq!(
        serve.port, home_port,
        "honored home config port must be used exactly"
    );
    assert!(!serve.stderr_snapshot().contains("serving on port"));
    assert!(serve.stop().success());
}

#[test]
fn home_config_port_beats_global_file_port_by_presence() {
    let fixtures = TestFixtures::new();
    let home_port = reserve_ephemeral_port();
    let file_port = reserve_ephemeral_port();
    assert_ne!(home_port, file_port);
    write_global_config(&fixtures, &format!("server:\n  port: {file_port}\n"));
    let home_fixture = tempfile::tempdir().expect("home fixture");
    std::fs::write(
        home_fixture.path().join(".lotar"),
        format!("server:\n  port: {home_port}\n"),
    )
    .expect("write fixture home config");

    let mut serve = spawn_serve_home(
        fixtures.temp_dir.path(),
        &["serve", "--host", "127.0.0.1"],
        home_fixture.path(),
    );
    assert_eq!(
        serve.port, home_port,
        "honored home config must outrank the global config file"
    );
    assert!(serve.stop().success());
}

#[test]
fn ignored_home_config_port_cannot_block_default_fallback() {
    let fixtures = TestFixtures::new();
    let home_fixture = tempfile::tempdir().expect("home fixture");
    std::fs::write(
        home_fixture.path().join(".lotar"),
        "server:\n  port: 7777\n",
    )
    .expect("write fixture home config");

    // Default hygiene keeps LOTAR_IGNORE_HOME_CONFIG=1; HOME points at the
    // fixture to prove the ignored layer is never consulted.
    let mut serve = spawn_serve_home_disabled(fixtures.temp_dir.path(), home_fixture.path());
    let stderr = serve.stderr_snapshot();

    assert_ne!(
        serve.port, 7777,
        "ignored home config must not contribute a port"
    );
    if serve.port == 8080 {
        assert!(
            !stderr.contains("serving on port"),
            "free default must not warn: {stderr}"
        );
    } else {
        assert!(
            stderr.contains("8080") && stderr.contains(&serve.port.to_string()),
            "implicit fallback warning must name preferred and actual: {stderr}"
        );
    }
    assert!(serve.stop().success());
}

#[test]
fn invalid_env_port_fails_before_bind() {
    let fixtures = TestFixtures::new();
    let (status, stdout, stderr) = run_serve_env(
        fixtures.temp_dir.path(),
        &["serve", "--host", "127.0.0.1"],
        &[("LOTAR_PORT", "not-a-port")],
    );
    assert!(
        !status.success(),
        "invalid env port must fail instead of moving"
    );
    assert!(
        stderr.contains("LOTAR_PORT") && stderr.to_lowercase().contains("invalid port"),
        "error must name the env var and the problem: {stderr}"
    );
    assert!(
        !stdout.contains("URL: http") && !stderr.contains("serving on port"),
        "no banner, no fallback notice: {stdout} / {stderr}"
    );
}

#[test]
fn unparseable_global_config_fails_before_bind() {
    let fixtures = TestFixtures::new();
    write_global_config(&fixtures, "server:\n  port: [not, a, number]\n");

    let (status, stdout, stderr) = run_serve_env(
        fixtures.temp_dir.path(),
        &["serve", "--host", "127.0.0.1"],
        &[],
    );
    assert!(
        !status.success(),
        "unparseable config must fail instead of guessing a port"
    );
    assert!(
        stderr.contains("Invalid global config"),
        "error must name the config layer: {stderr}"
    );
    assert!(
        !stdout.contains("URL: http") && !stderr.contains("serving on port"),
        "no banner, no fallback notice: {stdout} / {stderr}"
    );
}

#[test]
fn config_file_port_zero_binds_os_assigned_strictly() {
    let fixtures = TestFixtures::new();
    write_global_config(&fixtures, "server:\n  port: 0\n");

    let mut serve = spawn_serve_env(
        fixtures.temp_dir.path(),
        &["serve", "--host", "127.0.0.1"],
        &[],
    );
    assert_ne!(
        serve.port, 0,
        "config-file port 0 must bind an OS-assigned actual port"
    );
    let stderr = serve.stderr_snapshot();
    assert!(
        !stderr.contains("serving on port"),
        "explicit port 0 is not a fallback: {stderr}"
    );
    let stdout = serve.stdout_snapshot();
    assert!(
        stdout.contains(&format!("   URL: http://127.0.0.1:{}", serve.port)),
        "banner must advertise the actual port: {stdout}"
    );
    assert!(serve.stop().success());
}

/// Run `config show --format json` in an isolated workspace and parse the
/// payload; pins resolved values and winner labels without binding ports.
fn config_show_json(
    cwd: &std::path::Path,
    env_sets: &[(&str, &str)],
    args: &[&str],
) -> serde_json::Value {
    let mut cmd = prepare_command(cwd, args);
    for (key, value) in env_sets {
        cmd.env(key, value);
    }
    let output = cmd.output().expect("run config show");
    assert!(
        output.status.success(),
        "config show failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).expect("config show JSON")
}

#[test]
fn config_show_reports_env_winner_over_global_file_default_number() {
    let fixtures = TestFixtures::new();
    write_global_config(&fixtures, "server:\n  port: 9999\n");

    let payload = config_show_json(
        fixtures.temp_dir.path(),
        &[("LOTAR_PORT", "8080")],
        &["--format", "json", "config", "show"],
    );
    assert_eq!(
        payload["config"]["server_port"], 8080,
        "env request for the default number must win over the file's 9999: {payload}"
    );
    assert_eq!(
        payload["sources"]["server.port"], "env",
        "winner label must credit env, not the config file: {payload}"
    );
}

#[test]
fn config_show_reports_home_winner_over_global_file_default_number() {
    let fixtures = TestFixtures::new();
    write_global_config(&fixtures, "server:\n  port: 9999\n");
    let home_fixture = tempfile::tempdir().expect("home fixture");
    std::fs::write(
        home_fixture.path().join(".lotar"),
        "server:\n  port: 8080\n",
    )
    .expect("write fixture home config");

    let mut cmd = prepare_command(
        fixtures.temp_dir.path(),
        &["--format", "json", "config", "show"],
    );
    cmd.env_remove("RUST_TEST_THREADS")
        .env_remove("LOTAR_TEST_MODE")
        .env("LOTAR_IGNORE_HOME_CONFIG", "0")
        // Same isolation on both POSIX (HOME) and Windows (USERPROFILE).
        .env("HOME", home_fixture.path())
        .env("USERPROFILE", home_fixture.path());
    let output = cmd.output().expect("run config show");
    assert!(
        output.status.success(),
        "config show failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let payload: serde_json::Value = serde_json::from_slice(&output.stdout).expect("JSON");
    assert_eq!(
        payload["config"]["server_port"], 8080,
        "home request for the default number must win over the file's 9999: {payload}"
    );
    assert_eq!(
        payload["sources"]["server.port"], "home",
        "winner label must credit home, not the config file: {payload}"
    );
}

#[test]
fn config_show_reports_cli_override_winner_over_env() {
    let fixtures = TestFixtures::new();
    let env_port = reserve_ephemeral_port();

    let payload = config_show_json(
        fixtures.temp_dir.path(),
        &[("LOTAR_PORT", &env_port.to_string())],
        &[
            "--format",
            "json",
            "--config",
            "server.port=8080",
            "config",
            "show",
        ],
    );
    assert_eq!(
        payload["config"]["server_port"], 8080,
        "--config request for the default number must win over env: {payload}"
    );
    assert_eq!(
        payload["sources"]["server.port"], "cli",
        "winner label must credit the --config layer: {payload}"
    );
}

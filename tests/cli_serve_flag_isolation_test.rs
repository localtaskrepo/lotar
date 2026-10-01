//! `serve -p` flag-isolation contracts (user-authorized behavior exception).
//!
//! `serve` owns `-p` as its port short; the global `--project` short `-p`
//! keeps its meaning everywhere else, including before the `serve`
//! subcommand. These tests pin the clap-level scoping (no preprocess) plus
//! the real-binary help/banner behavior. Every spawned `lotar serve` child
//! is owned by the test that started it and is reaped before assertions end.

use clap::Parser;
use lotar::cli::{Cli, Commands};
use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpStream;
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

fn parse_direct(args: &[&str]) -> Cli {
    // Deliberately bypasses `normalize_args`: the short-flag scoping must
    // hold at the clap definition level, not only behind preprocessing.
    Cli::try_parse_from(args).expect("direct clap parse")
}

#[test]
fn serve_short_p_is_a_real_clap_port_flag() {
    let cli = parse_direct(&["lotar", "serve", "-p", "5050"]);
    match cli.command {
        Commands::Serve(serve_args) => assert_eq!(serve_args.port, Some(5050)),
        _ => panic!("expected serve command"),
    }
}

#[test]
fn serve_attached_short_port_parses_without_preprocess() {
    let cli = parse_direct(&["lotar", "serve", "-p5050"]);
    match cli.command {
        Commands::Serve(serve_args) => assert_eq!(serve_args.port, Some(5050)),
        _ => panic!("expected serve command"),
    }
}

#[test]
fn serve_long_port_flag_still_parses() {
    let cli = parse_direct(&["lotar", "serve", "--port", "5050"]);
    match cli.command {
        Commands::Serve(serve_args) => assert_eq!(serve_args.port, Some(5050)),
        _ => panic!("expected serve command"),
    }
}

#[test]
fn global_project_short_keeps_meaning_before_serve() {
    let cli = parse_direct(&["lotar", "-p", "web", "serve", "-p", "8081"]);
    assert_eq!(cli.project.as_deref(), Some("web"));
    match cli.command {
        Commands::Serve(serve_args) => assert_eq!(serve_args.port, Some(8081)),
        _ => panic!("expected serve command"),
    }
}

#[test]
fn global_project_short_keeps_meaning_inside_other_subcommands() {
    let cli = parse_direct(&["lotar", "whoami", "-p", "TEST"]);
    assert_eq!(cli.project.as_deref(), Some("TEST"));
}

#[test]
fn serve_accepts_and_ignores_long_project() {
    // `serve --project X` parsed before through the propagated global; the
    // serve-scoped shadow keeps it accepted (and ignored) without letting
    // the global `-p` short clash with the port short.
    let cli = parse_direct(&["lotar", "serve", "--project", "X", "-p", "9090"]);
    match cli.command {
        Commands::Serve(serve_args) => {
            assert_eq!(serve_args.port, Some(9090));
            assert_eq!(serve_args.project.as_deref(), Some("X"));
        }
        _ => panic!("expected serve command"),
    }
}

fn run_binary(cwd: &std::path::Path, args: &[&str]) -> (bool, String, String) {
    let output = Command::new(env!("CARGO_BIN_EXE_lotar"))
        .current_dir(cwd)
        .env_remove("LOTAR_TASKS_DIR")
        .env("LOTAR_IGNORE_HOME_CONFIG", "1")
        .env("LOTAR_TEST_SILENT", "1")
        .args(args)
        .output()
        .expect("run lotar binary");
    (
        output.status.success(),
        String::from_utf8_lossy(&output.stdout).into_owned(),
        String::from_utf8_lossy(&output.stderr).into_owned(),
    )
}

#[test]
fn serve_help_advertises_port_short_and_not_project_short() {
    let tmp = tempfile::tempdir().unwrap();
    let (ok, stdout, _stderr) = run_binary(tmp.path(), &["serve", "--help"]);
    assert!(ok, "serve --help must succeed");
    assert!(
        stdout.contains("-p <PORT>") && stdout.contains("--port <PORT>"),
        "serve --help must advertise -p as the port short: {stdout}"
    );
    assert!(
        !stdout.contains("-p, --project") && !stdout.contains("-p <PROJECT>"),
        "serve --help must not advertise -p as the project short: {stdout}"
    );
}

#[test]
fn root_help_still_advertises_project_short() {
    let tmp = tempfile::tempdir().unwrap();
    let (ok, stdout, _stderr) = run_binary(tmp.path(), &["--help"]);
    assert!(ok, "lotar --help must succeed");
    assert!(
        stdout.contains("--project, -p <PREFIX>"),
        "root help must keep advertising -p as the global project short: {stdout}"
    );
}

#[test]
fn serve_malformed_short_port_fails_as_a_port_error() {
    let tmp = tempfile::tempdir().unwrap();
    let (ok, _stdout, stderr) = run_binary(tmp.path(), &["serve", "-p", "not-a-port"]);
    assert!(!ok, "malformed port must fail");
    // clap is built without the error-context feature, so value errors do
    // not name the argument; the contract to pin is that the failure is a
    // value error for the port flag and is never attributed to a project
    // selector.
    assert!(
        stderr.to_lowercase().contains("invalid value"),
        "failure must be an invalid-value error: {stderr}"
    );
    assert!(
        !stderr.to_lowercase().contains("project"),
        "failure must not be attributed to the project selector: {stderr}"
    );
}

struct OwnedServeChild {
    child: Child,
    port: u16,
}

impl OwnedServeChild {
    fn stop(mut self) {
        // Test-owned teardown: stop endpoint, wake-up connection, bounded
        // wait, kill fallback, and always reap.
        for request in [
            "GET /__test/stop HTTP/1.1\r\nHost: 127.0.0.1\r\n\r\n",
            "GET /__test/stop HTTP/1.1\r\nHost: 127.0.0.1\r\n\r\n",
        ] {
            if let Ok(mut stream) = TcpStream::connect(("127.0.0.1", self.port)) {
                let _ = stream.set_read_timeout(Some(Duration::from_millis(250)));
                let _ = stream.write_all(request.as_bytes());
                let _ = stream.flush();
                let mut scratch = [0u8; 128];
                let _ = stream.read(&mut scratch);
            }
        }
        let deadline = Instant::now() + Duration::from_secs(5);
        while self.child.try_wait().map(|w| w.is_none()).unwrap_or(true) {
            if Instant::now() > deadline {
                let _ = self.child.kill();
                break;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        let _ = self.child.wait();
    }
}

fn spawn_serve_port_zero(tmp: &std::path::Path, port_flag: &str, value: &str) -> OwnedServeChild {
    let mut child = Command::new(env!("CARGO_BIN_EXE_lotar"))
        .current_dir(tmp)
        .env_remove("LOTAR_TASKS_DIR")
        .env("LOTAR_IGNORE_HOME_CONFIG", "1")
        .env("LOTAR_TEST_SILENT", "1")
        .env("LOTAR_ALLOW_TEST_STOP", "1")
        .args(["serve", "--host", "127.0.0.1", port_flag, value])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn lotar serve");

    let stdout = child.stdout.take().expect("piped stdout");
    let (tx, rx) = std::sync::mpsc::channel::<String>();
    std::thread::spawn(move || {
        for line in BufReader::new(stdout).lines() {
            match line {
                Ok(line) => {
                    if tx.send(line).is_err() {
                        break;
                    }
                }
                Err(_) => break,
            }
        }
    });

    let deadline = Instant::now() + Duration::from_secs(15);
    let mut seen = String::new();
    let port = loop {
        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            let _ = child.kill();
            let _ = child.wait();
            panic!("no readiness banner within 15s; stdout so far:\n{seen}");
        }
        match rx.recv_timeout(remaining) {
            Ok(line) => {
                seen.push_str(&line);
                seen.push('\n');
                if let Some(rest) = line.strip_prefix("   URL: http://") {
                    let (_host, port) = rest
                        .rsplit_once(':')
                        .unwrap_or_else(|| panic!("malformed readiness banner: {line}"));
                    break port.trim().parse::<u16>().unwrap_or_else(|_| {
                        panic!("non-numeric port in readiness banner: {line}")
                    });
                }
            }
            Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {
                let _ = child.kill();
                let _ = child.wait();
                panic!("readiness wait timed out; stdout so far:\n{seen}");
            }
            Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => {
                let status = child.wait().expect("reap serve child");
                panic!("serve exited before readiness ({status}); stdout so far:\n{seen}");
            }
        }
    };

    OwnedServeChild { child, port }
}

#[test]
fn serve_short_p_port_zero_banner_reports_actual_bound_port() {
    let tmp = tempfile::tempdir().unwrap();
    let serve = spawn_serve_port_zero(tmp.path(), "-p", "0");
    let port = serve.port;
    assert!(port != 0, "banner must report the actual bound port, got 0");
    // The advertised port must genuinely accept connections.
    TcpStream::connect(("127.0.0.1", port)).expect("connect to advertised ephemeral port");
    serve.stop();
}

#[test]
fn serve_long_port_zero_banner_reports_actual_bound_port() {
    let tmp = tempfile::tempdir().unwrap();
    let serve = spawn_serve_port_zero(tmp.path(), "--port", "0");
    assert!(
        serve.port != 0,
        "banner must report the actual bound port, got 0"
    );
    TcpStream::connect(("127.0.0.1", serve.port)).expect("connect to advertised port");
    serve.stop();
}

//! DEV-300: `config show` prints reviewers and the sync/agent settings, an
//! unset variable name is not sent as a token, and a sync with failed items
//! exits non-zero with status `partial`.
#![cfg(unix)]

use crate::common;
use common::TestFixtures;
use std::io::{BufRead, BufReader, Read, Write};

fn run(dir: &std::path::Path, args: &[&str], home: &std::path::Path) -> std::process::Output {
    common::lotar_cmd()
        .unwrap()
        .current_dir(dir)
        .env("LOTAR_DEFAULT_REPORTER", "tester")
        .env("HOME", home)
        .env("LOTAR_IGNORE_HOME_CONFIG", "0")
        .args(args)
        .output()
        .unwrap()
}

/// Answers every HTTP request with 500 until the test ends.
fn failing_server() -> String {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    std::thread::spawn(move || {
        for stream in listener.incoming().flatten() {
            let mut reader = BufReader::new(stream);
            let mut length = 0usize;
            loop {
                let mut line = String::new();
                if reader.read_line(&mut line).unwrap_or(0) == 0 || line == "\r\n" {
                    break;
                }
                if let Some(value) = line.to_ascii_lowercase().strip_prefix("content-length:") {
                    length = value.trim().parse().unwrap_or(0);
                }
            }
            let mut body = vec![0; length];
            let _ = reader.read_exact(&mut body);
            let reply = "{\"message\":\"stub failure\"}";
            let mut stream = reader.into_inner();
            let _ = write!(
                stream,
                "HTTP/1.1 500 Internal Server Error\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                reply.len(),
                reply
            );
        }
    });
    url
}

fn sync_workspace(token_env: &str, api_url: &str) -> TestFixtures {
    let fixtures = TestFixtures::new();
    let root = fixtures.get_temp_path();
    std::fs::create_dir_all(root.join(".tasks")).unwrap();
    std::fs::write(
        root.join(".tasks/config.yml"),
        "default:\n  project: GH\nremotes:\n  origin:\n    provider: github\n    repo: acme/demo\n    auth_profile: fixture\n    mapping:\n      title: title\n",
    )
    .unwrap();
    std::fs::write(
        root.join(".lotar"),
        format!(
            "auth_profiles:\n  fixture:\n    provider: github\n    method: token\n    token_env: {token_env}\n    api_url: {api_url}\n"
        ),
    )
    .unwrap();
    std::fs::set_permissions(
        root.join(".lotar"),
        std::os::unix::fs::PermissionsExt::from_mode(0o600),
    )
    .unwrap();
    let out = run(root, &["add", "push me", "-p", "GH"], root);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    fixtures
}

#[test]
fn config_show_prints_reviewers_and_sync_settings() {
    let fixtures = TestFixtures::new();
    let root = fixtures.get_temp_path();
    run(root, &["add", "a", "-p", "REV"], root);
    let set = run(
        root,
        &[
            "config",
            "set",
            "review.reviewers",
            "alice,bob",
            "--project",
            "REV",
        ],
        root,
    );
    assert!(
        set.status.success(),
        "{}",
        String::from_utf8_lossy(&set.stderr)
    );
    let shown = run(root, &["config", "show", "--project", "REV"], root);
    let text = String::from_utf8_lossy(&shown.stdout);
    assert!(
        text.contains("review:\n  reviewers:\n    - alice\n    - bob"),
        "{text}"
    );

    let full = run(root, &["config", "show", "--full"], root);
    let text = String::from_utf8_lossy(&full.stdout);
    assert!(text.contains("sync:\n  reports_dir: '@reports'"), "{text}");
    assert!(text.contains("agent:\n  context_enabled: true"), "{text}");
}

#[test]
fn an_unset_variable_name_is_not_sent_as_the_token() {
    let fixtures = sync_workspace("LOTAR_DEV300_UNSET_TOKEN", "http://127.0.0.1:9");
    let root = fixtures.get_temp_path();
    let out = run(root, &["push", "origin"], root);
    assert!(!out.status.success());
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("Environment variable 'LOTAR_DEV300_UNSET_TOKEN' is not set"),
        "{stderr}"
    );
}

#[test]
fn a_sync_with_failed_items_exits_non_zero() {
    let url = failing_server();
    let fixtures = sync_workspace("dev300-fake-token", &url);
    let root = fixtures.get_temp_path();

    let out = run(root, &["push", "origin", "--format", "json"], root);
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(!out.status.success(), "{stdout}");
    let json_line = stdout
        .lines()
        .find(|line| line.contains("\"summary\""))
        .unwrap_or_else(|| panic!("{stdout}"));
    let value: serde_json::Value = serde_json::from_str(json_line).unwrap();
    assert_eq!(value["status"], "partial", "{value}");
    assert_eq!(value["summary"]["failed"], 1, "{value}");

    // Text output, in a fresh workspace: the failed create above leaves a
    // pending-creation journal that blocks retries on purpose.
    let fixtures = sync_workspace("dev300-fake-token", &url);
    let root = fixtures.get_temp_path();
    let text = run(root, &["push", "origin"], root);
    assert_eq!(text.status.code(), Some(1));
    assert!(
        String::from_utf8_lossy(&text.stderr).contains("1 failed item"),
        "{}",
        String::from_utf8_lossy(&text.stderr)
    );
}

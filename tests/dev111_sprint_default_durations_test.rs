//! DEV-111: sprint default durations are validated when they are set.
//!
//! Policy: `sprints_defaults_length` takes a positive number of days or weeks
//! (`14d`, `2w`, `3 weeks`); `sprints_defaults_overdue_after` takes zero or
//! more hours, days or weeks (`12h`, `1d`). Both are capped at ten years and
//! an empty value clears the default. Invalid values are schema errors on
//! every mutation surface (CLI, dry-run, service/REST, MCP) — `--force`
//! cannot bypass them and rejected changes leave config bytes untouched.
//! Values already saved in a config file stay readable: `config validate`
//! reports them as warnings and status reads never panic on them.

mod common;

use crate::common::cargo_bin_silent;
use crate::common::env_mutex::EnvVarGuard;
use lotar::services::config_service::ConfigService;
use lotar::workspace::{TasksDirectoryResolver, TasksDirectorySource};
use std::collections::BTreeMap;
use std::fs;
use std::path::Path;
use tempfile::TempDir;

const SEED: &str = "default:\n  project: DEMO\nsprints:\n  defaults:\n    length: 2w\n";

fn workspace() -> (TempDir, std::path::PathBuf) {
    let tmp = TempDir::new().unwrap();
    let tasks_dir = tmp.path().join(".tasks");
    fs::create_dir_all(&tasks_dir).unwrap();
    fs::write(tasks_dir.join("config.yml"), SEED).unwrap();
    (tmp, tasks_dir)
}

fn resolver_for(path: &Path) -> TasksDirectoryResolver {
    TasksDirectoryResolver {
        path: path.to_path_buf(),
        source: TasksDirectorySource::CurrentDirectory,
    }
}

fn run_cli(cwd: &Path, args: &[&str]) -> (bool, String) {
    let mut cmd = cargo_bin_silent();
    let assert = cmd
        .current_dir(cwd)
        .env("LOTAR_IGNORE_HOME_CONFIG", "1")
        .args(args)
        .assert();
    let output = assert.get_output();
    (
        output.status.success(),
        format!(
            "{}{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        ),
    )
}

fn config_bytes(tasks_dir: &Path) -> String {
    fs::read_to_string(tasks_dir.join("config.yml")).unwrap()
}

fn values(field: &str, value: &str) -> BTreeMap<String, String> {
    BTreeMap::from([(field.to_string(), value.to_string())])
}

#[test]
fn dev111_cli_accepts_the_sprint_grammar_and_clears_with_empty() {
    let (tmp, tasks_dir) = workspace();
    for (field, value) in [
        ("sprints_defaults_length", "14d"),
        ("sprints_defaults_length", "3 weeks"),
        ("sprints_defaults_overdue_after", "12h"),
        ("sprints_defaults_overdue_after", "0d"),
    ] {
        let (ok, out) = run_cli(tmp.path(), &["config", "set", field, value, "--global"]);
        assert!(ok, "{field}={value} should be accepted: {out}");
    }
    let saved = config_bytes(&tasks_dir);
    assert!(saved.contains("length: 3 weeks"), "{saved}");
    assert!(saved.contains("overdue_after: 0d"), "{saved}");

    let (ok, out) = run_cli(
        tmp.path(),
        &[
            "config",
            "set",
            "sprints_defaults_overdue_after",
            "",
            "--global",
        ],
    );
    assert!(ok, "empty clears: {out}");
    assert!(!config_bytes(&tasks_dir).contains("overdue_after"));
}

#[test]
fn dev111_cli_rejects_invalid_values_without_writing_even_with_force_or_dry_run() {
    let (tmp, tasks_dir) = workspace();
    let cases = [
        (
            "sprints_defaults_length",
            "not-a-duration",
            "is not a duration",
        ),
        ("sprints_defaults_length", "12h", "is not a duration"),
        ("sprints_defaults_length", "0w", "longer than zero"),
        (
            "sprints_defaults_length",
            "99999999999999999999w",
            "maximum",
        ),
        ("sprints_defaults_length", "600w", "maximum"),
        (
            "sprints_defaults_overdue_after",
            "soon",
            "is not a duration",
        ),
        ("sprints_defaults_overdue_after", "87601h", "maximum"),
    ];
    for (field, value, expected) in cases {
        for extra in [None, Some("--dry-run"), Some("--force")] {
            let mut args = vec!["config", "set", field, value, "--global"];
            args.extend(extra);
            let (ok, out) = run_cli(tmp.path(), &args);
            assert!(!ok, "{field}={value} {extra:?} must be rejected: {out}");
            assert!(out.contains(expected), "{field}={value}: {out}");
            assert_eq!(
                config_bytes(&tasks_dir),
                SEED,
                "rejected change wrote config"
            );
        }
    }
}

#[test]
fn dev111_service_rest_and_mcp_reject_invalid_values_without_writing() {
    let (_tmp, tasks_dir) = workspace();
    let _guard = EnvVarGuard::set("LOTAR_TASKS_DIR", &tasks_dir.to_string_lossy());
    let resolver = resolver_for(&tasks_dir);

    // Service layer (backs REST and MCP).
    let err = ConfigService::set(
        &resolver,
        &values("sprints_defaults_length", "-2w"),
        true,
        None,
    )
    .expect_err("negative length must be rejected");
    assert!(err.to_string().contains("is not a duration"), "{err}");
    assert_eq!(config_bytes(&tasks_dir), SEED);

    // REST.
    let mut api = lotar::api_server::ApiServer::new();
    lotar::routes::initialize(&mut api);
    let resp = api.handle_request(&lotar::api_server::HttpRequest {
        method: "POST".to_string(),
        path: "/api/config/set".to_string(),
        query: Default::default(),
        headers: Default::default(),
        body: serde_json::to_vec(&serde_json::json!({
            "values": {"sprints_defaults_overdue_after": "whenever"},
            "global": true
        }))
        .unwrap(),
    });
    let body = String::from_utf8_lossy(&resp.body).into_owned();
    assert_eq!(resp.status, 400, "{body}");
    assert!(body.contains("is not a duration"), "{body}");
    assert_eq!(config_bytes(&tasks_dir), SEED);

    // MCP.
    let line = serde_json::to_string(&serde_json::json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "tools/call",
        "params": {
            "name": "config_set",
            "arguments": {"values": {"sprints_defaults_length": "1y"}, "global": true}
        }
    }))
    .unwrap();
    let resp = lotar::mcp::server::handle_json_line(&line);
    assert!(
        resp.contains("is not a duration"),
        "MCP must explain the rejection: {resp}"
    );
    assert_eq!(config_bytes(&tasks_dir), SEED);

    // A valid value still goes through the same path.
    ConfigService::set(
        &resolver,
        &values("sprints_defaults_length", "10d"),
        true,
        None,
    )
    .expect("valid length");
    assert!(config_bytes(&tasks_dir).contains("length: 10d"));
}

#[test]
fn dev111_saved_malformed_values_stay_readable_and_warn() {
    let (tmp, tasks_dir) = workspace();
    let legacy = "default:\n  project: DEMO\nsprints:\n  defaults:\n    length: not-a-duration\n    overdue_after: later\n";
    fs::write(tasks_dir.join("config.yml"), legacy).unwrap();

    // Unrelated reads and writes keep working.
    let (ok, out) = run_cli(tmp.path(), &["add", "Legacy config task"]);
    assert!(ok, "task creation must not be blocked: {out}");
    let (ok, out) = run_cli(
        tmp.path(),
        &["config", "set", "default_priority", "High", "--global"],
    );
    assert!(ok, "unrelated config change must not be blocked: {out}");
    assert!(config_bytes(&tasks_dir).contains("length: not-a-duration"));

    let (_, out) = run_cli(tmp.path(), &["config", "validate", "--global"]);
    assert!(out.contains("sprints.defaults.length"), "{out}");
    assert!(out.contains("sprints.defaults.overdue_after"), "{out}");
}

#[test]
fn dev111_out_of_range_sprint_length_does_not_panic_status_reads() {
    let (tmp, tasks_dir) = workspace();
    fs::create_dir_all(tasks_dir.join("@sprints")).unwrap();
    fs::write(
        tasks_dir.join("@sprints/1.yml"),
        "plan:\n  label: Huge\n  starts_at: 2026-10-01T00:00:00Z\n  length: 99999999999w\n",
    )
    .unwrap();
    let (ok, out) = run_cli(tmp.path(), &["sprint", "list"]);
    assert!(ok, "sprint list must not panic: {out}");
    assert!(out.contains("Huge"), "{out}");
}

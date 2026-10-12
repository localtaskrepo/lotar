//! DEV-102 regression coverage: config no-op mutations.
//!
//! A config-set request whose complete candidate equals the ORIGINAL RAW
//! configuration is an effective no-op: the stored file keeps its exact
//! bytes and mtime, caches are not invalidated, no `config_updated` event
//! fires, and the response reports `updated: false` through the existing
//! shape. The full validation contract still applies to no-ops (invalid
//! values, malformed current configuration, task conflicts and the force
//! boundary are unchanged). Provenance is respected: an explicit override
//! equal to an inherited default — and an explicit server-port pin the raw
//! file does not carry yet — are genuine changes, never no-ops.

mod common;

use crate::common::cargo_bin_silent;
use crate::common::env_mutex::EnvVarGuard;
use lotar::api_events;
use lotar::config::candidate::{self, ConfigScope, ConfigSetRequest};
use lotar::services::config_service::ConfigService;
use lotar::workspace::{TasksDirectoryResolver, TasksDirectorySource};
use std::collections::BTreeMap;
use std::fs;
use std::path::Path;
use std::time::{Duration, SystemTime};
use tempfile::TempDir;

fn resolver_for(path: &Path) -> TasksDirectoryResolver {
    TasksDirectoryResolver {
        path: path.to_path_buf(),
        source: TasksDirectorySource::CurrentDirectory,
    }
}

fn ensure_tasks_dir(root: &Path) -> std::path::PathBuf {
    let tasks_dir = root.join(".tasks");
    fs::create_dir_all(&tasks_dir).unwrap();
    tasks_dir
}

fn map(entries: &[(&str, &str)]) -> BTreeMap<String, String> {
    entries
        .iter()
        .map(|(k, v)| (k.to_string(), v.to_string()))
        .collect()
}

fn run_cli(cwd: &Path, args: &[&str]) -> (bool, String) {
    let mut cmd = cargo_bin_silent();
    let assert = cmd.current_dir(cwd).args(args).assert();
    let output = assert.get_output();
    let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
    let stderr = String::from_utf8_lossy(&output.stderr).into_owned();
    (output.status.success(), format!("{}{}", stdout, stderr))
}

/// Bytes and mtime of a stored file, for no-op preservation proofs.
fn file_state(path: &Path) -> (Vec<u8>, SystemTime) {
    let bytes = fs::read(path).unwrap();
    let mtime = fs::metadata(path).unwrap().modified().unwrap();
    (bytes, mtime)
}

fn apply(
    tasks_dir: &Path,
    scope: &ConfigScope,
    entries: &[(&str, &str)],
    force: bool,
    dry_run: bool,
) -> candidate::ConfigSetResult {
    candidate::apply_config_set(
        tasks_dir,
        scope,
        &ConfigSetRequest {
            entries: entries
                .iter()
                .map(|(k, v)| (k.to_string(), v.to_string()))
                .collect(),
            force,
            dry_run,
        },
    )
    .expect("candidate pipeline should accept the request")
}

// ---------------------------------------------------------------------------
// Service layer (backs REST and MCP)
// ---------------------------------------------------------------------------

#[test]
fn dev102_service_same_value_multifield_global_noop() {
    let tmp = TempDir::new().unwrap();
    let tasks_dir = ensure_tasks_dir(tmp.path());
    let resolver = resolver_for(&tasks_dir);
    let config_path = tasks_dir.join("config.yml");

    let seed = ConfigService::set(
        &resolver,
        &map(&[("default_assignee", "alice"), ("default_reporter", "bob")]),
        true,
        None,
    )
    .expect("seed global values");
    assert!(seed.updated, "seeding absent fields is a real change");
    let before = file_state(&config_path);

    // Re-sending the exact same multi-field request is a no-op.
    let noop = ConfigService::set(
        &resolver,
        &map(&[("default_assignee", "alice"), ("default_reporter", "bob")]),
        true,
        None,
    )
    .expect("same-value request must stay accepted");
    assert!(!noop.updated, "same values must not report updated");
    let after = file_state(&config_path);
    assert_eq!(before.0, after.0, "no-op must preserve global bytes");
    assert_eq!(
        before.1, after.1,
        "no-op must preserve the global config mtime (no rewrite)"
    );

    // Control: a genuinely different value still writes.
    let real = ConfigService::set(
        &resolver,
        &map(&[("default_assignee", "carol"), ("default_reporter", "bob")]),
        true,
        None,
    )
    .expect("real change must apply");
    assert!(real.updated, "changed value must report updated");
    assert_ne!(before.0, fs::read(&config_path).unwrap());
}

#[test]
fn dev102_service_same_value_project_noop() {
    let tmp = TempDir::new().unwrap();
    let tasks_dir = ensure_tasks_dir(tmp.path());
    let resolver = resolver_for(&tasks_dir);
    let config_path = tasks_dir.join("ENG/config.yml");

    let seed = ConfigService::set(
        &resolver,
        &map(&[("issue_states", "Todo,Done")]),
        false,
        Some("ENG"),
    )
    .expect("seed project override");
    assert!(seed.updated);
    let before = file_state(&config_path);

    let noop = ConfigService::set(
        &resolver,
        &map(&[("issue_states", "Todo,Done")]),
        false,
        Some("ENG"),
    )
    .expect("same-value project request must stay accepted");
    assert!(!noop.updated, "same project values must not report updated");
    let after = file_state(&config_path);
    assert_eq!(before.0, after.0, "no-op must preserve project bytes");
    assert_eq!(
        before.1, after.1,
        "no-op must preserve the project config mtime (no rewrite)"
    );
}

#[test]
fn dev102_service_clear_absent_override_is_noop_and_real_clear_writes() {
    let tmp = TempDir::new().unwrap();
    let tasks_dir = ensure_tasks_dir(tmp.path());
    let resolver = resolver_for(&tasks_dir);
    let config_path = tasks_dir.join("ENG/config.yml");

    // The project config exists (with a member override) but never carried
    // an issue_states override: clearing it changes nothing.
    ConfigService::set(&resolver, &map(&[("members", "alice")]), false, Some("ENG"))
        .expect("seed unrelated project override");
    let before = file_state(&config_path);

    let noop = ConfigService::set(&resolver, &map(&[("issue_states", "")]), false, Some("ENG"))
        .expect("clearing an absent override must stay accepted");
    assert!(!noop.updated, "clearing a non-existent override is a no-op");
    let after = file_state(&config_path);
    assert_eq!(before.0, after.0, "absent-clear must preserve bytes");
    assert_eq!(before.1, after.1, "absent-clear must preserve mtime");

    // Control: clearing an override that DOES exist is a real change.
    ConfigService::set(
        &resolver,
        &map(&[("issue_states", "Todo,Done")]),
        false,
        Some("ENG"),
    )
    .expect("seed states override");
    let real = ConfigService::set(&resolver, &map(&[("issue_states", "")]), false, Some("ENG"))
        .expect("real clear must apply");
    assert!(real.updated, "clearing an existing override writes");
    let cleared =
        lotar::config::persistence::load_project_config_from_dir("ENG", &tasks_dir).unwrap();
    assert!(cleared.issue_states.is_none(), "override removed");
}

// ---------------------------------------------------------------------------
// Candidate API: provenance and the raw-vs-resolved boundary
// ---------------------------------------------------------------------------

#[test]
fn dev102_candidate_explicit_override_equal_to_inherited_default_is_real_change() {
    let tmp = TempDir::new().unwrap();
    let tasks_dir = ensure_tasks_dir(tmp.path());
    let scope = ConfigScope::Project("ENG".to_string());
    let config_path = tasks_dir.join("ENG/config.yml");

    // The project config exists with an unrelated override, so the no-op
    // boundary is armed; the requested states list is EXACTLY the inherited
    // global default. A resolved-view comparison would call this a no-op —
    // the raw comparison must not: the explicit override is provenance.
    apply(&tasks_dir, &scope, &[("members", "alice")], false, false);
    let inherited = "Todo,InProgress,NeedsReview,Done";

    let first = apply(
        &tasks_dir,
        &scope,
        &[("issue_states", inherited)],
        false,
        false,
    );
    assert!(
        first.updated,
        "explicit override equal to the inherited default is a genuine change"
    );
    let stored =
        lotar::config::persistence::load_project_config_from_dir("ENG", &tasks_dir).unwrap();
    assert!(
        stored.issue_states.is_some(),
        "explicit override persisted: {stored:?}"
    );

    // Re-sending the same explicit override is now the no-op case.
    let before = file_state(&config_path);
    let second = apply(
        &tasks_dir,
        &scope,
        &[("issue_states", inherited)],
        false,
        false,
    );
    assert!(!second.updated, "identical re-send is a no-op");
    let after = file_state(&config_path);
    assert_eq!(before.0, after.0);
    assert_eq!(before.1, after.1);
}

#[test]
fn dev102_candidate_explicit_pin_of_builtin_port_is_change_then_noop() {
    let tmp = TempDir::new().unwrap();
    let tasks_dir = ensure_tasks_dir(tmp.path());
    let scope = ConfigScope::Global;
    let config_path = tasks_dir.join("config.yml");

    // Seed a global config WITHOUT a server.port pin.
    apply(
        &tasks_dir,
        &scope,
        &[("default_assignee", "alice")],
        false,
        false,
    );
    let seeded = fs::read_to_string(&config_path).unwrap();
    assert!(
        !seeded.contains("port:"),
        "seed must not carry a port pin: {seeded}"
    );

    // Pinning the built-in default port is a provenance change: the raw
    // file gains the explicit pin even though the value equals the default.
    let pin = apply(&tasks_dir, &scope, &[("server_port", "8080")], false, false);
    assert!(
        pin.updated,
        "explicit pin of the default port must be a real change"
    );
    let pinned = fs::read_to_string(&config_path).unwrap();
    assert!(
        pinned.contains("port: 8080"),
        "pin must be stored: {pinned}"
    );

    // Re-pinning the same stored port is a no-op.
    let before = file_state(&config_path);
    let re_pin = apply(&tasks_dir, &scope, &[("server_port", "8080")], false, false);
    assert!(!re_pin.updated, "same stored pin re-send is a no-op");
    let after = file_state(&config_path);
    assert_eq!(before.0, after.0, "no-op keeps pinned bytes");
    assert_eq!(before.1, after.1, "no-op keeps pinned mtime");
}

#[test]
fn dev102_candidate_global_noop_never_populates_builtin_port() {
    let tmp = TempDir::new().unwrap();
    let tasks_dir = ensure_tasks_dir(tmp.path());
    let scope = ConfigScope::Global;

    apply(
        &tasks_dir,
        &scope,
        &[("default_assignee", "alice")],
        false,
        false,
    );
    // Same-value no-op on a file without a pin: nothing may inject the
    // built-in port (ServerPortSaveIntent::PreserveExisting semantics).
    let noop = apply(
        &tasks_dir,
        &scope,
        &[("default_assignee", "alice")],
        false,
        false,
    );
    assert!(!noop.updated);
    let config = fs::read_to_string(tasks_dir.join("config.yml")).unwrap();
    assert!(
        !config.contains("port:"),
        "no-op must not populate the builtin port: {config}"
    );
}

// ---------------------------------------------------------------------------
// Dry-run and rejection contracts unchanged for no-op-shaped requests
// ---------------------------------------------------------------------------

#[test]
fn dev102_candidate_dryrun_real_change_preserves_bytes_and_mtime() {
    let tmp = TempDir::new().unwrap();
    let tasks_dir = ensure_tasks_dir(tmp.path());
    let scope = ConfigScope::Global;
    let config_path = tasks_dir.join("config.yml");

    apply(
        &tasks_dir,
        &scope,
        &[("default_assignee", "alice")],
        false,
        false,
    );
    let before = file_state(&config_path);

    let dry = apply(
        &tasks_dir,
        &scope,
        &[("default_assignee", "zoe")],
        false,
        true,
    );
    assert!(!dry.updated, "dry runs never report updated");
    let after = file_state(&config_path);
    assert_eq!(before.0, after.0, "dry-run preserves bytes");
    assert_eq!(before.1, after.1, "dry-run preserves mtime");
}

#[test]
fn dev102_candidate_force_cannot_escape_schema_errors() {
    let tmp = TempDir::new().unwrap();
    let tasks_dir = ensure_tasks_dir(tmp.path());
    let scope = ConfigScope::Global;

    let err = match candidate::apply_config_set(
        &tasks_dir,
        &scope,
        &ConfigSetRequest {
            entries: vec![("server_port".to_string(), "not-a-port".to_string())],
            force: true,
            dry_run: false,
        },
    ) {
        Err(err) => err,
        Ok(_) => panic!("force must not bypass schema validation"),
    };
    assert!(
        err.to_string().contains("Invalid port"),
        "schema error surfaced: {err}"
    );
    assert!(
        !tasks_dir.join("config.yml").exists(),
        "rejected forced set must not write"
    );
}

#[test]
fn dev102_noop_shaped_request_still_rejects_malformed_current_config() {
    let tmp = TempDir::new().unwrap();
    let tasks_dir = ensure_tasks_dir(tmp.path());
    let broken = "not: [a valid\n  yaml: {{{\n# precious comment\n";
    fs::write(tasks_dir.join("config.yml"), broken).unwrap();

    // The requested value happens to equal the builtin default, but the
    // current configuration is unreadable: the contract still rejects.
    let err = match candidate::apply_config_set(
        &tasks_dir,
        &ConfigScope::Global,
        &ConfigSetRequest {
            entries: vec![("default_assignee".to_string(), "alice".to_string())],
            force: false,
            dry_run: false,
        },
    ) {
        Err(err) => err,
        Ok(_) => panic!("malformed current config must block even no-op-shaped sets"),
    };
    assert!(
        err.to_string().contains("Malformed global config"),
        "malformed config surfaced: {err}"
    );
    let after = fs::read_to_string(tasks_dir.join("config.yml")).unwrap();
    assert_eq!(after, broken, "rejected request preserves bytes exactly");
}

// ---------------------------------------------------------------------------
// REST route: existing response shape, event only on genuine writes
// ---------------------------------------------------------------------------

fn rest_set(values: serde_json::Value, global: bool) -> (u16, serde_json::Value) {
    let mut api = lotar::api_server::ApiServer::new();
    lotar::routes::initialize(&mut api);
    let resp = api.handle_request(&lotar::api_server::HttpRequest {
        method: "POST".to_string(),
        path: "/api/config/set".to_string(),
        query: Default::default(),
        headers: Default::default(),
        body: serde_json::to_vec(&serde_json::json!({
            "values": values,
            "global": global
        }))
        .unwrap(),
    });
    let body = serde_json::from_slice(&resp.body).unwrap_or_default();
    (resp.status, body)
}

fn next_config_event(rx: &std::sync::mpsc::Receiver<api_events::ApiEvent>) -> Option<String> {
    rx.recv_timeout(Duration::from_millis(500))
        .ok()
        .map(|event| event.kind)
}

#[test]
fn dev102_rest_noop_reports_updated_false_and_emits_no_event() {
    let tmp = TempDir::new().unwrap();
    let tasks_dir = ensure_tasks_dir(tmp.path());
    let _guard = EnvVarGuard::set("LOTAR_TASKS_DIR", &tasks_dir.to_string_lossy());

    let resolver = resolver_for(&tasks_dir);
    ConfigService::set(
        &resolver,
        &map(&[("default_assignee", "alice")]),
        true,
        None,
    )
    .expect("seed global value");
    let before = file_state(&tasks_dir.join("config.yml"));

    let rx = api_events::subscribe();
    let (status, body) = rest_set(serde_json::json!({"default_assignee": "alice"}), true);
    assert_eq!(status, 200, "no-op stays a success: {body}");
    assert_eq!(
        body.get("data").and_then(|d| d.get("updated")),
        Some(&serde_json::json!(false)),
        "existing shape reports updated:false: {body}"
    );
    assert!(
        next_config_event(&rx).is_none(),
        "no-op must not emit config_updated"
    );
    let after = file_state(&tasks_dir.join("config.yml"));
    assert_eq!(before.0, after.0, "REST no-op preserves bytes");
    assert_eq!(before.1, after.1, "REST no-op preserves mtime");

    // Control: a genuine change emits exactly one config_updated.
    let (status, body) = rest_set(serde_json::json!({"default_assignee": "carol"}), true);
    assert_eq!(status, 200);
    assert_eq!(
        body.get("data").and_then(|d| d.get("updated")),
        Some(&serde_json::json!(true))
    );
    assert_eq!(
        next_config_event(&rx).as_deref(),
        Some("config_updated"),
        "real change must emit config_updated"
    );
}

#[test]
fn dev102_rest_empty_values_updated_false_no_event() {
    let tmp = TempDir::new().unwrap();
    let tasks_dir = ensure_tasks_dir(tmp.path());
    let _guard = EnvVarGuard::set("LOTAR_TASKS_DIR", &tasks_dir.to_string_lossy());

    let rx = api_events::subscribe();
    let (status, body) = rest_set(serde_json::json!({}), true);
    assert_eq!(status, 200, "empty entries keep the existing shape: {body}");
    assert_eq!(
        body.get("data").and_then(|d| d.get("updated")),
        Some(&serde_json::json!(false)),
        "empty entries report updated:false: {body}"
    );
    assert!(
        next_config_event(&rx).is_none(),
        "empty entries must not emit config_updated"
    );
    assert!(
        !tasks_dir.join("config.yml").exists(),
        "empty entries must not create artifacts"
    );
}

// ---------------------------------------------------------------------------
// CLI and MCP entry points share the pipeline no-op behavior
// ---------------------------------------------------------------------------

#[test]
fn dev102_cli_same_value_preserves_bytes_and_mtime() {
    let tmp = TempDir::new().unwrap();
    let (ok, output) = run_cli(
        tmp.path(),
        &["config", "set", "default_assignee", "alice", "--global"],
    );
    assert!(ok, "seed CLI set: {output}");
    let config_path = tmp.path().join(".tasks/config.yml");
    let before = file_state(&config_path);

    let (ok, output) = run_cli(
        tmp.path(),
        &["config", "set", "default_assignee", "alice", "--global"],
    );
    assert!(ok, "same-value CLI set stays a success: {output}");
    let after = file_state(&config_path);
    assert_eq!(before.0, after.0, "CLI no-op preserves bytes");
    assert_eq!(before.1, after.1, "CLI no-op preserves mtime");
}

#[test]
fn dev102_mcp_same_value_preserves_bytes_and_mtime() {
    let tmp = TempDir::new().unwrap();
    let tasks_dir = ensure_tasks_dir(tmp.path());
    let _guard = EnvVarGuard::set("LOTAR_TASKS_DIR", &tasks_dir.to_string_lossy());

    let resolver = resolver_for(&tasks_dir);
    ConfigService::set(
        &resolver,
        &map(&[("default_assignee", "alice")]),
        true,
        None,
    )
    .expect("seed global value");
    let before = file_state(&tasks_dir.join("config.yml"));

    let line = serde_json::to_string(&serde_json::json!({
        "jsonrpc": "2.0",
        "id": 5,
        "method": "tools/call",
        "params": {
            "name": "config_set",
            "arguments": {"values": {"default_assignee": "alice"}, "global": true}
        }
    }))
    .unwrap();
    let resp: serde_json::Value =
        serde_json::from_str(&lotar::mcp::server::handle_json_line(&line)).unwrap();
    assert!(
        resp.get("error").is_none(),
        "same-value MCP set must succeed: {resp}"
    );
    let result = resp.get("result").cloned().unwrap_or_default();
    assert_ne!(
        result.get("isError"),
        Some(&serde_json::json!(true)),
        "no-op is not a tool failure: {resp}"
    );
    let after = file_state(&tasks_dir.join("config.yml"));
    assert_eq!(before.0, after.0, "MCP no-op preserves bytes");
    assert_eq!(before.1, after.1, "MCP no-op preserves mtime");
}

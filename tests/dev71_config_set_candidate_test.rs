//! DEV-71 regression coverage: one validated config-set candidate.
//!
//! Exercises the real CLI process (no git, isolated temp workspaces with home
//! config ignored), the service layer directly, the REST route, and the MCP
//! tool. Invariants: rejected changes create no artifacts and preserve bytes,
//! multi-field sets are all-or-nothing, conflicts reference real task IDs,
//! dry runs run the full pipeline minus the write, force escapes only task
//! conflicts, and field-precise project clears preserve sibling settings.

mod common;

use crate::common::cargo_bin_silent;
use crate::common::env_mutex::EnvVarGuard;
use lotar::services::config_service::ConfigService;
use lotar::workspace::{TasksDirectoryResolver, TasksDirectorySource};
use std::collections::BTreeMap;
use std::fs;
use std::path::Path;
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

/// Run the real lotar CLI in `cwd`; returns (success, combined output).
fn run_cli(cwd: &Path, args: &[&str]) -> (bool, String) {
    let mut cmd = cargo_bin_silent();
    let assert = cmd.current_dir(cwd).args(args).assert();
    let output = assert.get_output();
    let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
    let stderr = String::from_utf8_lossy(&output.stderr).into_owned();
    (output.status.success(), format!("{}{}", stdout, stderr))
}

fn seed_task(tasks_dir: &Path, project: &str, numeric: u64, status: &str) {
    let dir = tasks_dir.join(project);
    fs::create_dir_all(&dir).unwrap();
    fs::write(
        dir.join(format!("{}.yml", numeric)),
        format!(
            "title: Task {numeric}\nstatus: {status}\npriority: Medium\ntype: Task\ncreated: \"2026-01-01T00:00:00Z\"\n"
        ),
    )
    .unwrap();
}

fn map(entries: &[(&str, &str)]) -> BTreeMap<String, String> {
    entries
        .iter()
        .map(|(k, v)| (k.to_string(), v.to_string()))
        .collect()
}

// ---------------------------------------------------------------------------
// CLI: no side effects on rejection / dry-run
// ---------------------------------------------------------------------------

#[test]
fn dev71_cli_invalid_dryrun_fails_and_creates_nothing() {
    let tmp = TempDir::new().unwrap();
    let (ok, output) = run_cli(
        tmp.path(),
        &["config", "set", "not_a_field", "x", "--dry-run"],
    );
    assert!(!ok, "invalid dry-run must exit nonzero: {output}");
    assert!(
        !tmp.path().join(".tasks").exists(),
        "invalid dry-run must not create the tasks directory"
    );
}

#[test]
fn dev71_cli_invalid_real_run_creates_nothing() {
    let tmp = TempDir::new().unwrap();
    let (ok, output) = run_cli(tmp.path(), &["config", "set", "not_a_field", "x"]);
    assert!(!ok, "invalid set must exit nonzero: {output}");
    assert!(
        !tmp.path().join(".tasks").exists(),
        "rejected set must not create the tasks directory"
    );
}

#[test]
fn dev71_cli_invalid_schema_dryrun_fails() {
    let tmp = TempDir::new().unwrap();
    let (ok, output) = run_cli(
        tmp.path(),
        &[
            "config",
            "set",
            "server_port",
            "not-a-port",
            "--global",
            "--dry-run",
        ],
    );
    assert!(!ok, "schema-invalid dry-run must exit nonzero: {output}");
    assert!(!tmp.path().join(".tasks").exists());
}

#[test]
fn dev71_cli_explicit_tasks_dir_rejection_leaves_no_config() {
    let tmp = TempDir::new().unwrap();
    let ws = tmp.path().join("ws");
    let (ok, output) = run_cli(
        tmp.path(),
        &[
            "--tasks-dir",
            ws.to_str().unwrap(),
            "config",
            "set",
            "bogus",
            "x",
        ],
    );
    assert!(!ok, "rejected set must fail: {output}");
    // The resolver creates the tasks directory itself (pre-existing,
    // documented --tasks-dir behavior) but nothing else may appear.
    assert!(ws.exists(), "resolver-created tasks dir may exist");
    assert!(
        !ws.join("config.yml").exists(),
        "rejected set must not create config.yml"
    );
    let entries: Vec<_> = fs::read_dir(&ws).unwrap().collect();
    assert!(
        entries.is_empty(),
        "rejected set must not create project dirs"
    );
}

// ---------------------------------------------------------------------------
// CLI: canonical dotted names
// ---------------------------------------------------------------------------

#[test]
fn dev71_cli_dotted_server_port_auto_promotes_to_global() {
    let tmp = TempDir::new().unwrap();
    let (ok, output) = run_cli(tmp.path(), &["config", "set", "server.port", "9000"]);
    assert!(ok, "dotted server.port should apply: {output}");
    assert!(
        output.contains("Automatically treating 'server.port'"),
        "auto-promotion should be reported: {output}"
    );
    let config = fs::read_to_string(tmp.path().join(".tasks/config.yml")).unwrap();
    assert!(
        config.contains("server:"),
        "canonical server section: {config}"
    );
    assert!(config.contains("port: 9000"), "port value: {config}");
    let entries: Vec<String> = fs::read_dir(tmp.path().join(".tasks"))
        .unwrap()
        .filter_map(|e| e.ok())
        .filter(|e| e.path().is_dir())
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .collect();
    assert!(
        entries.is_empty(),
        "global-only field must not create project dirs: {entries:?}"
    );
}

#[test]
fn dev71_cli_dotted_default_strict_members_project() {
    let tmp = TempDir::new().unwrap();
    let (ok, output) = run_cli(
        tmp.path(),
        &["--project=ENG", "config", "set", "members", "alice,bob"],
    );
    assert!(ok, "members should apply: {output}");
    let (ok, output) = run_cli(
        tmp.path(),
        &[
            "--project=ENG",
            "config",
            "set",
            "default.strict_members",
            "true",
        ],
    );
    assert!(ok, "dotted default.strict_members should apply: {output}");
    let config = fs::read_to_string(tmp.path().join(".tasks/ENG/config.yml")).unwrap();
    assert!(
        config.contains("strict_members: true"),
        "canonical strict_members: {config}"
    );
    assert!(config.contains("alice"), "members preserved: {config}");
}

#[test]
fn dev71_cli_dotted_issue_tags_global() {
    let tmp = TempDir::new().unwrap();
    let (ok, output) = run_cli(
        tmp.path(),
        &[
            "config",
            "set",
            "issue.tags",
            "frontend,backend",
            "--global",
        ],
    );
    assert!(ok, "dotted issue.tags should apply: {output}");
    let config = fs::read_to_string(tmp.path().join(".tasks/config.yml")).unwrap();
    assert!(config.contains("tags:"), "canonical issue tags: {config}");
    assert!(config.contains("frontend"), "tag value: {config}");
}

#[test]
fn dev71_cli_dryrun_leaves_bytes_identical() {
    let tmp = TempDir::new().unwrap();
    let (ok, output) = run_cli(tmp.path(), &["config", "set", "server.port", "9000"]);
    assert!(ok, "{output}");
    let before = fs::read(tmp.path().join(".tasks/config.yml")).unwrap();
    let (ok, output) = run_cli(
        tmp.path(),
        &["config", "set", "server.port", "9001", "--dry-run"],
    );
    assert!(ok, "valid dry-run must succeed: {output}");
    let after = fs::read(tmp.path().join(".tasks/config.yml")).unwrap();
    assert_eq!(before, after, "dry-run must not modify config bytes");
}

// ---------------------------------------------------------------------------
// CLI: real task conflicts
// ---------------------------------------------------------------------------

fn eng_project_with_inprogress_task(tmp: &TempDir) {
    let tasks_dir = ensure_tasks_dir(tmp.path());
    seed_task(&tasks_dir, "ENG", 1, "InProgress");
}

#[test]
fn dev71_cli_task_conflict_blocked_with_real_task_id() {
    let tmp = TempDir::new().unwrap();
    eng_project_with_inprogress_task(&tmp);
    let config_path = tmp.path().join(".tasks/ENG/config.yml");
    let (ok, output) = run_cli(
        tmp.path(),
        &[
            "--project=ENG",
            "config",
            "set",
            "issue_states",
            "Todo,Done",
        ],
    );
    assert!(!ok, "conflicting set must fail: {output}");
    assert!(
        output.contains("ENG-1"),
        "conflict must reference the real task ID: {output}"
    );
    assert!(
        output.contains("--force"),
        "conflict must hint the force escape: {output}"
    );
    assert!(
        !config_path.exists(),
        "blocked change must not write a project config"
    );
    let task = fs::read_to_string(tmp.path().join(".tasks/ENG/1.yml")).unwrap();
    assert!(
        task.contains("InProgress"),
        "task must be untouched: {task}"
    );
}

#[test]
fn dev71_cli_task_conflict_force_applies() {
    let tmp = TempDir::new().unwrap();
    eng_project_with_inprogress_task(&tmp);
    let (ok, output) = run_cli(
        tmp.path(),
        &[
            "--project=ENG",
            "config",
            "set",
            "issue_states",
            "Todo,Done",
            "--force",
        ],
    );
    assert!(ok, "force must apply despite task conflict: {output}");
    let config = fs::read_to_string(tmp.path().join(".tasks/ENG/config.yml")).unwrap();
    assert!(config.contains("Done"), "new states stored: {config}");
    let task = fs::read_to_string(tmp.path().join(".tasks/ENG/1.yml")).unwrap();
    assert!(
        task.contains("InProgress"),
        "force must not rewrite tasks: {task}"
    );
}

#[test]
fn dev71_cli_dryrun_conflict_nonzero_without_force() {
    let tmp = TempDir::new().unwrap();
    eng_project_with_inprogress_task(&tmp);
    let (ok, output) = run_cli(
        tmp.path(),
        &[
            "--project=ENG",
            "config",
            "set",
            "issue_states",
            "Todo,Done",
            "--dry-run",
        ],
    );
    assert!(
        !ok,
        "dry-run conflict without force must exit nonzero (D2): {output}"
    );
    assert!(
        !tmp.path().join(".tasks/ENG/config.yml").exists(),
        "dry-run must not write"
    );
}

#[test]
fn dev71_cli_dryrun_force_conflict_succeeds_without_write() {
    let tmp = TempDir::new().unwrap();
    eng_project_with_inprogress_task(&tmp);
    let (ok, output) = run_cli(
        tmp.path(),
        &[
            "--project=ENG",
            "config",
            "set",
            "issue_states",
            "Todo,Done",
            "--dry-run",
            "--force",
        ],
    );
    assert!(ok, "dry-run with force should report would-apply: {output}");
    assert!(
        !tmp.path().join(".tasks/ENG/config.yml").exists(),
        "dry-run with force must not write"
    );
}

#[test]
fn dev71_cli_force_does_not_escape_schema_errors() {
    let tmp = TempDir::new().unwrap();
    let (ok, output) = run_cli(
        tmp.path(),
        &[
            "config",
            "set",
            "server.port",
            "not-a-port",
            "--force",
            "--dry-run",
        ],
    );
    assert!(!ok, "schema errors must block even with force: {output}");
    assert!(!tmp.path().join(".tasks").exists());
}

#[test]
fn dev71_cli_malformed_existing_config_bytes_preserved() {
    let tmp = TempDir::new().unwrap();
    let tasks_dir = ensure_tasks_dir(tmp.path());
    let broken = "not: [a valid\n  yaml: {{{\n# precious comment\n";
    fs::write(tasks_dir.join("config.yml"), broken).unwrap();
    let (ok, output) = run_cli(
        tmp.path(),
        &["config", "set", "default_assignee", "alice", "--global"],
    );
    assert!(!ok, "malformed config must block the change: {output}");
    let after = fs::read_to_string(tasks_dir.join("config.yml")).unwrap();
    assert_eq!(after, broken, "rejected change must preserve bytes exactly");
}

#[test]
fn dev71_cli_malformed_task_file_fails_closed() {
    let tmp = TempDir::new().unwrap();
    let tasks_dir = ensure_tasks_dir(tmp.path());
    seed_task(&tasks_dir, "ENG", 1, "InProgress");
    fs::write(tasks_dir.join("ENG/2.yml"), "{{{ not yaml").unwrap();
    let (ok, output) = run_cli(
        tmp.path(),
        &[
            "--project=ENG",
            "config",
            "set",
            "issue_states",
            "Todo,Done",
        ],
    );
    assert!(!ok, "malformed task file must block validation: {output}");
    assert!(
        output.contains("2.yml"),
        "error must name the unreadable file: {output}"
    );
    assert!(
        !tasks_dir.join("ENG/config.yml").exists(),
        "blocked change must not write"
    );
}

#[test]
fn dev71_cli_preexisting_invalid_task_does_not_block_unrelated_change() {
    let tmp = TempDir::new().unwrap();
    let tasks_dir = ensure_tasks_dir(tmp.path());
    // A pre-existing task whose status is already outside the resolved list.
    seed_task(&tasks_dir, "ENG", 1, "Weird");
    // Unrelated field change: cannot newly invalidate anything.
    let (ok, output) = run_cli(
        tmp.path(),
        &[
            "--project=ENG",
            "config",
            "set",
            "default_assignee",
            "alice",
        ],
    );
    assert!(ok, "unrelated change must not be blocked: {output}");
    // Re-asserting the same effective enum list is not a new violation either.
    let (ok, output) = run_cli(
        tmp.path(),
        &[
            "--project=ENG",
            "config",
            "set",
            "issue_states",
            "Todo,InProgress,Verify,Blocked,Done",
        ],
    );
    assert!(
        ok,
        "same-list change must not trip on pre-existing invalid tasks: {output}"
    );
}

// ---------------------------------------------------------------------------
// CLI: -C overrides accept canonical dotted names
// ---------------------------------------------------------------------------

#[test]
fn dev71_cli_config_override_accepts_dotted_alias() {
    let tmp = TempDir::new().unwrap();
    let (ok, output) = run_cli(
        tmp.path(),
        &["--config", "default.strict_members=false", "config", "show"],
    );
    assert!(ok, "-C must accept canonical dotted field names: {output}");
}

// ---------------------------------------------------------------------------
// Service layer (also backs REST and MCP)
// ---------------------------------------------------------------------------

#[test]
fn dev71_service_multi_field_all_or_nothing() {
    let tmp = TempDir::new().unwrap();
    let tasks_dir = ensure_tasks_dir(tmp.path());
    let resolver = resolver_for(&tasks_dir);

    // Valid entry first (BTreeMap order), invalid second: nothing may save.
    let err = ConfigService::set(
        &resolver,
        &map(&[("default_assignee", "alice"), ("issue_types", "!!!")]),
        true,
        None,
    )
    .expect_err("invalid later entry must reject the whole request");
    assert!(
        err.to_string().contains("issue_types"),
        "error should name the offending field: {err}"
    );
    assert!(
        !tasks_dir.join("config.yml").exists(),
        "rejected multi-field set must not create the config file"
    );

    // All-valid multi-field set: one atomic write carries both values.
    let outcome = ConfigService::set(
        &resolver,
        &map(&[("default_assignee", "alice"), ("default_reporter", "bob")]),
        true,
        None,
    )
    .expect("valid multi-field set should apply");
    assert!(outcome.updated);
    let config = fs::read_to_string(tasks_dir.join("config.yml")).unwrap();
    assert!(config.contains("alice"), "assignee stored: {config}");
    assert!(config.contains("bob"), "reporter stored: {config}");
}

#[test]
fn dev71_service_canonical_duplicate_conflicts_rejected() {
    let tmp = TempDir::new().unwrap();
    let tasks_dir = ensure_tasks_dir(tmp.path());
    let resolver = resolver_for(&tasks_dir);

    let err = ConfigService::set(
        &resolver,
        &map(&[("tags", "a"), ("issue.tags", "b")]),
        true,
        None,
    )
    .expect_err("ambiguous alias values must be rejected");
    assert!(
        err.to_string().contains("Conflicting duplicate entries"),
        "deterministic collision error: {err}"
    );

    // Identical values through different spellings dedup silently.
    ConfigService::set(
        &resolver,
        &map(&[("tags", "a,b"), ("issue.tags", "a,b")]),
        true,
        None,
    )
    .expect("identical canonical duplicates should dedup");
}

#[test]
fn dev71_service_global_change_conflicts_on_real_tasks() {
    let tmp = TempDir::new().unwrap();
    let tasks_dir = ensure_tasks_dir(tmp.path());
    seed_task(&tasks_dir, "ENG", 1, "InProgress");
    seed_task(&tasks_dir, "ENG", 2, "Todo");
    let resolver = resolver_for(&tasks_dir);

    let err = ConfigService::set(
        &resolver,
        &map(&[("issue_states", "Todo,Done")]),
        true,
        None,
    )
    .expect_err("global enum shrink conflicts with real tasks");
    let message = err.to_string();
    assert!(message.contains("ENG-1"), "real task id: {message}");
    assert!(!message.contains("PROJ-1"), "no placeholder ids: {message}");
    assert!(
        !tasks_dir.join("config.yml").exists(),
        "rejected global change must not write"
    );
}

#[test]
fn dev71_service_project_override_masks_global_change() {
    let tmp = TempDir::new().unwrap();
    let tasks_dir = ensure_tasks_dir(tmp.path());
    let resolver = resolver_for(&tasks_dir);

    // MASK overrides the enum list; its InProgress task is shielded from
    // global shrinks. OPEN inherits the global list; its Done task is not.
    ConfigService::set(
        &resolver,
        &map(&[("issue_states", "Todo,InProgress,Done")]),
        false,
        Some("MASK"),
    )
    .expect("seed MASK override");
    seed_task(&tasks_dir, "MASK", 1, "InProgress");
    seed_task(&tasks_dir, "OPEN", 1, "Done");

    // Shrinking the global list past InProgress would conflict on MASK's
    // task if the override did not shield it...
    ConfigService::set(
        &resolver,
        &map(&[("issue_states", "Todo,Done")]),
        true,
        None,
    )
    .expect("masked project must not conflict");

    // ...but a further shrink newly invalidates OPEN's task.
    let err = ConfigService::set(&resolver, &map(&[("issue_states", "Todo")]), true, None)
        .expect_err("unshielded project must conflict");
    let message = err.to_string();
    assert!(
        message.contains("OPEN-1"),
        "conflict names the affected project's task: {message}"
    );
    assert!(
        !message.contains("MASK-1"),
        "shielded project must not be reported: {message}"
    );
}

#[test]
fn dev71_service_inherited_cross_field_error_without_tasks() {
    let tmp = TempDir::new().unwrap();
    let tasks_dir = ensure_tasks_dir(tmp.path());
    let resolver = resolver_for(&tasks_dir);

    ConfigService::set(
        &resolver,
        &map(&[("branch_status_aliases", "{wip: InProgress}")]),
        true,
        None,
    )
    .expect("seed global alias");

    // LIB has no tasks at all: the inherited alias target still must not be
    // broken by a project-level enum change.
    let err = ConfigService::set(
        &resolver,
        &map(&[("issue_states", "Todo,Done")]),
        false,
        Some("LIB"),
    )
    .expect_err("resolved alias target loss must block");
    assert!(
        err.to_string().contains("branch_status_aliases") || err.to_string().contains("alias"),
        "cross-field resolved error surfaced: {err}"
    );
}

#[test]
fn dev71_service_default_status_mismatch_stays_warning() {
    let tmp = TempDir::new().unwrap();
    let tasks_dir = ensure_tasks_dir(tmp.path());
    let resolver = resolver_for(&tasks_dir);

    let outcome = ConfigService::set(&resolver, &map(&[("default_status", "Weird")]), true, None)
        .expect("default_status mismatch is a warning, not an error");
    assert!(outcome.updated);
    assert!(
        outcome
            .validation
            .warnings
            .iter()
            .any(|w| w.message.contains("not found")),
        "warning surfaced: {:?}",
        outcome.validation.warnings
    );
}

#[test]
fn dev71_service_remotes_map_still_settable() {
    let tmp = TempDir::new().unwrap();
    let tasks_dir = ensure_tasks_dir(tmp.path());
    let resolver = resolver_for(&tasks_dir);

    // SyncHub-style values map carrying a remotes payload.
    ConfigService::set(
        &resolver,
        &map(&[(
            "remotes",
            "{origin: {provider: github, repo: octocat/hello}}",
        )]),
        true,
        None,
    )
    .expect("remotes map should remain settable");
    let config = fs::read_to_string(tasks_dir.join("config.yml")).unwrap();
    assert!(config.contains("remotes"), "remotes stored: {config}");
    assert!(
        config.contains("octocat/hello"),
        "remote payload stored: {config}"
    );
}

#[test]
fn dev71_service_field_precise_agent_clears() {
    let tmp = TempDir::new().unwrap();
    let tasks_dir = ensure_tasks_dir(tmp.path());
    let resolver = resolver_for(&tasks_dir);

    ConfigService::set(
        &resolver,
        &map(&[
            ("agent_on_start_status", "InProgress"),
            ("agent_on_start_reassign_to", "bob"),
            ("agent_on_success_status", "Done"),
        ]),
        false,
        Some("ENG"),
    )
    .expect("seed agent overrides");

    // Clearing on_start's status keeps its reassign sibling.
    ConfigService::set(
        &resolver,
        &map(&[("agent_on_start_status", "")]),
        false,
        Some("ENG"),
    )
    .expect("clear on_start status");
    let cfg = lotar::config::persistence::load_project_config_from_dir("ENG", &tasks_dir).unwrap();
    let automation = cfg.agent_automation.as_ref().expect("section survives");
    let on_start = automation.on_start.as_ref().expect("on_start survives");
    assert!(
        on_start.set_status.is_none(),
        "cleared subfield is gone: {:?}",
        on_start
    );
    assert_eq!(
        on_start.reassign_to.as_deref(),
        Some("bob"),
        "sibling reassign preserved: {:?}",
        on_start
    );
    assert!(
        automation
            .on_success
            .as_ref()
            .and_then(|a| a.set_status.as_ref())
            .is_some(),
        "unrelated sibling action preserved: {:?}",
        automation
    );

    // Clearing the last on_start subfield removes only that action.
    ConfigService::set(
        &resolver,
        &map(&[("agent_on_start_reassign_to", "")]),
        false,
        Some("ENG"),
    )
    .expect("clear on_start reassign");
    let cfg = lotar::config::persistence::load_project_config_from_dir("ENG", &tasks_dir).unwrap();
    let automation = cfg.agent_automation.as_ref().expect("section survives");
    assert!(automation.on_start.is_none(), "emptied action pruned");
    assert!(automation.on_success.is_some(), "on_success still present");

    // Clearing the final subfield prunes the whole section.
    ConfigService::set(
        &resolver,
        &map(&[("agent_on_success_status", "")]),
        false,
        Some("ENG"),
    )
    .expect("clear on_success status");
    let cfg = lotar::config::persistence::load_project_config_from_dir("ENG", &tasks_dir).unwrap();
    assert!(
        cfg.agent_automation.is_none(),
        "emptied section pruned: {:?}",
        cfg.agent_automation
    );
}

// ---------------------------------------------------------------------------
// REST route
// ---------------------------------------------------------------------------

#[test]
fn dev71_rest_conflict_rejects_with_400() {
    let tmp = TempDir::new().unwrap();
    let tasks_dir = ensure_tasks_dir(tmp.path());
    seed_task(&tasks_dir, "ENG", 1, "InProgress");
    let _guard = EnvVarGuard::set("LOTAR_TASKS_DIR", &tasks_dir.to_string_lossy());

    let mut api = lotar::api_server::ApiServer::new();
    lotar::routes::initialize(&mut api);

    let resp = api.handle_request(&lotar::api_server::HttpRequest {
        method: "POST".to_string(),
        path: "/api/config/set".to_string(),
        query: Default::default(),
        headers: Default::default(),
        body: serde_json::to_vec(&serde_json::json!({
            "values": {"issue_states": "Todo,Done"},
            "global": true
        }))
        .unwrap(),
    });
    assert_eq!(
        resp.status,
        400,
        "REST task conflicts must reject (D1): {}",
        String::from_utf8_lossy(&resp.body)
    );
    assert!(
        !tasks_dir.join("config.yml").exists(),
        "rejected REST set must not write"
    );
    let body = String::from_utf8_lossy(&resp.body).into_owned();
    assert!(body.contains("ENG-1"), "real task id in envelope: {body}");
}

// ---------------------------------------------------------------------------
// MCP tool
// ---------------------------------------------------------------------------

#[test]
fn dev71_mcp_config_set_conflict_rejects() {
    let tmp = TempDir::new().unwrap();
    let tasks_dir = ensure_tasks_dir(tmp.path());
    seed_task(&tasks_dir, "ENG", 1, "InProgress");
    let _guard = EnvVarGuard::set("LOTAR_TASKS_DIR", &tasks_dir.to_string_lossy());

    let line = serde_json::to_string(&serde_json::json!({
        "jsonrpc": "2.0",
        "id": 7,
        "method": "tools/call",
        "params": {
            "name": "config_set",
            "arguments": {
                "values": {"issue_states": "Todo,Done"},
                "global": true
            }
        }
    }))
    .unwrap();
    let resp_line = lotar::mcp::server::handle_json_line(&line);
    let resp: serde_json::Value = serde_json::from_str(&resp_line).unwrap();
    let error = resp
        .get("error")
        .expect("MCP task conflicts must reject without force");
    assert_eq!(
        error.get("code").and_then(|c| c.as_i64()),
        Some(-32002),
        "config set error code: {resp}"
    );
    assert!(
        !tasks_dir.join("config.yml").exists(),
        "rejected MCP set must not write"
    );
}

// ---------------------------------------------------------------------------
// Review fix: malformed raw values cannot normalize into an accepted clear
// (service, REST, and MCP share the CLI's invalid-value contract)
// ---------------------------------------------------------------------------

/// Global states [Todo, Done]; project sends "Todo,,Done". The dedup
/// comparison must not run before raw-value validation turns the empty CSV
/// entry into a rejection — the previous behavior cleared the override (200).
fn seed_global_states(tasks_dir: &Path) {
    let resolver = resolver_for(tasks_dir);
    ConfigService::set(
        &resolver,
        &map(&[("issue_states", "Todo,Done")]),
        true,
        None,
    )
    .expect("seed global states");
}

#[test]
fn dev71_service_rejects_malformed_csv_before_dedup_clear() {
    let tmp = TempDir::new().unwrap();
    let tasks_dir = ensure_tasks_dir(tmp.path());
    seed_global_states(&tasks_dir);
    let resolver = resolver_for(&tasks_dir);

    // A project override must exist so a bogus "clear" would be observable.
    ConfigService::set(
        &resolver,
        &map(&[("issue_states", "Todo,InProgress,Done")]),
        false,
        Some("ENG"),
    )
    .expect("seed project override");
    let before = fs::read(tasks_dir.join("ENG/config.yml")).unwrap();

    let err = ConfigService::set(
        &resolver,
        &map(&[("issue_states", "Todo,,Done")]),
        false,
        Some("ENG"),
    )
    .expect_err("malformed CSV must be rejected before the dedup conversion");
    let message = err.to_string();
    assert!(
        message.contains("entries cannot be empty") || message.contains("Invalid value"),
        "schema error surfaced: {message}"
    );

    // No override was cleared and the bytes are untouched.
    let after = fs::read(tasks_dir.join("ENG/config.yml")).unwrap();
    assert_eq!(
        before, after,
        "rejected request must preserve project bytes"
    );
}

#[test]
fn dev71_cli_rejects_malformed_csv_same_contract() {
    let tmp = TempDir::new().unwrap();
    let tasks_dir = ensure_tasks_dir(tmp.path());
    seed_global_states(&tasks_dir);

    let (ok, output) = run_cli(
        tmp.path(),
        &[
            "--project=ENG",
            "config",
            "set",
            "issue_states",
            "Todo,,Done",
        ],
    );
    assert!(!ok, "CLI must reject the malformed CSV: {output}");
    assert!(
        !tasks_dir.join("ENG/config.yml").exists(),
        "rejected CLI set must not create or clear any override"
    );
}

#[test]
fn dev71_rest_rejects_malformed_csv_no_artifacts() {
    let tmp = TempDir::new().unwrap();
    let tasks_dir = tmp.path().join(".tasks"); // intentionally absent
    let _guard = EnvVarGuard::set("LOTAR_TASKS_DIR", &tasks_dir.to_string_lossy());

    let mut api = lotar::api_server::ApiServer::new();
    lotar::routes::initialize(&mut api);
    let resp = api.handle_request(&lotar::api_server::HttpRequest {
        method: "POST".to_string(),
        path: "/api/config/set".to_string(),
        query: Default::default(),
        headers: Default::default(),
        body: serde_json::to_vec(&serde_json::json!({
            "values": {"issue_states": "Todo,,Done"},
            "global": true
        }))
        .unwrap(),
    });
    assert_eq!(
        resp.status,
        400,
        "REST must reject malformed values: {}",
        String::from_utf8_lossy(&resp.body)
    );
    assert!(
        !tasks_dir.join("config.yml").exists(),
        "rejected REST set must not create config artifacts"
    );
}

#[test]
fn dev71_mcp_rejects_malformed_csv() {
    let tmp = TempDir::new().unwrap();
    let tasks_dir = tmp.path().join(".tasks");
    let _guard = EnvVarGuard::set("LOTAR_TASKS_DIR", &tasks_dir.to_string_lossy());

    let line = serde_json::to_string(&serde_json::json!({
        "jsonrpc": "2.0",
        "id": 9,
        "method": "tools/call",
        "params": {
            "name": "config_set",
            "arguments": {"values": {"issue_states": "Todo,,Done"}, "global": true}
        }
    }))
    .unwrap();
    let resp: serde_json::Value =
        serde_json::from_str(&lotar::mcp::server::handle_json_line(&line)).unwrap();
    assert!(
        resp.get("error").is_some(),
        "MCP must reject malformed values: {resp}"
    );
    assert!(
        !tasks_dir.join("config.yml").exists(),
        "rejected MCP set must not create config artifacts"
    );
}

#[test]
fn dev71_project_empty_clear_still_works() {
    let tmp = TempDir::new().unwrap();
    let tasks_dir = ensure_tasks_dir(tmp.path());
    let resolver = resolver_for(&tasks_dir);
    seed_global_states(&tasks_dir);

    ConfigService::set(
        &resolver,
        &map(&[("issue_states", "Todo,InProgress,Done")]),
        false,
        Some("ENG"),
    )
    .expect("seed project override");

    // Empty value remains the documented field-precise clear.
    ConfigService::set(&resolver, &map(&[("issue_states", "")]), false, Some("ENG"))
        .expect("empty clear must stay accepted");
    let cfg = lotar::config::persistence::load_project_config_from_dir("ENG", &tasks_dir).unwrap();
    assert!(cfg.issue_states.is_none(), "override cleared: {cfg:?}");
}

// ---------------------------------------------------------------------------
// Review follow-ups: force scope, malformed project config during global
// changes, and runtime-precedence masking
// ---------------------------------------------------------------------------

#[test]
fn dev71_cli_force_cannot_bypass_resolved_invariant() {
    let tmp = TempDir::new().unwrap();
    let tasks_dir = ensure_tasks_dir(tmp.path());
    let resolver = resolver_for(&tasks_dir);
    ConfigService::set(
        &resolver,
        &map(&[("branch_status_aliases", "{wip: InProgress}")]),
        true,
        None,
    )
    .expect("seed global alias");

    let (ok, output) = run_cli(
        tmp.path(),
        &[
            "--project=ENG",
            "config",
            "set",
            "issue_states",
            "Todo,Done",
            "--force",
        ],
    );
    assert!(
        !ok,
        "force must not bypass resolved-configuration invariants: {output}"
    );
    assert!(
        !tasks_dir.join("ENG/config.yml").exists(),
        "blocked force run must not write"
    );
}

#[test]
fn dev71_cli_global_change_malformed_project_config_bytes_preserved() {
    let tmp = TempDir::new().unwrap();
    let tasks_dir = ensure_tasks_dir(tmp.path());
    let broken = "project:\n  name: [broken {{\n# keep my bytes\n";
    fs::create_dir_all(tasks_dir.join("MALF")).unwrap();
    fs::write(tasks_dir.join("MALF/config.yml"), broken).unwrap();

    let (ok, output) = run_cli(
        tmp.path(),
        &["config", "set", "issue_states", "Todo,Done", "--global"],
    );
    assert!(
        !ok,
        "malformed affected project config must fail closed: {output}"
    );
    assert!(
        output.contains("MALF"),
        "error should identify the project: {output}"
    );
    let after = fs::read_to_string(tasks_dir.join("MALF/config.yml")).unwrap();
    assert_eq!(after, broken, "malformed project config bytes preserved");
    assert!(
        !tasks_dir.join("config.yml").exists(),
        "blocked global change must not write"
    );
}

#[test]
fn dev71_cli_env_masked_field_skips_task_conflicts() {
    // Runtime precedence: LOTAR_ISSUE_STATES masks the global candidate for
    // every project, so shrinking the stored global list cannot newly
    // invalidate tasks under this invocation. Isolated entirely through the
    // subprocess environment; the test process never mutates env globals.
    let tmp = TempDir::new().unwrap();
    let tasks_dir = ensure_tasks_dir(tmp.path());
    seed_task(&tasks_dir, "ENG", 1, "InProgress");

    let mut cmd = cargo_bin_silent();
    cmd.env("LOTAR_ISSUE_STATES", "Todo,InProgress,Done");
    let assert = cmd
        .current_dir(tmp.path())
        .args(["config", "set", "issue_states", "Todo,Done", "--global"])
        .assert();
    let output = assert.get_output();
    assert!(
        output.status.success(),
        "env-masked candidate must not conflict with tasks: {}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let config = fs::read_to_string(tasks_dir.join("config.yml")).unwrap();
    assert!(config.contains("Done"), "global write landed: {config}");
}

//! DEV-102 regression coverage: automation config no-op mutations.
//!
//! `AutomationService::set` is a full-file replace: the incoming YAML is
//! parsed and validated, the CURRENT file must parse before comparison,
//! and an effective-equal submission (same complete ordered rules and
//! `max_iterations`, allowing parse-normalized spellings) skips the write —
//! preserving stored bytes, mtime, comments, and unknown extensions, and
//! emitting no `config_updated` event — while reporting `updated: false`
//! through the existing response shape. Genuine replacements (changed
//! rules, reordering, `max_iterations`, or a missing file) still write and
//! emit. Invalid incoming files and unreadable current files still reject.

mod common;

use crate::common::env_mutex::EnvVarGuard;
use lotar::api_events;
use lotar::services::automation_service::AutomationService;
use std::fs;
use std::path::Path;
use std::time::{Duration, SystemTime};
use tempfile::TempDir;

fn ensure_tasks_dir(root: &Path) -> std::path::PathBuf {
    let tasks_dir = root.join(".tasks");
    fs::create_dir_all(&tasks_dir).unwrap();
    tasks_dir
}

/// Bytes and mtime of a stored file, for no-op preservation proofs.
fn file_state(path: &Path) -> (Vec<u8>, SystemTime) {
    let bytes = fs::read(path).unwrap();
    let mtime = fs::metadata(path).unwrap().modified().unwrap();
    (bytes, mtime)
}

const RULES_YAML: &str = "automation:\n  max_iterations: 3\n  rules:\n    - name: done-notify\n      cooldown: 60s\n      when:\n        changes:\n          status:\n            to: Done\n      on:\n        updated:\n          comment: \"Nice work\"\n";

// ---------------------------------------------------------------------------
// Service layer (backs the REST route)
// ---------------------------------------------------------------------------

#[test]
fn dev102_automation_same_yaml_global_noop() {
    let tmp = TempDir::new().unwrap();
    let tasks_dir = ensure_tasks_dir(tmp.path());
    let path = tasks_dir.join("automation.yml");

    let seed =
        AutomationService::set(&tasks_dir, None, RULES_YAML).expect("seed global automation file");
    assert!(seed.updated, "creating the missing file writes");
    let before = file_state(&path);

    let noop = AutomationService::set(&tasks_dir, None, RULES_YAML)
        .expect("same-yaml submission must stay accepted");
    assert!(!noop.updated, "same complete rules report updated:false");
    let after = file_state(&path);
    assert_eq!(before.0, after.0, "no-op preserves automation bytes");
    assert_eq!(
        before.1, after.1,
        "no-op preserves the automation file mtime (no rewrite)"
    );
}

#[test]
fn dev102_automation_same_yaml_project_noop() {
    let tmp = TempDir::new().unwrap();
    let tasks_dir = ensure_tasks_dir(tmp.path());
    let path = tasks_dir.join("ENG/automation.yml");

    AutomationService::set(&tasks_dir, Some("ENG"), RULES_YAML)
        .expect("seed project automation file");
    let before = file_state(&path);

    let noop = AutomationService::set(&tasks_dir, Some("ENG"), RULES_YAML)
        .expect("same-yaml project submission must stay accepted");
    assert!(!noop.updated, "same project rules report updated:false");
    let after = file_state(&path);
    assert_eq!(before.0, after.0, "project no-op preserves bytes");
    assert_eq!(before.1, after.1, "project no-op preserves mtime");
}

#[test]
fn dev102_automation_equivalent_allowed_syntax_is_noop_and_preserves_raw() {
    let tmp = TempDir::new().unwrap();
    let tasks_dir = ensure_tasks_dir(tmp.path());
    let path = tasks_dir.join("automation.yml");

    // Hand-written stored file using legacy spellings: the `triggers`
    // alias and a user comment. Parse normalization maps the alias onto
    // the same rule structure as the canonical `when` spelling.
    let raw_stored = "# my team's automation, do not reformat\nautomation:\n  rules:\n    - name: done-notify\n      cooldown: 60s\n      triggers:\n        changes:\n          status:\n            to: Done\n      on:\n        updated:\n          comment: \"Nice work\"\n";
    fs::write(&path, raw_stored).unwrap();
    let before = file_state(&path);

    // Canonical-form submission of the SAME rules (no max_iterations on
    // either side): equal after parsing.
    let canonical = "automation:\n  rules:\n    - name: done-notify\n      cooldown: 60s\n      when:\n        changes:\n          status:\n            to: Done\n      on:\n        updated:\n          comment: \"Nice work\"\n";
    let noop = AutomationService::set(&tasks_dir, None, canonical)
        .expect("equivalent allowed syntax must stay accepted");
    assert!(
        !noop.updated,
        "equivalent syntax (triggers alias) is a no-op"
    );
    let after = file_state(&path);
    assert_eq!(
        before.0, after.0,
        "no-op keeps the raw file (comment and alias spelling preserved)"
    );
    assert_eq!(before.1, after.1, "no-op keeps the raw mtime");

    // The bare rule-list form carries no max_iterations and normalizes to
    // the same projection: also a no-op over the stored map form.
    let bare_list = "automation:\n  - name: done-notify\n    cooldown: 60s\n    when:\n      changes:\n        status:\n          to: Done\n    on:\n      updated:\n        comment: \"Nice work\"\n";
    let noop = AutomationService::set(&tasks_dir, None, bare_list)
        .expect("bare-list equivalent syntax must stay accepted");
    assert!(!noop.updated, "bare-list vs rules-map spelling is a no-op");
    assert_eq!(
        before.0,
        fs::read(&path).unwrap(),
        "still the original raw bytes"
    );
}

#[test]
fn dev102_automation_condition_and_tag_shorthands_preserve_raw_file() {
    let tmp = TempDir::new().unwrap();
    let tasks_dir = ensure_tasks_dir(tmp.path());
    let path = tasks_dir.join("automation.yml");
    let raw = "# preserve condition and tag shorthands\nautomation:\n  rules:\n    - when:\n        all:\n          - status: Todo\n          - priority: [High, Critical]\n        custom_fields:\n          team: Engineering\n        changes:\n          status:\n            from: Todo\n            to: [Done, Closed]\n      on:\n        updated:\n          add:\n            tags: notify\n          set:\n            labels: reviewed\n";
    fs::write(&path, raw).unwrap();
    let before = file_state(&path);
    let expanded = "automation:\n  rules:\n    - when:\n        all:\n          - status: {equals: Todo}\n          - priority: {any: [High, Critical]}\n        custom_fields:\n          team: {equals: Engineering}\n        changes:\n          status:\n            from: {equals: Todo}\n            to: {any: [Done, Closed]}\n      on:\n        updated:\n          add:\n            tags: [notify]\n          set:\n            labels: [reviewed]\n";
    let noop = AutomationService::set(&tasks_dir, None, expanded).unwrap();
    assert!(
        !noop.updated,
        "shorthands and expanded predicates are equivalent"
    );
    assert_eq!(before, file_state(&path), "preserve raw bytes and mtime");

    let changed = expanded.replace("equals: Engineering", "equals: Design");
    assert!(
        AutomationService::set(&tasks_dir, None, &changed)
            .unwrap()
            .updated
    );
    assert_ne!(before.0, fs::read(&path).unwrap());
    assert!(
        !AutomationService::set(&tasks_dir, None, &changed)
            .unwrap()
            .updated
    );
}

#[test]
fn dev102_automation_assignee_placeholder_forms_are_genuine_changes() {
    let tmp = TempDir::new().unwrap();
    let tasks_dir = ensure_tasks_dir(tmp.path());
    let path = tasks_dir.join("automation.yml");
    for token in ["@any", "@agent", "@user", " @ANY "] {
        let scalar = format!(
            "automation:\n  rules:\n    - when:\n        assignee: '{token}'\n      on:\n        updated:\n          comment: matched\n"
        );
        let mapped = scalar.replace(
            &format!("assignee: '{token}'"),
            &format!("assignee: {{equals: '{token}'}}"),
        );
        AutomationService::set(&tasks_dir, None, &scalar).unwrap();
        let scalar_bytes = fs::read(&path).unwrap();
        assert!(
            AutomationService::set(&tasks_dir, None, &mapped)
                .unwrap()
                .updated,
            "scalar placeholder to literal map must write: {token}"
        );
        assert_ne!(scalar_bytes, fs::read(&path).unwrap());
        assert!(
            !AutomationService::set(&tasks_dir, None, &mapped)
                .unwrap()
                .updated
        );
        assert!(
            AutomationService::set(&tasks_dir, None, &scalar)
                .unwrap()
                .updated,
            "literal map to scalar placeholder must write: {token}"
        );
        assert!(
            !AutomationService::set(&tasks_dir, None, &scalar)
                .unwrap()
                .updated
        );
    }
}

#[test]
fn dev102_automation_changed_rules_maxiterations_and_order_write() {
    let tmp = TempDir::new().unwrap();
    let tasks_dir = ensure_tasks_dir(tmp.path());
    let path = tasks_dir.join("automation.yml");

    AutomationService::set(&tasks_dir, None, RULES_YAML).expect("seed");
    let before = file_state(&path);

    // Changed rule content (comment text) is a genuine replacement.
    let changed_comment = RULES_YAML.replace("Nice work", "Great job");
    let real = AutomationService::set(&tasks_dir, None, &changed_comment)
        .expect("changed comment text must apply");
    assert!(real.updated, "changed rule action writes");
    assert_ne!(before.0, fs::read(&path).unwrap());

    // Changed max_iterations is a genuine replacement.
    let changed_iterations = RULES_YAML.replace("max_iterations: 3", "max_iterations: 5");
    let real = AutomationService::set(&tasks_dir, None, &changed_iterations)
        .expect("changed max_iterations must apply");
    assert!(real.updated, "changed max_iterations writes");

    // Removed max_iterations (None) is a genuine replacement too.
    let no_iterations = "automation:\n  rules:\n    - name: done-notify\n      cooldown: 60s\n      when:\n        changes:\n          status:\n            to: Done\n      on:\n        updated:\n          comment: \"Great job\"\n";
    let real = AutomationService::set(&tasks_dir, None, no_iterations)
        .expect("removing max_iterations must apply");
    assert!(real.updated, "removing max_iterations writes");

    // Adding a second rule and reordering both are genuine replacements.
    let two_rules = "automation:\n  max_iterations: 3\n  rules:\n    - name: a-first\n      when:\n        changes:\n          status:\n            to: Done\n      on:\n        updated:\n          comment: \"one\"\n    - name: done-notify\n      cooldown: 60s\n      when:\n        changes:\n          status:\n            to: Done\n      on:\n        updated:\n          comment: \"Great job\"\n";
    let real = AutomationService::set(&tasks_dir, None, two_rules).expect("added rule must apply");
    assert!(real.updated, "added rule writes");
    let reordered = "automation:\n  max_iterations: 3\n  rules:\n    - name: done-notify\n      cooldown: 60s\n      when:\n        changes:\n          status:\n            to: Done\n      on:\n        updated:\n          comment: \"Great job\"\n    - name: a-first\n      when:\n        changes:\n          status:\n            to: Done\n      on:\n        updated:\n          comment: \"one\"\n";
    let real =
        AutomationService::set(&tasks_dir, None, reordered).expect("reordered rules must apply");
    assert!(
        real.updated,
        "rule reordering is a genuine change (order matters)"
    );

    // The reordered file is now current: re-sending it is the no-op again.
    let noop = AutomationService::set(&tasks_dir, None, reordered)
        .expect("same reordered file must stay accepted");
    assert!(!noop.updated, "identical re-send is a no-op");
}

#[test]
fn dev102_automation_invalid_incoming_still_rejected() {
    let tmp = TempDir::new().unwrap();
    let tasks_dir = ensure_tasks_dir(tmp.path());
    let path = tasks_dir.join("automation.yml");

    AutomationService::set(&tasks_dir, None, RULES_YAML).expect("seed");
    let before = file_state(&path);

    // Malformed YAML rejects before anything else.
    let err = match AutomationService::set(&tasks_dir, None, "automation: [{{{{") {
        Err(err) => err,
        Ok(_) => panic!("malformed incoming YAML must reject"),
    };
    assert!(
        err.to_string().contains("Serialization error"),
        "parse error surfaced: {err}"
    );

    // Schema-valid YAML with an invalid cooldown rejects via validation.
    let bad_cooldown = RULES_YAML.replace("cooldown: 60s", "cooldown: soon");
    let err = match AutomationService::set(&tasks_dir, None, &bad_cooldown) {
        Err(err) => err,
        Ok(_) => panic!("invalid rule content must reject"),
    };
    assert!(
        err.to_string().contains("Cooldown"),
        "cooldown validation surfaced: {err}"
    );

    let after = file_state(&path);
    assert_eq!(before.0, after.0, "rejected submissions preserve bytes");
    assert_eq!(before.1, after.1, "rejected submissions preserve mtime");
}

#[test]
fn dev102_automation_malformed_current_file_blocks_set() {
    let tmp = TempDir::new().unwrap();
    let tasks_dir = ensure_tasks_dir(tmp.path());
    let path = tasks_dir.join("automation.yml");

    let broken = "automation: [not a valid\n  rule: {{{\n# precious comment\n";
    fs::write(&path, broken).unwrap();

    // The incoming file is valid, but the CURRENT file must parse before
    // equality/no-op or replacement is decided: fail closed, never replace.
    let err = match AutomationService::set(&tasks_dir, None, RULES_YAML) {
        Err(err) => err,
        Ok(_) => panic!("malformed current automation file must block the set"),
    };
    assert!(
        err.to_string().contains("current global automation"),
        "error names the unreadable current file: {err}"
    );
    let after = fs::read_to_string(&path).unwrap();
    assert_eq!(after, broken, "blocked set preserves the raw bytes");
}

// ---------------------------------------------------------------------------
// REST route: existing response shape, event only on genuine writes
// ---------------------------------------------------------------------------

fn rest_set(yaml: &str, project: Option<&str>) -> (u16, serde_json::Value) {
    let mut api = lotar::api_server::ApiServer::new();
    lotar::routes::initialize(&mut api);
    let mut body = serde_json::json!({"yaml": yaml, "global": project.is_none()});
    if let Some(project) = project {
        body["project"] = serde_json::json!(project);
    }
    let resp = api.handle_request(&lotar::api_server::HttpRequest {
        method: "POST".to_string(),
        path: "/api/automation/set".to_string(),
        query: Default::default(),
        headers: Default::default(),
        body: serde_json::to_vec(&body).unwrap(),
    });
    let parsed = serde_json::from_slice(&resp.body).unwrap_or_default();
    (resp.status, parsed)
}

fn next_config_event(rx: &std::sync::mpsc::Receiver<api_events::ApiEvent>) -> Option<String> {
    rx.recv_timeout(Duration::from_millis(500))
        .ok()
        .map(|event| event.kind)
}

#[test]
fn dev102_automation_rest_noop_no_event_real_change_emits() {
    let tmp = TempDir::new().unwrap();
    let tasks_dir = ensure_tasks_dir(tmp.path());
    let _guard = EnvVarGuard::set("LOTAR_TASKS_DIR", &tasks_dir.to_string_lossy());
    let path = tasks_dir.join("automation.yml");

    AutomationService::set(&tasks_dir, None, RULES_YAML).expect("seed via service");
    let before = file_state(&path);

    let rx = api_events::subscribe();
    let (status, body) = rest_set(RULES_YAML, None);
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
    let after = file_state(&path);
    assert_eq!(before.0, after.0, "REST no-op preserves bytes");
    assert_eq!(before.1, after.1, "REST no-op preserves mtime");

    // Control: a genuine replacement emits exactly one config_updated.
    let (status, body) = rest_set(
        &RULES_YAML.replace("max_iterations: 3", "max_iterations: 4"),
        None,
    );
    assert_eq!(status, 200);
    assert_eq!(
        body.get("data").and_then(|d| d.get("updated")),
        Some(&serde_json::json!(true))
    );
    assert_eq!(
        next_config_event(&rx).as_deref(),
        Some("config_updated"),
        "real replacement must emit config_updated"
    );
}

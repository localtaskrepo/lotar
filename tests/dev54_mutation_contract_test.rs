use lotar::api_server::{ApiServer, HttpRequest};
use lotar::routes;
use serde_json::{Value, json};
use std::collections::HashMap;
use std::path::PathBuf;
mod common;
use crate::common::env_mutex::EnvVarGuard;

fn mk_req(method: &str, path: &str, query: &[(&str, &str)], body: Value) -> HttpRequest {
    let mut q = HashMap::new();
    for (k, v) in query {
        q.insert((*k).to_string(), (*v).to_string());
    }
    HttpRequest {
        method: method.to_string(),
        path: path.to_string(),
        query: q,
        headers: HashMap::new(),
        body: serde_json::to_vec(&body).unwrap(),
    }
}

struct Dev54Fixture {
    _tmp: tempfile::TempDir,
    tasks_dir: PathBuf,
    _guard_tasks: EnvVarGuard,
    _guard_fast: EnvVarGuard,
}

fn isolated_workspace() -> Dev54Fixture {
    let _guard_fast = EnvVarGuard::set("LOTAR_TEST_FAST_IO", "1");
    let tmp = tempfile::tempdir().unwrap();
    let tasks_dir = tmp.path().join(".tasks");
    std::fs::create_dir_all(&tasks_dir).unwrap();
    let _guard_tasks = EnvVarGuard::set("LOTAR_TASKS_DIR", &tasks_dir.to_string_lossy());
    Dev54Fixture {
        _tmp: tmp,
        tasks_dir,
        _guard_tasks,
        _guard_fast,
    }
}

fn seed_project_config(tasks_dir: &std::path::Path, project: &str, yaml: &str) {
    let dir = tasks_dir.join(project);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("config.yml"), yaml).unwrap();
}

fn count_task_files(tasks_dir: &std::path::Path, project: &str) -> usize {
    let dir = tasks_dir.join(project);
    let Ok(entries) = std::fs::read_dir(&dir) else {
        return 0;
    };
    entries
        .filter_map(|e| e.ok())
        .filter(|e| {
            let name = e.file_name().to_string_lossy().to_string();
            name.ends_with(".yml") && name != "config.yml"
        })
        .count()
}

fn read_task_yaml(tasks_dir: &std::path::Path, id: &str) -> String {
    let project = id.split('-').next().unwrap();
    let numeric = id.split('-').nth(1).unwrap();
    std::fs::read_to_string(tasks_dir.join(project).join(format!("{numeric}.yml")))
        .unwrap_or_else(|_| panic!("task file for {id} should exist"))
}

fn server() -> ApiServer {
    let mut api = ApiServer::new();
    routes::initialize(&mut api);
    api
}

fn body_of(resp: &lotar::api_server::HttpResponse) -> Value {
    serde_json::from_slice(&resp.body).unwrap()
}

const ALPHA_STATES: &str = "issue_states: [Queued, Active, Review, Complete]\n";
const BETA_STATES: &str = "issue_states: [Todo, InProgress, Done]\n";

#[test]
fn rest_create_uses_project_scoped_enum_validation() {
    let fx = isolated_workspace();
    seed_project_config(&fx.tasks_dir, "ALPHA", ALPHA_STATES);
    seed_project_config(&fx.tasks_dir, "BETA", BETA_STATES);
    let api = server();

    // Explicit project-only status is accepted and persisted atomically.
    let resp = api.handle_request(&mk_req(
        "POST",
        "/api/tasks/add",
        &[],
        json!({"title": "Alpha review", "project": "ALPHA", "status": "Review"}),
    ));
    assert_eq!(resp.status, 201, "create with project status");
    let created = body_of(&resp);
    let id = created["data"]["id"].as_str().unwrap().to_string();
    assert_eq!(created["data"]["status"], "Review");
    assert!(read_task_yaml(&fx.tasks_dir, &id).contains("status: Review"));

    // A status that exists globally but not for ALPHA must be rejected.
    let resp = api.handle_request(&mk_req(
        "POST",
        "/api/tasks/add",
        &[],
        json!({"title": "Alpha todo", "project": "ALPHA", "status": "Todo"}),
    ));
    assert_eq!(resp.status, 400, "global-only status rejected for ALPHA");
    assert!(
        body_of(&resp)["error"]["message"]
            .as_str()
            .unwrap()
            .contains("Queued"),
        "error should list project statuses"
    );
    assert_eq!(count_task_files(&fx.tasks_dir, "ALPHA"), 1);
}

#[test]
fn rest_update_honors_status_with_project_scoped_validation() {
    let fx = isolated_workspace();
    seed_project_config(&fx.tasks_dir, "ALPHA", ALPHA_STATES);
    seed_project_config(&fx.tasks_dir, "BETA", BETA_STATES);
    let api = server();

    let alpha = body_of(&api.handle_request(&mk_req(
        "POST",
        "/api/tasks/add",
        &[],
        json!({"title": "A", "project": "ALPHA", "status": "Queued"}),
    )))["data"]["id"]
        .as_str()
        .unwrap()
        .to_string();
    let beta = body_of(&api.handle_request(&mk_req(
        "POST",
        "/api/tasks/add",
        &[],
        json!({"title": "B", "project": "BETA"}),
    )))["data"]["id"]
        .as_str()
        .unwrap()
        .to_string();

    // /api/tasks/update now applies status changes with project validation.
    let resp = api.handle_request(&mk_req(
        "POST",
        "/api/tasks/update",
        &[],
        json!({"id": alpha, "status": "Complete"}),
    ));
    assert_eq!(resp.status, 200, "update alpha to Complete");
    assert_eq!(body_of(&resp)["data"]["status"], "Complete");

    let resp = api.handle_request(&mk_req(
        "POST",
        "/api/tasks/update",
        &[],
        json!({"id": alpha, "status": "InProgress"}),
    ));
    assert_eq!(resp.status, 400, "BETA-only status rejected on ALPHA");

    let resp = api.handle_request(&mk_req(
        "POST",
        "/api/tasks/update",
        &[],
        json!({"id": beta, "status": "InProgress"}),
    ));
    assert_eq!(resp.status, 200, "update beta to InProgress");
    assert_eq!(body_of(&resp)["data"]["status"], "InProgress");

    let resp = api.handle_request(&mk_req(
        "POST",
        "/api/tasks/update",
        &[],
        json!({"id": beta, "status": "Review"}),
    ));
    assert_eq!(resp.status, 400, "ALPHA-only status rejected on BETA");

    // /api/tasks/status shares the same project-aware validation.
    let resp = api.handle_request(&mk_req(
        "POST",
        "/api/tasks/status",
        &[],
        json!({"id": alpha, "status": "Active"}),
    ));
    assert_eq!(resp.status, 200, "status route alpha to Active");
    assert_eq!(body_of(&resp)["data"]["status"], "Active");
}

#[test]
fn rest_update_nullable_and_empty_clears() {
    let _fx = isolated_workspace();
    let api = server();

    let created = body_of(&api.handle_request(&mk_req(
        "POST",
        "/api/tasks/add",
        &[],
        json!({
            "title": "Clearable",
            "project": "CLRA",
            "reporter": "alice",
            "assignee": "bob",
            "due_date": "2026-12-31",
            "effort": "3d",
            "description": "to be cleared",
            "tags": ["one", "two"],
            "custom_fields": {"team": "infra"},
            "acceptance_criteria": ["works", "tested"]
        }),
    )));
    assert_eq!(created["data"]["custom_fields"]["team"], "infra");
    assert_eq!(
        created["data"]["acceptance_criteria"]
            .as_array()
            .unwrap()
            .len(),
        2
    );
    let id = created["data"]["id"].as_str().unwrap().to_string();

    // Omitted fields stay unchanged.
    let resp = api.handle_request(&mk_req(
        "POST",
        "/api/tasks/update",
        &[],
        json!({"id": id, "title": "Clearable renamed"}),
    ));
    assert_eq!(resp.status, 200);
    let data = body_of(&resp)["data"].clone();
    assert_eq!(data["assignee"], "bob");
    assert_eq!(data["due_date"], "2026-12-31");
    assert_eq!(data["description"], "to be cleared");

    // null clears every clearable field.
    let resp = api.handle_request(&mk_req(
        "POST",
        "/api/tasks/update",
        &[],
        json!({
            "id": id,
            "reporter": null,
            "assignee": null,
            "due_date": null,
            "effort": null,
            "description": null,
            "tags": null,
            "custom_fields": null,
            "acceptance_criteria": null
        }),
    ));
    assert_eq!(resp.status, 200, "null clears");
    let data = body_of(&resp)["data"].clone();
    assert!(data["reporter"].is_null());
    assert!(data["assignee"].is_null());
    assert!(data["due_date"].is_null());
    assert!(data["effort"].is_null());
    assert!(data["description"].is_null());
    assert!(data.get("tags").is_none() || data["tags"].as_array().unwrap().is_empty());
    assert!(
        data.get("custom_fields").is_none()
            || data["custom_fields"].as_object().unwrap().is_empty()
    );
    assert!(
        data.get("acceptance_criteria").is_none()
            || data["acceptance_criteria"].as_array().unwrap().is_empty()
    );

    // Historical empty-string clears remain supported.
    let resp = api.handle_request(&mk_req(
        "POST",
        "/api/tasks/update",
        &[],
        json!({"id": id, "assignee": "", "effort": ""}),
    ));
    assert_eq!(resp.status, 200);
    assert!(body_of(&resp)["data"]["assignee"].is_null());
    assert!(body_of(&resp)["data"]["effort"].is_null());

    // GET agrees with the cleared state.
    let resp = api.handle_request(&mk_req(
        "GET",
        "/api/tasks/get",
        &[("id", id.as_str())],
        json!({}),
    ));
    assert_eq!(resp.status, 200);
    assert!(body_of(&resp)["data"]["assignee"].is_null());
}

#[test]
fn rest_custom_fields_and_acceptance_criteria_roundtrip() {
    let fx = isolated_workspace();
    let api = server();

    let created = body_of(&api.handle_request(&mk_req(
        "POST",
        "/api/tasks/add",
        &[],
        json!({
            "title": "Roundtrip",
            "project": "RTRP",
            "custom_fields": {"severity": "high", "points": 5},
            "acceptance_criteria": ["given", "when", "then"]
        }),
    )));
    let id = created["data"]["id"].as_str().unwrap().to_string();
    assert_eq!(created["data"]["custom_fields"]["severity"], "high");
    assert_eq!(created["data"]["acceptance_criteria"][2], "then");

    let yaml = read_task_yaml(&fx.tasks_dir, &id);
    assert!(yaml.contains("acceptance_criteria:"));
    assert!(yaml.contains("severity"));

    // Replace semantics.
    let resp = api.handle_request(&mk_req(
        "POST",
        "/api/tasks/update",
        &[],
        json!({
            "id": id,
            "custom_fields": {"severity": "low"},
            "acceptance_criteria": ["single criterion"]
        }),
    ));
    assert_eq!(resp.status, 200);
    let data = body_of(&resp)["data"].clone();
    let fields = data["custom_fields"].as_object().unwrap();
    assert_eq!(fields.len(), 1);
    assert_eq!(fields["severity"], "low");
    assert_eq!(
        data["acceptance_criteria"].as_array().unwrap().len(),
        1,
        "criteria replace the whole list"
    );

    let fetched = body_of(&api.handle_request(&mk_req(
        "GET",
        "/api/tasks/get",
        &[("id", id.as_str())],
        json!({}),
    )));
    assert_eq!(fetched["data"]["custom_fields"]["severity"], "low");
    assert_eq!(
        fetched["data"]["acceptance_criteria"][0],
        "single criterion"
    );

    // Legacy fields kv payloads still work on update.
    let resp = api.handle_request(&mk_req(
        "POST",
        "/api/tasks/update",
        &[],
        json!({"id": id, "fields": {"severity": "medium"}}),
    ));
    assert_eq!(resp.status, 200);
    assert_eq!(
        body_of(&resp)["data"]["custom_fields"]["severity"],
        "medium"
    );
}

#[test]
fn rest_invalid_create_leaves_no_task_files() {
    let fx = isolated_workspace();
    seed_project_config(&fx.tasks_dir, "ORPH", "issue_states: [Todo, Done]\n");
    let api = server();

    let resp = api.handle_request(&mk_req(
        "POST",
        "/api/tasks/add",
        &[],
        json!({"title": "bad status", "project": "ORPH", "status": "NotAStatus"}),
    ));
    assert_eq!(resp.status, 400);
    assert_eq!(count_task_files(&fx.tasks_dir, "ORPH"), 0);

    let resp = api.handle_request(&mk_req(
        "POST",
        "/api/tasks/add",
        &[],
        json!({"title": "bad priority", "project": "ORPH", "priority": "Ultra"}),
    ));
    assert_eq!(resp.status, 400);
    assert_eq!(count_task_files(&fx.tasks_dir, "ORPH"), 0);

    // Invalid sprint reference is rejected before the task file is written.
    let resp = api.handle_request(&mk_req(
        "POST",
        "/api/tasks/add",
        &[],
        json!({"title": "bad sprint", "project": "ORPH", "sprints": [424242]}),
    ));
    assert_eq!(resp.status, 400);
    assert_eq!(count_task_files(&fx.tasks_dir, "ORPH"), 0);
}

#[test]
fn cli_add_status_flag_is_project_validated() {
    use crate::common::lotar_cmd;
    let fx = isolated_workspace();
    seed_project_config(
        &fx.tasks_dir,
        "CLIA",
        "issue_states: [Queued, Active, Review, Complete]\n",
    );

    let ok = lotar_cmd()
        .unwrap()
        .env("LOTAR_TEST_SILENT", "1")
        .args([
            "--tasks-dir",
            fx.tasks_dir.to_str().unwrap(),
            "add",
            "CLI atomic status",
            "-p",
            "CLIA",
            "--status",
            "Review",
        ])
        .output()
        .unwrap();
    assert!(
        ok.status.success(),
        "add --status failed: {}",
        String::from_utf8_lossy(&ok.stderr)
    );
    let stdout = String::from_utf8_lossy(&ok.stdout).to_string();
    let id = common::extract_task_id_from_output(&stdout).expect("task id in output");
    assert!(id.starts_with("CLIA-"), "unexpected id {id}");
    let yaml = read_task_yaml(&fx.tasks_dir, &id);
    assert!(
        yaml.contains("status: Review"),
        "yaml missing status: {yaml}"
    );

    let bad = lotar_cmd()
        .unwrap()
        .env("LOTAR_TEST_SILENT", "1")
        .args([
            "--tasks-dir",
            fx.tasks_dir.to_str().unwrap(),
            "add",
            "CLI bad status",
            "-p",
            "CLIA",
            "--status",
            "Todo",
        ])
        .output()
        .unwrap();
    assert!(
        !bad.status.success(),
        "out-of-project status must be rejected"
    );
    let stderr = String::from_utf8_lossy(&bad.stderr).to_string();
    assert!(stderr.contains("Valid statuses"), "stderr: {stderr}");
    assert_eq!(
        count_task_files(&fx.tasks_dir, "CLIA"),
        1,
        "rejected creation must not leave a task file"
    );
}

#[test]
fn rest_update_empty_title_rejected() {
    let _fx = isolated_workspace();
    let api = server();

    let id = body_of(&api.handle_request(&mk_req(
        "POST",
        "/api/tasks/add",
        &[],
        json!({"title": "Original", "project": "TITL"}),
    )))["data"]["id"]
        .as_str()
        .unwrap()
        .to_string();

    let resp = api.handle_request(&mk_req(
        "POST",
        "/api/tasks/update",
        &[],
        json!({"id": id, "title": ""}),
    ));
    assert_eq!(resp.status, 400, "blank title must be rejected");
    assert!(
        body_of(&resp)["error"]["message"]
            .as_str()
            .unwrap()
            .contains("Title cannot be empty")
    );

    let resp = api.handle_request(&mk_req(
        "POST",
        "/api/tasks/update",
        &[],
        json!({"id": id, "title": "   "}),
    ));
    assert_eq!(resp.status, 400, "whitespace-only title must be rejected");

    let resp = api.handle_request(&mk_req(
        "POST",
        "/api/tasks/add",
        &[],
        json!({"title": "   ", "project": "TITL"}),
    ));
    assert_eq!(resp.status, 400, "blank create title must be rejected");
    assert_eq!(count_task_files(&_fx.tasks_dir, "TITL"), 1);
}

#[test]
fn rest_explicit_tags_clear_does_not_reapply_default_tags() {
    let fx = isolated_workspace();
    seed_project_config(&fx.tasks_dir, "DEFT", "default_tags: [team-x]\n");
    let api = server();

    let created = body_of(&api.handle_request(&mk_req(
        "POST",
        "/api/tasks/add",
        &[],
        json!({"title": "Defaults", "project": "DEFT"}),
    )));
    let id = created["data"]["id"].as_str().unwrap().to_string();
    assert_eq!(
        created["data"]["tags"],
        json!(["team-x"]),
        "creation without tags applies configured defaults"
    );

    let resp = api.handle_request(&mk_req(
        "POST",
        "/api/tasks/update",
        &[],
        json!({"id": id, "tags": null}),
    ));
    assert_eq!(resp.status, 200);
    let data = body_of(&resp)["data"].clone();
    assert!(
        data.get("tags").is_none() || data["tags"].as_array().unwrap().is_empty(),
        "explicit null clear must not resurrect default_tags: {data}"
    );

    let fetched = body_of(&api.handle_request(&mk_req(
        "GET",
        "/api/tasks/get",
        &[("id", id.as_str())],
        json!({}),
    )));
    assert!(
        fetched["data"].get("tags").is_none()
            || fetched["data"]["tags"].as_array().unwrap().is_empty(),
        "reads must show the persisted clear"
    );
    let yaml = read_task_yaml(&fx.tasks_dir, &id);
    assert!(
        !yaml
            .lines()
            .any(|line| line.trim_start().starts_with("tags:")),
        "cleared tags must not be persisted: {yaml}"
    );

    let resp = api.handle_request(&mk_req(
        "POST",
        "/api/tasks/update",
        &[],
        json!({"id": id, "tags": []}),
    ));
    assert_eq!(resp.status, 200);
    let data = body_of(&resp)["data"].clone();
    assert!(
        data.get("tags").is_none() || data["tags"].as_array().unwrap().is_empty(),
        "explicit empty-list clear stays cleared: {data}"
    );

    let resp = api.handle_request(&mk_req(
        "POST",
        "/api/tasks/update",
        &[],
        json!({"id": id, "tags": ["manual"]}),
    ));
    assert_eq!(resp.status, 200);
    assert_eq!(body_of(&resp)["data"]["tags"], json!(["manual"]));
}

#[test]
fn rest_custom_fields_legacy_keys_survive_config_removal() {
    let fx = isolated_workspace();
    seed_project_config(
        &fx.tasks_dir,
        "CFGT",
        "custom:\n  fields: [severity, legacy_note]\n",
    );
    let api = server();

    let created = body_of(&api.handle_request(&mk_req(
        "POST",
        "/api/tasks/add",
        &[],
        json!({"title": "CF", "project": "CFGT", "custom_fields": {"severity": "high", "legacy_note": "keep"}}),
    )));
    let id = created["data"]["id"].as_str().unwrap().to_string();
    assert_eq!(created["data"]["custom_fields"]["legacy_note"], "keep");

    // Simulate the operator removing legacy_note from the project config.
    seed_project_config(&fx.tasks_dir, "CFGT", "custom:\n  fields: [severity]\n");

    // Replace-all echo that includes the unchanged legacy key must succeed
    // and preserve it.
    let resp = api.handle_request(&mk_req(
        "POST",
        "/api/tasks/update",
        &[],
        json!({"id": id, "custom_fields": {"severity": "low", "legacy_note": "keep"}}),
    ));
    assert_eq!(
        resp.status,
        200,
        "unchanged legacy key must survive config removal: {:?}",
        String::from_utf8_lossy(&resp.body)
    );
    let data = body_of(&resp)["data"].clone();
    assert_eq!(data["custom_fields"]["legacy_note"], "keep");
    assert_eq!(data["custom_fields"]["severity"], "low");

    // Editing the legacy value is still permitted (existing data).
    let resp = api.handle_request(&mk_req(
        "POST",
        "/api/tasks/update",
        &[],
        json!({"id": id, "custom_fields": {"severity": "low", "legacy_note": "changed"}}),
    ));
    assert_eq!(resp.status, 200);
    assert_eq!(
        body_of(&resp)["data"]["custom_fields"]["legacy_note"],
        "changed"
    );

    // Brand-new undeclared keys are rejected under the restricted config.
    let resp = api.handle_request(&mk_req(
        "POST",
        "/api/tasks/update",
        &[],
        json!({"id": id, "custom_fields": {"severity": "low", "legacy_note": "changed", "brand_new": "x"}}),
    ));
    assert_eq!(
        resp.status, 400,
        "new undeclared custom field must be rejected"
    );

    // Configured keys canonicalize: case variants fold to the configured
    // spelling instead of creating a duplicate entry.
    let resp = api.handle_request(&mk_req(
        "POST",
        "/api/tasks/update",
        &[],
        json!({"id": id, "custom_fields": {"Severity": "critical", "legacy_note": "changed"}}),
    ));
    assert_eq!(resp.status, 200);
    let fields = body_of(&resp)["data"]["custom_fields"].clone();
    assert_eq!(
        fields.as_object().unwrap().len(),
        2,
        "case variant must fold into the configured key: {fields}"
    );
    assert_eq!(fields["severity"], "critical");
}

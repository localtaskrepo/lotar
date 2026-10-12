//! DEV-102: sprint metadata no-op boundary.
//!
//! Sprint metadata updates (REST /api/sprints/update, MCP sprint_update,
//! CLI `sprint update`) apply a typed tri-state patch to a record loaded
//! fresh under the sprints lock. An effective no-op must preserve the
//! stored file byte-for-byte (including legacy spellings such as task
//! `order:` fields), its filesystem mtime, and the stored `modified`
//! stamp, while a genuine change rewrites the file canonically without
//! losing concurrent membership updates.

mod common;

use common::env_mutex::EnvVarGuard;
use lotar::api_types::TaskCreate;
use lotar::services::sprint_assignment;
use lotar::services::sprint_service::{SprintMetadataPatch, SprintService};
use lotar::services::task_service::TaskService;
use lotar::storage::manager::Storage;
use lotar::storage::sprint::{Sprint, SprintPlan};
use lotar::utils::paths;
use predicates::prelude::*;
use std::path::{Path, PathBuf};
use std::sync::Barrier;

fn write_minimal_config(tasks_dir: &Path) {
    let content = r#"default.project: DEV
issue.states: [Todo, InProgress, Done]
issue.types: [Feature, Bug, Chore]
issue.priorities: [Low, Medium, High]
issue.tags: [*]
"#;
    std::fs::create_dir_all(tasks_dir).unwrap();
    std::fs::write(paths::global_config_path(tasks_dir), content).unwrap();
}

struct Workspace {
    _tmp: tempfile::TempDir,
    tasks_dir: PathBuf,
}

fn workspace() -> Workspace {
    let tmp = tempfile::tempdir().unwrap();
    let tasks_dir = tmp.path().join(".tasks");
    write_minimal_config(&tasks_dir);
    Workspace {
        _tmp: tmp,
        tasks_dir,
    }
}

fn sprint_path(tasks_dir: &Path, id: u32) -> PathBuf {
    tasks_dir.join("@sprints").join(format!("{id}.yml"))
}

fn file_state(path: &Path) -> (Vec<u8>, std::time::SystemTime) {
    let bytes = std::fs::read(path).unwrap();
    let mtime = std::fs::metadata(path).unwrap().modified().unwrap();
    (bytes, mtime)
}

fn create_task(storage: &mut Storage, title: &str) -> String {
    let task = TaskService::create(
        storage,
        TaskCreate {
            title: title.to_string(),
            project: Some("DEV".to_string()),
            priority: Some("Medium".to_string()),
            task_type: Some("Feature".to_string()),
            ..TaskCreate::default()
        },
    )
    .expect("create task");
    task.id
}

fn create_sprint_with_member(label: &str, member: &str) -> (Workspace, u32) {
    let ws = workspace();
    let mut storage = Storage::new(&ws.tasks_dir);
    let task_id = create_task(&mut storage, member);
    let created = SprintService::create(
        &mut storage,
        Sprint {
            plan: Some(SprintPlan {
                label: Some(label.to_string()),
                goal: Some("Ship it".to_string()),
                ..SprintPlan::default()
            }),
            ..Sprint::default()
        },
        None,
    )
    .expect("create sprint");
    let mut records = SprintService::list(&storage).expect("list sprints");
    sprint_assignment::assign_tasks(
        &mut storage,
        &mut records,
        std::slice::from_ref(&task_id),
        Some(&created.record.id.to_string()),
        false,
        false,
    )
    .expect("assign member");
    (ws, created.record.id)
}

fn member_ids(storage: &Storage, id: u32) -> Vec<String> {
    SprintService::get(storage, id)
        .expect("read sprint")
        .sprint
        .tasks
        .into_iter()
        .map(|entry| entry.id)
        .collect()
}

fn stored_modified(storage: &Storage, id: u32) -> String {
    SprintService::get(storage, id)
        .expect("read sprint")
        .sprint
        .modified
        .expect("modified stamped")
}

#[test]
fn no_op_patch_preserves_bytes_mtime_modified_and_members() {
    let (ws, id) = create_sprint_with_member("Alpha", "member work");
    let path = sprint_path(&ws.tasks_dir, id);
    let mut storage = Storage::new(&ws.tasks_dir);

    let (bytes_before, mtime_before) = file_state(&path);
    let modified_before = stored_modified(&storage, id);

    let patch = SprintMetadataPatch {
        label: Some(Some("Alpha".to_string())),
        goal: Some(Some("Ship it".to_string())),
        ..SprintMetadataPatch::default()
    };
    let outcome =
        SprintService::update_with_patch(&mut storage, id, &patch).expect("no-op update succeeds");

    assert!(!outcome.changed, "identical patch must not write");
    assert!(outcome.warnings.is_empty());
    assert_eq!(file_state(&path), (bytes_before, mtime_before));
    assert_eq!(stored_modified(&storage, id), modified_before);
    assert_eq!(member_ids(&storage, id).len(), 1);
}

#[test]
fn empty_patch_is_a_no_op() {
    let (ws, id) = create_sprint_with_member("Alpha", "member work");
    let path = sprint_path(&ws.tasks_dir, id);
    let mut storage = Storage::new(&ws.tasks_dir);

    let before = file_state(&path);
    let outcome =
        SprintService::update_with_patch(&mut storage, id, &SprintMetadataPatch::default())
            .expect("empty patch succeeds");
    assert!(!outcome.changed);
    assert_eq!(file_state(&path), before);
}

#[test]
fn ignored_plan_length_preserves_noop_bytes_and_reports_its_warning() {
    let ws = workspace();
    let mut storage = Storage::new(&ws.tasks_dir);
    let created = SprintService::create(
        &mut storage,
        Sprint {
            plan: Some(SprintPlan {
                label: Some("Explicit end".into()),
                ends_at: Some("2026-04-14".into()),
                ..SprintPlan::default()
            }),
            ..Sprint::default()
        },
        None,
    )
    .unwrap();
    let path = sprint_path(&ws.tasks_dir, created.record.id);
    let before = file_state(&path);
    let result = SprintService::update_with_patch(
        &mut storage,
        created.record.id,
        &SprintMetadataPatch {
            plan_length: Some(Some("2w".into())),
            ..SprintMetadataPatch::default()
        },
    )
    .unwrap();
    assert!(!result.changed);
    assert_eq!(file_state(&path), before);
    assert_eq!(result.warnings.len(), 1);
    assert_eq!(
        result.warnings[0].code(),
        "length_discarded_in_favor_of_ends_at"
    );
}

#[test]
fn no_op_patch_keeps_legacy_task_order_on_disk() {
    let ws = workspace();
    let path = sprint_path(&ws.tasks_dir, 1);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(
        &path,
        "created: '2026-01-01T00:00:00+00:00'\nmodified: '2026-01-01T00:00:00+00:00'\nplan:\n  label: Alpha\n  goal: Ship it\ntasks:\n  - id: DEV-1\n    order: 3\n  - id: DEV-2\n    order: 1\n",
    )
    .unwrap();
    let mut storage = Storage::new(&ws.tasks_dir);

    let before = file_state(&path);
    let patch = SprintMetadataPatch {
        goal: Some(Some("Ship it".to_string())),
        ..SprintMetadataPatch::default()
    };
    let outcome = SprintService::update_with_patch(&mut storage, 1, &patch)
        .expect("no-op on legacy data succeeds");

    assert!(!outcome.changed, "no-op must not canonicalize legacy bytes");
    assert_eq!(file_state(&path), before);
    let raw = std::fs::read_to_string(&path).unwrap();
    assert!(
        raw.contains("order: 3"),
        "legacy order must survive a no-op"
    );
}

#[test]
fn real_change_rewrites_updates_modified_and_keeps_members() {
    let (ws, id) = create_sprint_with_member("Alpha", "member work");
    let path = sprint_path(&ws.tasks_dir, id);
    let mut storage = Storage::new(&ws.tasks_dir);

    let (bytes_before, mtime_before) = file_state(&path);
    let modified_before = stored_modified(&storage, id);
    let members_before = member_ids(&storage, id);

    let patch = SprintMetadataPatch {
        goal: Some(Some("New horizon".to_string())),
        ..SprintMetadataPatch::default()
    };
    let outcome =
        SprintService::update_with_patch(&mut storage, id, &patch).expect("real update succeeds");

    assert!(outcome.changed);
    let (bytes_after, mtime_after) = file_state(&path);
    assert_ne!(bytes_after, bytes_before);
    assert!(mtime_after >= mtime_before);
    assert_ne!(stored_modified(&storage, id), modified_before);
    assert_eq!(member_ids(&storage, id), members_before);
    let raw = std::fs::read_to_string(&path).unwrap();
    assert!(raw.contains("New horizon"));
}

#[test]
fn real_change_on_legacy_data_writes_canonical_form() {
    let ws = workspace();
    let path = sprint_path(&ws.tasks_dir, 1);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(
        &path,
        "created: '2026-01-01T00:00:00+00:00'\nmodified: '2026-01-01T00:00:00+00:00'\nplan:\n  label: Alpha\n  goal: Ship it\ntasks:\n  - id: DEV-1\n    order: 3\n",
    )
    .unwrap();
    let mut storage = Storage::new(&ws.tasks_dir);

    let patch = SprintMetadataPatch {
        goal: Some(Some("New horizon".to_string())),
        ..SprintMetadataPatch::default()
    };
    let outcome =
        SprintService::update_with_patch(&mut storage, 1, &patch).expect("real update succeeds");

    assert!(outcome.changed);
    let raw = std::fs::read_to_string(&path).unwrap();
    assert!(raw.contains("New horizon"));
    assert!(!raw.contains("order:"), "genuine writes stay canonical");
    assert!(raw.contains("- DEV-1"));
}

#[test]
fn patch_clears_fields_and_prunes_emptied_sections() {
    let ws = workspace();
    let mut storage = Storage::new(&ws.tasks_dir);
    SprintService::create(
        &mut storage,
        Sprint {
            plan: Some(SprintPlan {
                label: Some("Alpha".to_string()),
                goal: Some("Ship it".to_string()),
                capacity: Some(lotar::storage::sprint::SprintCapacity {
                    points: Some(5),
                    hours: Some(3),
                }),
                ..SprintPlan::default()
            }),
            actual: Some(lotar::storage::sprint::SprintActual {
                started_at: Some("2026-01-02T00:00:00+00:00".to_string()),
                closed_at: Some("2026-01-03T00:00:00+00:00".to_string()),
            }),
            ..Sprint::default()
        },
        None,
    )
    .expect("create sprint");

    let patch = SprintMetadataPatch {
        label: Some(None),
        capacity_points: Some(None),
        actual_closed_at: Some(None),
        ..SprintMetadataPatch::default()
    };
    let outcome =
        SprintService::update_with_patch(&mut storage, 1, &patch).expect("clearing patch succeeds");
    assert!(outcome.changed);

    let sprint = SprintService::get(&storage, 1)
        .expect("reread sprint")
        .sprint;
    let plan = sprint.plan.expect("plan survives partial clear");
    assert_eq!(plan.label, None);
    assert_eq!(plan.goal.as_deref(), Some("Ship it"));
    let capacity = plan.capacity.expect("capacity keeps hours");
    assert_eq!(capacity.points, None);
    assert_eq!(capacity.hours, Some(3));
    let actual = sprint.actual.expect("actual keeps started_at");
    assert!(actual.started_at.is_some());
    assert_eq!(actual.closed_at, None);

    let repeat =
        SprintService::update_with_patch(&mut storage, 1, &patch).expect("repeat clear succeeds");
    assert!(!repeat.changed, "repeated clear is a no-op");
}

#[test]
fn patch_on_missing_sprint_is_not_found() {
    let ws = workspace();
    let mut storage = Storage::new(&ws.tasks_dir);
    let err = SprintService::update_with_patch(
        &mut storage,
        999,
        &SprintMetadataPatch {
            label: Some(Some("X".to_string())),
            ..SprintMetadataPatch::default()
        },
    )
    .expect_err("unknown sprint must fail");
    assert!(matches!(
        err,
        lotar::errors::LoTaRError::SprintNotFound(999)
    ));
}

#[test]
fn snapshot_update_skips_identical_snapshots() {
    let (ws, id) = create_sprint_with_member("Alpha", "member work");
    let path = sprint_path(&ws.tasks_dir, id);
    let mut storage = Storage::new(&ws.tasks_dir);

    let before = file_state(&path);
    let identical = SprintService::get(&storage, id)
        .expect("read sprint")
        .sprint;
    let outcome = SprintService::update(&mut storage, id, identical).expect("update succeeds");
    assert!(!outcome.changed, "identical snapshot must not write");
    assert_eq!(file_state(&path), before);

    let mut changed = SprintService::get(&storage, id)
        .expect("read sprint")
        .sprint;
    changed.plan.as_mut().expect("plan").goal = Some("Newer".to_string());
    let outcome = SprintService::update(&mut storage, id, changed).expect("update succeeds");
    assert!(outcome.changed);
    assert!(std::fs::read_to_string(&path).unwrap().contains("Newer"));
}

#[test]
fn metadata_update_and_assignment_do_not_lose_members_in_either_order() {
    // Order 1: membership first, then the metadata patch.
    let (ws, id) = create_sprint_with_member("Alpha", "first member");
    let mut storage = Storage::new(&ws.tasks_dir);
    let second = create_task(&mut storage, "second member");
    let mut records = SprintService::list(&storage).expect("list sprints");
    sprint_assignment::assign_tasks(
        &mut storage,
        &mut records,
        std::slice::from_ref(&second),
        Some(&id.to_string()),
        false,
        false,
    )
    .expect("assign second member");

    let patch = SprintMetadataPatch {
        label: Some(Some("Renamed".to_string())),
        ..SprintMetadataPatch::default()
    };
    SprintService::update_with_patch(&mut storage, id, &patch).expect("rename succeeds");

    let raw = std::fs::read_to_string(sprint_path(&ws.tasks_dir, id)).unwrap();
    assert!(raw.contains("Renamed"));
    assert_eq!(member_ids(&storage, id).len(), 2, "members must survive");

    // Order 2: metadata patch first, then the membership write.
    let (ws, id) = create_sprint_with_member("Beta", "first member");
    let mut storage = Storage::new(&ws.tasks_dir);
    let patch = SprintMetadataPatch {
        label: Some(Some("Renamed too".to_string())),
        ..SprintMetadataPatch::default()
    };
    SprintService::update_with_patch(&mut storage, id, &patch).expect("rename succeeds");

    let second = create_task(&mut storage, "second member");
    let mut records = SprintService::list(&storage).expect("list sprints");
    sprint_assignment::assign_tasks(
        &mut storage,
        &mut records,
        std::slice::from_ref(&second),
        Some(&id.to_string()),
        false,
        false,
    )
    .expect("assign second member");

    let raw = std::fs::read_to_string(sprint_path(&ws.tasks_dir, id)).unwrap();
    assert!(raw.contains("Renamed too"));
    assert_eq!(member_ids(&storage, id).len(), 2, "members must survive");
}

#[test]
fn concurrent_metadata_patch_and_assignment_keep_both_effects() {
    let (ws, id) = create_sprint_with_member("Alpha", "first member");
    let barrier = Barrier::new(2);
    let tasks_dir = ws.tasks_dir.clone();

    let assign_dir = tasks_dir.clone();
    let assign_id = id;
    std::thread::scope(|scope| {
        scope.spawn(|| {
            let mut storage = Storage::new(&assign_dir);
            let second = create_task(&mut storage, "racing member");
            barrier.wait();
            let mut records = SprintService::list(&storage).expect("list sprints");
            sprint_assignment::assign_tasks(
                &mut storage,
                &mut records,
                std::slice::from_ref(&second),
                Some(&assign_id.to_string()),
                false,
                false,
            )
            .expect("assignment wins the lock race");
        });
        scope.spawn(|| {
            let mut storage = Storage::new(&tasks_dir);
            let patch = SprintMetadataPatch {
                label: Some(Some("Renamed under race".to_string())),
                ..SprintMetadataPatch::default()
            };
            barrier.wait();
            SprintService::update_with_patch(&mut storage, id, &patch)
                .expect("metadata patch wins the lock race");
        });
    });

    let storage = Storage::new(&ws.tasks_dir);
    let raw = std::fs::read_to_string(sprint_path(&ws.tasks_dir, id)).unwrap();
    assert!(raw.contains("Renamed under race"));
    assert_eq!(member_ids(&storage, id).len(), 2);
}

// REST surface

struct RestFixture {
    _tmp: tempfile::TempDir,
    tasks_dir: PathBuf,
    _guard: EnvVarGuard,
}

fn rest_workspace() -> RestFixture {
    let tmp = tempfile::tempdir_in(Path::new(env!("CARGO_TARGET_TMPDIR"))).unwrap();
    let tasks_dir = tmp.path().join("ws").join(".tasks");
    std::fs::create_dir_all(&tasks_dir).unwrap();
    write_minimal_config(&tasks_dir);
    let _guard = EnvVarGuard::set("LOTAR_TASKS_DIR", &tasks_dir.to_string_lossy());
    RestFixture {
        _tmp: tmp,
        tasks_dir,
        _guard,
    }
}

fn rest_server() -> lotar::api_server::ApiServer {
    let mut api = lotar::api_server::ApiServer::new();
    lotar::routes::initialize(&mut api);
    api
}

fn rest_req(method: &str, path: &str, body: serde_json::Value) -> lotar::api_server::HttpRequest {
    lotar::api_server::HttpRequest {
        method: method.to_string(),
        path: path.to_string(),
        query: std::collections::HashMap::new(),
        headers: std::collections::HashMap::new(),
        body: serde_json::to_vec(&body).unwrap(),
    }
}

fn rest_body(resp: &lotar::api_server::HttpResponse) -> serde_json::Value {
    serde_json::from_slice(&resp.body).unwrap()
}

fn rest_create_task_and_sprint(_fixture: &RestFixture) -> (String, u32) {
    let api = rest_server();
    let resp = api.handle_request(&rest_req(
        "POST",
        "/api/tasks/add",
        serde_json::json!({"title": "rest member", "project": "DEV"}),
    ));
    assert_eq!(resp.status, 201, "task create failed: {:?}", resp.body);
    let task_id = rest_body(&resp)["data"]["id"].as_str().unwrap().to_string();

    let resp = api.handle_request(&rest_req(
        "POST",
        "/api/sprints/create",
        serde_json::json!({"label": "Alpha", "goal": "Ship it"}),
    ));
    assert_eq!(resp.status, 200, "sprint create failed: {:?}", resp.body);
    let sprint_id = rest_body(&resp)["data"]["sprint"]["id"].as_u64().unwrap() as u32;

    let resp = api.handle_request(&rest_req(
        "POST",
        "/api/sprints/add",
        serde_json::json!({"sprint": sprint_id, "tasks": [task_id]}),
    ));
    assert_eq!(resp.status, 200, "assign failed: {:?}", resp.body);
    (task_id, sprint_id)
}

#[test]
fn rest_noop_update_preserves_file_and_members() {
    let fixture = rest_workspace();
    let (task_id, sprint_id) = rest_create_task_and_sprint(&fixture);
    let path = sprint_path(&fixture.tasks_dir, sprint_id);

    let (bytes_before, mtime_before) = file_state(&path);
    let api = rest_server();
    let resp = api.handle_request(&rest_req(
        "POST",
        "/api/sprints/update",
        serde_json::json!({"sprint": sprint_id, "label": "Alpha"}),
    ));
    assert_eq!(resp.status, 200, "no-op update failed: {:?}", resp.body);
    assert_eq!(
        rest_body(&resp)["data"]["status"].as_str(),
        Some("ok"),
        "response contract preserved"
    );

    assert_eq!(file_state(&path), (bytes_before, mtime_before));
    let storage = Storage::new(&fixture.tasks_dir);
    assert_eq!(member_ids(&storage, sprint_id), vec![task_id]);
}

#[test]
fn rest_empty_update_body_is_a_noop() {
    let fixture = rest_workspace();
    let (_, sprint_id) = rest_create_task_and_sprint(&fixture);
    let path = sprint_path(&fixture.tasks_dir, sprint_id);

    let before = file_state(&path);
    let api = rest_server();
    let resp = api.handle_request(&rest_req(
        "POST",
        "/api/sprints/update",
        serde_json::json!({"sprint": sprint_id}),
    ));
    assert_eq!(resp.status, 200, "empty update failed: {:?}", resp.body);
    assert_eq!(file_state(&path), before);
}

#[test]
fn rest_real_update_changes_bytes_and_keeps_members() {
    let fixture = rest_workspace();
    let (task_id, sprint_id) = rest_create_task_and_sprint(&fixture);
    let path = sprint_path(&fixture.tasks_dir, sprint_id);

    let (bytes_before, _) = file_state(&path);
    let api = rest_server();
    let resp = api.handle_request(&rest_req(
        "POST",
        "/api/sprints/update",
        serde_json::json!({"sprint": sprint_id, "goal": "New horizon"}),
    ));
    assert_eq!(resp.status, 200, "real update failed: {:?}", resp.body);
    let body = rest_body(&resp);
    assert_eq!(body["data"]["sprint"]["goal"].as_str(), Some("New horizon"));

    let (bytes_after, _) = file_state(&path);
    assert_ne!(bytes_after, bytes_before);
    let raw = std::fs::read_to_string(&path).unwrap();
    assert!(raw.contains("New horizon"));
    let storage = Storage::new(&fixture.tasks_dir);
    assert_eq!(member_ids(&storage, sprint_id), vec![task_id]);
}

#[test]
fn rest_null_and_empty_string_clears_still_write() {
    let fixture = rest_workspace();
    let (_, sprint_id) = rest_create_task_and_sprint(&fixture);
    let path = sprint_path(&fixture.tasks_dir, sprint_id);

    let (bytes_before, _) = file_state(&path);
    let api = rest_server();
    let resp = api.handle_request(&rest_req(
        "POST",
        "/api/sprints/update",
        serde_json::json!({"sprint": sprint_id, "goal": ""}),
    ));
    assert_eq!(resp.status, 200, "text clear failed: {:?}", resp.body);
    let (bytes_after, _) = file_state(&path);
    assert_ne!(bytes_after, bytes_before);
    let raw = std::fs::read_to_string(&path).unwrap();
    assert!(!raw.contains("Ship it"), "goal cleared on disk");

    let resp = api.handle_request(&rest_req(
        "POST",
        "/api/sprints/update",
        serde_json::json!({"sprint": sprint_id, "capacity_points": null, "capacity_hours": 3}),
    ));
    assert_eq!(resp.status, 200, "null capacity failed: {:?}", resp.body);
    let raw = std::fs::read_to_string(&path).unwrap();
    assert!(raw.contains("hours: 3"), "capacity hours written");
    assert!(!raw.contains("points:"), "null cleared capacity points");
}

// MCP surface

fn mcp_call(params: serde_json::Value) -> serde_json::Value {
    let line = serde_json::json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "tools/call",
        "params": {"name": "sprint_update", "arguments": params}
    });
    let resp = lotar::mcp::server::handle_json_line(&line.to_string());
    serde_json::from_str(&resp).unwrap()
}

fn mcp_payload(resp: &serde_json::Value) -> serde_json::Value {
    let text = resp["result"]["content"][0]["text"].as_str().unwrap();
    serde_json::from_str(text).unwrap()
}

#[test]
fn mcp_noop_update_preserves_file_and_members() {
    let (ws, id) = create_sprint_with_member("Alpha", "mcp member");
    let path = sprint_path(&ws.tasks_dir, id);
    let _guard = EnvVarGuard::set("LOTAR_TASKS_DIR", &ws.tasks_dir.to_string_lossy());

    let (bytes_before, mtime_before) = file_state(&path);
    let resp = mcp_call(serde_json::json!({"sprint_id": id, "label": "Alpha"}));
    assert!(resp.get("error").is_none(), "sprint_update failed: {resp}");
    let payload = mcp_payload(&resp);
    assert_eq!(payload["status"].as_str(), Some("ok"));

    assert_eq!(file_state(&path), (bytes_before, mtime_before));
    let storage = Storage::new(&ws.tasks_dir);
    assert_eq!(member_ids(&storage, id).len(), 1);
}

#[test]
fn mcp_real_update_changes_bytes_and_keeps_members() {
    let (ws, id) = create_sprint_with_member("Alpha", "mcp member");
    let path = sprint_path(&ws.tasks_dir, id);
    let _guard = EnvVarGuard::set("LOTAR_TASKS_DIR", &ws.tasks_dir.to_string_lossy());

    let (bytes_before, _) = file_state(&path);
    let resp = mcp_call(serde_json::json!({"sprint_id": id, "goal": "New horizon"}));
    assert!(resp.get("error").is_none(), "sprint_update failed: {resp}");
    let payload = mcp_payload(&resp);
    assert_eq!(payload["sprint"]["goal"].as_str(), Some("New horizon"));

    let (bytes_after, _) = file_state(&path);
    assert_ne!(bytes_after, bytes_before);
    let storage = Storage::new(&ws.tasks_dir);
    assert_eq!(member_ids(&storage, id).len(), 1);
}

#[test]
fn mcp_unknown_sprint_is_tool_error() {
    let ws = workspace();
    let _guard = EnvVarGuard::set("LOTAR_TASKS_DIR", &ws.tasks_dir.to_string_lossy());
    let resp = mcp_call(serde_json::json!({"sprint_id": 999, "label": "X"}));
    assert!(
        resp.get("error").is_none(),
        "domain failure is a tool result: {resp}"
    );
    assert_eq!(
        resp["result"]["isError"].as_bool(),
        Some(true),
        "unknown sprint must surface as isError: {resp}"
    );
    let text = resp["result"]["content"][0]["text"].as_str().unwrap();
    assert!(
        text.contains("not found"),
        "text should name the failure: {text}"
    );
}

// CLI surface

#[test]
fn cli_noop_update_preserves_file() {
    let (ws, id) = create_sprint_with_member("Alpha", "cli member");
    let path = sprint_path(&ws.tasks_dir, id);

    let (bytes_before, mtime_before) = file_state(&path);
    let mut cmd = common::lotar_cmd().unwrap();
    cmd.args(["sprint", "update", &id.to_string(), "--label", "Alpha"])
        .current_dir(ws._tmp.path())
        .assert()
        .success();

    assert_eq!(file_state(&path), (bytes_before, mtime_before));
}

#[test]
fn cli_real_update_changes_file() {
    let (ws, id) = create_sprint_with_member("Alpha", "cli member");
    let path = sprint_path(&ws.tasks_dir, id);

    let (bytes_before, _) = file_state(&path);
    let mut cmd = common::lotar_cmd().unwrap();
    cmd.args(["sprint", "update", &id.to_string(), "--goal", "New horizon"])
        .current_dir(ws._tmp.path())
        .assert()
        .success();

    let (bytes_after, _) = file_state(&path);
    assert_ne!(bytes_after, bytes_before);
    let raw = std::fs::read_to_string(&path).unwrap();
    assert!(raw.contains("New horizon"));
}

#[test]
fn cli_update_without_mutations_is_refused() {
    let (ws, id) = create_sprint_with_member("Alpha", "cli member");
    let path = sprint_path(&ws.tasks_dir, id);
    let before = file_state(&path);

    let mut cmd = common::lotar_cmd().unwrap();
    cmd.args(["sprint", "update", &id.to_string()])
        .current_dir(ws._tmp.path())
        .assert()
        .failure()
        .stderr(predicate::str::contains("No updates provided"));

    assert_eq!(file_state(&path), before);
}

#[test]
fn cli_update_unknown_sprint_reports_not_found() {
    let ws = workspace();
    let mut cmd = common::lotar_cmd().unwrap();
    cmd.args(["sprint", "update", "999", "--label", "X"])
        .current_dir(ws._tmp.path())
        .assert()
        .failure()
        .stderr(predicate::str::contains("Sprint not found: 999"));
}

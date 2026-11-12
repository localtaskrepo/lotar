//! DEV-393: releases like sprints. Release records under `@releases/`, a
//! ticket's `releases` list, and the CLI, REST, MCP, search and automation
//! surfaces over them.

use crate::common;
use common::env_mutex::EnvVarGuard;
use lotar::api_types::{TaskCreate, TaskUpdate};
use lotar::services::release_service::{ReleaseAssignmentAction, ReleaseService};
use lotar::services::task_service::TaskService;
use lotar::storage::manager::Storage;
use lotar::storage::release::Release;
use serde_json::{Value, json};
use std::path::{Path, PathBuf};

struct Workspace {
    tmp: tempfile::TempDir,
    tasks_dir: PathBuf,
}

fn workspace() -> Workspace {
    let tmp = tempfile::tempdir_in(Path::new(env!("CARGO_TARGET_TMPDIR"))).unwrap();
    let tasks_dir = tmp.path().join(".tasks");
    std::fs::create_dir_all(&tasks_dir).unwrap();
    std::fs::write(
        tasks_dir.join("config.yml"),
        "default:\n  project: REL\nissue:\n  states: [Todo, InProgress, Done]\n  types: [Feature, Bug]\n  priorities: [Low, Medium, High]\n",
    )
    .unwrap();
    Workspace { tmp, tasks_dir }
}

fn create_task(storage: &mut Storage, title: &str) -> String {
    TaskService::create(storage, TaskCreate { title: title.into(), project: Some("REL".into()), ..TaskCreate::default() }).unwrap().id
}

fn create_release(tasks_dir: &Path, version: &str) -> u32 {
    ReleaseService::create(tasks_dir, Release { version: version.into(), ..Release::default() }).unwrap().id
}

fn cli(ws: &Workspace, args: &[&str]) -> Value {
    let mut cmd = common::lotar_cmd().unwrap();
    let output = cmd.arg("--format").arg("json").args(args).current_dir(ws.tmp.path()).output().unwrap();
    assert!(output.status.success(), "lotar {args:?} failed: {}", String::from_utf8_lossy(&output.stderr));
    serde_json::from_slice(&output.stdout).unwrap_or_else(|e| panic!("lotar {args:?} printed no JSON ({e}): {}", String::from_utf8_lossy(&output.stdout)))
}

fn task_releases(tasks_dir: &Path, id: &str) -> Vec<u32> {
    TaskService::get(&Storage::new(tasks_dir), id, None).unwrap().releases
}

#[test]
fn old_task_yaml_loads_unchanged_and_releases_round_trip() {
    let ws = workspace();
    std::fs::create_dir_all(ws.tasks_dir.join("REL")).unwrap();
    let legacy = "title: Legacy\nstatus: Todo\npriority: Medium\ntask_type: Feature\ncreated: 2026-01-01T00:00:00Z\n";
    std::fs::write(ws.tasks_dir.join("REL/1.yml"), legacy).unwrap();
    assert!(task_releases(&ws.tasks_dir, "REL-1").is_empty());

    let id = create_release(&ws.tasks_dir, "v1.2.0");
    let mut storage = Storage::new(&ws.tasks_dir);
    TaskService::update(&mut storage, "REL-1", TaskUpdate { releases: Some(vec![id]), ..TaskUpdate::default() }).unwrap();
    let yaml = std::fs::read_to_string(ws.tasks_dir.join("REL/1.yml")).unwrap();
    assert!(yaml.contains("releases:\n- 1"), "{yaml}");
    assert_eq!(task_releases(&ws.tasks_dir, "REL-1"), vec![id]);

    // Unknown release ids are refused.
    let err = TaskService::update(&mut storage, "REL-1", TaskUpdate { releases: Some(vec![99]), ..TaskUpdate::default() }).unwrap_err();
    assert!(err.to_string().contains("#99"), "{err}");
}

#[test]
fn versions_are_unique_without_the_v_and_listed_in_version_order() {
    let ws = workspace();
    create_release(&ws.tasks_dir, "0.10.0");
    create_release(&ws.tasks_dir, "v0.9.1");
    let err = ReleaseService::create(&ws.tasks_dir, Release { version: "0.9.1".into(), ..Release::default() }).unwrap_err();
    assert!(err.to_string().contains("already has version 0.9.1"), "{err}");
    let err = ReleaseService::create(&ws.tasks_dir, Release { version: "next".into(), ..Release::default() }).unwrap_err();
    assert!(err.to_string().contains("is not a version"), "{err}");
    let versions: Vec<String> = ReleaseService::list(&ws.tasks_dir).unwrap().into_iter().map(|r| r.release.version).collect();
    assert_eq!(versions, ["0.9.1", "0.10.0"]);
}

#[test]
fn membership_is_not_exclusive_and_move_replaces_it() {
    let ws = workspace();
    let mut storage = Storage::new(&ws.tasks_dir);
    let task = create_task(&mut storage, "Fix crash");
    let patch = create_release(&ws.tasks_dir, "0.15.1");
    let minor = create_release(&ws.tasks_dir, "0.16.0");

    ReleaseService::assign(&mut storage, "0.15.1", std::slice::from_ref(&task), ReleaseAssignmentAction::Add).unwrap();
    ReleaseService::assign(&mut storage, "#2", std::slice::from_ref(&task), ReleaseAssignmentAction::Add).unwrap();
    assert_eq!(task_releases(&ws.tasks_dir, &task), vec![patch, minor]);

    let again = ReleaseService::assign(&mut storage, "0.16.0", std::slice::from_ref(&task), ReleaseAssignmentAction::Add).unwrap();
    assert_eq!((again.modified.len(), again.unchanged.len()), (0, 1));

    ReleaseService::assign(&mut storage, "0.16.0", std::slice::from_ref(&task), ReleaseAssignmentAction::Move).unwrap();
    assert_eq!(task_releases(&ws.tasks_dir, &task), vec![minor]);
    ReleaseService::assign(&mut storage, "0.16.0", std::slice::from_ref(&task), ReleaseAssignmentAction::Remove).unwrap();
    assert!(task_releases(&ws.tasks_dir, &task).is_empty());

    let history = TaskService::get(&storage, &task, None).unwrap().history;
    assert!(history.iter().flat_map(|entry| &entry.changes).any(|change| change.field == "releases"));
}

#[test]
fn delete_takes_the_release_off_its_tickets() {
    let ws = workspace();
    let mut storage = Storage::new(&ws.tasks_dir);
    let task = create_task(&mut storage, "One");
    let keep = create_release(&ws.tasks_dir, "1.0.0");
    let gone = create_release(&ws.tasks_dir, "1.1.0");
    TaskService::update(&mut storage, &task, TaskUpdate { releases: Some(vec![keep, gone]), ..TaskUpdate::default() }).unwrap();

    let removed = ReleaseService::delete(&mut storage, gone).unwrap();
    assert_eq!(removed, vec![task.clone()]);
    assert_eq!(task_releases(&ws.tasks_dir, &task), vec![keep]);
    assert!(!ws.tasks_dir.join("@releases/2.yml").exists());
}

#[test]
fn cli_creates_fills_releases_and_finds_a_release() {
    let ws = workspace();
    let mut storage = Storage::new(&ws.tasks_dir);
    let first = create_task(&mut storage, "First");
    let second = create_task(&mut storage, "Second");
    let third = create_task(&mut storage, "Third");

    let created = cli(&ws, &["release", "create", "v0.15.0", "--label", "Spring", "--planned", "2026-12-01"]);
    assert_eq!(created["release"]["version"], "0.15.0");
    assert_eq!(created["release"]["state"], "planned");
    cli(&ws, &["release", "create", "0.14.2"]);

    let added = cli(&ws, &["release", "add", "spring", &first, &second]);
    assert_eq!(added["modified"], json!([first, second]));
    cli(&ws, &["release", "add", "0.14.2", "--select", "title:third"]);
    cli(&ws, &["release", "move", "0.14.2", &first]);

    let backlog = cli(&ws, &["release", "backlog"]);
    assert_eq!(backlog["total"], 0);

    let list = cli(&ws, &["release", "list"]);
    let versions: Vec<&str> = list["releases"].as_array().unwrap().iter().map(|r| r["version"].as_str().unwrap()).collect();
    assert_eq!(versions, ["0.14.2", "0.15.0"]);
    assert_eq!(list["releases"][0]["task_count"], 2);

    let released = cli(&ws, &["release", "release", "0.15.0", "--at", "2026-12-02"]);
    assert_eq!(released["release"]["state"], "released");
    assert_eq!(released["release"]["released_at"], "2026-12-02");
    let reopened = cli(&ws, &["release", "reopen", "#1"]);
    assert_eq!(reopened["release"]["state"], "planned");

    let shown = cli(&ws, &["release", "show", "0.14.2"]);
    let ids: Vec<&str> = shown["tasks"].as_array().unwrap().iter().map(|t| t["id"].as_str().unwrap()).collect();
    assert_eq!(ids.len(), 2);
    assert!(ids.contains(&first.as_str()) && ids.contains(&third.as_str()));

    // `release:` finds tickets by version, range, label and `none`.
    let found = |query: &str| -> Vec<String> {
        let out = cli(&ws, &["list", query]);
        let mut ids: Vec<String> = out["tasks"].as_array().unwrap().iter().map(|t| t["id"].as_str().unwrap().to_string()).collect();
        ids.sort();
        ids
    };
    assert_eq!(found("release:0.15"), vec![second.clone()]);
    assert_eq!(found("release>=0.14 release<0.15").len(), 2);
    assert_eq!(found("release:spring"), vec![second.clone()]);
    cli(&ws, &["release", "remove", "0.15.0", &second]);
    assert_eq!(found("release:none"), vec![second.clone()]);

    let deleted = cli(&ws, &["release", "delete", "0.14.2", "--force"]);
    assert_eq!(deleted["deleted"], true);
    assert_eq!(found("release:none").len(), 3);
}

// REST

fn rest(method: &str, path: &str, query: &[(&str, &str)], body: Value) -> (u16, Value) {
    let mut api = lotar::api_server::ApiServer::new();
    lotar::routes::initialize(&mut api);
    let request = lotar::api_server::HttpRequest {
        method: method.into(),
        path: path.into(),
        query: query.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect(),
        headers: Default::default(),
        body: serde_json::to_vec(&body).unwrap(),
    };
    let response = api.handle_request(&request);
    (response.status, serde_json::from_slice(&response.body).unwrap_or(Value::Null))
}

#[test]
fn rest_creates_fills_releases_and_marks_them_released() {
    let ws = workspace();
    let _env = EnvVarGuard::set("LOTAR_TASKS_DIR", &ws.tasks_dir.to_string_lossy());
    let mut storage = Storage::new(&ws.tasks_dir);
    let task = create_task(&mut storage, "Ship it");
    let open = create_task(&mut storage, "Later");

    let (status, body) = rest("POST", "/api/releases/create", &[], json!({"version": "2.0.0", "planned_date": "2000-01-01"}));
    assert_eq!(status, 200, "{body}");
    assert_eq!(body["data"]["release"]["overdue"], true);
    let (status, body) = rest("POST", "/api/releases/create", &[], json!({"version": "v2.0.0"}));
    assert_eq!(status, 400, "{body}");

    let (status, body) = rest("POST", "/api/releases/add", &[], json!({"release": 1, "tasks": [task]}));
    assert_eq!(status, 200, "{body}");
    assert_eq!(body["data"]["modified"], json!([task]));

    let (_, body) = rest("GET", "/api/releases/get", &[("release", "2.0.0")], Value::Null);
    assert_eq!(body["data"]["tasks"][0]["id"], json!(task));
    assert_eq!(body["data"]["tasks"][0]["releases"], json!([1]));

    let (_, body) = rest("GET", "/api/releases/backlog", &[], Value::Null);
    let backlog: Vec<&str> = body["data"]["tasks"].as_array().unwrap().iter().map(|t| t["id"].as_str().unwrap()).collect();
    assert_eq!(backlog, [open.as_str()]);

    let (_, body) = rest("GET", "/api/tasks/list", &[("query", "release:2.0")], Value::Null);
    assert_eq!(body["data"]["total"], 1, "{body}");

    let (status, body) = rest("POST", "/api/releases/release", &[], json!({"release": "#1"}));
    assert_eq!(status, 200, "{body}");
    assert_eq!(body["data"]["release"]["state"], "released");
    assert_eq!(body["data"]["release"]["overdue"], false);
    let (_, body) = rest("POST", "/api/releases/update", &[], json!({"release": 1, "released_at": ""}));
    assert_eq!(body["data"]["release"]["state"], "planned");
    let (_, body) = rest("POST", "/api/releases/update", &[], json!({"release": 1, "planned_date": ""}));
    assert_eq!(body["data"]["changed"], true);
    assert!(body["data"]["release"].get("planned_date").is_none());

    let (status, _) = rest("POST", "/api/releases/update", &[], json!({"release": 1, "planned_date": "soon"}));
    assert_eq!(status, 400);
    let (status, _) = rest("GET", "/api/releases/get", &[("release", "#9")], Value::Null);
    assert_eq!(status, 404);

    let (status, body) = rest("POST", "/api/tasks/update", &[], json!({"id": open, "releases": [1]}));
    assert_eq!(status, 200, "{body}");
    let (_, body) = rest("GET", "/api/releases/list", &[], Value::Null);
    assert_eq!(body["data"]["releases"][0]["task_count"], 2);

    let (status, body) = rest("POST", "/api/releases/delete", &[], json!({"release": 1}));
    assert_eq!(status, 200, "{body}");
    assert_eq!(body["data"]["removed_from"].as_array().unwrap().len(), 2);
}

// MCP

fn mcp(tool: &str, arguments: Value) -> Value {
    let line = json!({"jsonrpc": "2.0", "id": 1, "method": "tools/call", "params": {"name": tool, "arguments": arguments}});
    let response: Value = serde_json::from_str(&lotar::mcp::server::handle_json_line(&line.to_string())).unwrap();
    let text = response["result"]["content"][0]["text"].as_str().unwrap_or_else(|| panic!("{tool}: {response}"));
    assert_ne!(response["result"]["isError"], true, "{tool}: {text}");
    serde_json::from_str(text).unwrap()
}

#[test]
fn mcp_tools_create_fill_and_find_releases() {
    let ws = workspace();
    let _env = EnvVarGuard::set("LOTAR_TASKS_DIR", &ws.tasks_dir.to_string_lossy());
    let mut storage = Storage::new(&ws.tasks_dir);
    let task = create_task(&mut storage, "MCP task");

    let created = mcp("release_create", json!({"version": "3.1.0", "label": "Autumn"}));
    assert_eq!(created["release"]["display_name"], "3.1.0 (Autumn)");
    let added = mcp("release_add", json!({"release": "autumn", "query": "title:mcp"}));
    assert_eq!(added["modified"], json!([task]));
    let listed = mcp("task_list", json!({"releases": "3.1"}));
    assert_eq!(listed["total"], 1, "{listed}");
    let fetched = mcp("release_get", json!({"release": 1}));
    assert_eq!(fetched["release"]["open_count"], 1);
    let released = mcp("release_release", json!({"release": "3.1.0"}));
    assert_eq!(released["release"]["state"], "released");
    let backlog = mcp("release_backlog", json!({}));
    assert_eq!(backlog["total"], 0);
    mcp("release_remove", json!({"release": 1, "tasks": task}));
    assert_eq!(mcp("release_list", json!({}))["releases"][0]["task_count"], 0);
    let updated = mcp("task_update", json!({"id": task, "patch": {"releases": [1]}}));
    assert_eq!(updated["task"]["releases"].as_array().map(Vec::len).or(updated["releases"].as_array().map(Vec::len)), Some(1), "{updated}");
    assert_eq!(mcp("release_delete", json!({"release": "#1"}))["removed_from"], json!([task]));
}

// Automation

#[test]
fn automation_conditions_events_and_actions_on_releases() {
    let ws = workspace();
    let _env = [EnvVarGuard::set("LOTAR_IGNORE_HOME_CONFIG", "1"), EnvVarGuard::clear("LOTAR_PROJECT")];
    let mut storage = Storage::new(&ws.tasks_dir);
    let task = create_task(&mut storage, "Automated");
    create_release(&ws.tasks_dir, "1.0.0");
    create_release(&ws.tasks_dir, "1.1.0");
    std::fs::write(
        ws.tasks_dir.join("automation.yml"),
        "automation:\n  rules:\n    - name: Plan done work\n      when: { status: Done, release: { exists: false } }\n      on:\n        updated:\n          add: { release: '@next' }\n    - name: Announce\n      when: { release: { version: '>=1.0 <1.1' } }\n      on:\n        release_changed:\n          comment: 'in ${{ticket.releases[*]}}'\n",
    )
    .unwrap();

    // A condition on the ticket's releases and an add action with `@next`.
    TaskService::update(&mut storage, &task, TaskUpdate { status: Some("Done".into()), ..TaskUpdate::default() }).unwrap();
    assert_eq!(task_releases(&ws.tasks_dir, &task), vec![1], "the next planned release is 1.0.0");

    // `release_changed` fires when a person changes a ticket's releases;
    // a version condition picks the release.
    let other = create_task(&mut storage, "Other");
    TaskService::update(&mut storage, &other, TaskUpdate { releases: Some(vec![2]), ..TaskUpdate::default() }).unwrap();
    assert!(TaskService::get(&storage, &other, None).unwrap().comments.is_empty(), "1.1.0 is outside the range");
    TaskService::update(&mut storage, &other, TaskUpdate { releases: Some(vec![1]), ..TaskUpdate::default() }).unwrap();
    let comments: Vec<String> = TaskService::get(&storage, &other, None).unwrap().comments.into_iter().map(|comment| comment.text).collect();
    assert_eq!(comments, ["in 1"]);
}

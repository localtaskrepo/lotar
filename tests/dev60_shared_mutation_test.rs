//! DEV-60: bulk comments and reference changes through the shared
//! mutation pipeline.
//!
//! Public-boundary regression coverage:
//! - bulk MCP comments delegate to the shared comment pipeline: identical
//!   history/timestamps, `on.commented` fired exactly once per task,
//!   per-item failures with the updated/failed/stop_on_error envelope
//! - reference mutations (all kinds) run through the locked read-modify-
//!   write primitive: one history entry + one timestamp bump per changed
//!   op, byte-identical files and zero hooks on no-ops, and no lost
//!   concurrent writes to unrelated fields
//! - changed reference ops dispatch post-commit automation (bare
//!   `on.updated` + legacy `start` fire; conditioned rules stay silent on
//!   the empty changeset) and emit exactly one REST task event, strictly
//!   after every attachment store lock dropped
//! - bulk code detach works outside Git; bulk code/file ADD and file
//!   remove still require the repo root per item
//!
//! No git tooling: repo-anchored fixtures live under CARGO_TARGET_TMPDIR
//! (ancestry-only `.git` discovery); the Gitless fixture lives under the
//! plain temp dir outside the checkout. No Git gating.
use lotar::api_events::{self, ApiEvent};
use lotar::api_server::{ApiServer, HttpRequest};
use lotar::routes;
use lotar::services::automation_service::AutomationService;
use lotar::services::reference_service::ReferenceService;
use lotar::services::task_service::TaskService;
use lotar::storage::manager::Storage;
use lotar::types::ReferenceEntry;
use serde_json::{Value, json};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::mpsc;
use std::time::{Duration, Instant};
mod common;
use crate::common::env_mutex::EnvVarGuard;

// ---------------------------------------------------------------------------
// Fixtures
// ---------------------------------------------------------------------------

struct Fixture {
    _tmp: tempfile::TempDir,
    tasks_dir: PathBuf,
    _guard_tasks: EnvVarGuard,
    _guard_fast: EnvVarGuard,
}

/// Repo-anchored workspace (find_repo_root resolves by ancestry) with an
/// env-guarded LOTAR_TASKS_DIR so REST/MCP surfaces resolve this workspace.
fn workspace(project: &str) -> Fixture {
    let _guard_fast = EnvVarGuard::set("LOTAR_TEST_FAST_IO", "1");
    let tmp = tempfile::tempdir_in(Path::new(env!("CARGO_TARGET_TMPDIR"))).unwrap();
    let tasks_dir = tmp.path().join(".tasks");
    std::fs::create_dir_all(&tasks_dir).unwrap();
    std::fs::write(
        lotar::utils::paths::global_config_path(&tasks_dir),
        format!(
            "default.project: {project}\nissue.states: [Todo, InProgress, Done]\nissue.types: [Feature, Bug, Chore]\nissue.priorities: [Low, Medium, High]\n"
        ),
    )
    .unwrap();
    let _guard_tasks = EnvVarGuard::set("LOTAR_TASKS_DIR", &tasks_dir.to_string_lossy());
    Fixture {
        _tmp: tmp,
        tasks_dir,
        _guard_tasks,
        _guard_fast,
    }
}

/// Gitless workspace outside the checkout: plain temp dir with no `.git`
/// ancestor, for the bulk code-detach-without-repo contract.
fn gitless_workspace(project: &str) -> (tempfile::TempDir, PathBuf) {
    let tmp = tempfile::tempdir().unwrap();
    let tasks_dir = tmp.path().join(".tasks");
    std::fs::create_dir_all(&tasks_dir).unwrap();
    std::fs::write(
        lotar::utils::paths::global_config_path(&tasks_dir),
        format!(
            "default.project: {project}\nissue.states: [Todo, InProgress, Done]\nissue.types: [Feature, Bug, Chore]\nissue.priorities: [Low, Medium, High]\n"
        ),
    )
    .unwrap();
    (tmp, tasks_dir)
}

fn set_automation(tasks_dir: &Path, project: &str, rules: &str) {
    let yaml = format!("automation:\n  rules:\n{rules}");
    AutomationService::set(tasks_dir, Some(project), &yaml).expect("set automation");
}

fn create_task(storage: &mut Storage, project: &str, title: &str) -> lotar::api_types::TaskDTO {
    TaskService::create(
        storage,
        lotar::api_types::TaskCreate {
            title: title.to_string(),
            project: Some(project.to_string()),
            ..Default::default()
        },
    )
    .expect("create task")
}

fn server() -> ApiServer {
    let mut api = ApiServer::new();
    routes::initialize(&mut api);
    api
}

fn mk_req(method: &str, path: &str, body: Value) -> HttpRequest {
    let mut q = HashMap::new();
    HttpRequest {
        method: method.to_string(),
        path: path.to_string(),
        query: {
            q.insert("".to_string(), String::new());
            q.clear();
            q
        },
        headers: HashMap::new(),
        body: serde_json::to_vec(&body).unwrap(),
    }
}

fn rest_create(api: &ApiServer, project: &str, title: &str) -> String {
    let resp = api.handle_request(&mk_req(
        "POST",
        "/api/tasks/add",
        json!({"title": title, "project": project}),
    ));
    assert_eq!(resp.status, 201, "{}", String::from_utf8_lossy(&resp.body));
    serde_json::from_slice::<Value>(&resp.body).unwrap()["data"]["id"]
        .as_str()
        .unwrap()
        .to_string()
}

fn base64_encode(data: &[u8]) -> String {
    const TABLE: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::new();
    for chunk in data.chunks(3) {
        let b = [
            chunk[0],
            *chunk.get(1).unwrap_or(&0),
            *chunk.get(2).unwrap_or(&0),
        ];
        let n = (u32::from(b[0]) << 16) | (u32::from(b[1]) << 8) | u32::from(b[2]);
        out.push(TABLE[(n >> 18) as usize & 63] as char);
        out.push(TABLE[(n >> 12) as usize & 63] as char);
        out.push(if chunk.len() > 1 {
            TABLE[(n >> 6) as usize & 63] as char
        } else {
            '='
        });
        out.push(if chunk.len() > 2 {
            TABLE[n as usize & 63] as char
        } else {
            '='
        });
    }
    out
}

fn mcall(name: &str, args: Value) -> Value {
    let req = json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "tools/call",
        "params": {"name": name, "arguments": args}
    });
    let resp_line = lotar::mcp::server::handle_json_line(&serde_json::to_string(&req).unwrap());
    let resp: Value = serde_json::from_str(&resp_line).unwrap();
    let text = resp["result"]["content"][0]["text"]
        .as_str()
        .unwrap_or("{}");
    serde_json::from_str(text).unwrap_or(json!({}))
}

fn task_file(tasks_dir: &Path, id: &str) -> PathBuf {
    let (project, number) = id.rsplit_once('-').expect("full task id");
    tasks_dir.join(project).join(format!("{number}.yml"))
}

fn read_task(tasks_dir: &Path, id: &str) -> lotar::storage::task::Task {
    let yaml = std::fs::read_to_string(task_file(tasks_dir, id)).unwrap();
    lotar::storage::task::parse_task_yaml_tolerant(&yaml).expect("parse task yaml")
}

/// Count queued `task_updated` events whose payload id starts with the
/// given prefix. All emissions for a synchronous operation are already
/// queued when it returns; the bounded window only absorbs slow channel
/// delivery. Events from parallel tests are filtered by the unique prefix.
fn count_task_updates(rx: &mpsc::Receiver<ApiEvent>, id_prefix: &str) -> usize {
    let mut matching = 0;
    let deadline = Instant::now() + Duration::from_millis(300);
    while let Ok(event) = rx.recv_timeout(Duration::from_millis(50)) {
        if event.kind == "task_updated"
            && event
                .data
                .get("id")
                .and_then(Value::as_str)
                .is_some_and(|id| id.starts_with(id_prefix))
        {
            matching += 1;
        }
        if Instant::now() > deadline {
            break;
        }
    }
    matching
}

fn history_entries<'a>(
    task: &'a lotar::storage::task::Task,
    field: &str,
) -> Vec<&'a lotar::types::TaskChange> {
    task.history
        .iter()
        .flat_map(|entry| entry.changes.iter())
        .filter(|change| change.field == field)
        .collect()
}

fn tag_count(task: &lotar::storage::task::Task, tag: &str) -> usize {
    task.tags.iter().filter(|t| t.as_str() == tag).count()
}

// ---------------------------------------------------------------------------
// Bulk comments through the shared pipeline
// ---------------------------------------------------------------------------

#[test]
fn bulk_comment_fires_on_commented_exactly_once_per_task() {
    let fx = workspace("D6A1");
    let mut storage = Storage::new(&fx.tasks_dir);
    let one = create_task(&mut storage, "D6A1", "Bulk one");
    let two = create_task(&mut storage, "D6A1", "Bulk two");
    set_automation(
        &fx.tasks_dir,
        "D6A1",
        "    - name: bulk-seen\n      on: {commented: {add: {tags: [bulk-seen]}}}\n",
    );

    let payload = mcall(
        "task_bulk_comment_add",
        json!({"ids": [one.id, two.id], "text": "bulk note"}),
    );

    assert_eq!(payload["status"], "ok", "{payload}");
    let updated = payload["updated"].as_array().expect("updated array");
    assert_eq!(updated.len(), 2, "{payload}");
    assert!(
        payload["failed"].as_array().unwrap().is_empty(),
        "{payload}"
    );

    for id in [&one.id, &two.id] {
        let task = read_task(&fx.tasks_dir, id);
        assert_eq!(
            task.comments.len(),
            1,
            "{id}: exactly one comment, got {:?}",
            task.comments
        );
        assert_eq!(task.comments[0].text, "bulk note");
        let added = history_entries(&task, "comment_added");
        assert_eq!(added.len(), 1, "{id}: one comment_added history entry");
        assert_eq!(
            task.modified,
            task.history.last().map(|e| e.at.clone()).unwrap(),
            "{id}: modified matches the recorded history timestamp"
        );
        // on.commented fired exactly once: the tag is present exactly once.
        assert_eq!(tag_count(&task, "bulk-seen"), 1, "{id}: {:?}", task.tags);
    }
}

#[test]
fn bulk_comment_per_item_failures_alias_and_stop_on_error() {
    let fx = workspace("D6A2");
    let mut storage = Storage::new(&fx.tasks_dir);
    let one = create_task(&mut storage, "D6A2", "Alias target");
    let two = create_task(&mut storage, "D6A2", "Second target");

    // Mixed success: a padded alias canonicalizes, a malformed id fails as
    // its own item, and the batch continues.
    let payload = mcall(
        "task_bulk_comment_add",
        json!({"ids": [format!("{}001", &one.id[..one.id.len()-1]), "garbage", two.id], "text": "mix"}),
    );
    assert_eq!(payload["status"], "ok", "{payload}");
    let updated = payload["updated"].as_array().unwrap();
    assert_eq!(updated.len(), 2, "{payload}");
    assert_eq!(
        updated[0]["id"], one.id,
        "padded alias canonicalizes: {payload}"
    );
    assert_eq!(updated[1]["id"], two.id, "{payload}");
    let failed = payload["failed"].as_array().unwrap();
    assert_eq!(failed.len(), 1, "{payload}");
    assert_eq!(failed[0]["id"], "garbage", "{payload}");
    assert!(
        failed[0]["error"]
            .as_str()
            .unwrap()
            .contains("Invalid task ID"),
        "{payload}"
    );

    // stop_on_error aborts at the first failing item.
    let payload = mcall(
        "task_bulk_comment_add",
        json!({"ids": ["garbage", two.id], "text": "stop", "stop_on_error": true}),
    );
    assert_eq!(payload["status"], "ok", "{payload}");
    assert!(
        payload["updated"].as_array().unwrap().is_empty(),
        "{payload}"
    );
    assert_eq!(payload["failed"].as_array().unwrap().len(), 1, "{payload}");
    let task = read_task(&fx.tasks_dir, &two.id);
    assert_eq!(
        task.comments.len(),
        1,
        "stop_on_error leaves later items untouched: {:?}",
        task.comments
    );
    assert_eq!(
        task.comments[0].text, "mix",
        "the aborted item never landed"
    );
}

#[test]
fn automation_comment_uses_automation_actor_and_never_recurses() {
    let fx = workspace("D6A3");
    let mut storage = Storage::new(&fx.tasks_dir);
    let task = create_task(&mut storage, "D6A3", "Recursion target");
    set_automation(
        &fx.tasks_dir,
        "D6A3",
        "    - name: auto-note\n      on: {commented: {comment: \"auto note\"}}\n",
    );

    TaskService::add_comment(&mut storage, &task.id, "user note").expect("add comment");

    let stored = read_task(&fx.tasks_dir, &task.id);
    // The user comment plus exactly one automation comment — the automation
    // comment never re-dispatched on.commented.
    assert_eq!(stored.comments.len(), 2, "{:?}", stored.comments);
    assert_eq!(stored.comments[0].text, "user note");
    assert_eq!(stored.comments[1].text, "auto note");
    let automation_entries: Vec<_> = stored
        .history
        .iter()
        .filter(|entry| entry.actor.as_deref() == Some("automation"))
        .flat_map(|entry| entry.changes.iter())
        .filter(|change| change.field == "comment_added")
        .collect();
    assert_eq!(
        automation_entries.len(),
        1,
        "automation actor recorded exactly once: {:?}",
        stored.history
    );
}

#[test]
fn update_comment_keeps_silent_on_commented_and_noops_byte_equal() {
    let fx = workspace("D6A4");
    let mut storage = Storage::new(&fx.tasks_dir);
    let task = create_task(&mut storage, "D6A4", "Edit target");
    set_automation(
        &fx.tasks_dir,
        "D6A4",
        "    - name: seen\n      on: {commented: {add: {tags: [seen]}}}\n",
    );
    TaskService::add_comment(&mut storage, &task.id, "first").expect("add");

    // Editing the comment must NOT fire on.commented again.
    TaskService::update_comment(&mut storage, &task.id, 0, "edited").expect("edit");
    let stored = read_task(&fx.tasks_dir, &task.id);
    assert_eq!(tag_count(&stored, "seen"), 1, "{:?}", stored.tags);
    let edited: Vec<_> = history_entries(&stored, "comment#1");
    assert_eq!(edited.len(), 1);
    assert_eq!(edited[0].old.as_deref(), Some("first"));
    assert_eq!(edited[0].new.as_deref(), Some("edited"));

    // Re-saving the identical text is a byte-equal no-op.
    let before = std::fs::read(task_file(&fx.tasks_dir, &task.id)).unwrap();
    TaskService::update_comment(&mut storage, &task.id, 0, "edited").expect("noop edit");
    let after = std::fs::read(task_file(&fx.tasks_dir, &task.id)).unwrap();
    assert_eq!(
        before, after,
        "no-op comment edit must not rewrite the file"
    );
    let stored = read_task(&fx.tasks_dir, &task.id);
    assert_eq!(history_entries(&stored, "comment#1").len(), 1);
}

// ---------------------------------------------------------------------------
// Reference mutations: history, timestamps, events, no-ops
// ---------------------------------------------------------------------------

#[test]
fn reference_change_history_timestamp_and_event_exactly_once() {
    let fx = workspace("D6A5");
    let api = server();
    let id = rest_create(&api, "D6A5", "Reference target");

    // Changed op: one event, one history entry, timestamp == entry time.
    let rx = api_events::subscribe();
    let resp = api.handle_request(&mk_req(
        "POST",
        "/api/tasks/references/link/add",
        json!({"id": id, "url": "https://example.com/dev60"}),
    ));
    assert_eq!(resp.status, 200, "{}", String::from_utf8_lossy(&resp.body));
    assert_eq!(
        count_task_updates(&rx, "D6A5-"),
        1,
        "exactly one task_updated"
    );
    let task = read_task(&fx.tasks_dir, &id);
    let added = history_entries(&task, "reference_added");
    assert_eq!(added.len(), 1, "{:?}", task.history);
    assert_eq!(added[0].old, None);
    assert_eq!(
        added[0].new.as_deref(),
        Some("link:https://example.com/dev60")
    );
    let entry_at = task
        .history
        .iter()
        .find(|e| e.changes.iter().any(|c| c.field == "reference_added"))
        .unwrap()
        .at
        .clone();
    assert_eq!(
        task.modified, entry_at,
        "modified bumps with the history entry"
    );
    assert!(
        task.references
            .iter()
            .any(|r| r.link.as_deref() == Some("https://example.com/dev60"))
    );

    // No-op op: byte-identical file, zero events, zero history churn.
    let before = std::fs::read(task_file(&fx.tasks_dir, &id)).unwrap();
    let rx = api_events::subscribe();
    let resp = api.handle_request(&mk_req(
        "POST",
        "/api/tasks/references/link/add",
        json!({"id": id, "url": "https://example.com/dev60"}),
    ));
    assert_eq!(resp.status, 200, "{}", String::from_utf8_lossy(&resp.body));
    let added_flag = serde_json::from_slice::<Value>(&resp.body).unwrap()["data"]["added"]
        .as_bool()
        .unwrap();
    assert!(!added_flag, "no-op attach reports added=false");
    assert_eq!(count_task_updates(&rx, "D6A5-"), 0, "no-op emits no event");
    let after = std::fs::read(task_file(&fx.tasks_dir, &id)).unwrap();
    assert_eq!(
        before, after,
        "no-op reference op must not rewrite the file"
    );

    // Removed op: one history entry with old set, new absent.
    let rx = api_events::subscribe();
    let resp = api.handle_request(&mk_req(
        "POST",
        "/api/tasks/references/link/remove",
        json!({"id": id, "url": "https://example.com/dev60"}),
    ));
    assert_eq!(resp.status, 200, "{}", String::from_utf8_lossy(&resp.body));
    assert_eq!(count_task_updates(&rx, "D6A5-"), 1);
    let task = read_task(&fx.tasks_dir, &id);
    let removed = history_entries(&task, "reference_removed");
    assert_eq!(removed.len(), 1, "{:?}", task.history);
    assert_eq!(
        removed[0].old.as_deref(),
        Some("link:https://example.com/dev60")
    );
    assert_eq!(removed[0].new, None);
}

#[test]
fn reference_failure_emits_no_events_or_history() {
    let _fx = workspace("D6A6");
    let api = server();
    let _ = rest_create(&api, "D6A6", "Existing");

    let rx = api_events::subscribe();
    let resp = api.handle_request(&mk_req(
        "POST",
        "/api/tasks/references/link/add",
        json!({"id": "D6A6-99", "url": "https://example.com/missing"}),
    ));
    assert_eq!(resp.status, 404, "{}", String::from_utf8_lossy(&resp.body));
    assert_eq!(
        count_task_updates(&rx, "D6A6-"),
        0,
        "failed op emits nothing"
    );
}

#[test]
fn reference_update_automation_bare_updated_fires_conditioned_stays_silent() {
    // Bare on.updated rules fire exactly once for a changed reference op
    // and never for a no-op; conditioned rules see an empty changeset and
    // stay silent (references are not in the condition vocabulary).
    let fx = workspace("D6A7");
    let api = server();
    let id = rest_create(&api, "D6A7", "Automation target");
    set_automation(
        &fx.tasks_dir,
        "D6A7",
        "    - name: touch\n      on: {updated: {add: {tags: [touched]}}}\n    - name: on-done\n      when: {changes: {status: {to: Done}}}\n      on: {updated: {add: {tags: [done-tag]}}}\n",
    );

    let resp = api.handle_request(&mk_req(
        "POST",
        "/api/tasks/references/add",
        json!({"id": id, "kind": "jira", "value": "abc-9"}),
    ));
    assert_eq!(resp.status, 200, "{}", String::from_utf8_lossy(&resp.body));

    let task = read_task(&fx.tasks_dir, &id);
    assert_eq!(
        tag_count(&task, "touched"),
        1,
        "bare on.updated fired once: {:?}",
        task.tags
    );
    assert_eq!(
        tag_count(&task, "done-tag"),
        0,
        "conditioned rule stayed silent: {:?}",
        task.tags
    );

    // No-op attach does not re-fire.
    let resp = api.handle_request(&mk_req(
        "POST",
        "/api/tasks/references/add",
        json!({"id": id, "kind": "jira", "value": "ABC-9"}),
    ));
    assert_eq!(resp.status, 200);
    let task = read_task(&fx.tasks_dir, &id);
    assert_eq!(tag_count(&task, "touched"), 1, "{:?}", task.tags);
}

// ---------------------------------------------------------------------------
// Concurrency: locked read-modify-write loses no unrelated fields
// ---------------------------------------------------------------------------

#[test]
fn concurrent_comments_and_references_preserve_all_writes() {
    let (tmp, tasks_dir) = gitless_workspace("D6C1");
    let mut storage = Storage::new(&tasks_dir);
    let task = create_task(&mut storage, "D6C1", "Concurrency target");
    drop(storage);

    let root = tasks_dir.clone();
    let id = task.id.clone();
    let mut handles = Vec::new();
    for i in 0..3 {
        let root = root.clone();
        let id = id.clone();
        handles.push(std::thread::spawn(move || {
            let mut storage = Storage::new(&root);
            let outcome = ReferenceService::attach_link_reference(
                &mut storage,
                &id,
                &format!("https://example.com/concurrent/{i}"),
            )
            .expect("concurrent link attach");
            assert!(outcome.changed, "concurrent attach reports changed");
        }));
    }
    for i in 0..3 {
        let root = root.clone();
        let id = id.clone();
        handles.push(std::thread::spawn(move || {
            let mut storage = Storage::new(&root);
            TaskService::add_comment(&mut storage, &id, &format!("comment {i}"))
                .expect("concurrent comment");
        }));
    }
    for handle in handles {
        handle.join().expect("threads finish");
    }

    let stored = read_task(&tasks_dir, &task.id);
    let links: Vec<_> = stored
        .references
        .iter()
        .filter_map(|r| r.link.clone())
        .collect();
    for i in 0..3 {
        assert!(
            links
                .iter()
                .any(|l| l == &format!("https://example.com/concurrent/{i}")),
            "reference {i} survived concurrent writers: {links:?}"
        );
    }
    assert_eq!(stored.comments.len(), 3, "{:?}", stored.comments);
    assert_eq!(
        history_entries(&stored, "reference_added").len(),
        3,
        "{:?}",
        stored.history
    );
    assert_eq!(history_entries(&stored, "comment_added").len(), 3);
    drop(tmp);
}

// ---------------------------------------------------------------------------
// Bulk references: Gitless code detach, per-arm repo-root requirement
// ---------------------------------------------------------------------------

#[test]
fn gitless_bulk_code_remove_needs_no_repo_root_but_add_does() {
    let (tmp, tasks_dir) = gitless_workspace("D6C2");
    assert!(
        lotar::utils::git::find_repo_root(&tasks_dir).is_none(),
        "fixture precondition: no repo root by ancestry"
    );

    // Seed a code reference through the shared locked primitive.
    let mut storage = Storage::new(&tasks_dir);
    let task = create_task(&mut storage, "D6C2", "Gitless target");
    storage
        .mutate_task(&task.id, |t| {
            t.references.push(ReferenceEntry {
                code: Some("src/thing.rs#10-20".to_string()),
                ..Default::default()
            });
            Ok(true)
        })
        .expect("seed reference");

    let _guard_tasks = EnvVarGuard::set("LOTAR_TASKS_DIR", &tasks_dir.to_string_lossy());
    let _guard_fast = EnvVarGuard::set("LOTAR_TEST_FAST_IO", "1");

    // Bulk code REMOVE works without Git (parity with the single handler).
    let payload = mcall(
        "task_bulk_reference_remove",
        json!({"ids": [task.id], "kind": "code", "value": "src/thing.rs#L10-L20"}),
    );
    assert_eq!(payload["status"], "ok", "{payload}");
    let updated = payload["updated"].as_array().unwrap();
    assert_eq!(updated.len(), 1, "{payload}");
    assert_eq!(updated[0]["changed"], true, "{payload}");
    assert!(
        payload["failed"].as_array().unwrap().is_empty(),
        "{payload}"
    );
    let stored = read_task(&tasks_dir, &task.id);
    assert!(
        stored.references.iter().all(|r| r.code.is_none()),
        "{:?}",
        stored.references
    );
    assert_eq!(history_entries(&stored, "reference_removed").len(), 1);

    // Bulk code ADD still requires the repo root — as a per-item failure,
    // not a whole-batch abort.
    let payload = mcall(
        "task_bulk_reference_add",
        json!({"ids": [task.id], "kind": "code", "value": "src/thing.rs#10-20"}),
    );
    assert_eq!(payload["status"], "ok", "{payload}");
    let failed = payload["failed"].as_array().unwrap();
    assert_eq!(failed.len(), 1, "{payload}");
    assert!(
        failed[0]["error"]
            .as_str()
            .unwrap()
            .contains("Unable to locate git repository"),
        "{payload}"
    );
    assert!(
        payload["updated"].as_array().unwrap().is_empty(),
        "{payload}"
    );

    // File operations also need the root, per item.
    let payload = mcall(
        "task_bulk_reference_remove",
        json!({"ids": [task.id], "kind": "file", "value": "src/thing.rs"}),
    );
    let failed = payload["failed"].as_array().unwrap();
    assert_eq!(failed.len(), 1, "{payload}");
    assert!(
        failed[0]["error"]
            .as_str()
            .unwrap()
            .contains("Unable to locate git repository"),
        "{payload}"
    );
    drop(tmp);
}

#[test]
fn bulk_reference_per_item_envelopes_and_alias() {
    let fx = workspace("D6C3");
    let mut storage = Storage::new(&fx.tasks_dir);
    let one = create_task(&mut storage, "D6C3", "Bulk ref one");
    let two = create_task(&mut storage, "D6C3", "Bulk ref two");

    let payload = mcall(
        "task_bulk_reference_add",
        json!({"ids": [format!("{}001", &one.id[..one.id.len()-1]), two.id, "bogus"], "kind": "jira", "value": "XYZ-1"}),
    );
    assert_eq!(payload["status"], "ok", "{payload}");
    let updated = payload["updated"].as_array().unwrap();
    assert_eq!(updated.len(), 2, "{payload}");
    // The envelope echoes the submitted id (padded alias) while the task
    // DTO carries the canonical spelling — the preexisting bulk contract.
    assert_eq!(updated[0]["id"], "D6C3-001", "{payload}");
    assert_eq!(
        updated[0]["task"]["id"], one.id,
        "canonical task id: {payload}"
    );
    assert_eq!(updated[0]["changed"], true, "{payload}");
    let failed = payload["failed"].as_array().unwrap();
    assert_eq!(failed.len(), 1, "{payload}");
    assert_eq!(failed[0]["id"], "bogus", "{payload}");

    for id in [&one.id, &two.id] {
        let stored = read_task(&fx.tasks_dir, id);
        assert_eq!(history_entries(&stored, "reference_added").len(), 1, "{id}");
    }

    // stop_on_error halts before later items mutate.
    let payload = mcall(
        "task_bulk_reference_add",
        json!({"ids": ["bogus", two.id], "kind": "jira", "value": "XYZ-2", "stop_on_error": true}),
    );
    assert!(
        payload["updated"].as_array().unwrap().is_empty(),
        "{payload}"
    );
    let stored = read_task(&fx.tasks_dir, &two.id);
    assert!(
        stored
            .references
            .iter()
            .all(|r| r.jira.as_deref() != Some("XYZ-2")),
        "the aborted item's value never landed: {:?}",
        stored.references
    );
}

// ---------------------------------------------------------------------------
// Attachment store locks drop before post-commit hooks
// ---------------------------------------------------------------------------

#[test]
fn upload_and_attachment_add_run_hooks_after_store_lock_release() {
    let fx = workspace("D6C5");
    let api = server();
    let id = rest_create(&api, "D6C5", "Upload hooks target");
    // The rule effect must be countable per firing, so it appends a
    // comment instead of adding a deduping tag.
    set_automation(
        &fx.tasks_dir,
        "D6C5",
        "    - name: touch\n      on: {updated: {comment: \"hook-fired\"}}\n",
    );

    // Real REST upload: if hooks ran under the store lock, the in-process
    // dispatch guard would panic and fail this test; the rule firing proves
    // dispatch ran at all.
    let resp = api.handle_request(&mk_req(
        "POST",
        "/api/tasks/attachments/upload",
        json!({"id": id, "filename": "hooks.txt", "content_base64": base64_encode(b"hooks-blob")}),
    ));
    assert_eq!(resp.status, 200, "{}", String::from_utf8_lossy(&resp.body));
    let task = read_task(&fx.tasks_dir, &id);
    assert_eq!(
        task.comments
            .iter()
            .filter(|c| c.text == "hook-fired")
            .count(),
        1,
        "{:?}",
        task.comments
    );
    assert_eq!(history_entries(&task, "reference_added").len(), 1);

    // MCP attachment add goes through the same ordering.
    let resp = api.handle_request(&mk_req(
        "POST",
        "/api/tasks/add",
        json!({"title": "MCP hooks target", "project": "D6C5"}),
    ));
    let second = serde_json::from_slice::<Value>(&resp.body).unwrap()["data"]["id"]
        .as_str()
        .unwrap()
        .to_string();
    let stored_path = {
        let resp = api.handle_request(&mk_req(
            "POST",
            "/api/tasks/attachments/upload",
            json!({"id": second, "filename": "mcp.txt", "content_base64": base64_encode(b"mcp-blob")}),
        ));
        assert_eq!(resp.status, 200);
        serde_json::from_slice::<Value>(&resp.body).unwrap()["data"]["stored_path"]
            .as_str()
            .unwrap()
            .to_string()
    };
    // The upload already attached the managed reference (and fired the
    // rule once through the REST path). A changed MCP attachment mutation —
    // the remove — must dispatch its hooks after the per-item store lock:
    // the in-process guard would panic otherwise, and the rule fires again.
    let payload = mcall(
        "task_reference_remove",
        json!({"id": second, "kind": "attachment", "value": stored_path}),
    );
    assert_eq!(payload["changed"], true, "{payload}");
    let task = read_task(&fx.tasks_dir, &second);
    assert!(
        task.references.iter().all(|r| r.attachment.is_none()),
        "{:?}",
        task.references
    );
    assert_eq!(
        task.comments
            .iter()
            .filter(|c| c.text == "hook-fired")
            .count(),
        2,
        "the changed MCP mutation fired the rule again after the lock dropped: {:?}",
        task.comments
    );
}

#[test]
fn upload_noop_attach_is_store_only_and_emits_no_task_event() {
    let fx = workspace("D6C6");
    let api = server();
    let id = rest_create(&api, "D6C6", "Dedup target");

    let rx = api_events::subscribe();
    let resp = api.handle_request(&mk_req(
        "POST",
        "/api/tasks/attachments/upload",
        json!({"id": id, "filename": "dup.txt", "content_base64": base64_encode(b"dup-blob")}),
    ));
    assert_eq!(resp.status, 200, "{}", String::from_utf8_lossy(&resp.body));
    assert_eq!(
        count_task_updates(&rx, "D6C6-"),
        1,
        "first upload emits once"
    );

    // Second upload of identical content: the blob dedups and the attach is
    // a no-op (attached=false). The store changed; the task did not — no
    // task event, no history churn.
    let before = std::fs::read(task_file(&fx.tasks_dir, &id)).unwrap();
    let rx = api_events::subscribe();
    let resp = api.handle_request(&mk_req(
        "POST",
        "/api/tasks/attachments/upload",
        json!({"id": id, "filename": "dup.txt", "content_base64": base64_encode(b"dup-blob")}),
    ));
    assert_eq!(resp.status, 200, "{}", String::from_utf8_lossy(&resp.body));
    let data = serde_json::from_slice::<Value>(&resp.body).unwrap()["data"].clone();
    assert_eq!(data["attached"], false, "{data}");
    assert!(
        data["stored_path"].as_str().is_some_and(|p| !p.is_empty()),
        "{data}"
    );
    assert_eq!(
        count_task_updates(&rx, "D6C6-"),
        0,
        "store-only change emits no task event"
    );
    let after = std::fs::read(task_file(&fx.tasks_dir, &id)).unwrap();
    assert_eq!(before, after, "task bytes untouched by a no-op attach");

    // Attachment remove without typed membership fails closed, keeps the
    // store untouched, and emits nothing.
    let rx = api_events::subscribe();
    let resp = api.handle_request(&mk_req(
        "POST",
        "/api/tasks/attachments/remove",
        json!({"id": id, "stored_path": "no-such-blob.txt"}),
    ));
    assert_eq!(resp.status, 400, "{}", String::from_utf8_lossy(&resp.body));
    assert_eq!(count_task_updates(&rx, "D6C6-"), 0);
}

#[test]
fn mcp_single_attachment_add_checks_blob_under_the_store_lock() {
    let fx = workspace("D6C7");
    let api = server();
    let id = rest_create(&api, "D6C7", "Lock ordering add target");

    // Hold the store lock independently: the single-task attachment add
    // must acquire the lock BEFORE its blob-existence check (matching the
    // bulk handler and CLI), so the call fails with the lock contention
    // error rather than an immediate missing-blob "not found". Deterministic
    // ordering proof bounded by the standard 2s lock wait — no sleeps.
    let store_root = fx.tasks_dir.join("@attachments");
    std::fs::create_dir_all(&store_root).unwrap();
    let guard = lotar::services::attachment_service::AttachmentService::lock_store(&store_root)
        .expect("independent store lock");

    let req = json!({
        "jsonrpc": "2.0", "id": 1, "method": "tools/call",
        "params": {"name": "task_reference_add",
                   "arguments": {"id": id, "kind": "attachment", "value": "missing-blob.txt"}}
    });
    let line = lotar::mcp::server::handle_json_line(&serde_json::to_string(&req).unwrap());
    let resp: Value = serde_json::from_str(&line).unwrap();
    let message = resp["error"]["data"]["message"]
        .as_str()
        .unwrap_or_default();
    assert!(
        message.contains("attachments-store") && message.contains("busy"),
        "lock must be acquired before the blob check: {message}"
    );

    // After release the same call fails on the missing blob (check intact),
    // and nothing was attached.
    drop(guard);
    let line = lotar::mcp::server::handle_json_line(&serde_json::to_string(&req).unwrap());
    let resp: Value = serde_json::from_str(&line).unwrap();
    let message = resp["error"]["data"]["message"]
        .as_str()
        .unwrap_or_default();
    assert!(
        message.to_lowercase().contains("not found"),
        "fail-closed missing-blob check still applies: {message}"
    );
    let task = read_task(&fx.tasks_dir, &id);
    assert!(
        task.references.iter().all(|r| r.attachment.is_none()),
        "{:?}",
        task.references
    );
}

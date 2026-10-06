//! DEV-92: soft deletion lifecycle for backend surfaces.
//!
//! Covers:
//! - default soft deletion: in-place `deleted_at` tombstone, `modified`
//!   unchanged, history appended, file preserved
//! - idempotency: repeated soft delete and restore-of-active are no-ops
//! - restore: clears the tombstone, validates current config, `modified`
//!   unchanged, history appended
//! - hard deletion: physical file removal, retained attachment blobs,
//!   warnings with stored paths and incoming relationships
//! - visibility: default get/list hide tombstones; `include_deleted` and
//!   `deletion` filters are strict
//! - mutation guards: update/comment/reference mutations refuse tombstones
//! - identity: soft-deleted numeric IDs stay reserved (no reuse), legacy
//!   bool delete contract preserved
//! - attachment reachability and sprint analytics treat tombstones per the
//!   DEV-92 contract

use lotar::api_server::{ApiServer, HttpRequest};
use lotar::routes;
use lotar::services::attachment_service::AttachmentService;
use lotar::services::sprint_service::SprintService;
use lotar::services::task_service::TaskService;
use lotar::storage::manager::Storage;
use serde_json::{Value, json};
use std::collections::HashMap;
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

struct Dev92Fixture {
    _tmp: tempfile::TempDir,
    tasks_dir: std::path::PathBuf,
    _guard_tasks: EnvVarGuard,
    _guard_fast: EnvVarGuard,
}

fn isolated_workspace() -> Dev92Fixture {
    let _guard_fast = EnvVarGuard::set("LOTAR_TEST_FAST_IO", "1");
    let tmp = tempfile::tempdir().unwrap();
    let tasks_dir = tmp.path().join(".tasks");
    std::fs::create_dir_all(&tasks_dir).unwrap();
    let _guard_tasks = EnvVarGuard::set("LOTAR_TASKS_DIR", &tasks_dir.to_string_lossy());
    Dev92Fixture {
        _tmp: tmp,
        tasks_dir,
        _guard_tasks,
        _guard_fast,
    }
}

fn server() -> ApiServer {
    let mut api = ApiServer::new();
    routes::initialize(&mut api);
    api
}

fn body_of(resp: &lotar::api_server::HttpResponse) -> Value {
    serde_json::from_slice(&resp.body).unwrap()
}

fn rest_create(api: &ApiServer, project: &str, title: &str) -> String {
    let resp = api.handle_request(&mk_req(
        "POST",
        "/api/tasks/add",
        &[],
        json!({"title": title, "project": project}),
    ));
    assert_eq!(resp.status, 201, "create in {project}");
    body_of(&resp)["data"]["id"].as_str().unwrap().to_string()
}

fn task_path(fx: &Dev92Fixture, id: &str) -> std::path::PathBuf {
    let (project, number) = id.rsplit_once('-').expect("canonical id");
    fx.tasks_dir.join(project).join(format!("{number}.yml"))
}

fn yaml_of(fx: &Dev92Fixture, id: &str) -> String {
    std::fs::read_to_string(task_path(fx, id)).expect("task file present")
}

/// `modified` of the stored YAML: None when the field is absent (fresh
/// tasks serialize without it). Lifecycle transitions must preserve the
/// exact previous state, absence included.
fn modified_of(yaml: &str) -> Option<String> {
    yaml.lines()
        .find_map(|line| line.strip_prefix("modified:"))
        .map(|value| value.trim().to_string())
}

/// Raw stored task (including tombstones) straight from storage.
fn stored_task(fx: &Dev92Fixture, id: &str) -> lotar::storage::task::Task {
    let storage = Storage::try_open(&fx.tasks_dir).unwrap();
    let (project, _) = id.rsplit_once('-').expect("canonical id");
    storage.get(id, project).expect("stored task")
}

#[test]
fn soft_delete_defaults_to_tombstone_with_unchanged_modified() {
    let fx = isolated_workspace();
    let api = server();
    let id = rest_create(&api, "TP", "Soft target");
    assert_eq!(id, "TP-1");

    // Give the task a non-empty modified timestamp via a normal mutation so
    // the preserved-modified property is non-vacuous.
    let resp = api.handle_request(&mk_req(
        "POST",
        "/api/tasks/update",
        &[],
        json!({"id": id, "priority": "Low"}),
    ));
    assert_eq!(resp.status, 200);

    let before = yaml_of(&fx, &id);
    let before_modified = modified_of(&before).expect("update sets modified");
    let resp = api.handle_request(&mk_req("POST", "/api/tasks/delete", &[], json!({"id": id})));
    assert_eq!(resp.status, 200, "soft delete succeeds");
    let data = body_of(&resp)["data"].clone();
    assert_eq!(data["deleted"], json!(true));
    assert_eq!(data["hard"], json!(false));
    assert_eq!(data["warnings"], json!([]));

    // In-place tombstone: same file, deleted_at present, sprints/content
    // preserved, modified untouched, history entry appended.
    let after = yaml_of(&fx, &id);
    assert!(after.contains("deleted_at:"), "{after}");
    assert!(!after.contains("deleted_at: null"), "{after}");
    assert_eq!(modified_of(&after), Some(before_modified));
    assert!(after.contains("title: Soft target"), "{after}");
    assert!(after.contains("field: deleted"), "history entry: {after}");
    assert!(task_path(&fx, &id).exists(), "file preserved");

    // Default get hides the tombstone; strict include_deleted reveals it.
    let resp = api.handle_request(&mk_req(
        "GET",
        "/api/tasks/get",
        &[("id", id.as_str())],
        json!({}),
    ));
    assert_eq!(resp.status, 404, "default get hides deleted");
    let resp = api.handle_request(&mk_req(
        "GET",
        "/api/tasks/get",
        &[("id", id.as_str()), ("include_deleted", "true")],
        json!({}),
    ));
    assert_eq!(resp.status, 200, "include_deleted reveals tombstone");
    let data = body_of(&resp)["data"].clone();
    assert!(data["deleted_at"].is_string());
    // Strict boolean grammar: anything but true/false is a 400.
    let resp = api.handle_request(&mk_req(
        "GET",
        "/api/tasks/get",
        &[("id", id.as_str()), ("include_deleted", "yes")],
        json!({}),
    ));
    assert_eq!(resp.status, 400);
}

#[test]
fn soft_delete_and_restore_are_idempotent() {
    let fx = isolated_workspace();
    let api = server();
    let id = rest_create(&api, "TP", "Idempotent target");

    let resp = api.handle_request(&mk_req("POST", "/api/tasks/delete", &[], json!({"id": id})));
    assert_eq!(resp.status, 200);
    let once = yaml_of(&fx, &id);

    // Repeated soft delete: deleted stays true, bytes unchanged.
    let resp = api.handle_request(&mk_req("POST", "/api/tasks/delete", &[], json!({"id": id})));
    assert_eq!(resp.status, 200);
    assert_eq!(body_of(&resp)["data"]["deleted"], json!(true));
    assert_eq!(yaml_of(&fx, &id), once, "repeat delete must not rewrite");

    // Restore: history entry appended, modified still the pre-delete value.
    let created_modified = modified_of(&yaml_of(&fx, &id));
    let resp = api.handle_request(&mk_req(
        "POST",
        "/api/tasks/restore",
        &[],
        json!({"id": id}),
    ));
    assert_eq!(resp.status, 200, "restore succeeds");
    let data = body_of(&resp)["data"].clone();
    assert!(data.get("deleted_at").is_none() || data["deleted_at"].is_null());
    let yaml = yaml_of(&fx, &id);
    assert!(!yaml.contains("deleted_at:"), "{yaml}");
    assert!(yaml.contains("field: restored"), "{yaml}");
    assert_eq!(modified_of(&yaml), created_modified);

    // Restore of an active task: no-op, no new history, no rewrite.
    let before_noop = yaml.clone();
    let resp = api.handle_request(&mk_req(
        "POST",
        "/api/tasks/restore",
        &[],
        json!({"id": id}),
    ));
    assert_eq!(resp.status, 200, "restore of active task is a no-op");
    assert_eq!(
        yaml_of(&fx, &id),
        before_noop,
        "no rewrite on no-op restore"
    );

    // Missing tasks are 404 on both surfaces.
    let resp = api.handle_request(&mk_req(
        "POST",
        "/api/tasks/delete",
        &[],
        json!({"id": "TP-99"}),
    ));
    assert_eq!(resp.status, 404);
    let resp = api.handle_request(&mk_req(
        "POST",
        "/api/tasks/restore",
        &[],
        json!({"id": "TP-99"}),
    ));
    assert_eq!(resp.status, 404);
}

#[test]
fn deletion_filter_is_strict_and_shared_by_list_and_export() {
    let _fx = isolated_workspace();
    let api = server();
    let active = rest_create(&api, "TP", "Active task");
    let deleted = rest_create(&api, "TP", "Doomed task");
    let resp = api.handle_request(&mk_req(
        "POST",
        "/api/tasks/delete",
        &[],
        json!({"id": deleted}),
    ));
    assert_eq!(resp.status, 200);

    let ids = |value: &Value| -> Vec<String> {
        value["data"]["tasks"]
            .as_array()
            .expect("tasks array")
            .iter()
            .map(|t| t["id"].as_str().unwrap().to_string())
            .collect()
    };
    let resp = api.handle_request(&mk_req("GET", "/api/tasks/list", &[], json!({})));
    assert_eq!(resp.status, 200);
    assert_eq!(ids(&body_of(&resp)), vec![active.clone()]);

    let resp = api.handle_request(&mk_req(
        "GET",
        "/api/tasks/list",
        &[("deletion", "deleted")],
        json!({}),
    ));
    assert_eq!(resp.status, 200);
    assert_eq!(ids(&body_of(&resp)), vec![deleted.clone()]);

    let resp = api.handle_request(&mk_req(
        "GET",
        "/api/tasks/list",
        &[("deletion", "all")],
        json!({}),
    ));
    assert_eq!(resp.status, 200);
    assert_eq!(ids(&body_of(&resp)).len(), 2);

    // Strict grammar: invalid values are 400, and `deletion` is reserved so
    // it never falls through to custom-field filtering.
    for bad in ["Deleted", "tombstone", "active "] {
        let resp = api.handle_request(&mk_req(
            "GET",
            "/api/tasks/list",
            &[("deletion", bad)],
            json!({}),
        ));
        assert_eq!(resp.status, 400, "deletion={bad:?} must be rejected");
    }

    // Export accepts the same reserved key.
    let resp = api.handle_request(&mk_req(
        "GET",
        "/api/tasks/export",
        &[("deletion", "deleted")],
        json!({}),
    ));
    assert_eq!(resp.status, 200, "export honors deletion=deleted");
    let export = String::from_utf8_lossy(&resp.body).to_string();
    assert!(export.contains("Doomed task"), "{export}");
    assert!(!export.contains("Active task"), "{export}");
}

#[test]
fn hard_delete_removes_file_preserves_blob_and_warns() {
    let fx = isolated_workspace();
    let api = server();
    let target = rest_create(&api, "TP", "Hard target");
    let dependent = rest_create(&api, "TP", "Dependent task");

    // Managed attachment on the target plus an incoming relationship.
    let mut storage = Storage::try_open(&fx.tasks_dir).unwrap();
    let (name, _created) =
        AttachmentService::store_bytes(&fx.tasks_dir.join("@attachments"), "notes.txt", b"blob")
            .unwrap();
    let _attach =
        AttachmentService::attach_managed_reference(&mut storage, &target, &name).unwrap();
    let update = lotar::api_types::TaskUpdate {
        relationships: Some(lotar::types::TaskRelationships {
            depends_on: vec![target.clone()],
            ..Default::default()
        }),
        ..Default::default()
    };
    TaskService::update(&mut storage, &dependent, update).unwrap();
    drop(storage);

    // Soft-delete first, then preview and execute hard deletion.
    let resp = api.handle_request(&mk_req(
        "POST",
        "/api/tasks/delete",
        &[],
        json!({"id": target}),
    ));
    assert_eq!(resp.status, 200);

    let storage = Storage::try_open(&fx.tasks_dir).unwrap();
    let preview = TaskService::preview_delete(&storage, &target, None, true).unwrap();
    assert!(!preview.deleted, "preview mutates nothing");
    assert!(!preview.warnings.is_empty());
    drop(storage);

    let resp = api.handle_request(&mk_req(
        "POST",
        "/api/tasks/delete",
        &[],
        json!({"id": target, "hard": true}),
    ));
    assert_eq!(resp.status, 200, "hard delete on tombstone");
    let data = body_of(&resp)["data"].clone();
    assert_eq!(data["deleted"], json!(true));
    assert_eq!(data["hard"], json!(true));
    let joined = data["warnings"]
        .as_array()
        .unwrap()
        .iter()
        .map(|w| w.as_str().unwrap())
        .collect::<Vec<_>>()
        .join("\n");
    assert!(
        joined.contains(&name),
        "warnings include stored name: {joined}"
    );
    assert!(
        joined.contains("@attachments"),
        "warnings include stored path: {joined}"
    );
    assert!(
        joined.contains(&dependent),
        "warnings include dependent canonical id: {joined}"
    );
    assert!(
        joined.contains("depends_on"),
        "warnings name the relationship field: {joined}"
    );

    // Physical file gone, blob retained, dependent untouched.
    assert!(!task_path(&fx, &target).exists(), "file physically removed");
    assert!(
        fx.tasks_dir.join("@attachments").join(&name).exists(),
        "blob preserved"
    );
    let dependent_task = stored_task(&fx, &dependent);
    assert_eq!(
        dependent_task.relationships.depends_on,
        vec![target.clone()]
    );

    // Hard delete of a missing task is 404.
    let resp = api.handle_request(&mk_req(
        "POST",
        "/api/tasks/delete",
        &[],
        json!({"id": "TP-77", "hard": true}),
    ));
    assert_eq!(resp.status, 404);
}

#[test]
fn mutation_guards_reject_deleted_until_restored() {
    let fx = isolated_workspace();
    let api = server();
    let id = rest_create(&api, "TP", "Guarded task");
    let resp = api.handle_request(&mk_req("POST", "/api/tasks/delete", &[], json!({"id": id})));
    assert_eq!(resp.status, 200);

    let update = json!({"id": id, "priority": "Low"});
    let resp = api.handle_request(&mk_req("POST", "/api/tasks/update", &[], update.clone()));
    assert_eq!(resp.status, 400, "update refuses tombstone");
    assert!(String::from_utf8_lossy(&resp.body).contains("restore"));

    let resp = api.handle_request(&mk_req(
        "POST",
        "/api/tasks/comment",
        &[],
        json!({"id": id, "text": "should fail"}),
    ));
    assert_eq!(resp.status, 400, "comment refuses tombstone");

    let mut storage = Storage::try_open(&fx.tasks_dir).unwrap();
    let err = AttachmentService::attach_managed_reference(&mut storage, &id, "whatever.<hash>")
        .unwrap_err();
    assert!(err.to_string().contains("restore"), "{err}");
    drop(storage);

    // After restore the same mutations succeed.
    let resp = api.handle_request(&mk_req(
        "POST",
        "/api/tasks/restore",
        &[],
        json!({"id": id}),
    ));
    assert_eq!(resp.status, 200);
    let resp = api.handle_request(&mk_req("POST", "/api/tasks/update", &[], update));
    assert_eq!(resp.status, 200, "update works after restore");
}

#[test]
fn restore_validates_current_config_without_mutation() {
    let fx = isolated_workspace();
    let api = server();
    let id = rest_create(&api, "TP", "Stale status task");
    let resp = api.handle_request(&mk_req("POST", "/api/tasks/delete", &[], json!({"id": id})));
    assert_eq!(resp.status, 200);

    // Shrink the project's status vocabulary so the stored status is now
    // invalid; restore must fail closed without clearing the tombstone.
    std::fs::write(
        fx.tasks_dir.join("TP").join("config.yml"),
        "issue_states: [Ready, Doing, Finished]\n",
    )
    .unwrap();
    let yaml_before = yaml_of(&fx, &id);
    let resp = api.handle_request(&mk_req(
        "POST",
        "/api/tasks/restore",
        &[],
        json!({"id": id}),
    ));
    assert_eq!(resp.status, 400, "restore blocked by current config");
    assert!(
        String::from_utf8_lossy(&resp.body).contains("status"),
        "error names the offending field"
    );
    assert_eq!(yaml_of(&fx, &id), yaml_before, "no mutation on failure");

    // Widening the vocabulary again unblocks the restore.
    std::fs::write(
        fx.tasks_dir.join("TP").join("config.yml"),
        "issue_states: [Todo, InProgress, Done, Ready, Doing, Finished]\n",
    )
    .unwrap();
    let resp = api.handle_request(&mk_req(
        "POST",
        "/api/tasks/restore",
        &[],
        json!({"id": id}),
    ));
    assert_eq!(resp.status, 200, "restore succeeds once config allows it");
    assert!(stored_task(&fx, &id).deleted_at.is_none());
}

#[test]
fn soft_deleted_ids_stay_reserved() {
    let _fx = isolated_workspace();
    let api = server();
    let first = rest_create(&api, "TP", "one");
    rest_create(&api, "TP", "two");
    let third = rest_create(&api, "TP", "three");
    assert_eq!((first.as_str(), third.as_str()), ("TP-1", "TP-3"));

    let resp = api.handle_request(&mk_req(
        "POST",
        "/api/tasks/delete",
        &[],
        json!({"id": third}),
    ));
    assert_eq!(resp.status, 200);
    let fourth = rest_create(&api, "TP", "four");
    assert_eq!(fourth, "TP-4", "tombstone reserves its numeric id");

    // Hard deletion frees the highest id again (pre-existing max+1 behavior).
    let resp = api.handle_request(&mk_req(
        "POST",
        "/api/tasks/delete",
        &[],
        json!({"id": fourth, "hard": true}),
    ));
    assert_eq!(resp.status, 200);
    let again = rest_create(&api, "TP", "four again");
    assert_eq!(again, "TP-4", "physical removal restores max+1 reuse");
}

#[test]
fn legacy_service_delete_keeps_bool_contract() {
    let fx = isolated_workspace();
    let api = server();
    let id = rest_create(&api, "TP", "legacy target");

    let mut storage = Storage::try_open(&fx.tasks_dir).unwrap();
    assert!(!TaskService::delete(&mut storage, "TP-99", None).unwrap());
    assert!(TaskService::delete(&mut storage, &id, None).unwrap());
    assert!(task_path(&fx, &id).exists(), "legacy delete is soft now");
    assert!(stored_task(&fx, &id).deleted_at.is_some());
    // Repeated legacy delete stays true and idempotent.
    assert!(TaskService::delete(&mut storage, &id, None).unwrap());
    // Cross-project refusal stays an error.
    let err = TaskService::delete(&mut storage, "TP-1", Some("OTHER")).unwrap_err();
    assert!(
        err.to_string().contains("refusing to cross projects"),
        "{err}"
    );
}

#[test]
fn attachment_reachability_counts_tombstones() {
    let fx = isolated_workspace();
    let api = server();
    let id = rest_create(&api, "TP", "attachment holder");
    let mut storage = Storage::try_open(&fx.tasks_dir).unwrap();
    let (name, _created) =
        AttachmentService::store_bytes(&fx.tasks_dir.join("@attachments"), "doc.txt", b"shared")
            .unwrap();
    let _attach = AttachmentService::attach_managed_reference(&mut storage, &id, &name).unwrap();
    let hash = AttachmentService::extract_hash_tag(&name).unwrap();
    let root = fx.tasks_dir.join("@attachments");
    assert!(AttachmentService::is_hash_referenced(
        &storage, &root, &hash
    ));

    // Soft delete keeps the blob reachable through the tombstone.
    TaskService::delete(&mut storage, &id, None).unwrap();
    assert!(
        AttachmentService::is_hash_referenced(&storage, &root, &hash),
        "tombstone still references its blob"
    );

    // Hard delete releases the reference; the blob file itself remains
    // untouched by deletion.
    TaskService::delete_with_options(&mut storage, &id, None, true).unwrap();
    assert!(!AttachmentService::is_hash_referenced(
        &storage, &root, &hash
    ));
    assert!(root.join(&name).exists(), "hard delete never removes blobs");
}

#[test]
fn sprint_analytics_exclude_deleted_but_integrity_scans_include_them() {
    let fx = isolated_workspace();
    let api = server();
    let keep = rest_create(&api, "TP", "active member");
    let drop = rest_create(&api, "TP", "deleted member");

    let resp = api.handle_request(&mk_req(
        "POST",
        "/api/sprints/create",
        &[],
        json!({"name": "Dev92 sprint"}),
    ));
    assert_eq!(resp.status, 200);
    let sprint_id = body_of(&resp)["data"]["sprint"]["id"].as_u64().unwrap() as u32;

    let mut storage = Storage::try_open(&fx.tasks_dir).unwrap();
    for member in [&keep, &drop] {
        TaskService::update(
            &mut storage,
            member,
            lotar::api_types::TaskUpdate {
                sprints: Some(vec![sprint_id]),
                ..Default::default()
            },
        )
        .unwrap();
    }
    TaskService::delete(&mut storage, &drop, None).unwrap();

    let records = SprintService::list(&storage).unwrap();
    let record = records.iter().find(|r| r.id == sprint_id).unwrap();
    let loaded = SprintService::load_tasks_for_record(&storage, record);
    assert_eq!(
        loaded.iter().map(|(id, _)| id.as_str()).collect::<Vec<_>>(),
        vec![keep.as_str()],
        "sprint analytics are active-only"
    );

    // Tombstones keep their sprint memberships on disk (membership lives in
    // the @sprints record, not the task YAML) and integrity scans still
    // observe the tombstone.
    let stored = stored_task(&fx, &drop);
    assert!(stored.deleted_at.is_some());
    assert!(
        record
            .sprint
            .tasks
            .iter()
            .any(|entry| entry.id.trim() == drop),
        "tombstone keeps its sprint membership entry"
    );
    let report = lotar::services::sprint_integrity::detect_missing_sprints(&storage, &records);
    assert_eq!(
        report.scanned_tasks, 2,
        "integrity scans include tombstones"
    );
}

#[test]
fn restore_rejects_unreadable_configuration_without_clearing_tombstone() {
    let fx = isolated_workspace();
    let api = server();
    let id = rest_create(&api, "TP", "restore config failure");
    let mut storage = Storage::try_open(&fx.tasks_dir).unwrap();
    TaskService::delete(&mut storage, &id, None).unwrap();
    let before = yaml_of(&fx, &id);
    std::fs::write(fx.tasks_dir.join("config.yml"), "issue: [\n").unwrap();
    let error = TaskService::restore(&mut storage, &id, None).unwrap_err();
    assert!(error.to_string().contains("invalid current configuration"));
    assert_eq!(yaml_of(&fx, &id), before);
}

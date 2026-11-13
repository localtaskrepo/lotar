//! DEV-301: `/api/tasks/update` takes an optional `expected_modified`
//! precondition; a task changed since then answers 409 and writes nothing.

use crate::common;
use common::env_mutex::EnvVarGuard;
use lotar::api_server::{ApiServer, HttpRequest};
use lotar::api_types::{TaskCreate, TaskUpdate};
use lotar::services::task_service::TaskService;
use std::collections::HashMap;

fn post(server: &ApiServer, body: serde_json::Value) -> (u16, serde_json::Value) {
    let response = server.handle_request(&HttpRequest {
        method: "POST".to_string(),
        path: "/api/tasks/update".to_string(),
        query: HashMap::new(),
        headers: HashMap::from([("content-type".to_string(), "application/json".to_string())]),
        body: serde_json::to_vec(&body).unwrap(),
    });
    let value = serde_json::from_slice(&response.body).unwrap_or(serde_json::Value::Null);
    (response.status, value)
}

#[test]
fn stale_expected_modified_is_a_conflict() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path().join(".tasks");
    std::fs::create_dir_all(&root).unwrap();
    let _scope = EnvVarGuard::set("LOTAR_TASKS_DIR", &root.to_string_lossy());
    let _home = EnvVarGuard::set("LOTAR_IGNORE_HOME_CONFIG", "1");

    let mut storage = lotar::storage::manager::Storage::new(&root);
    let created = TaskService::create(&mut storage, TaskCreate { title: "Original".into(), project: Some("ST".into()), ..Default::default() }).unwrap();
    let loaded = created.modified.clone();

    // Someone else edits the task after the caller loaded it.
    std::thread::sleep(std::time::Duration::from_millis(5));
    let outside = TaskService::update(&mut storage, &created.id, TaskUpdate { description: Some("Agent text".into()), ..Default::default() }).unwrap();
    assert_ne!(outside.modified, loaded);

    let mut server = ApiServer::new();
    lotar::routes::initialize(&mut server);

    let (status, body) = post(&server, serde_json::json!({"id": created.id, "description": "Mine", "expected_modified": loaded}));
    assert_eq!(status, 409, "{body}");
    assert_eq!(body["error"]["code"], "CONFLICT", "{body}");
    let stored = TaskService::get(&storage, &created.id, None).unwrap();
    assert_eq!(stored.description.as_deref(), Some("Agent text"));

    // With the current value the update goes through.
    let (status, body) = post(&server, serde_json::json!({"id": created.id, "title": "Renamed", "expected_modified": outside.modified}));
    assert_eq!(status, 200, "{body}");
    assert_eq!(body["data"]["title"], "Renamed");
    assert_eq!(body["data"]["description"], "Agent text");

    // Without a precondition nothing changes for existing callers.
    let (status, _) = post(&server, serde_json::json!({"id": created.id, "title": "Again"}));
    assert_eq!(status, 200);

    let (status, body) = post(&server, serde_json::json!({"id": created.id, "title": "x", "expected_modified": 5}));
    assert_eq!(status, 400, "{body}");

    // A hand-written task without `modified` shows `created` instead, and
    // that value is what the precondition compares (smoke caught this).
    std::fs::write(root.join("ST").join("7.yml"), "title: Hand written\nstatus: Todo\npriority: Medium\ntype: Feature\ncreated: 2026-01-01T00:00:00Z\n")
        .unwrap();
    let shown = TaskService::get(&storage, "ST-7", None).unwrap();
    assert_eq!(shown.modified, "2026-01-01T00:00:00Z");
    let (status, body) = post(&server, serde_json::json!({"id": "ST-7", "title": "Edited", "expected_modified": shown.modified}));
    assert_eq!(status, 200, "{body}");

    // Other formats are compared as written.
    std::fs::write(root.join("ST").join("8.yml"), "title: Dated\nstatus: Todo\npriority: Medium\ntype: Feature\ncreated: 2026-01-01\nmodified: 2026-02-01\n")
        .unwrap();
    let (status, body) = post(&server, serde_json::json!({"id": "ST-8", "title": "Edited", "expected_modified": "2026-02-01"}));
    assert_eq!(status, 200, "{body}");
    let (status, _) = post(&server, serde_json::json!({"id": "ST-8", "title": "Again", "expected_modified": "2026-02-01"}));
    assert_eq!(status, 409);
}

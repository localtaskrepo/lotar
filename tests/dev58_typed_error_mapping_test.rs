//! DEV-58: typed REST error mapping over the public HTTP surface.
//!
//! Table-driven negative cases for task create/get/update/delete/status/
//! comments and sprint summary/burndown/update/delete: invalid inputs stay
//! 400 INVALID_ARGUMENT, missing resources are 404 NOT_FOUND, and storage/
//! serialization faults are 500 INTERNAL — each preserving its error code
//! and message. Fixtures are isolated temporary workspaces driven through
//! the in-process `ApiServer`; failures are made deterministic (malformed
//! YAML, a directory in place of a task file, a held storage lock, or a
//! pending transaction journal) with no sleeps, retries, or weakened
//! bounds.

mod common;

use common::env_mutex::EnvVarGuard;
use lotar::api_server::{ApiServer, HttpRequest, HttpResponse};
use lotar::routes;
use serde_json::{Value, json};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

struct Dev58Fixture {
    _tmp: tempfile::TempDir,
    tasks_dir: PathBuf,
    _guard: EnvVarGuard,
}

fn isolated_workspace() -> Dev58Fixture {
    let tmp = tempfile::tempdir_in(Path::new(env!("CARGO_TARGET_TMPDIR"))).unwrap();
    let tasks_dir = tmp.path().join("ws").join(".tasks");
    std::fs::create_dir_all(&tasks_dir).unwrap();
    let _guard = EnvVarGuard::set("LOTAR_TASKS_DIR", &tasks_dir.to_string_lossy());
    Dev58Fixture {
        _tmp: tmp,
        tasks_dir,
        _guard,
    }
}

fn server() -> ApiServer {
    let mut api = ApiServer::new();
    routes::initialize(&mut api);
    api
}

fn req(method: &str, path: &str, body: Value) -> HttpRequest {
    req_query(method, path, &[], body)
}

fn req_query(method: &str, path: &str, query: &[(&str, &str)], body: Value) -> HttpRequest {
    let mut q = HashMap::new();
    for (key, value) in query {
        q.insert((*key).to_string(), (*value).to_string());
    }
    HttpRequest {
        method: method.to_string(),
        path: path.to_string(),
        query: q,
        headers: HashMap::new(),
        body: serde_json::to_vec(&body).unwrap(),
    }
}

fn body_of(resp: &HttpResponse) -> Value {
    serde_json::from_slice(&resp.body).unwrap()
}

fn create_task(api: &ApiServer, project: &str, title: &str) -> String {
    let resp = api.handle_request(&req(
        "POST",
        "/api/tasks/add",
        json!({"title": title, "project": project}),
    ));
    assert_eq!(
        resp.status,
        201,
        "task create failed: {}",
        String::from_utf8_lossy(&resp.body)
    );
    body_of(&resp)["data"]["id"].as_str().unwrap().to_string()
}

/// A workspace with one task (TP-1) and one sprint (#1) to mutate.
fn populated_workspace() -> Dev58Fixture {
    let fixture = isolated_workspace();
    let api = server();
    create_task(&api, "TP", "seed");
    let resp = api.handle_request(&req("POST", "/api/sprints/create", json!({"label": "S1"})));
    assert_eq!(
        resp.status,
        200,
        "sprint create failed: {}",
        String::from_utf8_lossy(&resp.body)
    );
    fixture
}

/// Assert the canonical error envelope: status, code, and message fragment.
fn assert_error(resp: &HttpResponse, status: u16, code: &str, message_part: &str) {
    assert_eq!(
        resp.status,
        status,
        "status: {}",
        String::from_utf8_lossy(&resp.body)
    );
    let body = body_of(resp);
    assert_eq!(body["error"]["code"].as_str(), Some(code), "code: {}", body);
    let message = body["error"]["message"].as_str().unwrap_or_default();
    assert!(
        message.contains(message_part),
        "message {message:?} must contain {message_part:?}"
    );
}

/// Plain validation/lookup cases over a standard populated workspace.
/// Each row: (name, request, expected status, expected code, message part).
#[test]
fn typed_negative_cases_preserve_code_and_message() {
    let _fixture = populated_workspace();
    let api = server();

    let cases: &[(&str, HttpRequest, u16, &str, &str)] = &[
        // Task create: input validation stays 400.
        (
            "create missing title",
            req("POST", "/api/tasks/add", json!({"project": "TP"})),
            400,
            "INVALID_ARGUMENT",
            "Missing required field: title",
        ),
        (
            "create sprint id zero",
            req(
                "POST",
                "/api/tasks/add",
                json!({"title": "x", "sprints": [0]}),
            ),
            400,
            "INVALID_ARGUMENT",
            "sprints must be an array of positive integers",
        ),
        (
            "create sprint id syntax",
            req(
                "POST",
                "/api/tasks/add",
                json!({"title": "x", "sprints": ["soon"]}),
            ),
            400,
            "INVALID_ARGUMENT",
            "sprints must be an array of positive integers",
        ),
        // Task create referencing a missing sprint: typed NOT_FOUND.
        (
            "create unknown sprint",
            req(
                "POST",
                "/api/tasks/add",
                json!({"title": "x", "sprints": [424242]}),
            ),
            404,
            "NOT_FOUND",
            "Sprint not found: 424242",
        ),
        // Task get.
        (
            "get missing id param",
            req_query("GET", "/api/tasks/get", &[("id", "")], json!({})),
            400,
            "INVALID_ARGUMENT",
            "Missing id",
        ),
        (
            "get invalid id syntax",
            req_query("GET", "/api/tasks/get", &[("id", "not-an-id")], json!({})),
            400,
            "INVALID_ARGUMENT",
            "Invalid task ID",
        ),
        (
            "get unknown id",
            req_query("GET", "/api/tasks/get", &[("id", "TP-999")], json!({})),
            404,
            "NOT_FOUND",
            "Task not found",
        ),
        (
            "get invalid lifecycle flag",
            req_query(
                "GET",
                "/api/tasks/get",
                &[("id", "TP-1"), ("include_deleted", "maybe")],
                json!({}),
            ),
            400,
            "INVALID_ARGUMENT",
            "Invalid include_deleted value",
        ),
        // Task update and status.
        (
            "update missing id",
            req("POST", "/api/tasks/update", json!({"title": "x"})),
            400,
            "INVALID_ARGUMENT",
            "Missing id",
        ),
        (
            "update unknown id",
            req(
                "POST",
                "/api/tasks/update",
                json!({"id": "TP-999", "title": "x"}),
            ),
            404,
            "NOT_FOUND",
            "Task not found",
        ),
        (
            "status missing status",
            req("POST", "/api/tasks/status", json!({"id": "TP-1"})),
            400,
            "INVALID_ARGUMENT",
            "Missing status",
        ),
        (
            "status invalid value",
            req(
                "POST",
                "/api/tasks/status",
                json!({"id": "TP-1", "status": "Bogus"}),
            ),
            400,
            "INVALID_ARGUMENT",
            "Bogus",
        ),
        (
            "status unknown id",
            req(
                "POST",
                "/api/tasks/status",
                json!({"id": "TP-999", "status": "Todo"}),
            ),
            404,
            "NOT_FOUND",
            "Task not found",
        ),
        // Comments.
        (
            "comment unknown id",
            req(
                "POST",
                "/api/tasks/comment",
                json!({"id": "TP-999", "text": "hi"}),
            ),
            404,
            "NOT_FOUND",
            "Task not found",
        ),
        (
            "comment empty text",
            req(
                "POST",
                "/api/tasks/comment",
                json!({"id": "TP-1", "text": ""}),
            ),
            400,
            "INVALID_ARGUMENT",
            "Missing text",
        ),
        (
            "comment update out-of-range index",
            req(
                "POST",
                "/api/tasks/comment/update",
                json!({"id": "TP-1", "index": 9, "text": "x"}),
            ),
            400,
            "INVALID_ARGUMENT",
            "",
        ),
        // Delete and restore.
        (
            "delete invalid id syntax",
            req("POST", "/api/tasks/delete", json!({"id": "!!"})),
            400,
            "INVALID_ARGUMENT",
            "",
        ),
        (
            "delete unknown id",
            req("POST", "/api/tasks/delete", json!({"id": "TP-999"})),
            404,
            "NOT_FOUND",
            "Task not found",
        ),
        (
            "restore unknown id",
            req("POST", "/api/tasks/restore", json!({"id": "TP-999"})),
            404,
            "NOT_FOUND",
            "Task not found",
        ),
        // Sprint summary and burndown.
        (
            "summary sprint zero",
            req_query("GET", "/api/sprints/summary", &[("sprint", "0")], json!({})),
            400,
            "INVALID_ARGUMENT",
            "must be a positive integer",
        ),
        (
            "summary sprint syntax",
            req_query(
                "GET",
                "/api/sprints/summary",
                &[("sprint", "abc")],
                json!({}),
            ),
            400,
            "INVALID_ARGUMENT",
            "must be a positive integer",
        ),
        (
            "summary unknown sprint",
            req_query(
                "GET",
                "/api/sprints/summary",
                &[("sprint", "999")],
                json!({}),
            ),
            404,
            "NOT_FOUND",
            "Sprint not found: 999",
        ),
        (
            "burndown unknown sprint",
            req_query(
                "GET",
                "/api/sprints/burndown",
                &[("sprint", "999")],
                json!({}),
            ),
            404,
            "NOT_FOUND",
            "Sprint not found: 999",
        ),
        // Sprint update and delete.
        (
            "sprint update zero id",
            req(
                "POST",
                "/api/sprints/update",
                json!({"sprint": 0, "label": "Z"}),
            ),
            400,
            "INVALID_ARGUMENT",
            "Sprint identifier must be provided",
        ),
        (
            "sprint update unknown id",
            req(
                "POST",
                "/api/sprints/update",
                json!({"sprint": 9999, "label": "G"}),
            ),
            404,
            "NOT_FOUND",
            "Sprint not found: 9999",
        ),
        (
            "sprint delete unknown id",
            req("POST", "/api/sprints/delete", json!({"sprint": 9999})),
            404,
            "NOT_FOUND",
            "Sprint not found: 9999",
        ),
        // Attachment path resolution: typed invalid vs missing.
        (
            "attachment path escape",
            req_query(
                "GET",
                "/api/attachments/get",
                &[("path", "../outside.txt")],
                json!({}),
            ),
            400,
            "INVALID_ARGUMENT",
            "Invalid attachment path",
        ),
        (
            "attachment path missing blob",
            req_query(
                "GET",
                "/api/attachments/get",
                &[("path", "absent.abc123.txt")],
                json!({}),
            ),
            404,
            "NOT_FOUND",
            "Attachment not found",
        ),
    ];

    for (name, request, status, code, message_part) in cases {
        let resp = api.handle_request(request);
        assert_error(&resp, *status, code, message_part);
        // Every case carries the canonical envelope shape.
        let body = body_of(&resp);
        assert!(
            body["error"]["message"].is_string() && body["error"]["code"].is_string(),
            "{name}: envelope must be {{error: {{code, message}}}}, got {body}"
        );
    }
}

/// Malformed stored SPRINT YAML surfaces as 500 INTERNAL
/// (SerializationError) on the sprint read endpoints, with the diagnostic
/// preserved. (Malformed TASK YAML is deliberately skipped by identity
/// resolution and reads as a 404, not a server fault.)
#[test]
fn malformed_sprint_yaml_is_internal_on_summary_burndown_and_update() {
    let fixture = populated_workspace();
    std::fs::write(
        fixture.tasks_dir.join("@sprints").join("1.yml"),
        "plan: [unclosed\n\tbad: indent\n",
    )
    .unwrap();

    let api = server();
    for request in [
        req_query("GET", "/api/sprints/summary", &[("sprint", "1")], json!({})),
        req_query(
            "GET",
            "/api/sprints/burndown",
            &[("sprint", "1")],
            json!({}),
        ),
        req(
            "POST",
            "/api/sprints/update",
            json!({"sprint": 1, "label": "X"}),
        ),
    ] {
        let resp = api.handle_request(&request);
        assert_error(&resp, 500, "INTERNAL", "Serialization error");
    }
}

/// A genuine IO failure reading a sprint file (directory where the file
/// must be) is a 500 INTERNAL with the IO diagnostic preserved.
#[test]
fn unreadable_sprint_file_is_internal() {
    let fixture = populated_workspace();
    let path = fixture.tasks_dir.join("@sprints").join("1.yml");
    std::fs::remove_file(&path).unwrap();
    std::fs::create_dir(&path).unwrap();

    let api = server();
    let resp = api.handle_request(&req_query(
        "GET",
        "/api/sprints/summary",
        &[("sprint", "1")],
        json!({}),
    ));
    assert_error(&resp, 500, "INTERNAL", "IO error");
}

/// Lock contention on the task store stays a client-side 400 with the
/// production "still busy" diagnostics preserved (io WouldBlock, typed —
/// never matched on message text by the route). The comment pipeline is a
/// direct locked mutation, so the held lock fails it within the standard
/// bounded window.
#[test]
fn task_lock_contention_is_a_retryable_client_error() {
    let fixture = populated_workspace();
    let project_dir = fixture.tasks_dir.join("TP");
    let _held = lotar::storage::safety::acquire_storage_lock(&project_dir, "task").unwrap();

    let api = server();
    let resp = api.handle_request(&req(
        "POST",
        "/api/tasks/comment",
        json!({"id": "TP-1", "text": "blocked by lock"}),
    ));
    assert_error(&resp, 400, "INVALID_ARGUMENT", "still busy after 2 seconds");
}

/// A pending transaction journal refuses non-participating mutations as a
/// client-correctable 400 (typed journal marker) with the recovery hint
/// preserved.
#[test]
fn pending_journal_refusal_is_a_client_error() {
    let fixture = populated_workspace();
    std::fs::write(
        fixture.tasks_dir.join(".txn-pending.json"),
        "{\"version\":1}",
    )
    .unwrap();

    let api = server();
    let resp = api.handle_request(&req(
        "POST",
        "/api/tasks/comment",
        json!({"id": "TP-1", "text": "while journal pending"}),
    ));
    assert_error(
        &resp,
        400,
        "INVALID_ARGUMENT",
        "pending task/sprint transaction journal",
    );
}

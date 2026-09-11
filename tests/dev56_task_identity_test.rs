//! DEV-56: canonical task IDs and locations across storage roots.
//!
//! Public-boundary regression coverage for:
//! - hyphenated project prefixes (`ABC-OPS-12`) through CRUD, comments,
//!   status, sprints, references, attachments, REST and MCP
//! - padded numeric aliases (`TP-001` == `TP-1`)
//! - nested (sibling workspace) root discovery consistency between
//!   unfiltered, project-filtered, full-ID and numeric lookups
//! - fail-closed behavior for ambiguous and malformed identifiers
//! - explicit refusal (before side effects) of cross-root mutations

use lotar::api_server::{ApiServer, HttpRequest};
use lotar::routes;
use lotar::services::task_service::TaskService;
use lotar::storage::manager::Storage;
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

struct Dev56Fixture {
    _tmp: tempfile::TempDir,
    tasks_dir: PathBuf,
    _guard_tasks: EnvVarGuard,
    _guard_fast: EnvVarGuard,
}

fn isolated_workspace() -> Dev56Fixture {
    isolated_workspace_in(std::env::temp_dir().as_path())
}

/// Same hermetic workspace as [`isolated_workspace`], but rooted under `base`
/// instead of the process temp dir. Used by the MCP store-lock test to place
/// the workspace inside the checkout so repo-root discovery succeeds by
/// ancestry in sandboxes that forbid creating any `.git` marker.
fn isolated_workspace_in(base: &std::path::Path) -> Dev56Fixture {
    let _guard_fast = EnvVarGuard::set("LOTAR_TEST_FAST_IO", "1");
    let tmp = tempfile::tempdir_in(base).unwrap();
    let tasks_dir = tmp.path().join("main").join(".tasks");
    std::fs::create_dir_all(&tasks_dir).unwrap();
    let _guard_tasks = EnvVarGuard::set("LOTAR_TASKS_DIR", &tasks_dir.to_string_lossy());
    Dev56Fixture {
        _tmp: tmp,
        tasks_dir,
        _guard_tasks,
        _guard_fast,
    }
}

impl Dev56Fixture {
    /// Create a nested workspace tasks root discovered as a sibling of the
    /// primary one (a child of the primary workspace directory).
    fn sibling_tasks_dir(&self, name: &str) -> PathBuf {
        let workspace = self.tasks_dir.parent().unwrap().to_path_buf();
        let sibling = workspace.join(name).join(".tasks");
        std::fs::create_dir_all(&sibling).unwrap();
        sibling
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
    assert_eq!(resp.status, 201, "create in {project}: {}", resp.body.len());
    body_of(&resp)["data"]["id"].as_str().unwrap().to_string()
}

fn rest_get(api: &ApiServer, id: &str) -> lotar::api_server::HttpResponse {
    api.handle_request(&mk_req("GET", "/api/tasks/get", &[("id", id)], json!({})))
}

fn write_task_file(tasks_dir: &std::path::Path, project: &str, number: u64, title: &str) {
    let dir = tasks_dir.join(project);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(
        dir.join(format!("{number}.yml")),
        format!(
            "title: {title}\nstatus: Todo\npriority: Medium\ntype: Feature\ncreated: 2026-01-01T00:00:00Z\n"
        ),
    )
    .unwrap();
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
    serde_json::from_str(&resp_line).unwrap()
}

fn mcp_text(resp: &Value) -> Option<String> {
    resp.get("result")
        .and_then(|r| r.get("content"))
        .and_then(|c| c.as_array())
        .and_then(|items| items.first())
        .and_then(|item| item.get("text"))
        .and_then(|t| t.as_str())
        .map(|s| s.to_string())
}

// ---------------------------------------------------------------------------
// Hyphenated prefixes through public boundaries
// ---------------------------------------------------------------------------

#[test]
fn hyphenated_prefix_crud_comment_and_status_roundtrip() {
    let fx = isolated_workspace();
    let api = server();

    let id = rest_create(&api, "ABC-OPS", "Hyphenated identity");
    assert_eq!(id, "ABC-OPS-1", "first task numbered under ABC-OPS");
    assert!(
        fx.tasks_dir.join("ABC-OPS").join("1.yml").exists(),
        "file stored under the hyphenated project folder"
    );

    // Read back through REST with the full hyphenated ID.
    let resp = rest_get(&api, &id);
    assert_eq!(resp.status, 200, "get hyphenated id");
    assert_eq!(body_of(&resp)["data"]["title"], "Hyphenated identity");

    // Status transition.
    let resp = api.handle_request(&mk_req(
        "POST",
        "/api/tasks/status",
        &[],
        json!({"id": id, "status": "InProgress"}),
    ));
    assert_eq!(resp.status, 200, "status update on hyphenated id");
    let yaml = std::fs::read_to_string(fx.tasks_dir.join("ABC-OPS").join("1.yml")).unwrap();
    assert!(yaml.contains("status: InProgress"), "{yaml}");

    // Comment via REST.
    let resp = api.handle_request(&mk_req(
        "POST",
        "/api/tasks/comment",
        &[],
        json!({"id": id, "text": "comment on hyphenated id"}),
    ));
    assert_eq!(resp.status, 200, "comment on hyphenated id");

    // Delete via REST.
    let resp = api.handle_request(&mk_req("POST", "/api/tasks/delete", &[], json!({"id": id})));
    assert_eq!(resp.status, 200, "delete hyphenated id");
    assert!(!fx.tasks_dir.join("ABC-OPS").join("1.yml").exists());
}

#[test]
fn hyphenated_prefix_reference_and_sprint_boundaries() {
    let _fx = isolated_workspace();
    let api = server();

    let id = rest_create(&api, "ABC-OPS", "Refs and sprints");
    let id2 = rest_create(&api, "ABC-OPS", "Second task");

    // Link reference attach via REST.
    let resp = api.handle_request(&mk_req(
        "POST",
        "/api/tasks/references/link/add",
        &[],
        json!({"id": id, "url": "https://example.com/ops"}),
    ));
    assert_eq!(
        resp.status,
        200,
        "attach link reference: {}",
        resp.body.len()
    );
    assert!(
        body_of(&resp)["data"]["task"]["references"]
            .as_array()
            .is_some_and(|refs| !refs.is_empty()),
        "reference recorded"
    );

    // Attachment upload attaches a managed file reference to the task.
    let content = base64_encode(b"attachment-bytes");
    let resp = api.handle_request(&mk_req(
        "POST",
        "/api/tasks/attachments/upload",
        &[],
        json!({"id": id, "filename": "evidence.txt", "content_base64": content}),
    ));
    assert_eq!(resp.status, 200, "attachment upload: {}", resp.body.len());
    let upload = body_of(&resp);
    assert_eq!(upload["data"]["attached"], true, "attachment attached");
    let stored = upload["data"]["stored_path"].as_str().unwrap().to_string();
    let resp = api.handle_request(&mk_req(
        "POST",
        "/api/tasks/attachments/remove",
        &[],
        json!({"id": id, "stored_path": stored}),
    ));
    assert_eq!(resp.status, 200, "attachment remove: {}", resp.body.len());

    // Sprint create + assign the hyphenated task.
    let resp = api.handle_request(&mk_req(
        "POST",
        "/api/sprints/create",
        &[],
        json!({"name": "Ops sprint"}),
    ));
    assert_eq!(resp.status, 200, "create sprint: {}", resp.body.len());
    let resp = api.handle_request(&mk_req(
        "POST",
        "/api/sprints/add",
        &[],
        json!({"sprint": 1, "tasks": [id, id2], "allow_closed": true}),
    ));
    assert_eq!(
        resp.status,
        200,
        "sprint assign hyphenated ids: {}",
        String::from_utf8_lossy(&resp.body)
    );
    let modified = body_of(&resp)["data"]["modified"]
        .as_array()
        .cloned()
        .unwrap_or_default();
    assert!(
        modified.len() == 2,
        "both hyphenated ids assigned: {modified:?}"
    );

    // Sprint listing still resolves hyphenated members to their tasks.
    let resp = api.handle_request(&mk_req(
        "GET",
        "/api/sprints/list",
        &[("include_tasks", "true")],
        json!({}),
    ));
    assert_eq!(resp.status, 200, "sprint list");
}

#[test]
fn mcp_get_and_comment_support_hyphenated_ids() {
    let fx = isolated_workspace();
    let api = server();
    let id = rest_create(&api, "ABC-OPS", "MCP boundary");

    let resp = mcall("task_get", json!({"id": id}));
    assert!(resp.get("error").is_none(), "task_get: {resp}");
    let text = mcp_text(&resp).unwrap();
    let task: Value = serde_json::from_str(&text).unwrap();
    assert_eq!(task["id"], id);

    let resp = mcall("task_comment_add", json!({"id": id, "text": "via mcp"}));
    assert!(resp.get("error").is_none(), "task_comment_add: {resp}");

    let yaml = std::fs::read_to_string(fx.tasks_dir.join("ABC-OPS").join("1.yml")).unwrap();
    assert!(yaml.contains("via mcp"), "{yaml}");
}

// ---------------------------------------------------------------------------
// Padded aliases
// ---------------------------------------------------------------------------

#[test]
fn padded_alias_resolves_same_task_through_boundaries() {
    let fx = isolated_workspace();
    let api = server();
    let id = rest_create(&api, "TP", "Padded alias target");
    assert_eq!(id, "TP-1");

    let resp = rest_get(&api, "TP-001");
    assert_eq!(resp.status, 200, "padded alias resolves via REST");
    assert_eq!(body_of(&resp)["data"]["id"], "TP-1");

    let storage = Storage::try_open(&fx.tasks_dir).unwrap();
    let direct = storage.get("TP-001", "TP").expect("padded storage get");
    assert_eq!(direct.title, "Padded alias target");

    let numeric = storage.resolve_numeric_id("1").expect("numeric resolve");
    assert_eq!(numeric.0, "TP-1");
}

// ---------------------------------------------------------------------------
// Nested (sibling workspace) roots
// ---------------------------------------------------------------------------

#[test]
fn nested_root_discovery_is_consistent_across_query_shapes() {
    let fx = isolated_workspace();
    let api = server();
    // Primary-root task to prove cross-root mixing.
    rest_create(&api, "DEV", "primary root task");

    let sibling = fx.sibling_tasks_dir("sibling");
    write_task_file(&sibling, "NEST", 1, "NESTED-ROOT-MARKER");

    // Unfiltered list must discover nested-root tasks.
    let resp = api.handle_request(&mk_req("GET", "/api/tasks/list", &[], json!({})));
    assert_eq!(resp.status, 200);
    let ids: Vec<String> = body_of(&resp)["data"]["tasks"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|t| t["id"].as_str().map(|s| s.to_string()))
        .collect();
    assert!(
        ids.contains(&"NEST-1".to_string()),
        "unfiltered list finds nested task: {ids:?}"
    );
    assert!(ids.contains(&"DEV-1".to_string()), "primary tasks listed");

    // Project-filtered list finds it too.
    let resp = api.handle_request(&mk_req(
        "GET",
        "/api/tasks/list",
        &[("project", "NEST")],
        json!({}),
    ));
    let ids: Vec<String> = body_of(&resp)["data"]["tasks"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|t| t["id"].as_str().map(|s| s.to_string()))
        .collect();
    assert_eq!(ids, vec!["NEST-1".to_string()]);

    // Full-ID get reads the nested task from its actual root.
    let resp = rest_get(&api, "NEST-1");
    assert_eq!(resp.status, 200, "full-id get across roots");
    assert_eq!(body_of(&resp)["data"]["title"], "NESTED-ROOT-MARKER");

    // NEST-1 and DEV-1 both store number 1 across roots: numeric lookup is
    // ambiguous and fails closed rather than picking a root arbitrarily.
    let storage = Storage::try_open(&fx.tasks_dir).unwrap();
    let err = match storage.resolve_numeric_id("1") {
        Err(err) => err,
        Ok((id, _)) => panic!("ambiguous numeric must fail closed, got {id}"),
    };
    assert!(
        err.to_string()
            .contains("matches multiple storage locations"),
        "{err}"
    );
    // A number unique to one root still resolves.
    write_task_file(&fx.tasks_dir, "DEV", 5, "primary dev five");
    let resolved = storage.resolve_numeric_id("5").expect("unique numeric");
    assert_eq!(resolved.0, "DEV-5");
}

#[test]
fn nested_root_numeric_lookup_prefers_nothing_when_ambiguous() {
    let fx = isolated_workspace();
    // Same number under two projects across two roots: numeric identity is
    // ambiguous and must fail closed instead of picking arbitrarily.
    write_task_file(&fx.tasks_dir, "DEV", 1, "primary dev one");
    let sibling = fx.sibling_tasks_dir("sibling");
    write_task_file(&sibling, "NEST", 1, "nested one");

    let storage = Storage::try_open(&fx.tasks_dir).unwrap();
    let err = match storage.resolve_numeric_id("1") {
        Err(err) => err,
        Ok((id, _)) => panic!("ambiguous numeric must fail closed, resolved {id}"),
    };
    assert!(
        err.to_string()
            .contains("matches multiple storage locations"),
        "{err}"
    );
    // Legacy wrapper degrades to None rather than guessing.
    assert!(storage.find_task_by_numeric_id("1").is_none());
}

#[test]
fn duplicate_full_id_across_roots_fails_closed() {
    let fx = isolated_workspace();
    write_task_file(&fx.tasks_dir, "DUP", 7, "primary duplicate");
    let sibling = fx.sibling_tasks_dir("sibling");
    write_task_file(&sibling, "DUP", 7, "sibling duplicate");

    let storage = Storage::try_open(&fx.tasks_dir).unwrap();
    // Plain get never maps the ambiguous ID to an arbitrary root.
    assert!(storage.get("DUP-7", "DUP").is_none());
    // Resolver reports the ambiguity with both locations.
    let err = storage
        .resolve_task_location("DUP-7")
        .expect_err("duplicate id must be ambiguous");
    assert!(err.to_string().contains("matches multiple"), "{err}");
    // Service get maps the failure to TaskNotFound, not a wrong task.
    let err = TaskService::get(&storage, "DUP-7", None).unwrap_err();
    assert!(
        matches!(err, lotar::errors::LoTaRError::TaskNotFound(_)),
        "{err}"
    );
    // Mutation of the ambiguous ID refuses before any side effect.
    let mut storage = storage;
    let err = storage
        .edit("DUP-7", &lotar::storage::task::Task::default())
        .unwrap_err();
    assert!(err.to_string().contains("matches multiple"), "{err}");
    assert!(fx.tasks_dir.join("DUP").join("7.yml").exists());
    assert!(sibling.join("DUP").join("7.yml").exists());
}

#[test]
fn cross_root_mutation_refused_before_side_effects() {
    let fx = isolated_workspace();
    let sibling = fx.sibling_tasks_dir("sibling");
    write_task_file(&sibling, "NEST", 3, "nested immutable");

    let storage = Storage::try_open(&fx.tasks_dir).unwrap();
    // Reads resolve the nested task...
    assert_eq!(
        TaskService::get(&storage, "NEST-3", None).unwrap().title,
        "nested immutable"
    );

    // ...but mutation refuses with an actionable error and no file change.
    let mut storage = Storage::try_open(&fx.tasks_dir).unwrap();
    let err = TaskService::update(
        &mut storage,
        "NEST-3",
        lotar::api_types::TaskUpdate {
            title: Some("hijacked".to_string()),
            ..Default::default()
        },
    )
    .unwrap_err();
    let message = err.to_string();
    assert!(
        message.contains("cannot be modified from"),
        "clear cross-root refusal: {message}"
    );
    let yaml = std::fs::read_to_string(sibling.join("NEST").join("3.yml")).unwrap();
    assert!(
        yaml.contains("nested immutable"),
        "sibling untouched: {yaml}"
    );

    // Direct storage edit and delete refuse as well.
    let err = storage
        .edit("NEST-3", &lotar::storage::task::Task::default())
        .unwrap_err();
    assert!(err.to_string().contains("cannot be modified from"), "{err}");
    let err = storage.delete("NEST-3", "NEST").unwrap_err();
    assert!(err.to_string().contains("cannot be modified from"), "{err}");
    assert!(sibling.join("NEST").join("3.yml").exists());
}

// ---------------------------------------------------------------------------
// Malformed identifiers fail closed
// ---------------------------------------------------------------------------

#[test]
fn malformed_ids_fail_closed_at_boundaries() {
    let fx = isolated_workspace();
    let api = server();
    let _ = rest_create(&api, "DEV", "seed");

    for bad in [
        "DEV-ABC",
        "DEV-",
        "DEV--",
        "DEV-+12",
        "DEV-1.5",
        "../evil-1",
        "DEV- 1",
        "-1",
        "DEV-99999999999999999999999",
    ] {
        let resp = rest_get(&api, bad);
        assert!(
            resp.status == 400 || resp.status == 404,
            "malformed {bad:?} must fail closed, got {}",
            resp.status
        );
        let second = api.handle_request(&mk_req(
            "POST",
            "/api/tasks/comment",
            &[],
            json!({"id": bad, "text": "x"}),
        ));
        assert!(
            second.status == 400 || second.status == 404,
            "comment on malformed {bad:?} must fail closed"
        );
    }

    // Traversal never reads files outside the tasks root.
    let storage = Storage::try_open(&fx.tasks_dir).unwrap();
    assert!(storage.get("../evil-1", "evil").is_none());
    assert!(storage.get("a/b-1", "a").is_none());
}

// ---------------------------------------------------------------------------
// Review regressions (DEV-56 backend review)
// ---------------------------------------------------------------------------

fn count_stored_files(dir: &std::path::Path) -> usize {
    std::fs::read_dir(dir)
        .map(|entries| {
            entries
                .flatten()
                .filter(|e| e.path().is_file() && !e.file_name().to_string_lossy().starts_with('.'))
                .count()
        })
        .unwrap_or(0)
}

#[test]
fn delete_project_mismatch_refused_and_bytes_unchanged() {
    let fx = isolated_workspace();
    let api = server();
    write_task_file(&fx.tasks_dir, "TP", 5, "tp five");
    write_task_file(&fx.tasks_dir, "DEV", 5, "dev five");
    let tp_five = fx.tasks_dir.join("TP").join("5.yml");
    let dev_five = fx.tasks_dir.join("DEV").join("5.yml");

    // Storage layer: the numeric suffix must never be retargeted at the
    // provided project's folder.
    let mut storage = Storage::try_open(&fx.tasks_dir).unwrap();
    let err = storage.delete("DEV-5", "TP").unwrap_err();
    assert!(
        err.to_string().contains("refusing to cross projects"),
        "{err}"
    );
    assert!(tp_five.exists() && dev_five.exists(), "no bytes changed");

    // REST: mismatch is a 400, not a silent cross-project delete.
    let resp = api.handle_request(&mk_req(
        "POST",
        "/api/tasks/delete",
        &[("project", "TP")],
        json!({"id": "DEV-5"}),
    ));
    assert_eq!(resp.status, 400, "REST mismatch refused");
    assert!(
        tp_five.exists() && dev_five.exists(),
        "REST bytes unchanged"
    );

    // MCP boundary behaves the same.
    let resp = mcall("task_delete", json!({"id": "DEV-5", "project": "TP"}));
    assert!(resp.get("error").is_some(), "MCP mismatch refused: {resp}");
    assert!(
        resp["error"]["data"]["message"]
            .as_str()
            .is_some_and(|m| m.contains("refusing to cross projects")),
        "{resp}"
    );
    assert!(tp_five.exists() && dev_five.exists(), "MCP bytes unchanged");

    // Correct pairing still deletes exactly the right file.
    let resp = api.handle_request(&mk_req(
        "POST",
        "/api/tasks/delete",
        &[("project", "DEV")],
        json!({"id": "DEV-5"}),
    ));
    assert_eq!(resp.status, 200);
    assert!(!dev_five.exists());
    assert!(tp_five.exists(), "TP untouched by valid delete");
}

#[test]
fn padded_alias_sprint_membership_is_canonical_across_reload() {
    let fx = isolated_workspace();
    let api = server();
    rest_create(&api, "TP", "Padded member");
    let resp = api.handle_request(&mk_req(
        "POST",
        "/api/sprints/create",
        &[],
        json!({"name": "Canonical sprint"}),
    ));
    assert_eq!(resp.status, 200, "{}", String::from_utf8_lossy(&resp.body));

    // Legacy sprint file carrying a PADDED membership spelling.
    let sprint_file = fx.tasks_dir.join("@sprints").join("1.yml");
    std::fs::write(&sprint_file, "tasks:\n  - id: TP-001\n  - id: OTHER-9\n").unwrap();

    let storage = Storage::try_open(&fx.tasks_dir).unwrap();
    // Canonical reads see the padded legacy membership (no missed lookup).
    let dto = TaskService::get(&storage, "TP-1", None).unwrap();
    assert_eq!(dto.sprints, vec![1], "padded legacy entry visible");

    // Padded update keeping the same membership must not rewrite the file.
    let before = std::fs::read_to_string(&sprint_file).unwrap();
    let mut storage = Storage::try_open(&fx.tasks_dir).unwrap();
    TaskService::update(
        &mut storage,
        "TP-001",
        lotar::api_types::TaskUpdate {
            sprints: Some(vec![1]),
            ..Default::default()
        },
    )
    .unwrap();
    assert_eq!(
        std::fs::read_to_string(&sprint_file).unwrap(),
        before,
        "no rewrite when canonically unchanged"
    );

    // Removal via the canonical id drops the padded spelling too.
    TaskService::update(
        &mut storage,
        "TP-1",
        lotar::api_types::TaskUpdate {
            sprints: Some(vec![]),
            ..Default::default()
        },
    )
    .unwrap();
    let yaml = std::fs::read_to_string(&sprint_file).unwrap();
    assert!(!yaml.contains("TP-0"), "padded spelling removed: {yaml}");

    // The assignment boundary canonicalizes padded tokens before writing.
    let resolved =
        lotar::services::sprint_assignment::resolve_task_identifier(&storage, "TP-001").unwrap();
    assert_eq!(resolved, "TP-1");
    let resp = api.handle_request(&mk_req(
        "POST",
        "/api/sprints/add",
        &[],
        json!({"sprint": 1, "tasks": ["TP-001"], "allow_closed": true}),
    ));
    assert_eq!(resp.status, 200, "{}", String::from_utf8_lossy(&resp.body));
    let yaml = std::fs::read_to_string(&sprint_file).unwrap();
    let entry_lines: Vec<&str> = yaml.lines().map(str::trim).collect();
    assert!(
        entry_lines.contains(&"- TP-1"),
        "canonical spelling persisted: {yaml}"
    );
    assert!(
        !entry_lines.contains(&"- TP-001"),
        "no padded duplicate: {yaml}"
    );
    assert_eq!(
        yaml.matches("TP-").count(),
        1,
        "exactly one membership entry: {yaml}"
    );

    // Comment DTOs surface the canonical id, never the padded alias.
    let dto = TaskService::add_comment(&mut storage, "TP-001", "canonical please").unwrap();
    assert_eq!(dto.id, "TP-1");
}

#[test]
fn attachment_upload_prechecks_before_any_write() {
    let fx = isolated_workspace();
    let api = server();
    let id = rest_create(&api, "TP", "Upload target");
    let attachments = fx.tasks_dir.join("@attachments");

    let upload = |task_id: &str, name: &str, bytes: &[u8]| {
        api.handle_request(&mk_req(
            "POST",
            "/api/tasks/attachments/upload",
            &[],
            json!({"id": task_id, "filename": name, "content_base64": base64_encode(bytes)}),
        ))
    };

    // Unknown task: 404, zero bytes written.
    let resp = upload("GHOST-9", "a.txt", b"alpha");
    assert_eq!(resp.status, 404, "{}", String::from_utf8_lossy(&resp.body));
    assert_eq!(count_stored_files(&attachments), 0);

    // Nested-root task: cross-root mutation refused before any write.
    let sibling = fx.sibling_tasks_dir("sibling");
    write_task_file(&sibling, "NEST", 1, "nested upload target");
    let resp = upload("NEST-1", "b.txt", b"beta");
    assert_eq!(resp.status, 400, "{}", String::from_utf8_lossy(&resp.body));
    assert_eq!(count_stored_files(&attachments), 0);

    // Ambiguous duplicate id across roots: refused before any write.
    write_task_file(&fx.tasks_dir, "DUP", 7, "primary dup");
    write_task_file(&sibling, "DUP", 7, "sibling dup");
    let resp = upload("DUP-7", "c.txt", b"gamma");
    assert_eq!(resp.status, 400, "{}", String::from_utf8_lossy(&resp.body));
    assert_eq!(count_stored_files(&attachments), 0);

    // Happy path: upload stores once, dedups identical content.
    let resp = upload(&id, "first.txt", b"duplicate-me");
    assert_eq!(resp.status, 200, "{}", String::from_utf8_lossy(&resp.body));
    assert_eq!(count_stored_files(&attachments), 1);
    let resp = upload(&id, "second.txt", b"duplicate-me");
    assert_eq!(resp.status, 200, "{}", String::from_utf8_lossy(&resp.body));
    assert_eq!(
        count_stored_files(&attachments),
        1,
        "content dedup preserved"
    );

    // A failure AFTER the precheck (read-only project dir breaks the task
    // edit) must not leave the just-created upload file behind.
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let project_dir = fx.tasks_dir.join("TP");
        std::fs::set_permissions(&project_dir, std::fs::Permissions::from_mode(0o500)).unwrap();
        let resp = upload(&id, "orphan.txt", b"orphan-bytes");
        std::fs::set_permissions(&project_dir, std::fs::Permissions::from_mode(0o755)).unwrap();
        assert_ne!(resp.status, 200, "attach must fail on read-only project");
        assert_eq!(
            count_stored_files(&attachments),
            1,
            "orphaned upload cleaned up; pre-existing dedup content kept"
        );
    }
}

#[test]
fn agent_context_refuses_cross_root_writes_and_reads_actual_root() {
    let fx = isolated_workspace();
    let sibling = fx.sibling_tasks_dir("sibling");
    write_task_file(&sibling, "NEST", 3, "nested context target");

    let cfg = lotar::config::types::ResolvedConfig::from_global(
        lotar::config::types::GlobalConfig::default(),
    );

    // Cross-root write is refused BEFORE any file or directory is created.
    let err = lotar::services::agent_context_service::AgentContextService::append_messages(
        &fx.tasks_dir,
        &cfg,
        "NEST-3",
        vec![lotar::services::agent_context_service::build_user_message(
            "hello",
        )],
        None,
    )
    .unwrap_err();
    assert!(err.to_string().contains("cannot be written from"), "{err}");
    assert!(
        !fx.tasks_dir.join("NEST").exists(),
        "primary root must stay untouched"
    );
    assert!(
        !sibling.join("NEST").join("NEST-3.context").exists(),
        "no context created anywhere on refusal"
    );

    // Actual-root-aware read: a context placed next to the nested task file
    // is found even when resolving from the primary workspace.
    let context_dir = sibling.join("NEST");
    std::fs::create_dir_all(&context_dir).unwrap();
    let payload = serde_json::json!({
        "ticket_id": "NEST-3",
        "updated_at": "2026-01-01T00:00:00Z",
        "messages": [{"role": "user", "content": "nested", "at": "2026-01-01T00:00:00Z"}]
    });
    std::fs::write(
        context_dir.join("NEST-3.context"),
        serde_json::to_string(&payload).unwrap(),
    )
    .unwrap();
    let loaded = lotar::services::agent_context_service::AgentContextService::load(
        &fx.tasks_dir,
        &cfg,
        "NEST-3",
    )
    .unwrap();
    assert!(loaded.is_some(), "actual-root context readable");
    assert_eq!(loaded.unwrap().ticket_id, "NEST-3");
}

#[test]
fn delete_event_emits_canonical_id_for_padded_alias() {
    let fx = isolated_workspace();
    let api = server();
    let id = rest_create(&api, "TP", "Event target");
    assert_eq!(id, "TP-1");

    // Subscribe BEFORE the delete so the event cannot be missed. The
    // registry is process-global, so drain until OUR event arrives.
    let rx = lotar::api_events::subscribe();
    let resp = api.handle_request(&mk_req(
        "POST",
        "/api/tasks/delete",
        &[],
        json!({"id": "TP-001"}),
    ));
    assert_eq!(resp.status, 200, "padded alias delete");
    assert!(!fx.tasks_dir.join("TP").join("1.yml").exists());

    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    loop {
        let event = rx
            .recv_timeout(deadline.saturating_duration_since(std::time::Instant::now()))
            .expect("task_deleted event must arrive");
        if event.kind == "task_deleted"
            && event.data.get("id").and_then(|v| v.as_str()) == Some("TP-1")
        {
            break;
        }
    }
}

#[test]
fn mcp_single_file_reference_blocks_on_held_store_lock() {
    // File references resolve against the git repo root. find_repo_root
    // accepts both a .git directory and a worktree-style .git file, so try a
    // fixture-local marker (no git tooling involved) in both styles. When the
    // sandbox denies creating every .git path, fall back to a workspace under
    // CARGO_TARGET_TMPDIR inside the real checkout so repo-root discovery
    // succeeds by ancestry; the store lock and blob still live in the
    // fixture's own tasks root, never the real backlog. Only a genuinely
    // repo-less build keeps the fail-closed-only arm (remaining gap tracked
    // in DEV-79).
    enum MarkerStyle {
        Dir,
        File,
    }
    fn probe_marker_style() -> Option<MarkerStyle> {
        let probe = tempfile::tempdir().unwrap();
        let marker = probe.path().join(".git");
        if std::fs::create_dir_all(&marker).is_ok() {
            return Some(MarkerStyle::Dir);
        }
        if std::fs::write(&marker, "gitdir: nowhere\n").is_ok() {
            return Some(MarkerStyle::File);
        }
        None
    }

    let (fx, repo_root_available) = match probe_marker_style() {
        Some(style) => {
            let fx = isolated_workspace();
            let git_marker = fx.tasks_dir.parent().unwrap().join(".git");
            match style {
                MarkerStyle::Dir => std::fs::create_dir_all(&git_marker).unwrap(),
                MarkerStyle::File => std::fs::write(&git_marker, "gitdir: nowhere\n").unwrap(),
            }
            eprintln!("lock section via fixture-local .git marker");
            (fx, true)
        }
        None => {
            // No marker can be created in this sandbox: place the workspace
            // under CARGO_TARGET_TMPDIR inside the real checkout and let
            // repo-root discovery walk up to the existing .git instead. The
            // fixture still owns its tasks root and attachments store, so
            // the real backlog is never touched.
            let fx = isolated_workspace_in(std::path::Path::new(env!("CARGO_TARGET_TMPDIR")));
            let by_ancestry = lotar::utils::git::find_repo_root(&fx.tasks_dir).is_some();
            if by_ancestry {
                eprintln!("lock section via real repo root discovered by ancestry");
            } else {
                eprintln!(
                    "skipping lock section: no .git marker creatable and no repo ancestor (repo-less build); remaining gap tracked in DEV-79"
                );
            }
            (fx, by_ancestry)
        }
    };
    let api = server();
    let _tp1 = rest_create(&api, "TP", "Blob owner");
    let _tp2 = rest_create(&api, "TP", "Single MCP target");
    if !repo_root_available {
        // Repo-rooted file references are untestable here: assert the
        // handler still fails closed (no task changes) and skip the lock
        // section, matching the git_available convention.
        let resp = mcall(
            "task_reference_add",
            json!({"id": "TP-2", "kind": "file", "value": "@attachments/whatever.txt"}),
        );
        assert!(
            resp.get("error").is_some(),
            "file reference without a repo root must fail closed: {resp}"
        );
        let storage = Storage::try_open(&fx.tasks_dir).unwrap();
        let tp2 = TaskService::get(&storage, "TP-2", None).unwrap();
        assert!(tp2.references.iter().all(|r| r.file.is_none()));
        return;
    }
    // TP-1 receives a committed blob whose path the MCP client references.
    let content = base64_encode(b"mcp-single-blob");
    let resp = api.handle_request(&mk_req(
        "POST",
        "/api/tasks/attachments/upload",
        &[],
        json!({"id": "TP-1", "filename": "shared.txt", "content_base64": content}),
    ));
    assert_eq!(resp.status, 200, "{}", String::from_utf8_lossy(&resp.body));
    let filename = body_of(&resp)["data"]["stored_path"]
        .as_str()
        .unwrap()
        .to_string();
    let attachments = fx.tasks_dir.join("@attachments");

    // The value must resolve under the discovered repo root; absolute paths
    // are accepted and the blob lives below this workspace.
    let blob_abs = attachments.join(&filename);

    // Independently held fs2 store lock == another LoTaR process.
    let held =
        lotar::storage::safety::acquire_storage_lock(&attachments, "attachments-store").unwrap();

    // Single-task MCP file add must fail closed while the store is locked,
    // leaving the task untouched.
    let resp = mcall(
        "task_reference_add",
        json!({"id": "TP-2", "kind": "file", "value": blob_abs.to_string_lossy()}),
    );
    assert!(
        resp.get("error").is_some(),
        "locked store must refuse: {resp}"
    );
    let message = resp["error"]["data"]["message"]
        .as_str()
        .or_else(|| resp["error"]["message"].as_str())
        .unwrap_or_default()
        .to_string();
    assert!(
        message.contains("attachments-store") || message.contains("busy"),
        "fail-closed lock diagnostics: {message}"
    );
    let storage = Storage::try_open(&fx.tasks_dir).unwrap();
    let tp2 = TaskService::get(&storage, "TP-2", None).unwrap();
    assert!(
        tp2.references.iter().all(|r| r.file.is_none()),
        "no task changes while locked: {:?}",
        tp2.references
    );

    // After release the same MCP call succeeds.
    drop(held);
    let resp = mcall(
        "task_reference_add",
        json!({"id": "TP-2", "kind": "file", "value": blob_abs.to_string_lossy()}),
    );
    assert!(resp.get("error").is_none(), "after release: {resp}");
    let storage = Storage::try_open(&fx.tasks_dir).unwrap();
    let tp2 = TaskService::get(&storage, "TP-2", None).unwrap();
    assert!(
        tp2.references.iter().any(|r| r.file.is_some()),
        "reference attached after release: {:?}",
        tp2.references
    );
}

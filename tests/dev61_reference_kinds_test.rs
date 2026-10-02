//! DEV-61: repository-file references vs managed attachments.
//!
//! Public-boundary regression coverage for the typed reference contract:
//! - `file` references are repository-relative and can never resolve
//!   inside the configured attachments store (default, custom absolute
//!   config, and symlinked entries included)
//! - uploads attach typed `attachment` entries; dedup/refcount and blob
//!   reclamation operate on `attachment` entries only
//! - attachment remove requires typed managed membership on the task
//!   before any blob cleanup; wrong-kind values never delete blobs or
//!   repository files
//! - REST `/api/tasks/references/file/add|remove`, MCP `file`/`attachment`
//!   kinds (single + bulk), and serialization all follow the same contract
//! - managed operations still coordinate on the cross-process store lock
//!
//! These tests use no git tooling: `.git` is never created or invoked,
//! and repo-root discovery happens by ancestry from a workspace placed
//! under CARGO_TARGET_TMPDIR inside the real checkout. The suite must
//! stay compiled (and executing) in every environment, including
//! Git-denied sandboxes (only source-local `git_required` modules are ever
//! excluded, and never by compilation).
use lotar::api_server::{ApiServer, HttpRequest};
use lotar::routes;
use serde_json::{Value, json};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
mod common;
use crate::common::env_mutex::EnvVarGuard;

/// Fixture workspace placed under CARGO_TARGET_TMPDIR (inside the real
/// checkout's target directory) so `find_repo_root` resolves by ANCESTRY
/// to the checkout's existing `.git` even in sandboxes that forbid
/// creating any `.git` marker (this one does). No git tooling or commands
/// are involved; every file lives inside the fixture's own workspace.
/// Repo-relative reference values are expressed through [`Dev61Fixture::rel`].
struct Dev61Fixture {
    _tmp: tempfile::TempDir,
    workspace: PathBuf,
    tasks_dir: PathBuf,
    /// Repo-root-relative prefix (forward slashes, trailing slash, empty
    /// when the workspace IS the repo root) for workspace file paths.
    ref_prefix: String,
    repo_root_available: bool,
    /// The discovered repository root for direct service-level calls.
    repo_root: PathBuf,
    _guard_tasks: EnvVarGuard,
    _guard_fast: EnvVarGuard,
}

fn isolated_workspace() -> Dev61Fixture {
    let _guard_fast = EnvVarGuard::set("LOTAR_TEST_FAST_IO", "1");
    let tmp = tempfile::tempdir_in(std::path::Path::new(env!("CARGO_TARGET_TMPDIR"))).unwrap();
    let workspace = tmp.path().join("main");
    let tasks_dir = workspace.join(".tasks");
    std::fs::create_dir_all(&tasks_dir).unwrap();
    std::fs::create_dir_all(workspace.join("src")).unwrap();
    std::fs::write(workspace.join("src/example.rs"), "fn main() {}\n").unwrap();
    let _guard_tasks = EnvVarGuard::set("LOTAR_TASKS_DIR", &tasks_dir.to_string_lossy());

    let repo_root = lotar::utils::git::find_repo_root(&tasks_dir);
    let (repo_root_available, ref_prefix) = match &repo_root {
        Some(root) => (
            true,
            workspace
                .strip_prefix(root)
                .map(|rel| {
                    let raw = rel.to_string_lossy().replace('\\', "/");
                    if raw.is_empty() {
                        String::new()
                    } else {
                        format!("{raw}/")
                    }
                })
                .unwrap_or_default(),
        ),
        None => (false, String::new()),
    };

    let repo_root = repo_root.unwrap_or_else(|| workspace.clone());
    Dev61Fixture {
        _tmp: tmp,
        workspace,
        tasks_dir,
        ref_prefix,
        repo_root_available,
        repo_root,
        _guard_tasks,
        _guard_fast,
    }
}

impl Dev61Fixture {
    fn attachments(&self) -> PathBuf {
        self.tasks_dir.join("@attachments")
    }

    /// Express a workspace-relative path as the repo-relative reference
    /// value the surfaces accept. Asserts the workspace really is inside
    /// the discovered repo root (guarded by `repo_root_available`).
    fn rel(&self, path: &str) -> String {
        assert!(
            self.workspace.join(path).starts_with(&self.workspace),
            "fixture-relative path escapes the workspace"
        );
        format!("{}{}", self.ref_prefix, path)
    }

    fn blob_count(&self) -> usize {
        std::fs::read_dir(self.attachments())
            .map(|entries| {
                entries
                    .flatten()
                    .filter(|e| {
                        e.path().is_file() && !e.file_name().to_string_lossy().starts_with('.')
                    })
                    .count()
            })
            .unwrap_or(0)
    }
}

fn server() -> ApiServer {
    let mut api = ApiServer::new();
    routes::initialize(&mut api);
    api
}

fn mk_req(method: &str, path: &str, body: Value) -> HttpRequest {
    mk_req_query(method, path, &[], body)
}

fn mk_req_query(method: &str, path: &str, query: &[(&str, &str)], body: Value) -> HttpRequest {
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

fn body_of(resp: &lotar::api_server::HttpResponse) -> Value {
    serde_json::from_slice(&resp.body).unwrap()
}

fn rest_create(api: &ApiServer, project: &str, title: &str) -> String {
    let resp = api.handle_request(&mk_req(
        "POST",
        "/api/tasks/add",
        json!({"title": title, "project": project}),
    ));
    assert_eq!(
        resp.status,
        201,
        "create in {project}: {}",
        String::from_utf8_lossy(&resp.body)
    );
    body_of(&resp)["data"]["id"].as_str().unwrap().to_string()
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

fn upload(api: &ApiServer, id: &str, filename: &str, bytes: &[u8]) -> String {
    let resp = api.handle_request(&mk_req(
        "POST",
        "/api/tasks/attachments/upload",
        json!({"id": id, "filename": filename, "content_base64": base64_encode(bytes)}),
    ));
    assert_eq!(resp.status, 200, "upload for {id}");
    body_of(&resp)["data"]["stored_path"]
        .as_str()
        .unwrap()
        .to_string()
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

/// True when the tool call failed: JSON-RPC protocol error or recoverable
/// domain failure surfaced as result.isError (DEV-63).
fn mcp_failed(resp: &Value) -> bool {
    resp.get("error").is_some()
        || resp.get("result").and_then(|result| result.get("isError"))
            == Some(&serde_json::json!(true))
}

fn task_yaml_has_reference_key(tasks_dir: &Path, project: &str, number: &str, key: &str) -> bool {
    let yaml =
        std::fs::read_to_string(tasks_dir.join(project).join(format!("{number}.yml"))).unwrap();
    yaml.lines().any(|line| {
        line.trim_start()
            .trim_start_matches("- ")
            .starts_with(&format!("{key}:"))
    })
}

// ---------------------------------------------------------------------------
// REST repository-file references
// ---------------------------------------------------------------------------

#[test]
fn rest_file_reference_add_and_remove_roundtrip() {
    let _fx = isolated_workspace();
    if !_fx.repo_root_available {
        eprintln!("skipping: no repo root discoverable by ancestry from CARGO_TARGET_TMPDIR");
        return;
    }
    let api = server();
    let id = rest_create(&api, "TP", "File ref target");

    let example = _fx.rel("src/example.rs");
    let resp = api.handle_request(&mk_req(
        "POST",
        "/api/tasks/references/file/add",
        json!({"id": id, "path": example}),
    ));
    assert_eq!(resp.status, 200, "{}", String::from_utf8_lossy(&resp.body));
    let data = body_of(&resp)["data"].clone();
    assert_eq!(data["added"], true);
    let refs = data["task"]["references"].as_array().unwrap();
    assert!(
        refs.iter()
            .any(|r| r["file"].as_str() == Some(example.as_str())),
        "{refs:?}"
    );
    assert!(
        refs.iter().all(|r| r.get("attachment").is_none()),
        "file add never writes attachment entries: {refs:?}"
    );

    // Idempotent re-add reports added=false.
    let resp = api.handle_request(&mk_req(
        "POST",
        "/api/tasks/references/file/add",
        json!({"id": id, "path": example}),
    ));
    assert_eq!(resp.status, 200);
    assert_eq!(body_of(&resp)["data"]["added"], false);

    let resp = api.handle_request(&mk_req(
        "POST",
        "/api/tasks/references/file/remove",
        json!({"id": id, "path": example}),
    ));
    assert_eq!(resp.status, 200);
    let removed_body = body_of(&resp);
    assert_eq!(removed_body["data"]["removed"], true);
    let refs = removed_body["data"]["task"]["references"]
        .as_array()
        .cloned()
        .unwrap_or_default();
    assert!(
        refs.iter()
            .all(|r| r.get("file").is_none() && r.get("attachment").is_none()),
        "{refs:?}"
    );

    // Repository file is untouched by reference detach.
    assert!(_fx.workspace.join("src/example.rs").exists());
}

#[test]
fn rest_file_reference_rejects_attachment_store_paths() {
    let fx = isolated_workspace();
    if !fx.repo_root_available {
        eprintln!("skipping: no repo root discoverable by ancestry from CARGO_TARGET_TMPDIR");
        return;
    }
    let api = server();
    let id = rest_create(&api, "TP", "Store guard target");
    let stored = upload(&api, &id, "guard.txt", b"store-guard-blob");
    assert_eq!(fx.blob_count(), 1);

    for value in [
        fx.rel(&format!(".tasks/@attachments/{stored}")),
        fx.attachments().join(&stored).to_string_lossy().to_string(),
    ] {
        let resp = api.handle_request(&mk_req(
            "POST",
            "/api/tasks/references/file/add",
            json!({"id": id, "path": value}),
        ));
        assert_eq!(
            resp.status, 400,
            "store path must be rejected as a file reference: {value}"
        );
    }

    // A symlink at the repo root pointing into the store is rejected too
    // (both sides are canonicalized).
    #[cfg(unix)]
    std::os::unix::fs::symlink(
        fx.attachments().join(&stored),
        fx.workspace.join("into-store.txt"),
    )
    .unwrap();
    #[cfg(unix)]
    {
        let resp = api.handle_request(&mk_req(
            "POST",
            "/api/tasks/references/file/add",
            json!({"id": id, "path": fx.rel("into-store.txt")}),
        ));
        assert_eq!(resp.status, 400, "symlink into store must be rejected");
    }

    // Task and blob are untouched.
    let resp = api.handle_request(&mk_req_query(
        "GET",
        "/api/tasks/get",
        &[("id", id.as_str())],
        json!({}),
    ));
    let refs = body_of(&resp)["data"]["references"]
        .as_array()
        .unwrap()
        .clone();
    assert!(
        refs.iter().all(|r| r.get("file").is_none()),
        "no file entry after rejections: {refs:?}"
    );
    assert_eq!(fx.blob_count(), 1, "blob intact");
}

#[test]
fn file_reference_rejects_custom_configured_store_location() {
    let fx = isolated_workspace();
    if !fx.repo_root_available {
        eprintln!("skipping: no repo root discoverable by ancestry from CARGO_TARGET_TMPDIR");
        return;
    }
    // Custom store outside the tasks dir, addressed by absolute config.
    let custom_store = fx._tmp.path().join("custom-store");
    std::fs::create_dir_all(&custom_store).unwrap();
    std::fs::write(custom_store.join("blob.txt"), "custom-store-blob\n").unwrap();
    std::fs::write(
        fx.tasks_dir.join("config.yml"),
        format!(
            "attachments:\n    dir: {}\n",
            custom_store.to_string_lossy()
        ),
    )
    .unwrap();

    let mut storage = lotar::storage::manager::Storage::new(&fx.tasks_dir);
    let task = lotar::storage::task::Task::new(
        fx.tasks_dir.clone(),
        "Custom store guard".to_string(),
        lotar::types::Priority::from("Medium"),
    );
    storage.add(&task, "TP", None).unwrap();

    let err = lotar::services::reference_service::ReferenceService::attach_file_reference(
        &mut storage,
        &fx.repo_root,
        "TP-1",
        &custom_store.join("blob.txt").to_string_lossy(),
    )
    .unwrap_err();
    assert!(
        err.to_string().contains("managed attachments store"),
        "custom store rejection: {err}"
    );
}

// ---------------------------------------------------------------------------
// Managed attachment membership and reclamation
// ---------------------------------------------------------------------------

#[test]
fn attachment_remove_requires_typed_membership_fail_closed() {
    let fx = isolated_workspace();
    let api = server();
    let owner = rest_create(&api, "TP", "Blob owner");
    let other = rest_create(&api, "TP", "Unrelated task");
    let stored = upload(&api, &owner, "members.txt", b"membership-blob");

    // Wrong task: no managed membership -> fail closed, blob untouched.
    let resp = api.handle_request(&mk_req(
        "POST",
        "/api/tasks/attachments/remove",
        json!({"id": other, "stored_path": stored}),
    ));
    assert_eq!(resp.status, 400, "missing managed ref must fail closed");
    assert_eq!(fx.blob_count(), 1, "blob survives failed membership gate");

    // Owner remove succeeds and reclaims the blob.
    let resp = api.handle_request(&mk_req(
        "POST",
        "/api/tasks/attachments/remove",
        json!({"id": owner, "stored_path": stored}),
    ));
    assert_eq!(resp.status, 200);
    let data = body_of(&resp)["data"].clone();
    assert_eq!(data["deleted"], true);
    assert_eq!(data["still_referenced"], false);
    assert_eq!(fx.blob_count(), 0, "blob reclaimed");

    // Second remove from the same task: membership gone -> fail closed.
    let resp = api.handle_request(&mk_req(
        "POST",
        "/api/tasks/attachments/remove",
        json!({"id": owner, "stored_path": stored}),
    ));
    assert_eq!(resp.status, 400, "idempotent remove must fail closed");
}

#[test]
fn wrong_kind_value_never_deletes_blob_or_repo_file() {
    let fx = isolated_workspace();
    if !fx.repo_root_available {
        eprintln!("skipping: no repo root discoverable by ancestry from CARGO_TARGET_TMPDIR");
        return;
    }
    let api = server();
    let owner = rest_create(&api, "TP", "Blob owner");
    let reader = rest_create(&api, "TP", "Repo file holder");
    let stored = upload(&api, &owner, "shared.txt", b"wrong-kind-blob");

    // A repository file at the repo root with the exact stored leaf name.
    std::fs::write(fx.workspace.join(&stored), "repository file\n").unwrap();
    let repo_ref = fx.rel(&stored);
    let resp = api.handle_request(&mk_req(
        "POST",
        "/api/tasks/references/file/add",
        json!({"id": reader, "path": repo_ref}),
    ));
    assert_eq!(resp.status, 200, "same-named repo file is a legal file ref");

    // Attachment remove from the reader: it holds only a `file` entry, so
    // there is no typed managed membership -> refuse without touching the
    // store blob or the repository file.
    let resp = api.handle_request(&mk_req(
        "POST",
        "/api/tasks/attachments/remove",
        json!({"id": reader, "stored_path": stored}),
    ));
    assert_eq!(resp.status, 400, "file entry is not managed membership");
    assert_eq!(fx.blob_count(), 1, "blob intact");
    assert!(fx.workspace.join(&stored).exists(), "repo file intact");

    // The owner's managed reference still counts for reclamation.
    let resp = api.handle_request(&mk_req(
        "POST",
        "/api/tasks/attachments/remove",
        json!({"id": owner, "stored_path": stored}),
    ));
    assert_eq!(resp.status, 200);
    let data = body_of(&resp)["data"].clone();
    assert_eq!(data["deleted"], true);
    assert!(
        fx.workspace.join(&stored).exists(),
        "repo file never deleted"
    );
    assert_eq!(fx.blob_count(), 0, "blob reclaimed via true membership");
}

#[test]
fn attachment_dedup_and_refcount_preserved() {
    let fx = isolated_workspace();
    let api = server();
    let a = rest_create(&api, "TP", "Shared A");
    let b = rest_create(&api, "TP", "Shared B");

    let first = upload(&api, &a, "first.txt", b"dedupe-me");
    let second = upload(&api, &b, "second.txt", b"dedupe-me");
    assert_eq!(first, second, "identical content dedups to one blob");
    assert_eq!(fx.blob_count(), 1);

    // Remove from A: B still holds a managed reference -> no deletion.
    let resp = api.handle_request(&mk_req(
        "POST",
        "/api/tasks/attachments/remove",
        json!({"id": a, "stored_path": first}),
    ));
    assert_eq!(resp.status, 200);
    let data = body_of(&resp)["data"].clone();
    assert_eq!(data["still_referenced"], true);
    assert_eq!(data["deleted"], false);
    assert_eq!(fx.blob_count(), 1);

    // Remove from B: last managed reference -> blob reclaimed.
    let resp = api.handle_request(&mk_req(
        "POST",
        "/api/tasks/attachments/remove",
        json!({"id": b, "stored_path": second}),
    ));
    assert_eq!(resp.status, 200);
    let data = body_of(&resp)["data"].clone();
    assert_eq!(data["deleted"], true);
    assert_eq!(fx.blob_count(), 0);
}

#[test]
fn typed_keys_coexist_with_same_basename_without_conflation() {
    let fx = isolated_workspace();
    if !fx.repo_root_available {
        eprintln!("skipping: no repo root discoverable by ancestry from CARGO_TARGET_TMPDIR");
        return;
    }
    let api = server();
    let id = rest_create(&api, "TP", "Coexistence target");
    let stored = upload(&api, &id, "coexist.txt", b"coexist-blob");

    // Same string under both keys: managed attachment (from upload) plus a
    // repository file reference for a repo file with the same leaf name.
    std::fs::write(fx.workspace.join(&stored), "repository file\n").unwrap();
    let file_value = fx.rel(&stored);
    let resp = api.handle_request(&mk_req(
        "POST",
        "/api/tasks/references/file/add",
        json!({"id": id, "path": file_value}),
    ));
    assert_eq!(resp.status, 200);
    assert!(
        task_yaml_has_reference_key(&fx.tasks_dir, "TP", "1", "file")
            && task_yaml_has_reference_key(&fx.tasks_dir, "TP", "1", "attachment"),
        "both typed keys serialized distinctly"
    );

    // File remove drops only the `file` entry.
    let resp = api.handle_request(&mk_req(
        "POST",
        "/api/tasks/references/file/remove",
        json!({"id": id, "path": file_value}),
    ));
    assert_eq!(resp.status, 200);
    assert_eq!(body_of(&resp)["data"]["removed"], true);
    assert!(fx.workspace.join(&stored).exists(), "repo file untouched");
    assert_eq!(fx.blob_count(), 1, "blob untouched by file detach");
    assert!(
        !task_yaml_has_reference_key(&fx.tasks_dir, "TP", "1", "file"),
        "file entry gone"
    );
    assert!(
        task_yaml_has_reference_key(&fx.tasks_dir, "TP", "1", "attachment"),
        "attachment entry survives file detach"
    );

    // Attachment remove then reclaims via true membership.
    let resp = api.handle_request(&mk_req(
        "POST",
        "/api/tasks/attachments/remove",
        json!({"id": id, "stored_path": stored}),
    ));
    assert_eq!(resp.status, 200);
    assert_eq!(fx.blob_count(), 0);
    assert!(
        fx.workspace.join(&stored).exists(),
        "repo file never deleted"
    );
}

#[test]
fn uploads_serialize_as_typed_attachment_references() {
    let fx = isolated_workspace();
    let api = server();
    let id = rest_create(&api, "TP", "Serialization target");
    let stored = upload(&api, &id, "ser.txt", b"serialization-blob");

    let resp = api.handle_request(&mk_req_query(
        "GET",
        "/api/tasks/get",
        &[("id", id.as_str())],
        json!({}),
    ));
    let refs = body_of(&resp)["data"]["references"]
        .as_array()
        .unwrap()
        .clone();
    assert_eq!(refs.len(), 1);
    assert_eq!(refs[0]["attachment"].as_str(), Some(stored.as_str()));
    assert!(
        refs[0].get("file").is_none(),
        "DTO must not carry a file key for managed blobs: {refs:?}"
    );
    assert!(
        task_yaml_has_reference_key(&fx.tasks_dir, "TP", "1", "attachment"),
        "task YAML stores the attachment key"
    );
}

// ---------------------------------------------------------------------------
// Managed name validation and MCP parity
// ---------------------------------------------------------------------------

#[test]
fn managed_names_reject_path_shapes() {
    let fx = isolated_workspace();
    let mut storage = lotar::storage::manager::Storage::new(&fx.tasks_dir);
    let task = lotar::storage::task::Task::new(
        fx.tasks_dir.clone(),
        "Name validation".to_string(),
        lotar::types::Priority::from("Medium"),
    );
    storage.add(&task, "TP", None).unwrap();

    for bad in [
        "../escape.txt",
        "a/b.txt",
        "/abs/blob.txt",
        ".",
        "..",
        "a\\b.txt",
    ] {
        let err = lotar::services::attachment_service::AttachmentService::attach_managed_reference(
            &mut storage,
            "TP-1",
            bad,
        )
        .unwrap_err();
        assert!(
            matches!(err, lotar::errors::LoTaRError::ValidationError(_)),
            "{bad:?} must be a validation error, got {err}"
        );
    }

    let outcome = lotar::services::attachment_service::AttachmentService::attach_managed_reference(
        &mut storage,
        "TP-1",
        "  name.abc.def.ext  ",
    )
    .unwrap();
    let (task, added) = (outcome.task, outcome.changed);
    assert!(added);
    assert!(
        task.references
            .iter()
            .any(|r| r.attachment.as_deref() == Some("name.abc.def.ext")),
        "managed names are trimmed, never path-joined: {:?}",
        task.references
    );
}

#[test]
fn mcp_file_and_attachment_kinds_follow_the_contract() {
    let fx = isolated_workspace();
    if !fx.repo_root_available {
        eprintln!("skipping: no repo root discoverable by ancestry from CARGO_TARGET_TMPDIR");
        return;
    }
    let api = server();
    let _tp1 = rest_create(&api, "TP", "MCP target one");
    let _tp2 = rest_create(&api, "TP", "MCP target two");
    let stored = upload(&api, "TP-1", "mcp.txt", b"mcp-parity-blob");

    // File kind: repository-relative add/remove round trip.
    let example = fx.rel("src/example.rs");
    let resp = mcall(
        "task_reference_add",
        json!({"id": "TP-2", "kind": "file", "value": example}),
    );
    assert!(resp.get("error").is_none(), "MCP file add: {resp}");
    let resp = mcall(
        "task_reference_remove",
        json!({"id": "TP-2", "kind": "file", "value": example}),
    );
    assert!(resp.get("error").is_none(), "MCP file remove: {resp}");

    // File kind refuses store paths (relative and absolute).
    for value in [
        fx.rel(&format!(".tasks/@attachments/{stored}")),
        fx.attachments().join(&stored).to_string_lossy().to_string(),
    ] {
        let resp = mcall(
            "task_reference_add",
            json!({"id": "TP-2", "kind": "file", "value": value}),
        );
        assert!(
            mcp_failed(&resp),
            "MCP file add must reject store paths: {resp}"
        );
    }

    // Attachment kind: reference-only attach under the store lock; detach
    // never deletes blobs.
    let resp = mcall(
        "task_reference_add",
        json!({"id": "TP-2", "kind": "attachment", "value": stored}),
    );
    assert!(resp.get("error").is_none(), "MCP attachment add: {resp}");
    assert_eq!(fx.blob_count(), 1, "MCP attachment add creates no blob");

    let resp = mcall(
        "task_reference_remove",
        json!({"id": "TP-2", "kind": "attachment", "value": stored}),
    );
    assert!(resp.get("error").is_none(), "MCP attachment remove: {resp}");
    assert_eq!(fx.blob_count(), 1, "reference-only detach keeps the blob");
    assert!(fx.attachments().join(&stored).exists());

    // Attachment kind fails closed on non-leaf values and unknown blobs.
    for bad in ["../escape.txt", "a/b.txt", "missing.<32hex>.txt"] {
        let resp = mcall(
            "task_reference_add",
            json!({"id": "TP-2", "kind": "attachment", "value": bad}),
        );
        assert!(
            mcp_failed(&resp),
            "MCP attachment add must reject {bad}: {resp}"
        );
    }

    // Bulk attachment add under a HELD store lock follows the per-item
    // contract (review B1): the tool itself succeeds, every item of the
    // contended store fails individually with lock diagnostics, and no
    // reference appears on any task.
    let held = lotar::storage::safety::acquire_storage_lock(&fx.attachments(), "attachments-store")
        .unwrap();
    let resp = mcall(
        "task_bulk_reference_add",
        json!({"ids": ["TP-1", "TP-2"], "kind": "attachment", "value": stored}),
    );
    assert!(
        resp.get("error").is_none(),
        "per-item contract: the tool must succeed while a store is locked: {resp}"
    );
    let text = resp["result"]["content"][0]["text"].as_str().unwrap();
    let payload: Value = serde_json::from_str(text).unwrap();
    assert_eq!(
        payload["updated"].as_array().unwrap().len(),
        0,
        "no item completes while the store lock is held: {payload}"
    );
    let failed = payload["failed"].as_array().unwrap().clone();
    assert_eq!(failed.len(), 2, "both items fail per item: {payload}");
    for item in &failed {
        let message = item["error"].as_str().unwrap_or_default();
        assert!(
            message.contains("attachments-store") || message.contains("busy"),
            "fail-closed lock diagnostics: {message}"
        );
    }
    {
        let failed_ids: Vec<&str> = failed
            .iter()
            .filter_map(|item| item["id"].as_str())
            .collect();
        assert!(failed_ids.contains(&"TP-1") && failed_ids.contains(&"TP-2"));
    }
    for id in ["TP-1", "TP-2"] {
        // TP-1 legitimately carries the upload reference; TP-2 must have
        // gained nothing while the lock was held.
        if id == "TP-2" {
            assert_eq!(storage_task_refcount(&fx, id), 0, "no ref while locked");
        }
    }
    drop(held);

    // After release the same bulk call completes both items.
    let resp = mcall(
        "task_bulk_reference_add",
        json!({"ids": ["TP-1", "TP-2"], "kind": "attachment", "value": stored}),
    );
    assert!(resp.get("error").is_none(), "bulk after release: {resp}");
    let text = resp["result"]["content"][0]["text"].as_str().unwrap();
    let payload: Value = serde_json::from_str(text).unwrap();
    assert_eq!(payload["failed"].as_array().unwrap().len(), 0);
    assert_eq!(payload["updated"].as_array().unwrap().len(), 2);
}

// ---------------------------------------------------------------------------
// Review B1/B2: mixed-project bulk attachments with per-project stores
// ---------------------------------------------------------------------------

fn seed_project_store_override(fx: &Dev61Fixture, project: &str, dir: &str) {
    let dir_path = fx.tasks_dir.join(project);
    std::fs::create_dir_all(&dir_path).unwrap();
    std::fs::write(
        dir_path.join("config.yml"),
        format!("attachments:\n    dir: \"{dir}\"\n"),
    )
    .unwrap();
}

fn bulk_call(name: &str, ids: &[&str], kind: &str, value: &str) -> Value {
    let resp = mcall(name, json!({"ids": ids, "kind": kind, "value": value}));
    assert!(
        resp.get("error").is_none(),
        "{name} tool-level error: {resp}"
    );
    let text = resp["result"]["content"][0]["text"].as_str().unwrap();
    serde_json::from_str(text).unwrap()
}

fn storage_task_refcount(fx: &Dev61Fixture, id: &str) -> usize {
    let storage = lotar::storage::manager::Storage::try_open(&fx.tasks_dir).unwrap();
    lotar::services::task_service::TaskService::get(&storage, id, None)
        .unwrap()
        .references
        .len()
}

#[test]
fn bulk_attachment_uses_each_tasks_own_configured_store() {
    let fx = isolated_workspace();
    // Project TQ configures a DIFFERENT store via a relative
    // attachments.dir override (supported config surface).
    seed_project_store_override(&fx, "TQ", "@attachments-tq");

    let api = server();
    let _tp1 = rest_create(&api, "TP", "Blob owner A");
    let _tp2 = rest_create(&api, "TP", "Target in A");
    let _tq1 = rest_create(&api, "TQ", "Target in B");

    // Blob exists ONLY in store A (TP's default store).
    let stored_a = upload(&api, "TP-1", "shared.txt", b"store-a-blob");
    assert!(fx.tasks_dir.join("@attachments").join(&stored_a).exists());
    assert!(
        !fx.tasks_dir
            .join("@attachments-tq")
            .join(&stored_a)
            .exists()
    );

    // Bulk add across both projects: TP-2 must attach from store A; TQ-1
    // must FAIL per item (blob absent in store B) without aborting TP-2
    // and without creating any reference on TQ-1.
    let payload = bulk_call(
        "task_bulk_reference_add",
        &["TP-2", "TQ-1"],
        "attachment",
        &stored_a,
    );
    let updated = payload["updated"].as_array().unwrap().clone();
    let failed = payload["failed"].as_array().unwrap().clone();
    assert_eq!(updated.len(), 1, "only TP-2 updated: {payload}");
    assert_eq!(updated[0]["id"], "TP-2");
    assert_eq!(failed.len(), 1, "TQ-1 fails per item: {payload}");
    assert_eq!(failed[0]["id"], "TQ-1");
    assert!(
        failed[0]["error"]
            .as_str()
            .is_some_and(|m| m.contains("not found")),
        "missing-blob diagnostics: {payload}"
    );
    assert_eq!(storage_task_refcount(&fx, "TQ-1"), 0, "no bad ref created");

    // Seeding store B with its own blob makes TQ-1 attachable while TP-2
    // (store A) still fails for it, proving the store lookup is per task
    // project, not per batch.
    let (stored_b, created) = lotar::services::attachment_service::AttachmentService::store_bytes(
        &fx.tasks_dir.join("@attachments-tq"),
        "other.txt",
        b"store-b-blob",
    )
    .unwrap();
    assert!(created);
    let payload = bulk_call(
        "task_bulk_reference_add",
        &["TP-2", "TQ-1"],
        "attachment",
        &stored_b,
    );
    let failed = payload["failed"].as_array().unwrap().clone();
    let updated = payload["updated"].as_array().unwrap().clone();
    assert_eq!(
        failed.len(),
        1,
        "TP-2 must fail for store-B blob: {payload}"
    );
    assert_eq!(failed[0]["id"], "TP-2");
    assert_eq!(
        updated.len(),
        1,
        "TQ-1 attaches from its own store: {payload}"
    );
    assert_eq!(updated[0]["id"], "TQ-1");
    assert_eq!(updated[0]["changed"], true);
}

#[test]
fn bulk_attachment_lock_contention_is_per_store() {
    let fx = isolated_workspace();
    seed_project_store_override(&fx, "TQ", "@attachments-tq");

    let api = server();
    let _tp1 = rest_create(&api, "TP", "Blob owner A");
    let _tp2 = rest_create(&api, "TP", "Target in A");
    let _tq1 = rest_create(&api, "TQ", "Target in B");

    let stored_a = upload(&api, "TP-1", "a.txt", b"store-a-blob");
    let (stored_b, _) = lotar::services::attachment_service::AttachmentService::store_bytes(
        &fx.tasks_dir.join("@attachments-tq"),
        "b.txt",
        b"store-b-blob",
    )
    .unwrap();

    // The store-B blob must genuinely exist so the contended item fails
    // on the LOCK, not on a missing-blob precheck.
    assert!(
        fx.tasks_dir
            .join("@attachments-tq")
            .join(&stored_b)
            .exists(),
        "store-B blob exists before contention"
    );

    // Independently hold STORE B's lock (== another LoTaR process).
    let held = lotar::storage::safety::acquire_storage_lock(
        &fx.tasks_dir.join("@attachments-tq"),
        "attachments-store",
    )
    .unwrap();

    // Mixed batch: TP-2's item (store A, unlocked) succeeds; TQ-1's item
    // (store B, contended) fails per item. No whole-batch abort.
    let resp = mcall(
        "task_bulk_reference_add",
        json!({"ids": ["TP-2", "TQ-1"], "kind": "attachment", "value": stored_a}),
    );
    assert!(resp.get("error").is_none(), "batch must not abort: {resp}");
    let text = resp["result"]["content"][0]["text"].as_str().unwrap();
    let payload: Value = serde_json::from_str(text).unwrap();
    let failed = payload["failed"].as_array().unwrap().clone();
    assert_eq!(payload["updated"].as_array().unwrap().len(), 1, "{payload}");
    assert_eq!(failed.len(), 1, "{payload}");
    assert_eq!(failed[0]["id"], "TQ-1");
    let failure_message = failed[0]["error"].as_str().unwrap_or_default();
    assert!(
        failure_message.contains("attachments-store") || failure_message.contains("busy"),
        "TQ-1 must fail on the held lock, not a missing blob: {failure_message}"
    );
    assert!(
        !failure_message.to_ascii_lowercase().contains("not found"),
        "blob existence must not be the failure cause: {failure_message}"
    );

    drop(held);
    // After release, a store-B item proceeds.
    let payload = bulk_call(
        "task_bulk_reference_add",
        &["TQ-1"],
        "attachment",
        &stored_b,
    );
    assert_eq!(payload["failed"].as_array().unwrap().len(), 0, "{payload}");
    assert!(storage_task_refcount(&fx, "TQ-1") >= 1);
}

#[test]
fn bulk_attachment_remove_detaches_with_missing_blobs() {
    let fx = isolated_workspace();
    seed_project_store_override(&fx, "TQ", "@attachments-tq");

    let api = server();
    let _tp1 = rest_create(&api, "TP", "Remove target A");
    let _tq1 = rest_create(&api, "TQ", "Remove target B");

    // Attach real references, then delete both blobs to simulate
    // reclaimed/missing storage: bulk REMOVE must still detach (B2 — no
    // unconditional existence precheck on remove).
    let stored_a = upload(&api, "TP-1", "a.txt", b"will-vanish");
    let (stored_b, created) = lotar::services::attachment_service::AttachmentService::store_bytes(
        &fx.tasks_dir.join("@attachments-tq"),
        "b.txt",
        b"will-vanish-too",
    )
    .unwrap();
    assert!(created);
    let payload = bulk_call(
        "task_bulk_reference_add",
        &["TQ-1"],
        "attachment",
        &stored_b,
    );
    assert_eq!(payload["failed"].as_array().unwrap().len(), 0, "{payload}");

    std::fs::remove_file(fx.tasks_dir.join("@attachments").join(&stored_a)).unwrap();
    std::fs::remove_file(fx.tasks_dir.join("@attachments-tq").join(&stored_b)).unwrap();

    let payload = bulk_call(
        "task_bulk_reference_remove",
        &["TP-1", "TQ-1"],
        "attachment",
        &stored_a,
    );
    // Per-item outcomes: TQ-1 never had stored_a, so its remove is a
    // no-op success (changed=false), not a failure.
    assert_eq!(payload["failed"].as_array().unwrap().len(), 0, "{payload}");
    let updated = payload["updated"].as_array().unwrap().clone();
    assert_eq!(updated.len(), 2, "{payload}");
    assert_eq!(updated[0]["id"], "TP-1");
    assert_eq!(updated[0]["changed"], true);
    assert_eq!(updated[1]["id"], "TQ-1");
    assert_eq!(updated[1]["changed"], false);
    assert_eq!(storage_task_refcount(&fx, "TP-1"), 0);

    // And the stale store-B reference detaches cleanly too.
    let payload = bulk_call(
        "task_bulk_reference_remove",
        &["TQ-1"],
        "attachment",
        &stored_b,
    );
    assert_eq!(payload["failed"].as_array().unwrap().len(), 0, "{payload}");
    assert_eq!(storage_task_refcount(&fx, "TQ-1"), 0);
}

// ---------------------------------------------------------------------------
// Review N1: store-aware reclamation refcount
// ---------------------------------------------------------------------------

#[test]
fn attachment_refcount_is_store_aware() {
    let fx = isolated_workspace();
    seed_project_store_override(&fx, "TQ", "@attachments-tq");

    let api = server();
    let _tp1 = rest_create(&api, "TP", "A owner");
    let _tq1 = rest_create(&api, "TQ", "B owner");

    // IDENTICAL content in two stores yields the same stored leaf name
    // (content-hash naming): same-name blobs in different stores.
    let stored = upload(&api, "TP-1", "twin.txt", b"twin-blob");
    let (stored_b, created) = lotar::services::attachment_service::AttachmentService::store_bytes(
        &fx.tasks_dir.join("@attachments-tq"),
        "twin.txt",
        b"twin-blob",
    )
    .unwrap();
    assert!(created);
    assert_eq!(stored, stored_b, "identical content -> identical leaf name");

    // Attach the same-name reference to the store-B task as well.
    let payload = bulk_call("task_bulk_reference_add", &["TQ-1"], "attachment", &stored);
    assert_eq!(payload["failed"].as_array().unwrap().len(), 0, "{payload}");

    // Removing store A's copy must NOT see TQ-1's reference (it points at
    // store B's blob): still_referenced=false, store A reclaimed, store B
    // and its reference intact.
    let resp = api.handle_request(&mk_req(
        "POST",
        "/api/tasks/attachments/remove",
        json!({"id": "TP-1", "stored_path": stored}),
    ));
    assert_eq!(resp.status, 200, "{}", String::from_utf8_lossy(&resp.body));
    let data = body_of(&resp)["data"].clone();
    assert_eq!(
        data["still_referenced"], false,
        "cross-store same-hash ref must not block"
    );
    assert_eq!(data["deleted"], true);
    assert!(
        !fx.tasks_dir.join("@attachments").join(&stored).exists(),
        "store A blob reclaimed"
    );
    assert!(
        fx.tasks_dir
            .join("@attachments-tq")
            .join(&stored_b)
            .exists(),
        "store B blob untouched"
    );
    assert_eq!(storage_task_refcount(&fx, "TQ-1"), 1, "TQ-1 keeps its ref");

    // Removing through store B also reclaims independently.
    let resp = api.handle_request(&mk_req(
        "POST",
        "/api/tasks/attachments/remove",
        json!({"id": "TQ-1", "stored_path": stored_b}),
    ));
    assert_eq!(resp.status, 200);
    let data = body_of(&resp)["data"].clone();
    assert_eq!(data["deleted"], true);
    assert!(
        !fx.tasks_dir
            .join("@attachments-tq")
            .join(&stored_b)
            .exists()
    );
}

// ---------------------------------------------------------------------------
// Review N2: code references and preview cannot alias store blobs
// ---------------------------------------------------------------------------

#[test]
fn code_references_reject_store_paths_and_stale_detach_still_works() {
    let fx = isolated_workspace();
    if !fx.repo_root_available {
        eprintln!("skipping: no repo root discoverable by ancestry from CARGO_TARGET_TMPDIR");
        return;
    }
    let api = server();
    let id = rest_create(&api, "TP", "Code guard target");
    let stored = upload(&api, &id, "code.txt", b"code-guard-blob");

    // Service-level attach rejects the store target (anchored and bare).
    let store_rel_anchored = format!("{}#1", fx.rel(&format!(".tasks/@attachments/{stored}")));
    let mut storage = lotar::storage::manager::Storage::new(&fx.tasks_dir);
    for value in [
        store_rel_anchored.clone(),
        fx.attachments().join(&stored).to_string_lossy().to_string(),
    ] {
        let err = lotar::services::reference_service::ReferenceService::attach_code_reference(
            &mut storage,
            &fx.repo_root,
            "TP-1",
            &value,
        )
        .unwrap_err();
        assert!(
            err.to_string().contains("managed attachments store"),
            "code attach must reject store paths: {err}"
        );
    }
    assert_eq!(storage_task_refcount(&fx, "TP-1"), 1, "no code ref created");

    // MCP boundary rejects it too.
    let resp = mcall(
        "task_reference_add",
        json!({"id": "TP-1", "kind": "code", "value": store_rel_anchored}),
    );
    assert!(mcp_failed(&resp), "MCP code add must reject store path");

    // Snippet preview route rejects store targets as well.
    let resp = api.handle_request(&mk_req_query(
        "GET",
        "/api/references/snippet",
        &[("code", store_rel_anchored.as_str())],
        json!({}),
    ));
    assert_eq!(resp.status, 400, "preview must reject store targets");
    assert!(
        String::from_utf8_lossy(&resp.body).contains("managed attachments store"),
        "{}",
        String::from_utf8_lossy(&resp.body)
    );

    // Stale cleanup stays possible: attach a real code reference, delete
    // the file, detach still works without requiring the file to exist.
    let example_ref = format!("{}#1", fx.rel("src/example.rs"));
    let outcome = lotar::services::reference_service::ReferenceService::attach_code_reference(
        &mut storage,
        &fx.repo_root,
        "TP-1",
        &example_ref,
    )
    .unwrap();
    let added = outcome.changed;
    assert!(added);
    std::fs::remove_file(fx.workspace.join("src/example.rs")).unwrap();
    let removed = lotar::services::reference_service::ReferenceService::detach_code_reference(
        &mut storage,
        "TP-1",
        &example_ref,
    )
    .unwrap()
    .changed;
    assert!(removed, "stale code detach must not require the file");
}

// ---------------------------------------------------------------------------
// Review blocker 2: the store guard fails closed on invalid configuration
// ---------------------------------------------------------------------------

#[test]
fn store_guard_fails_closed_on_invalid_configuration() {
    let fx = isolated_workspace();
    if !fx.repo_root_available {
        eprintln!("skipping: no repo root discoverable by ancestry from CARGO_TARGET_TMPDIR");
        return;
    }
    let api = server();
    let id = rest_create(&api, "TP", "Fail-closed config target");
    let example = fx.rel("src/example.rs");

    let assert_denied_everywhere = |context: &str| {
        // Service: ordinary repo file attach is DENIED because the store
        // set cannot be verified (an unknown custom store might contain
        // the aliased blob).
        let mut storage = lotar::storage::manager::Storage::new(&fx.tasks_dir);
        let outcome = lotar::services::reference_service::ReferenceService::attach_file_reference(
            &mut storage,
            &fx.repo_root,
            "TP-1",
            &example,
        );
        let err = match outcome {
            Err(e) => e,
            Ok(outcome) => panic!(
                "{context}: expected denial but attach succeeded (added={}) refs={:?}",
                outcome.changed, outcome.task.references
            ),
        };
        assert!(
            err.to_string().contains("attachment store guard"),
            "{context}: file attach denied with config diagnostics: {err}"
        );
        let err = lotar::services::reference_service::ReferenceService::attach_code_reference(
            &mut storage,
            &fx.repo_root,
            "TP-1",
            &format!("{example}#1"),
        )
        .unwrap_err();
        assert!(
            err.to_string().contains("attachment store guard"),
            "{context}: code attach denied: {err}"
        );
        let err = lotar::services::reference_service::ReferenceService::snippet_for_code_guarded(
            &fx.tasks_dir,
            &fx.repo_root,
            &format!("{example}#1"),
            1,
            1,
        )
        .unwrap_err();
        assert!(
            err.contains("attachment store guard"),
            "{context}: preview denied: {err}"
        );

        // REST boundaries surface the same denial.
        let resp = api.handle_request(&mk_req(
            "POST",
            "/api/tasks/references/file/add",
            json!({"id": id, "path": example}),
        ));
        assert_eq!(resp.status, 400, "{context}: REST file add denied");
        let resp = api.handle_request(&mk_req_query(
            "GET",
            "/api/references/snippet",
            &[("code", format!("{example}#1").as_str())],
            json!({}),
        ));
        assert_eq!(resp.status, 400, "{context}: REST preview denied");

        // MCP boundary surfaces the same denial.
        let resp = mcall(
            "task_reference_add",
            json!({"id": "TP-1", "kind": "file", "value": example}),
        );
        assert!(mcp_failed(&resp), "{context}: MCP file add denied: {resp}");

        // User-owned data is preserved: no reference was created.
        assert_eq!(storage_task_refcount(&fx, "TP-1"), 0, "{context}: no refs");
    };

    // Invalid GLOBAL config (unbalanced flow sequence): deny until fixed.
    std::fs::write(
        fx.tasks_dir.join("config.yml"),
        "attachments:\n    dir: [oops\n",
    )
    .unwrap();
    assert_denied_everywhere("invalid global config");

    // Invalid PROJECT config in an UNRELATED project also denies: a
    // per-project error must not silently miss that project's custom
    // store.
    std::fs::write(
        fx.tasks_dir.join("config.yml"),
        "default:\n    project: TP\n",
    )
    .unwrap();
    std::fs::create_dir_all(fx.tasks_dir.join("OTHER")).unwrap();
    std::fs::write(
        fx.tasks_dir.join("OTHER").join("config.yml"),
        "attachments:\n    dir: {{oops\n",
    )
    .unwrap();
    assert_denied_everywhere("invalid project config");

    // Genuinely missing config is the standard default and allows normal
    // repository references again (recovery after fixing the config).
    std::fs::remove_file(fx.tasks_dir.join("OTHER").join("config.yml")).unwrap();
    std::fs::write(
        fx.tasks_dir.join("config.yml"),
        "default:\n    project: TP\n",
    )
    .unwrap();
    let mut storage = lotar::storage::manager::Storage::new(&fx.tasks_dir);
    let outcome = lotar::services::reference_service::ReferenceService::attach_file_reference(
        &mut storage,
        &fx.repo_root,
        "TP-1",
        &example,
    )
    .unwrap();
    let (task, added) = (outcome.task, outcome.changed);
    assert!(added, "valid config restores ordinary file references");
    assert!(
        task.references
            .iter()
            .any(|r| r.file.as_deref() == Some(example.as_str()))
    );
}

// ---------------------------------------------------------------------------
// Prefix-route download contract (OpenAPI `/api/attachments/h/{hash}/{filename}`)
// ---------------------------------------------------------------------------

#[test]
fn hash_prefix_route_downloads_blob_contract() {
    // No repo root needed: the prefix route only touches the store.
    let _fx = isolated_workspace();
    let api = server();
    let id = rest_create(&api, "TP", "Hash route target");
    let content = b"hash-route-blob";
    let stored = upload(&api, &id, "evidence.bin", content);
    let hash = lotar::services::attachment_service::AttachmentService::extract_hash_tag(&stored)
        .expect("stored leaf carries a hash tag");

    // Prefix dispatch through the real server: template route serves bytes.
    let resp = api.handle_request(&mk_req(
        "GET",
        &format!("/api/attachments/h/{hash}/original-name.bin"),
        json!({}),
    ));
    assert_eq!(resp.status, 200, "{}", String::from_utf8_lossy(&resp.body));
    assert_eq!(resp.body, content.to_vec());
    let disposition = resp
        .headers
        .iter()
        .find(|(name, _)| name.eq_ignore_ascii_case("content-disposition"))
        .map(|(_, value)| value.clone())
        .unwrap_or_default();
    assert!(
        disposition.starts_with("inline"),
        "default disposition inline: {disposition}"
    );

    // download=1 forces a download disposition.
    let resp = api.handle_request(&mk_req_query(
        "GET",
        &format!("/api/attachments/h/{hash}/original-name.bin"),
        &[("download", "1")],
        json!({}),
    ));
    assert_eq!(resp.status, 200);
    assert_eq!(resp.body, content.to_vec());
    let disposition = resp
        .headers
        .iter()
        .find(|(name, _)| name.eq_ignore_ascii_case("content-disposition"))
        .map(|(_, value)| value.clone())
        .unwrap_or_default();
    assert!(
        disposition.starts_with("attachment"),
        "download flag forces attachment disposition: {disposition}"
    );

    // Unknown hash fails closed with 404 and no bytes.
    let resp = api.handle_request(&mk_req(
        "GET",
        "/api/attachments/h/0123456789abcdef0123456789abcdef/missing.bin",
        json!({}),
    ));
    assert_eq!(resp.status, 404, "unknown hash must 404");

    // The prefix template is surfaced by production route introspection in
    // the exact spec shape (guarded by the OpenAPI contract test).
    let mut introspected = lotar::api_server::ApiServer::new();
    lotar::routes::initialize(&mut introspected);
    assert!(
        introspected
            .registered_routes()
            .iter()
            .any(|(method, path)| method == "GET" && path == "/api/attachments/h/{hash}/{filename}"),
        "prefix template must be visible in registered_routes"
    );
}

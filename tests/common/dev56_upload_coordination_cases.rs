//! DEV-56 review regression: concurrent attachment uploads of identical
//! content must be coordinated so a request that fails and rolls its blob
//! back can never break a concurrent request that deduped the same blob.
//! Included from `routes/tasks.rs` via `#[path]` so `#[cfg(test)]`-only
//! fault hooks stay unreachable from production code paths.

use super::*;
use std::path::PathBuf;
use std::sync::mpsc::channel;

#[path = "env_mutex.rs"]
mod env_mutex;
use env_mutex::EnvVarGuard;

struct UploadFixture {
    _tmp: tempfile::TempDir,
    tasks_dir: PathBuf,
    _guard: EnvVarGuard,
}

fn upload_fixture() -> UploadFixture {
    let tmp = tempfile::tempdir().unwrap();
    let tasks_dir = tmp.path().join("main").join(".tasks");
    std::fs::create_dir_all(&tasks_dir).unwrap();
    // Set before any worker threads spawn and restored (in Drop) after they
    // join, so no thread observes a concurrent env mutation.
    let guard = EnvVarGuard::set("LOTAR_TASKS_DIR", &tasks_dir.to_string_lossy());
    UploadFixture {
        _tmp: tmp,
        tasks_dir,
        _guard: guard,
    }
}

fn mk_upload_req(id: &str, filename: &str, bytes: &[u8]) -> HttpRequest {
    use base64::Engine;
    HttpRequest {
        method: "POST".to_string(),
        path: "/api/tasks/attachments/upload".to_string(),
        query: std::collections::HashMap::new(),
        headers: std::collections::HashMap::new(),
        body: serde_json::to_vec(&serde_json::json!({
            "id": id,
            "filename": filename,
            "content_base64": base64::engine::general_purpose::STANDARD.encode(bytes),
        }))
        .unwrap(),
    }
}

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

/// Request A stores the blob, parks between store and attach (holding the
/// store coordination lock), then fails its attach and rolls the blob back.
/// Request B uploads identical content concurrently: it must be serialized
/// behind A's decision, then re-create the blob and attach cleanly. Outcome:
/// A errored with no orphan, B attached with an intact reference.
#[test]
fn concurrent_failed_upload_cannot_break_a_deduping_peer() {
    let fixture = upload_fixture();
    let attachments = fixture.tasks_dir.join("@attachments");

    // Two target tasks in the primary root.
    let mut seed = crate::storage::manager::Storage::new(&fixture.tasks_dir);
    for title in ["A target", "B target"] {
        let task = crate::storage::task::Task::new(
            fixture.tasks_dir.clone(),
            title.to_string(),
            crate::types::Priority::from("Medium"),
        );
        seed.add(&task, "TP", None).unwrap();
    }

    let (stored_tx, stored_rx) = channel::<()>();
    let (release_tx, release_rx) = channel::<()>();
    let tasks_dir_a = fixture.tasks_dir.clone();

    let a = std::thread::spawn(move || {
        // Thread-local injection: park between store and attach, then fail.
        crate::services::attachment_service::upload_fault::arm_park_after_store(
            stored_tx, release_rx,
        );
        crate::services::attachment_service::upload_fault::fail_next_attach();
        let mut api = ApiServer::new();
        crate::routes::initialize(&mut api);
        (
            api.handle_request(&mk_upload_req("TP-1", "a.txt", b"shared-blob"))
                .status,
            tasks_dir_a,
        )
    });

    // Deterministic ordering: A created the blob and is parked (holding the
    // store lock) before B starts. B attempts the same content concurrently
    // and must wait for A's rollback decision.
    stored_rx
        .recv_timeout(std::time::Duration::from_secs(10))
        .expect("A must signal after storing the blob");
    let tasks_dir_b = fixture.tasks_dir.clone();
    let b = std::thread::spawn(move || {
        let mut api = ApiServer::new();
        crate::routes::initialize(&mut api);
        (
            api.handle_request(&mk_upload_req("TP-2", "b.txt", b"shared-blob"))
                .status,
            tasks_dir_b,
        )
    });

    // Release A: attach fails (injected), cleanup deletes its just-created
    // blob, and only then may B proceed to re-create and attach.
    release_tx.send(()).unwrap();

    let (status_a, tasks_dir) = a.join().expect("A thread");
    let (status_b, _) = b.join().expect("B thread");

    assert_ne!(status_a, 200, "A must fail after injected attach failure");
    assert_eq!(status_b, 200, "B must succeed: A={status_a} B={status_b}");

    // Exactly one blob exists (B's fresh copy); no orphan from A.
    assert_eq!(count_stored_files(&attachments), 1, "no orphan, one blob");

    // B's task references the stored blob; A's task references nothing.
    let storage = crate::storage::manager::Storage::try_open(&tasks_dir).unwrap();
    let b_task = crate::services::task_service::TaskService::get(&storage, "TP-2", None).unwrap();
    assert!(
        b_task.references.iter().any(|r| r.file.is_some()),
        "B keeps its reference: {:?}",
        b_task.references
    );
    let a_task = crate::services::task_service::TaskService::get(&storage, "TP-1", None).unwrap();
    assert!(
        a_task.references.iter().all(|r| r.file.is_none()),
        "A must not keep a dangling reference: {:?}",
        a_task.references
    );

    // The blob B references still exists on disk (not removed by A's cleanup).
    let stored_path = b_task
        .references
        .iter()
        .find_map(|r| r.file.clone())
        .unwrap();
    let leaf = stored_path
        .rsplit('/')
        .next()
        .unwrap_or(stored_path.trim_start_matches('@'));
    let attachments_root = attachments.clone();
    assert!(
        std::fs::read_dir(&attachments_root)
            .map(|entries| {
                entries
                    .flatten()
                    .any(|e| e.file_name().to_string_lossy() == leaf)
            })
            .unwrap_or(false),
        "B's blob {stored_path} still exists under {}",
        attachments_root.display()
    );
}

/// Cross-instance coverage without a subprocess: an independently held fs2
/// file lock on the store (as any other LoTaR process would hold) makes the
/// upload fail closed within the standard bound, writing no blob; once
/// released, the same upload succeeds.
#[test]
fn upload_fails_closed_while_another_instance_holds_the_store_lock() {
    let fixture = upload_fixture();
    let attachments = fixture.tasks_dir.join("@attachments");
    let mut seed = crate::storage::manager::Storage::new(&fixture.tasks_dir);
    let task = crate::storage::task::Task::new(
        fixture.tasks_dir.clone(),
        "Locked store target".to_string(),
        crate::types::Priority::from("Medium"),
    );
    seed.add(&task, "TP", None).unwrap();

    // Held exactly as another API process would hold it (the store dir
    // already exists in production; create it like lock_store does).
    std::fs::create_dir_all(&attachments).unwrap();
    let held =
        crate::storage::safety::acquire_storage_lock(&attachments, "attachments-store").unwrap();
    let mut api = ApiServer::new();
    crate::routes::initialize(&mut api);
    let resp = api.handle_request(&mk_upload_req("TP-1", "held.txt", b"while-locked"));
    assert_ne!(
        resp.status, 200,
        "upload must fail closed while store is locked"
    );
    assert!(
        resp.body.len() < 4096,
        "bounded failure diagnostics: {}",
        String::from_utf8_lossy(&resp.body)
    );
    assert_eq!(count_stored_files(&attachments), 0, "no blob written");

    drop(held);
    let resp = api.handle_request(&mk_upload_req("TP-1", "held.txt", b"while-locked"));
    assert_eq!(resp.status, 200, "upload proceeds after release");
    assert_eq!(count_stored_files(&attachments), 1);
}

/// Upload rollback and remove reclamation serialize on the same store lock:
/// while a failing upload parks mid-critical-section, a concurrent remove of
/// an UNRELATED committed attachment waits, then completes cleanly. Neither
/// request leaves an orphan or a dangling reference.
#[test]
fn upload_rollback_and_remove_reclaim_serialize_on_the_store_lock() {
    let fixture = upload_fixture();
    let attachments = fixture.tasks_dir.join("@attachments");
    let mut seed = crate::storage::manager::Storage::new(&fixture.tasks_dir);
    for _ in 0..3 {
        let task = crate::storage::task::Task::new(
            fixture.tasks_dir.clone(),
            "Seed target".to_string(),
            crate::types::Priority::from("Medium"),
        );
        seed.add(&task, "TP", None).unwrap();
    }

    // Commit one attachment for TP-3 BEFORE arming any fault hooks.
    let mut api = ApiServer::new();
    crate::routes::initialize(&mut api);
    let committed = api.handle_request(&mk_upload_req("TP-3", "committed.txt", b"committed-blob"));
    assert_eq!(committed.status, 200);
    let committed_body: serde_json::Value = serde_json::from_slice(&committed.body).unwrap();
    let committed_path = committed_body["data"]["stored_path"]
        .as_str()
        .unwrap()
        .to_string();
    assert_eq!(count_stored_files(&attachments), 1);

    let (stored_tx, stored_rx) = channel::<()>();
    let (release_tx, release_rx) = channel::<()>();
    let tasks_dir_a = fixture.tasks_dir.clone();

    let a = std::thread::spawn(move || {
        crate::services::attachment_service::upload_fault::arm_park_after_store(
            stored_tx, release_rx,
        );
        crate::services::attachment_service::upload_fault::fail_next_attach();
        let mut api = ApiServer::new();
        crate::routes::initialize(&mut api);
        (
            api.handle_request(&mk_upload_req("TP-1", "rollback.txt", b"rollback-blob"))
                .status,
            tasks_dir_a,
        )
    });

    stored_rx
        .recv_timeout(std::time::Duration::from_secs(10))
        .expect("A stored and parked");

    // Concurrent remove of the committed attachment: serialized behind A's
    // parked critical section (blocks on the store lock, then proceeds).
    let tasks_dir_b = fixture.tasks_dir.clone();
    let remove_path = committed_path.clone();
    let b = std::thread::spawn(move || {
        let mut api = ApiServer::new();
        crate::routes::initialize(&mut api);
        let resp = api.handle_request(&mk_remove_req("TP-3", &remove_path));
        (resp.status, tasks_dir_b)
    });

    release_tx.send(()).unwrap();
    let (status_a, tasks_dir) = a.join().expect("A thread");
    let (status_b, _) = b.join().expect("B thread");

    assert_ne!(status_a, 200, "A failed as injected");
    assert_eq!(status_b, 200, "B remove completed after serialization");

    // No orphan from A, and the removed committed blob is gone.
    assert_eq!(
        count_stored_files(&attachments),
        0,
        "rollback blob reaped, committed blob reclaimed"
    );

    // Neither task keeps a dangling file reference.
    let storage = crate::storage::manager::Storage::try_open(&tasks_dir).unwrap();
    for id in ["TP-1", "TP-3"] {
        let dto = crate::services::task_service::TaskService::get(&storage, id, None).unwrap();
        assert!(
            dto.references.iter().all(|r| r.file.is_none()),
            "{id} must not keep a dangling reference: {:?}",
            dto.references
        );
    }
}

fn mk_remove_req(id: &str, stored_path: &str) -> HttpRequest {
    HttpRequest {
        method: "POST".to_string(),
        path: "/api/tasks/attachments/remove".to_string(),
        query: std::collections::HashMap::new(),
        headers: std::collections::HashMap::new(),
        body: serde_json::to_vec(&serde_json::json!({
            "id": id,
            "stored_path": stored_path,
        }))
        .unwrap(),
    }
}

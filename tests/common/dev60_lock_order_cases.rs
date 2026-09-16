//! DEV-60 lock-ordering proof (included in `reference_service`'s test
//! build so the `cfg(test)` store-lock counter is active): post-commit
//! reference dispatch refuses to run while an attachment store lock is
//! held on the same thread, and runs normally after it drops.

use super::*;
use crate::services::attachment_service::AttachmentService;

fn dev60_workspace(project: &str) -> (tempfile::TempDir, std::path::PathBuf) {
    let tmp = tempfile::tempdir().unwrap();
    let tasks_dir = tmp.path().join(".tasks");
    std::fs::create_dir_all(&tasks_dir).unwrap();
    std::fs::write(
        crate::utils::paths::global_config_path(&tasks_dir),
        format!("default.project: {project}\nissue.states: [Todo, InProgress, Done]\n"),
    )
    .unwrap();
    (tmp, tasks_dir)
}

#[test]
fn dispatch_post_commit_refuses_to_run_while_store_lock_is_held() {
    let (tmp, tasks_dir) = dev60_workspace("D6L1");
    let mut storage = crate::storage::manager::Storage::new(&tasks_dir);
    let task = crate::services::task_service::TaskService::create(
        &mut storage,
        crate::api_types::TaskCreate {
            title: "Lock ordering target".to_string(),
            project: Some("D6L1".to_string()),
            ..Default::default()
        },
    )
    .expect("create task");
    let outcome = ReferenceService::attach_link_reference(
        &mut storage,
        &task.id,
        "https://example.com/locked",
    )
    .expect("attach");

    // Hold the store lock on this thread: dispatch must trip its guard
    // (deterministic assertion, not timing) instead of running automation
    // under the lock.
    let store_root = tasks_dir.join("@attachments");
    let guard = AttachmentService::lock_store(&store_root).expect("store lock");
    assert_eq!(AttachmentService::store_locks_held(), 1);
    let panicked = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        ReferenceService::dispatch_post_commit(&mut storage, &outcome)
    }))
    .is_err();
    assert!(
        panicked,
        "dispatch must refuse to run while the store lock is held"
    );
    drop(guard);
    assert_eq!(AttachmentService::store_locks_held(), 0);

    // After the lock drops, dispatch runs (a harmless no-op without
    // automation rules).
    ReferenceService::dispatch_post_commit(&mut storage, &outcome);
    drop(tmp);
}

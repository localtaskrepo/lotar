use predicates::prelude::*;
use tempfile::TempDir;

mod common;
use common::env_mutex::EnvVarGuard;

use lotar::api_types::TaskCreate;
use lotar::services::task_service::TaskService;
use lotar::storage::manager::Storage;
use lotar::utils::paths;

fn write_minimal_config(tasks_dir: &std::path::Path) {
    let content = r#"default.project: TEST
issue.states: [Todo, InProgress, Done]
issue.types: [Feature, Bug, Chore]
issue.priorities: [Low, Medium, High]
"#;
    std::fs::write(paths::global_config_path(tasks_dir), content).unwrap();
}

/// Tests that need a real Git repository (see `common::require_git`).
/// The `git_required` module path is what the gitless nextest profile
/// excludes when the runtime Git capability probe fails (DEV-79).
mod git_required {
    use super::*;

    #[test]
    fn task_reference_add_and_remove_link_file_and_code() {
        crate::common::require_git();
        let temp = TempDir::new().unwrap();

        // Make this temp dir look like a git repo so code/file references can resolve.
        std::fs::create_dir_all(temp.path().join(".git")).unwrap();
        std::fs::create_dir_all(temp.path().join("src")).unwrap();
        std::fs::write(temp.path().join("src/example.rs"), "fn main() {}\n").unwrap();

        let tasks_dir = temp.path().join(".tasks");
        std::fs::create_dir_all(&tasks_dir).unwrap();
        write_minimal_config(&tasks_dir);

        let _guard = EnvVarGuard::set("LOTAR_TASKS_DIR", &tasks_dir.to_string_lossy());

        // Create a task with a known ID.
        let mut storage = Storage::new(&tasks_dir.clone());
        let created = TaskService::create(
            &mut storage,
            TaskCreate {
                title: "Reference CLI".to_string(),
                project: Some("TEST".to_string()),
                ..TaskCreate::default()
            },
        )
        .expect("create task");

        // Add link reference
        let mut cmd = crate::common::lotar_cmd().unwrap();
        cmd.current_dir(temp.path())
            .args([
                "task",
                "reference",
                "add",
                "link",
                &created.id,
                "https://example.com",
            ])
            .assert()
            .success()
            .stdout(predicate::str::contains("reference updated"));

        // Add file reference
        let mut cmd = crate::common::lotar_cmd().unwrap();
        cmd.current_dir(temp.path())
            .args([
                "task",
                "reference",
                "add",
                "file",
                &created.id,
                "src/example.rs",
            ])
            .assert()
            .success();

        // Add code reference
        let mut cmd = crate::common::lotar_cmd().unwrap();
        cmd.current_dir(temp.path())
            .args([
                "task",
                "reference",
                "add",
                "code",
                &created.id,
                "src/example.rs#1",
            ])
            .assert()
            .success();

        // Verify persisted
        let storage = Storage::new(&tasks_dir.clone());
        let task = storage.get(&created.id, "TEST").expect("task should exist");
        assert!(
            task.references
                .iter()
                .any(|r| r.link.as_deref() == Some("https://example.com"))
        );
        assert!(
            task.references
                .iter()
                .any(|r| r.file.as_deref() == Some("src/example.rs"))
        );
        assert!(
            task.references
                .iter()
                .any(|r| r.code.as_deref() == Some("src/example.rs#1"))
        );

        // Remove code reference
        let mut cmd = crate::common::lotar_cmd().unwrap();
        cmd.current_dir(temp.path())
            .args([
                "task",
                "reference",
                "remove",
                "code",
                &created.id,
                "src/example.rs#1",
            ])
            .assert()
            .success();

        // Remove file reference
        let mut cmd = crate::common::lotar_cmd().unwrap();
        cmd.current_dir(temp.path())
            .args([
                "task",
                "reference",
                "remove",
                "file",
                &created.id,
                "src/example.rs",
            ])
            .assert()
            .success();

        // Remove link reference
        let mut cmd = crate::common::lotar_cmd().unwrap();
        cmd.current_dir(temp.path())
            .args([
                "task",
                "reference",
                "remove",
                "link",
                &created.id,
                "https://example.com",
            ])
            .assert()
            .success();

        // Verify removed
        let storage = Storage::new(&tasks_dir);
        let task = storage.get(&created.id, "TEST").expect("task should exist");
        assert!(
            !task
                .references
                .iter()
                .any(|r| r.link.as_deref() == Some("https://example.com"))
        );
        assert!(
            !task
                .references
                .iter()
                .any(|r| r.file.as_deref() == Some("src/example.rs"))
        );
        assert!(
            !task
                .references
                .iter()
                .any(|r| r.code.as_deref() == Some("src/example.rs#1"))
        );
    }
}

#[test]
fn task_reference_add_and_remove_attachment_kind() {
    // Managed attachment references never resolve the repository root, so
    // this contract test runs even in sandboxes without `.git` markers.
    let temp = TempDir::new().unwrap();

    let _ = std::fs::create_dir_all(temp.path().join(".git"));

    let tasks_dir = temp.path().join(".tasks");
    std::fs::create_dir_all(&tasks_dir).unwrap();
    write_minimal_config(&tasks_dir);

    let _guard = EnvVarGuard::set("LOTAR_TASKS_DIR", &tasks_dir.to_string_lossy());

    let mut storage = Storage::new(&tasks_dir.clone());
    let created = TaskService::create(
        &mut storage,
        TaskCreate {
            title: "Attachment CLI".to_string(),
            project: Some("TEST".to_string()),
            ..TaskCreate::default()
        },
    )
    .expect("create task");

    // Seed a managed blob in the store the way an upload would.
    let store_root = tasks_dir.join("@attachments");
    let (stored_name, created_blob) =
        lotar::services::attachment_service::AttachmentService::store_bytes(
            &store_root,
            "evidence.txt",
            b"cli-attachment-blob",
        )
        .expect("store blob");
    assert!(created_blob);

    // Add managed attachment reference.
    let mut cmd = crate::common::lotar_cmd().unwrap();
    cmd.current_dir(temp.path())
        .args([
            "task",
            "reference",
            "add",
            "attachment",
            &created.id,
            &stored_name,
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("reference updated"));

    let storage = Storage::new(&tasks_dir.clone());
    let task = storage.get(&created.id, "TEST").expect("task should exist");
    assert!(
        task.references
            .iter()
            .any(|r| r.attachment.as_deref() == Some(stored_name.as_str())),
        "attachment kind writes the typed attachment key: {:?}",
        task.references
    );

    // Remove detaches the reference without deleting the blob.
    let mut cmd = crate::common::lotar_cmd().unwrap();
    cmd.current_dir(temp.path())
        .args([
            "task",
            "reference",
            "remove",
            "attachment",
            &created.id,
            &stored_name,
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("reference updated"));

    let storage = Storage::new(&tasks_dir.clone());
    let task = storage.get(&created.id, "TEST").expect("task should exist");
    assert!(
        task.references
            .iter()
            .all(|r| r.attachment.as_deref() != Some(stored_name.as_str()))
    );
    assert!(
        store_root.join(&stored_name).exists(),
        "CLI attachment detach is reference-only; the blob stays"
    );

    // Adding an unknown blob fails closed.
    let mut cmd = crate::common::lotar_cmd().unwrap();
    cmd.current_dir(temp.path())
        .args([
            "task",
            "reference",
            "add",
            "attachment",
            &created.id,
            "missing.0123456789abcdef0123456789abcdef.txt",
        ])
        .assert()
        .failure();
}

/// `reference add upload` stores a local file in the task project's
/// attachments store and attaches it (DEV-224): one stored copy per identical
/// file, the project's own `attachments.dir`, and nothing stored for an
/// unknown task.
#[test]
fn task_reference_upload_stores_and_attaches_a_local_file() {
    let temp = TempDir::new().unwrap();
    let tasks_dir = temp.path().join(".tasks");
    std::fs::create_dir_all(tasks_dir.join("TEST")).unwrap();
    write_minimal_config(&tasks_dir);
    // The project keeps its attachments inside its own folder.
    std::fs::write(
        tasks_dir.join("TEST").join("config.yml"),
        "attachments:\n  dir: TEST/@attachments\n",
    )
    .unwrap();
    let _guard = EnvVarGuard::set("LOTAR_TASKS_DIR", &tasks_dir.to_string_lossy());

    let mut storage = Storage::new(&tasks_dir.clone());
    let created = TaskService::create(
        &mut storage,
        TaskCreate {
            title: "Upload CLI".to_string(),
            project: Some("TEST".to_string()),
            ..TaskCreate::default()
        },
    )
    .expect("create task");

    let shot = temp.path().join("board header.txt");
    std::fs::write(&shot, b"snippet-bytes").unwrap();
    let upload = |id: &str| {
        let mut cmd = crate::common::lotar_cmd().unwrap();
        cmd.current_dir(temp.path()).args([
            "task",
            "reference",
            "add",
            "upload",
            id,
            shot.to_str().unwrap(),
        ]);
        cmd
    };

    upload(&created.id)
        .assert()
        .success()
        .stdout(predicate::str::contains("Stored as board-header."))
        .stdout(predicate::str::contains("reference updated"));
    let store = tasks_dir.join("TEST").join("@attachments");
    let stored: Vec<String> = std::fs::read_dir(&store)
        .unwrap()
        .filter_map(|entry| entry.ok()?.file_name().into_string().ok())
        .filter(|name| !name.starts_with('.'))
        .collect();
    assert_eq!(stored.len(), 1, "one stored file: {stored:?}");
    let task = Storage::new(&tasks_dir.clone())
        .get(&created.id, "TEST")
        .expect("task should exist");
    assert!(
        task.references
            .iter()
            .any(|r| r.attachment.as_deref() == Some(stored[0].as_str())),
        "the task references the stored file: {:?}",
        task.references
    );

    // Uploading the same file again keeps one copy and one reference.
    upload(&created.id)
        .assert()
        .success()
        .stdout(predicate::str::contains("already in desired state"));
    assert_eq!(
        std::fs::read_dir(&store)
            .unwrap()
            .filter(|entry| entry
                .as_ref()
                .is_ok_and(|e| !e.file_name().to_string_lossy().starts_with('.')))
            .count(),
        1
    );

    // An unknown task stores nothing new.
    upload("TEST-99").assert().failure();
    let missing = temp.path().join("missing.png");
    let mut cmd = crate::common::lotar_cmd().unwrap();
    cmd.current_dir(temp.path())
        .args([
            "task",
            "reference",
            "add",
            "upload",
            &created.id,
            missing.to_str().unwrap(),
        ])
        .assert()
        .failure()
        .stderr(predicate::str::contains("Cannot read"));
}

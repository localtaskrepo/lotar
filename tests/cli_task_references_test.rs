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

#[test]
fn task_reference_add_and_remove_link_file_and_code() {
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
    let mut storage = Storage::new(tasks_dir.clone());
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
    let storage = Storage::new(tasks_dir.clone());
    let task = storage
        .get(&created.id, "TEST".to_string())
        .expect("task should exist");
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
    let storage = Storage::new(tasks_dir);
    let task = storage
        .get(&created.id, "TEST".to_string())
        .expect("task should exist");
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

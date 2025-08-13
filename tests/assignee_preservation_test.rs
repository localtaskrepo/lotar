mod common;
use crate::common::env_mutex::lock_var;
use lotar::api_types::{TaskCreate, TaskUpdate};
use lotar::services::task_service::TaskService;
use lotar::storage::manager::Storage;
use lotar::types::TaskStatus;
use lotar::utils::paths;

#[test]
fn first_change_does_not_overwrite_existing_assignee() {
    let _env_tasks = lock_var("LOTAR_TASKS_DIR");
    let tmp = tempfile::tempdir().unwrap();
    let tasks_dir = tmp.path().join(".tasks");
    std::fs::create_dir_all(&tasks_dir).unwrap();
    unsafe {
        std::env::set_var("LOTAR_TASKS_DIR", tasks_dir.to_string_lossy().to_string());
    }

    // Configure default_reporter so auto-assign would choose it if eligible
    std::fs::write(
        paths::global_config_path(&tasks_dir),
        "default_project: AAA\nissue_states: [Todo, InProgress, Done]\nissue_types: [Feature, Bug, Chore]\nissue_priorities: [Low, Medium, High]\ndefault_reporter: ryan\n",
    ).unwrap();

    let mut storage = Storage::new(tasks_dir.clone());
    let created = TaskService::create(
        &mut storage,
        TaskCreate {
            title: "Preset assignee".into(),
            project: Some("AAA".into()),
            priority: None,
            task_type: None,
            reporter: None,
            assignee: Some("sam".into()),
            due_date: None,
            effort: None,
            description: None,
            category: None,
            tags: vec![],
            custom_fields: None,
        },
    )
    .unwrap();

    // Change status away from default; assignee should remain "sam"
    let mut storage = Storage::new(tasks_dir.clone());
    let updated = TaskService::update(
        &mut storage,
        &created.id,
        TaskUpdate {
            status: Some(TaskStatus::InProgress),
            ..Default::default()
        },
    )
    .unwrap();
    assert_eq!(
        updated.assignee.as_deref(),
        Some("sam"),
        "existing assignee must be preserved on first change"
    );

    unsafe {
        std::env::remove_var("LOTAR_TASKS_DIR");
    }
}

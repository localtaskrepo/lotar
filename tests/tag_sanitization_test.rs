mod common;
use common::env_mutex::EnvVarGuard;
use lotar::api_types::{TaskCreate, TaskUpdate};
use lotar::services::task_service::TaskService;
use lotar::storage::manager::Storage;
use lotar::types::{Priority, TaskType};
use lotar::utils::paths;

fn write_minimal_config(tasks_dir: &std::path::Path) {
    let content = r#"default.project: TEST
issue.states: [Todo, InProgress, Done]
issue.types: [Feature, Bug, Chore]
issue.priorities: [Low, Medium, High]
issue.tags: [*]
"#;
    std::fs::write(paths::global_config_path(tasks_dir), content).unwrap();
}

#[test]
fn service_filters_blank_tags_on_create_and_update() {
    let temp = tempfile::tempdir().unwrap();
    let repo_root = temp.path();
    let tasks_dir = repo_root.join(".tasks");
    std::fs::create_dir_all(&tasks_dir).unwrap();
    let _guard = EnvVarGuard::set("LOTAR_TASKS_DIR", &tasks_dir.to_string_lossy());
    write_minimal_config(&tasks_dir);

    let mut storage = Storage::new(tasks_dir.clone());
    let req = TaskCreate {
        title: "Tag normalization".to_string(),
        project: Some("TEST".to_string()),
        priority: Some(Priority::from("Medium")),
        task_type: Some(TaskType::from("Feature")),
        reporter: None,
        assignee: None,
        due_date: None,
        effort: None,
        description: None,
        tags: vec!["api".into(), " ".into(), "backend ".into(), "".into()],
        relationships: None,
        custom_fields: None,
    };

    let created = TaskService::create(&mut storage, req).expect("create task");
    assert_eq!(created.tags, vec!["api", "backend"]);

    let updated = TaskService::update(
        &mut storage,
        &created.id,
        TaskUpdate {
            tags: Some(vec![" new ".into(), "  ".into(), "ops".into()]),
            ..TaskUpdate::default()
        },
    )
    .expect("update task");

    assert_eq!(updated.tags, vec!["new", "ops"]);

    // ensure task persisted without empties
    let fetched = TaskService::get(&storage, &created.id, None).expect("get task");
    assert_eq!(fetched.tags, vec!["new", "ops"]);
}

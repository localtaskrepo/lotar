mod common;
use crate::common::env_mutex::lock_var;
use lotar::api_types::TaskCreate;
use lotar::services::task_service::TaskService;
use lotar::storage::manager::Storage;

#[test]
fn env_default_reporter_is_respected() {
    let _env_tasks = lock_var("LOTAR_TASKS_DIR");
    let _env_rep = lock_var("LOTAR_DEFAULT_REPORTER");

    let tmp = tempfile::tempdir().unwrap();
    let tasks_dir = tmp.path().join(".tasks");
    std::fs::create_dir_all(&tasks_dir).unwrap();
    unsafe {
        std::env::set_var("LOTAR_TASKS_DIR", tasks_dir.to_string_lossy().to_string());
        std::env::set_var("LOTAR_DEFAULT_REPORTER", "env-reporter@example.com");
    }

    let mut storage = Storage::new(tasks_dir);
    let req = TaskCreate {
        title: "Env reporter".to_string(),
        project: Some("TEST".to_string()),
        priority: None,
        task_type: None,
        reporter: None,
        assignee: None,
        due_date: None,
        effort: None,
        description: None,
        category: None,
        tags: vec![],
        custom_fields: None,
    };
    let created = TaskService::create(&mut storage, req).expect("create");
    assert_eq!(
        created.reporter.as_deref(),
        Some("env-reporter@example.com")
    );

    unsafe {
        std::env::remove_var("LOTAR_DEFAULT_REPORTER");
        std::env::remove_var("LOTAR_TASKS_DIR");
    }
}

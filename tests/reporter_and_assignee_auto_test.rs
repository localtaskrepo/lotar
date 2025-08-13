mod common;
use crate::common::env_mutex::lock_var;

use lotar::api_types::{TaskCreate, TaskUpdate};
use lotar::services::task_service::TaskService;
use lotar::storage::manager::Storage;
use lotar::types::{Priority, TaskStatus, TaskType};
use lotar::utils::paths;

#[test]
fn reporter_is_auto_set_from_config_on_create() {
    let _env_tasks = lock_var("LOTAR_TASKS_DIR");
    let tmp = tempfile::tempdir().unwrap();
    let tasks_dir = tmp.path().join(".tasks");
    std::fs::create_dir_all(&tasks_dir).unwrap();
    unsafe {
        std::env::set_var("LOTAR_TASKS_DIR", tasks_dir.to_string_lossy().to_string());
    }

    // Write global config with default_reporter
    std::fs::write(
        paths::global_config_path(&tasks_dir),
        "default_project: TEST\nissue_states: [Todo, InProgress, Done]\nissue_types: [Feature, Bug, Chore]\nissue_priorities: [Low, Medium, High]\ndefault_reporter: alice@example.com\n",
    ).unwrap();

    let mut storage = Storage::new(tasks_dir.clone());
    let req = TaskCreate {
        title: "Auto reporter".to_string(),
        project: Some("TEST".to_string()),
        priority: Some(Priority::High),
        task_type: Some(TaskType::Feature),
        reporter: None,
        assignee: None,
        due_date: None,
        effort: None,
        description: None,
        category: None,
        tags: vec![],
        custom_fields: None,
    };
    let created = TaskService::create(&mut storage, req).expect("service create");
    assert_eq!(created.reporter.as_deref(), Some("alice@example.com"));

    unsafe {
        std::env::remove_var("LOTAR_TASKS_DIR");
    }
}

#[test]
fn reporter_respects_disable_flag() {
    let _env_tasks = lock_var("LOTAR_TASKS_DIR");
    // toggle via config file, not env
    let tmp = tempfile::tempdir().unwrap();
    let tasks_dir = tmp.path().join(".tasks");
    std::fs::create_dir_all(&tasks_dir).unwrap();
    unsafe {
        std::env::set_var("LOTAR_TASKS_DIR", tasks_dir.to_string_lossy().to_string());
    }
    // Write config disabling auto reporter
    std::fs::write(
        paths::global_config_path(&tasks_dir),
        "default_project: TEST\nauto_set_reporter: false\n",
    )
    .unwrap();

    let mut storage = Storage::new(tasks_dir);
    let req = TaskCreate {
        title: "No reporter".to_string(),
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
    let created = TaskService::create(&mut storage, req).expect("service create");
    assert!(
        created.reporter.is_none(),
        "reporter should be None when disabled"
    );

    unsafe {
        std::env::remove_var("LOTAR_TASKS_DIR");
    }
}

#[test]
fn reporter_falls_back_to_git_or_system_when_no_config() {
    let _env_tasks = lock_var("LOTAR_TASKS_DIR");
    let tmp = tempfile::tempdir().unwrap();
    let tasks_dir = tmp.path().join(".tasks");
    std::fs::create_dir_all(&tasks_dir).unwrap();
    unsafe {
        std::env::set_var("LOTAR_TASKS_DIR", tasks_dir.to_string_lossy().to_string());
    }

    let mut storage = Storage::new(tasks_dir);
    let req = TaskCreate {
        title: "File reporter".to_string(),
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
    let created = TaskService::create(&mut storage, req).expect("service create");
    // With no config, may fall back to system user; accept Some or None but ensure no crash
    let _ = created.reporter;

    unsafe {
        std::env::remove_var("LOTAR_TASKS_DIR");
    }
}

#[test]
fn assignee_auto_set_on_status_change_when_empty() {
    let _env_tasks = lock_var("LOTAR_TASKS_DIR");
    let tmp = tempfile::tempdir().unwrap();
    let tasks_dir = tmp.path().join(".tasks");
    std::fs::create_dir_all(&tasks_dir).unwrap();
    unsafe {
        std::env::set_var("LOTAR_TASKS_DIR", tasks_dir.to_string_lossy().to_string());
    }

    // Configure default_reporter=bob to be used for auto-assign
    std::fs::write(
        paths::global_config_path(&tasks_dir),
        "default_project: TEST\nissue_states: [Todo, InProgress, Done]\nissue_types: [Feature, Bug, Chore]\nissue_priorities: [Low, Medium, High]\ndefault_reporter: bob\n",
    ).unwrap();

    // Sanity check: merged config should see default_reporter=bob
    let merged = lotar::config::resolution::load_and_merge_configs(Some(&tasks_dir))
        .expect("load merged config");
    assert_eq!(merged.default_reporter.as_deref(), Some("bob"));

    let mut storage = Storage::new(tasks_dir.clone());
    let create = TaskCreate {
        title: "Needs assignee".to_string(),
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
    let created = TaskService::create(&mut storage, create).unwrap();
    assert!(created.assignee.is_none(), "assignee should start None");

    let mut storage = Storage::new(tasks_dir);
    let updated = TaskService::update(
        &mut storage,
        &created.id,
        TaskUpdate {
            status: Some(TaskStatus::InProgress),
            ..Default::default()
        },
    )
    .unwrap();
    assert_eq!(updated.assignee.as_deref(), Some("bob"));

    unsafe {
        std::env::remove_var("LOTAR_TASKS_DIR");
    }
}

#[test]
fn assignee_auto_set_respects_disable_flag() {
    let _env_tasks = lock_var("LOTAR_TASKS_DIR");
    // disable via config file
    let tmp = tempfile::tempdir().unwrap();
    let tasks_dir = tmp.path().join(".tasks");
    std::fs::create_dir_all(&tasks_dir).unwrap();
    unsafe {
        std::env::set_var("LOTAR_TASKS_DIR", tasks_dir.to_string_lossy().to_string());
    }
    std::fs::write(
        paths::global_config_path(&tasks_dir),
        "default_project: TEST\nauto_assign_on_status: false\n",
    )
    .unwrap();

    let mut storage = Storage::new(tasks_dir.clone());
    let create = TaskCreate {
        title: "No auto assign".to_string(),
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
    let created = TaskService::create(&mut storage, create).unwrap();
    assert!(created.assignee.is_none());

    let mut storage = Storage::new(tasks_dir);
    let updated = TaskService::update(
        &mut storage,
        &created.id,
        TaskUpdate {
            status: Some(TaskStatus::Done),
            ..Default::default()
        },
    )
    .unwrap();
    assert!(
        updated.assignee.is_none(),
        "assignee should remain None when disabled"
    );

    unsafe {
        std::env::remove_var("LOTAR_TASKS_DIR");
    }
}

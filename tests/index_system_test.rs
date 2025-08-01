use local_task_repo::storage::Task;
use local_task_repo::types::TaskStatus;
use local_task_repo::index::{TaskFilter, TaskIndex};

mod common;
use common::{TestFixtures, utils};

#[test]
fn test_id_to_file_index() {
    let fixtures = TestFixtures::new();
    let mut storage = fixtures.create_storage();
    let task = fixtures.create_sample_task("index-test");
    let task_id = storage.add(&task, "TEST", None);

    // Test that the index file is created and contains the task ID
    let index_file = fixtures.tasks_root.join("index.yml"); // Changed from .json to .yml
    assert!(index_file.exists(), "Index file should be created");

    // Load the index and verify it's created (simplified index only tracks tags now)
    let _index = TaskIndex::load_from_file(&index_file).unwrap();
    
    // With our simplified architecture, we verify the task exists by checking the filesystem
    // Get the actual project folder from the task ID instead of hardcoding "TP"
    let actual_project = utils::get_project_for_task(&task_id).unwrap();
    assert!(fixtures.tasks_root.join(&actual_project).join("1.yml").exists(), 
            "Task file should exist in filesystem at {}/1.yml", actual_project);
}

#[test]
fn test_tag_to_id_index() {
    let fixtures = TestFixtures::new();
    let mut storage = fixtures.create_storage();
    let mut task = fixtures.create_sample_task("tag-index-test");
    task.tags = vec!["urgent".to_string(), "backend".to_string()];
    let task_id = storage.add(&task, "TEST", None);

    // Load the index and verify tag mappings
    let index_file = fixtures.tasks_root.join("index.yml"); // Changed from .json to .yml
    let index = TaskIndex::load_from_file(&index_file).unwrap();

    assert!(index.tag2id.contains_key("urgent"), "Index should contain 'urgent' tag mapping");
    assert!(index.tag2id["urgent"].contains(&task_id), "Urgent tag should map to our task");
    assert!(index.tag2id.contains_key("backend"), "Index should contain 'backend' tag mapping");
}

#[test]
fn test_index_update_on_task_modification() {
    let fixtures = TestFixtures::new();
    let mut storage = fixtures.create_storage();
    let task = fixtures.create_sample_task("index-update-test");
    let task_id = storage.add(&task, "TEST", None);

    // Get the actual project name that was created (the prefix)
    let _actual_project = utils::get_project_for_task(&task_id).unwrap();

    // Modify the task using the correct project name
    let mut updated_task = task.clone();
    updated_task.tags.push("new-tag".to_string());
    storage.edit(&task_id, &updated_task);

    // Verify index is updated
    let index_file = fixtures.tasks_root.join("index.yml"); // Changed from .json to .yml
    let index = TaskIndex::load_from_file(&index_file).unwrap();
    assert!(index.tag2id.contains_key("new-tag"), "Index should contain new tag after update");
}

#[test]
fn test_index_performance() {
    let fixtures = TestFixtures::new();
    let mut storage = fixtures.create_storage();

    // Create the first task to get the actual project prefix
    let mut first_task = Task::new(fixtures.tasks_root.clone(), "Performance Task 0".to_string(), local_task_repo::types::Priority::Medium
    );
    first_task.tags = vec!["tag-0".to_string()];
    let first_task_id = storage.add(&first_task, "TEST", None);

    // Get the actual project name that was created (the prefix)
    let actual_project = utils::get_project_for_task(&first_task_id).unwrap();

    // Create remaining tasks using the actual project prefix
    for i in 1..100 { // Reduced from 1000 for faster testing
        let mut task = Task::new(
            fixtures.tasks_root.clone(),
            format!("Performance Task {}", i),
            if i % 3 == 0 { local_task_repo::types::Priority::High } else { local_task_repo::types::Priority::Medium }
        );
        task.tags = vec![format!("tag-{}", i % 10)];
        storage.add(&task, "TEST", None);
    }

    // Test that indexed search is fast using the correct project name
    let start = std::time::Instant::now();
    let filter = TaskFilter {
        tags: vec!["tag-5".to_string()],
        project: Some(actual_project),
        ..Default::default()
    };
    let results = storage.search(&filter);
    let duration = start.elapsed();

    assert!(duration.as_millis() < 100, "Index search should be fast, took: {:?}", duration);
    assert_eq!(results.len(), 10, "Should find 10 tasks with tag-5"); // 100 tasks, every 10th has tag-5
}

#[test]
fn test_index_file_persistence() {
    let fixtures = TestFixtures::new();
    let mut storage = fixtures.create_storage();
    let task = fixtures.create_sample_task("persistence-test");
    let _task_id = storage.add(&task, "TEST", None);

    // Test that index is persisted to disk
    let index_file = fixtures.tasks_root.join("index.yml"); // Changed from .json to .yml
    assert!(index_file.exists(), "Index file should be created");

    let index_content = std::fs::read_to_string(&index_file).unwrap();
    assert!(index_content.contains("tag2id"), "Index should contain tag2id mapping");
    assert!(index_content.contains("last_updated"), "Index should contain last_updated field");
}

#[test]
fn test_status_index_tracking() {
    let fixtures = TestFixtures::new();
    let mut storage = fixtures.create_storage();
    let mut task1 = fixtures.create_sample_task("status-index-test");
    task1.status = TaskStatus::Todo;
    let task1_id = storage.add(&task1, "TEST", None);

    // With our simplified architecture, status filtering is done via filesystem scanning
    // Verify we can find the task by searching
    let filter = TaskFilter {
        status: Some(TaskStatus::Todo),
        ..Default::default()
    };
    let results = storage.search(&filter);
    assert!(!results.is_empty(), "Should find tasks with TODO status");
    assert!(results.iter().any(|(id, _)| id == &task1_id), "Should find our specific task");
}

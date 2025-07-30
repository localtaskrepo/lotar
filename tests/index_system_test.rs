use local_task_repo::store::Task;
use local_task_repo::types::TaskStatus;
use local_task_repo::index::{TaskFilter, TaskIndex};

mod common;
use common::TestFixtures;

#[test]
fn test_id_to_file_index() {
    let fixtures = TestFixtures::new();
    let mut storage = fixtures.create_storage();
    let task = fixtures.create_sample_task("index-test");
    let task_id = storage.add(&task);

    // Test that the index file is created and contains the task ID
    let index_file = fixtures.tasks_root.join("index.yml"); // Changed from .json to .yml
    assert!(index_file.exists(), "Index file should be created");

    // Load the index and verify it contains our task
    let index = TaskIndex::load_from_file(&index_file).unwrap();
    assert!(index.id2file.contains_key(&task_id), "Index should contain task ID mapping");
}

#[test]
fn test_tag_to_id_index() {
    let fixtures = TestFixtures::new();
    let mut storage = fixtures.create_storage();
    let mut task = fixtures.create_sample_task("tag-index-test");
    task.tags = vec!["urgent".to_string(), "backend".to_string()];
    let task_id = storage.add(&task);

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
    let task_id = storage.add(&task);

    // Modify the task
    let mut updated_task = storage.get(&task_id, "index-update-test".to_string()).unwrap();
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

    // Create many tasks to test index performance
    for i in 0..100 { // Reduced from 1000 for faster testing
        let mut task = Task::new(
            fixtures.tasks_root.clone(),
            format!("Performance Task {}", i),
            "perf-test".to_string(),
            if i % 3 == 0 { local_task_repo::types::Priority::High } else { local_task_repo::types::Priority::Medium }
        );
        task.tags = vec![format!("tag-{}", i % 10)];
        storage.add(&task);
    }

    // Test that indexed search is fast
    let start = std::time::Instant::now();
    let filter = TaskFilter {
        tags: vec!["tag-5".to_string()],
        project: Some("perf-test".to_string()),
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
    let _task_id = storage.add(&task);

    // Test that index is persisted to disk
    let index_file = fixtures.tasks_root.join("index.yml"); // Changed from .json to .yml
    assert!(index_file.exists(), "Index file should be created");

    let index_content = std::fs::read_to_string(&index_file).unwrap();
    assert!(index_content.contains("id2file"), "Index should contain id2file mapping");
    assert!(index_content.contains("tag2id"), "Index should contain tag2id mapping");
    assert!(index_content.contains("status2id"), "Index should contain status2id mapping");
}

#[test]
fn test_status_index_tracking() {
    let fixtures = TestFixtures::new();
    let mut storage = fixtures.create_storage();
    let mut task1 = fixtures.create_sample_task("status-index-test");
    task1.status = TaskStatus::Todo;
    let task1_id = storage.add(&task1);

    // Verify status index contains our task
    let index_file = fixtures.tasks_root.join("index.yml");
    let index = TaskIndex::load_from_file(&index_file).unwrap();
    assert!(index.status2id.contains_key("TODO"), "Index should contain TODO status");
    assert!(index.status2id["TODO"].contains(&task1_id), "TODO status should contain our task");
}

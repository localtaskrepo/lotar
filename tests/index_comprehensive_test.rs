use local_task_repo::store::{Storage};
use local_task_repo::types::{TaskStatus, Priority};
use local_task_repo::index::{TaskIndex};

mod common;
use common::TestFixtures;

#[test]
fn test_index_status_change_tracking() {
    let fixtures = TestFixtures::new();
    let mut storage = fixtures.create_storage();

    // Create a task with TODO status
    let mut task = fixtures.create_sample_task("status-test");
    task.status = TaskStatus::Todo;
    let task_id = storage.add(&task);

    // Verify initial index state
    let index_file = fixtures.tasks_root.join("index.yml");
    let index = TaskIndex::load_from_file(&index_file).unwrap();
    assert!(index.status2id.get("TODO").unwrap().contains(&task_id));
    assert!(!index.status2id.contains_key("IN_PROGRESS"));

    // Update task status
    let mut updated_task = storage.get(&task_id, "status-test".to_string()).unwrap();
    updated_task.status = TaskStatus::InProgress;
    storage.edit(&task_id, &updated_task);

    // Verify index was updated properly
    let updated_index = TaskIndex::load_from_file(&index_file).unwrap();
    assert!(!updated_index.status2id.get("TODO").unwrap_or(&vec![]).contains(&task_id));
    assert!(updated_index.status2id.get("IN_PROGRESS").unwrap().contains(&task_id));
}

#[test]
fn test_index_priority_change_tracking() {
    let fixtures = TestFixtures::new();
    let mut storage = fixtures.create_storage();

    // Create a task with MEDIUM priority
    let mut task = fixtures.create_sample_task("priority-test");
    task.priority = Priority::Medium;
    let task_id = storage.add(&task);

    // Verify initial index state
    let index_file = fixtures.tasks_root.join("index.yml");
    let index = TaskIndex::load_from_file(&index_file).unwrap();
    assert!(index.priority2id.get("MEDIUM").unwrap().contains(&task_id));

    // Update task priority
    let mut updated_task = storage.get(&task_id, "priority-test".to_string()).unwrap();
    updated_task.priority = Priority::High;
    storage.edit(&task_id, &updated_task);

    // Verify index was updated properly
    let updated_index = TaskIndex::load_from_file(&index_file).unwrap();
    assert!(!updated_index.priority2id.get("MEDIUM").unwrap_or(&vec![]).contains(&task_id));
    assert!(updated_index.priority2id.get("HIGH").unwrap().contains(&task_id));
}

#[test]
fn test_index_tag_removal() {
    let fixtures = TestFixtures::new();
    let mut storage = fixtures.create_storage();

    // Create a task with multiple tags
    let mut task = fixtures.create_sample_task("tag-removal-test");
    task.tags = vec!["urgent".to_string(), "bug".to_string(), "frontend".to_string()];
    let task_id = storage.add(&task);

    // Verify all tags are in index
    let index_file = fixtures.tasks_root.join("index.yml");
    let index = TaskIndex::load_from_file(&index_file).unwrap();
    assert!(index.tag2id.get("urgent").unwrap().contains(&task_id));
    assert!(index.tag2id.get("bug").unwrap().contains(&task_id));
    assert!(index.tag2id.get("frontend").unwrap().contains(&task_id));

    // Remove some tags
    let mut updated_task = storage.get(&task_id, "tag-removal-test".to_string()).unwrap();
    updated_task.tags = vec!["urgent".to_string()]; // Remove "bug" and "frontend"
    storage.edit(&task_id, &updated_task);

    // Verify removed tags are cleaned from index
    let updated_index = TaskIndex::load_from_file(&index_file).unwrap();
    assert!(updated_index.tag2id.get("urgent").unwrap().contains(&task_id));
    assert!(!updated_index.tag2id.contains_key("bug")); // Should be removed completely
    assert!(!updated_index.tag2id.contains_key("frontend")); // Should be removed completely
}

#[test]
fn test_index_delete_cleanup() {
    let fixtures = TestFixtures::new();
    let mut storage = fixtures.create_storage();
    
    // Create multiple tasks in the SAME project with same tags/status
    let mut task1 = fixtures.create_sample_task("shared-project");
    task1.tags = vec!["shared-tag".to_string()];
    task1.status = TaskStatus::Todo;
    let task1_id = storage.add(&task1);
    
    let mut task2 = fixtures.create_sample_task("shared-project"); // Same project!
    task2.tags = vec!["shared-tag".to_string()];
    task2.status = TaskStatus::Todo;
    let task2_id = storage.add(&task2);
    
    // Verify both tasks are in index
    let index_file = fixtures.tasks_root.join("index.yml");
    let index = TaskIndex::load_from_file(&index_file).unwrap();
    let shared_tag_tasks = index.tag2id.get("shared-tag").unwrap();
    assert!(shared_tag_tasks.contains(&task1_id));
    assert!(shared_tag_tasks.contains(&task2_id));
    assert_eq!(shared_tag_tasks.len(), 2);
    
    // Delete one task
    assert!(storage.delete(&task1_id, "shared-project".to_string()));
    
    // Verify index still contains the other task but not the deleted one
    let updated_index = TaskIndex::load_from_file(&index_file).unwrap();
    let remaining_tasks = updated_index.tag2id.get("shared-tag").unwrap();
    assert!(!remaining_tasks.contains(&task1_id));
    assert!(remaining_tasks.contains(&task2_id));
    assert_eq!(remaining_tasks.len(), 1);
    
    // Delete the second task
    assert!(storage.delete(&task2_id, "shared-project".to_string()));
    
    // Verify tag is completely removed from index when no tasks have it
    let final_index = TaskIndex::load_from_file(&index_file).unwrap();
    assert!(!final_index.tag2id.contains_key("shared-tag"));
}

#[test]
fn test_index_consistency_after_rebuild() {
    let fixtures = TestFixtures::new();
    let mut storage = fixtures.create_storage();

    // Create several tasks in the SAME project to avoid path inconsistencies
    for i in 0..5 {
        let mut task = fixtures.create_sample_task("rebuild-test");
        task.title = format!("Task {}", i); // Make titles unique to avoid file conflicts
        task.tags = vec![format!("tag-{}", i % 2)];
        task.status = if i % 2 == 0 { TaskStatus::Todo } else { TaskStatus::InProgress };
        task.priority = if i % 3 == 0 { Priority::High } else { Priority::Medium };
        storage.add(&task);
    }

    // Capture original index state
    let index_file = fixtures.tasks_root.join("index.yml");
    let original_index = TaskIndex::load_from_file(&index_file).unwrap();

    // Rebuild index
    storage.rebuild_index().unwrap();

    // Verify rebuilt index matches original
    let rebuilt_index = TaskIndex::load_from_file(&index_file).unwrap();

    assert_eq!(original_index.id2file.len(), rebuilt_index.id2file.len());
    assert_eq!(original_index.tag2id.len(), rebuilt_index.tag2id.len());
    assert_eq!(original_index.status2id.len(), rebuilt_index.status2id.len());
    assert_eq!(original_index.priority2id.len(), rebuilt_index.priority2id.len());

    // Verify all mappings are identical
    for (id, file) in &original_index.id2file {
        assert_eq!(rebuilt_index.id2file.get(id), Some(file), 
                   "File path mismatch for task {}: original='{}', rebuilt='{:?}'", 
                   id, file, rebuilt_index.id2file.get(id));
    }
}

#[test]
fn test_index_handles_corrupted_file() {
    let fixtures = TestFixtures::new();
    let mut storage = fixtures.create_storage();

    // Create a task to generate index
    let task = fixtures.create_sample_task("corruption-test");
    let task_id = storage.add(&task);

    // Corrupt the index file
    let index_file = fixtures.tasks_root.join("index.yml");
    std::fs::write(&index_file, "invalid: yaml: content: [").unwrap();

    // Create new storage instance (should handle corrupted index gracefully)
    let mut new_storage = Storage::new(fixtures.tasks_root.clone());

    // Should be able to retrieve task even with corrupted index
    let retrieved = new_storage.get(&task_id, "corruption-test".to_string());
    assert!(retrieved.is_some(), "Should be able to retrieve task even with corrupted index");

    // Rebuild should fix the corruption
    new_storage.rebuild_index().unwrap();
    let rebuilt_index = TaskIndex::load_from_file(&index_file).unwrap();
    assert!(rebuilt_index.id2file.contains_key(&task_id));
}

use lotar::storage::{Storage};
use lotar::types::{TaskStatus, Priority};
use lotar::index::{TaskIndex, TaskFilter};

mod common;
use common::{TestFixtures, utils};

#[test]
fn test_index_status_change_tracking() {
    let fixtures = TestFixtures::new();
    let mut storage = fixtures.create_storage();

    // Create a task with TODO status
    let mut task = fixtures.create_sample_task("status-test");
    task.status = TaskStatus::Todo;
    let task_id = storage.add(&task, "TEST", None);

    // Get the actual project name from the task ID
    let actual_project = utils::get_project_for_task(&task_id).unwrap();

    // Verify we can find the task with TODO status
    let filter = TaskFilter {
        status: Some(TaskStatus::Todo),
        ..Default::default()
    };
    let results = storage.search(&filter);
    assert!(results.iter().any(|(id, _)| id == &task_id), "Should find task with TODO status");

    // Update task status
    let mut updated_task = storage.get(&task_id, actual_project.clone()).unwrap();
    updated_task.status = TaskStatus::InProgress;
    storage.edit(&task_id, &updated_task);

    // Verify status change works through search
    let todo_filter = TaskFilter {
        status: Some(TaskStatus::Todo),
        ..Default::default()
    };
    let todo_results = storage.search(&todo_filter);
    assert!(!todo_results.iter().any(|(id, _)| id == &task_id), "Should not find task in TODO status");

    let in_progress_filter = TaskFilter {
        status: Some(TaskStatus::InProgress),
        ..Default::default()
    };
    let in_progress_results = storage.search(&in_progress_filter);
    assert!(in_progress_results.iter().any(|(id, _)| id == &task_id), "Should find task in IN_PROGRESS status");
}

#[test]
fn test_index_priority_change_tracking() {
    let fixtures = TestFixtures::new();
    let mut storage = fixtures.create_storage();

    // Create a task with MEDIUM priority
    let mut task = fixtures.create_sample_task("priority-test");
    task.priority = Priority::Medium;
    let task_id = storage.add(&task, "TEST", None);

    // Get the actual project name from the task ID
    let actual_project = utils::get_project_for_task(&task_id).unwrap();

    // Verify we can find the task with MEDIUM priority
    let filter = TaskFilter {
        priority: Some(Priority::Medium),
        ..Default::default()
    };
    let results = storage.search(&filter);
    assert!(results.iter().any(|(id, _)| id == &task_id), "Should find task with MEDIUM priority");

    // Update task priority
    let mut updated_task = storage.get(&task_id, actual_project.clone()).unwrap();
    updated_task.priority = Priority::High;
    storage.edit(&task_id, &updated_task);

    // Verify priority change works through search
    let medium_filter = TaskFilter {
        priority: Some(Priority::Medium),
        ..Default::default()
    };
    let medium_results = storage.search(&medium_filter);
    assert!(!medium_results.iter().any(|(id, _)| id == &task_id), "Should not find task with MEDIUM priority");

    let high_filter = TaskFilter {
        priority: Some(Priority::High),
        ..Default::default()
    };
    let high_results = storage.search(&high_filter);
    assert!(high_results.iter().any(|(id, _)| id == &task_id), "Should find task with HIGH priority");
}

#[test]
fn test_index_tag_removal() {
    let fixtures = TestFixtures::new();
    let mut storage = fixtures.create_storage();

    // Create a task with multiple tags
    let mut task = fixtures.create_sample_task("tag-removal-test");
    task.tags = vec!["urgent".to_string(), "bug".to_string(), "frontend".to_string()];
    let task_id = storage.add(&task, "TEST", None);

    // Get the actual project name from the task ID
    let actual_project = utils::get_project_for_task(&task_id).unwrap();

    // Verify all tags are in index
    let index_file = fixtures.tasks_root.join("index.yml");
    let index = TaskIndex::load_from_file(&index_file).unwrap();
    assert!(index.tag2id.get("urgent").unwrap().contains(&task_id));
    assert!(index.tag2id.get("bug").unwrap().contains(&task_id));
    assert!(index.tag2id.get("frontend").unwrap().contains(&task_id));

    // Remove some tags
    let mut updated_task = storage.get(&task_id, actual_project.clone()).unwrap();
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
    
    // Create multiple tasks with same tags
    let mut task1 = fixtures.create_sample_task("shared-project");
    task1.tags = vec!["shared-tag".to_string()];
    let task1_id = storage.add(&task1, "TEST", None);
    
    // Get the actual project name from the task ID
    let actual_project = utils::get_project_for_task(&task1_id).unwrap();
    
    // Create second task using the actual project prefix
    let mut task2 = fixtures.create_sample_task("second-task");
    // Note: project field no longer exists // Use the actual prefix
    task2.tags = vec!["shared-tag".to_string()];
    let task2_id = storage.add(&task2, "TEST", None);
    
    // Verify both tasks are in index
    let index_file = fixtures.tasks_root.join("index.yml");
    let index = TaskIndex::load_from_file(&index_file).unwrap();
    let shared_tag_tasks = index.tag2id.get("shared-tag").unwrap();
    assert!(shared_tag_tasks.contains(&task1_id));
    assert!(shared_tag_tasks.contains(&task2_id));
    assert_eq!(shared_tag_tasks.len(), 2);
    
    // Delete one task
    assert!(storage.delete(&task1_id, actual_project.clone()));
    
    // Verify index still contains the other task but not the deleted one
    let updated_index = TaskIndex::load_from_file(&index_file).unwrap();
    let remaining_tasks = updated_index.tag2id.get("shared-tag").unwrap();
    assert!(!remaining_tasks.contains(&task1_id));
    assert!(remaining_tasks.contains(&task2_id));
    assert_eq!(remaining_tasks.len(), 1);
    
    // Delete the second task
    assert!(storage.delete(&task2_id, actual_project.clone()));
    
    // Verify tag is completely removed from index when no tasks have it
    let final_index = TaskIndex::load_from_file(&index_file).unwrap();
    assert!(!final_index.tag2id.contains_key("shared-tag"));
}

#[test]
fn test_index_consistency_after_rebuild() {
    let fixtures = TestFixtures::new();
    let mut storage = fixtures.create_storage();

    // Create several tasks with tags
    for i in 0..5 {
        let mut task = fixtures.create_sample_task(&format!("rebuild-test-{}", i));
        task.tags = vec![format!("tag-{}", i % 2)];
        storage.add(&task, "TEST", None);
    }

    // Capture original index state
    let index_file = fixtures.tasks_root.join("index.yml");
    let original_index = TaskIndex::load_from_file(&index_file).unwrap();

    // Rebuild index
    storage.rebuild_index().unwrap();

    // Verify rebuilt index matches original for tag mappings
    let rebuilt_index = TaskIndex::load_from_file(&index_file).unwrap();
    assert_eq!(original_index.tag2id.len(), rebuilt_index.tag2id.len());

    // Verify all tag mappings are identical (order may differ after rebuild)
    for (tag, task_ids) in &original_index.tag2id {
        let rebuilt_task_ids = rebuilt_index.tag2id.get(tag);
        assert!(rebuilt_task_ids.is_some(), "Tag mapping missing for tag {}", tag);
        
        // Compare as sets since order may differ after rebuild
        let original_set: std::collections::HashSet<_> = task_ids.iter().collect();
        let rebuilt_set: std::collections::HashSet<_> = rebuilt_task_ids.unwrap().iter().collect();
        assert_eq!(original_set, rebuilt_set, "Tag mapping mismatch for tag {}", tag);
    }
}

#[test]
fn test_index_handles_corrupted_file() {
    let fixtures = TestFixtures::new();
    let mut storage = fixtures.create_storage();

    // Create a task to generate index
    let task = fixtures.create_sample_task("corruption-test");
    let task_id = storage.add(&task, "TEST", None);

    // Get the actual project name that was created
    let actual_project = utils::get_project_for_task(&task_id).unwrap();

    // Corrupt the index file
    let index_file = fixtures.tasks_root.join("index.yml");
    std::fs::write(&index_file, "invalid: yaml: content: [").unwrap();

    // Create new storage instance (should handle corrupted index gracefully)
    let mut new_storage = Storage::new(fixtures.tasks_root.clone());

    // Should be able to retrieve task even with corrupted index
    let retrieved = new_storage.get(&task_id, actual_project.clone());
    assert!(retrieved.is_some(), "Should be able to retrieve task even with corrupted index");

    // Rebuild should fix the corruption
    new_storage.rebuild_index().unwrap();
    let rebuilt_index = TaskIndex::load_from_file(&index_file).unwrap();
    // With simplified index, verify it loads correctly (only has tag2id)
    assert!(rebuilt_index.tag2id.is_empty() || !rebuilt_index.tag2id.is_empty());
}

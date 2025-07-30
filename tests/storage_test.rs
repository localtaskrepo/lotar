use local_task_repo::store::Task;
use local_task_repo::types::{Priority};

mod common;
use common::{TestFixtures, assertions};

#[test]
fn test_enhanced_task_creation() {
    let fixtures = TestFixtures::new();
    let mut storage = fixtures.create_storage();

    let task = Task::new(
        fixtures.tasks_root.clone(),
        "Enhanced Task Creation".to_string(),
        "test-project".to_string(),
        Priority::High
    );

    let task_id = storage.add(&task);
    assert!(!task_id.is_empty(), "Task ID should be assigned");
    assert!(task_id.starts_with("TEST-"), "First task should have formatted ID");

    // Verify task file was created with .yml extension
    assertions::assert_task_exists(&fixtures.tasks_root, "test-project", &task_id);

    // Verify metadata was created properly
    assertions::assert_metadata_updated(&fixtures.tasks_root, "test-project", 1, 1);
}

#[test]
fn test_enhanced_task_retrieval() {
    let fixtures = TestFixtures::new();
    let mut storage = fixtures.create_storage();

    let task = Task::new(
        fixtures.tasks_root.clone(),
        "Retrieval Test Task".to_string(),
        "test-project".to_string(),
        Priority::Medium
    );

    let task_id = storage.add(&task);

    // Test retrieval
    let retrieved_task = storage.get(&task_id, "test-project".to_string());
    assert!(retrieved_task.is_some(), "Task should be retrievable");

    let retrieved = retrieved_task.unwrap();
    assert_eq!(retrieved.title, "Retrieval Test Task");
    assert_eq!(retrieved.project, "test-project");
    assert_eq!(retrieved.priority, Priority::Medium);
}

#[test]
fn test_task_deletion() {
    let fixtures = TestFixtures::new();
    let mut storage = fixtures.create_storage();

    let task = Task::new(
        fixtures.tasks_root.clone(),
        "Task to Delete".to_string(),
        "test-project".to_string(),
        Priority::Low
    );

    let task_id = storage.add(&task);

    // Verify task exists
    assert!(storage.get(&task_id, "test-project".to_string()).is_some());

    // Delete the task
    let deleted = storage.delete(&task_id, "test-project".to_string());
    assert!(deleted, "Task should be successfully deleted");

    // Verify task no longer exists
    assert!(storage.get(&task_id, "test-project".to_string()).is_none());

    // Verify metadata was updated
    assertions::assert_metadata_updated(&fixtures.tasks_root, "test-project", 0, 1);
}

#[test]
fn test_sequential_task_ids() {
    let fixtures = TestFixtures::new();
    let mut storage = fixtures.create_storage();

    let task1 = Task::new(
        fixtures.tasks_root.clone(),
        "First Task".to_string(),
        "test-project".to_string(),
        Priority::Medium
    );

    let task2 = Task::new(
        fixtures.tasks_root.clone(),
        "Second Task".to_string(),
        "test-project".to_string(),
        Priority::High
    );

    let id1 = storage.add(&task1);
    let id2 = storage.add(&task2);

    // IDs should be different and sequential
    assert_ne!(id1, id2, "Task IDs should be different");
    assert_eq!(id1, "TEST-001", "First task should be TEST-001");
    assert_eq!(id2, "TEST-002", "Second task should be TEST-002");
}

#[test]
fn test_task_yaml_serialization() {
    let fixtures = TestFixtures::new();
    let mut task = fixtures.create_sample_task("yaml-test");
    task.subtitle = Some("Test subtitle".to_string());
    task.description = Some("Test description".to_string());
    task.tags = vec!["tag1".to_string(), "tag2".to_string()];

    // Serialize to YAML
    let yaml_content = serde_yaml::to_string(&task).unwrap();

    // Verify YAML contains expected fields
    assert!(yaml_content.contains("title: Sample Test Task"));
    assert!(yaml_content.contains("subtitle: Test subtitle"));
    assert!(yaml_content.contains("description: Test description"));
    assert!(yaml_content.contains("- tag1"));
    assert!(yaml_content.contains("- tag2"));

    // Deserialize back
    let deserialized_task: Task = serde_yaml::from_str(&yaml_content).unwrap();
    assert_eq!(deserialized_task.title, task.title);
    assert_eq!(deserialized_task.subtitle, task.subtitle);
    assert_eq!(deserialized_task.tags, task.tags);
}

#[test]
fn test_multiple_projects() {
    let fixtures = TestFixtures::new();
    let mut storage = fixtures.create_storage();

    // Create tasks in different projects
    let task1 = Task::new(
        fixtures.tasks_root.clone(),
        "Project A Task".to_string(),
        "project-a".to_string(),
        Priority::High
    );

    let task2 = Task::new(
        fixtures.tasks_root.clone(),
        "Project B Task".to_string(),
        "project-b".to_string(),
        Priority::Medium
    );

    let id1 = storage.add(&task1);
    let id2 = storage.add(&task2);

    // Verify tasks are stored in correct project directories
    assertions::assert_task_exists(&fixtures.tasks_root, "project-a", &id1);
    assertions::assert_task_exists(&fixtures.tasks_root, "project-b", &id2);

    // Verify cross-project retrieval works correctly
    let retrieved_a = storage.get(&id1, "project-a".to_string());
    let retrieved_b = storage.get(&id2, "project-b".to_string());

    assert!(retrieved_a.is_some());
    assert!(retrieved_b.is_some());

    // Verify cross-project isolation
    let wrong_project = storage.get(&id1, "project-b".to_string());
    assert!(wrong_project.is_none(), "Task should not be accessible from wrong project");
}

#[test]
fn test_task_with_all_fields() {
    let fixtures = TestFixtures::new();
    let mut task = Task::new(
        fixtures.tasks_root.clone(),
        "Complete Task".to_string(),
        "full-test".to_string(),
        Priority::High
    );

    // Set all optional fields
    task.subtitle = Some("Full task testing".to_string());
    task.description = Some("This task tests all possible fields".to_string());
    task.category = Some("testing".to_string());
    task.due_date = Some("2025-12-31".to_string());
    task.tags = vec!["comprehensive".to_string(), "testing".to_string(), "complete".to_string()];

    let mut storage = fixtures.create_storage();
    let task_id = storage.add(&task);

    let retrieved = storage.get(&task_id, "full-test".to_string()).unwrap();

    // Verify all fields are preserved
    assert_eq!(retrieved.title, "Complete Task");
    assert_eq!(retrieved.subtitle, Some("Full task testing".to_string()));
    assert_eq!(retrieved.description, Some("This task tests all possible fields".to_string()));
    assert_eq!(retrieved.category, Some("testing".to_string()));
    assert_eq!(retrieved.due_date, Some("2025-12-31".to_string()));
    assert_eq!(retrieved.tags.len(), 3);
    assert!(retrieved.tags.contains(&"comprehensive".to_string()));
    assert!(retrieved.created.len() > 0, "Created timestamp should be set");
}

#[test]
fn test_task_id_increment() {
    let fixtures = TestFixtures::new();
    let mut storage = fixtures.create_storage();

    // Create tasks with unique titles to avoid file conflicts
    let mut task1 = fixtures.create_sample_task("increment-test");
    task1.title = "First Task".to_string();
    let mut task2 = fixtures.create_sample_task("increment-test");
    task2.title = "Second Task".to_string();
    let mut task3 = fixtures.create_sample_task("increment-test");
    task3.title = "Third Task".to_string();

    let id1 = storage.add(&task1);
    let id2 = storage.add(&task2);
    let id3 = storage.add(&task3);

    // IDs should increment
    assert_eq!(id1, "INCR-001", "First task should be INCR-001");
    assert_eq!(id2, "INCR-002", "Second task should be INCR-002");
    assert_eq!(id3, "INCR-003", "Third task should be INCR-003");
}

#[test]
fn test_task_priority_validation() {
    let fixtures = TestFixtures::new();

    // Test all valid Priority enum values
    let priorities = vec![Priority::Low, Priority::Medium, Priority::High, Priority::Critical];

    for priority in priorities {
        let task = Task::new(
            fixtures.tasks_root.clone(),
            format!("Priority {:?} Task", priority),
            "priority-test".to_string(),
            priority.clone()
        );
        assert_eq!(task.priority, priority);
    }
}

#[test]
fn test_empty_project_name_handling() {
    let fixtures = TestFixtures::new();

    let task = Task::new(
        fixtures.tasks_root.clone(),
        "Empty Project Task".to_string(),
        "".to_string(),  // Empty project name
        Priority::Medium
    );

    // Should handle empty project gracefully
    assert_eq!(task.project, "");

    let mut storage = fixtures.create_storage();
    let task_id = storage.add(&task);
    assert!(!task_id.is_empty(), "Should still create task with empty project");
}

#[test]
fn test_special_characters_in_task_fields() {
    let fixtures = TestFixtures::new();
    let mut task = Task::new(
        fixtures.tasks_root.clone(),
        "Task with 特殊字符 and émojis 🚀".to_string(),
        "unicode-test".to_string(),
        Priority::Medium
    );

    task.description = Some("Description with\nnewlines and\ttabs".to_string());
    task.tags = vec!["tag-with-dashes".to_string(), "tag_with_underscores".to_string()];

    let mut storage = fixtures.create_storage();
    let task_id = storage.add(&task);

    let retrieved = storage.get(&task_id, "unicode-test".to_string()).unwrap();

    // Verify special characters are preserved
    assert_eq!(retrieved.title, "Task with 特殊字符 and émojis 🚀");
    assert!(retrieved.description.as_ref().unwrap().contains("\n"));
    assert!(retrieved.description.as_ref().unwrap().contains("\t"));
}

#[cfg(test)]
mod performance_tests {
    use super::*;
    use common::performance::{assert_performance_threshold};
    use std::time::Duration;

    #[test]
    fn test_task_creation_performance() {
        let fixtures = TestFixtures::new();
        let mut storage = fixtures.create_storage();

        assert_performance_threshold(
            || {
                let task = fixtures.create_sample_task("perf-test");
                storage.add(&task)
            },
            Duration::from_millis(50),
            "Task creation"
        );
    }

    #[test]
    fn test_bulk_task_creation() {
        let fixtures = TestFixtures::new();
        let mut storage = fixtures.create_storage();

        assert_performance_threshold(
            || {
                for i in 0..100 {
                    let priority = match i % 4 {
                        0 => Priority::Low,
                        1 => Priority::Medium,
                        2 => Priority::High,
                        _ => Priority::Critical,
                    };
                    let task = Task::new(
                        fixtures.tasks_root.clone(),
                        format!("Bulk Task {}", i),
                        "bulk-test".to_string(),
                        priority
                    );
                    storage.add(&task);
                }
            },
            Duration::from_millis(500),
            "Bulk task creation (100 tasks)"
        );
    }
}

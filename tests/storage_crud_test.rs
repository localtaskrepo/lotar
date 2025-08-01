use local_task_repo::storage::Task;
use local_task_repo::types::Priority;

mod common;
use common::{TestFixtures, assertions, utils};

/// Basic storage CRUD operations
#[cfg(test)]
mod storage_crud_tests {
    use super::*;

    #[test]
    fn test_task_creation() {
        let fixtures = TestFixtures::new();
        let mut storage = fixtures.create_storage();

        let task = Task::new(fixtures.tasks_root.clone(), "Test Task".to_string(), Priority::High
        );

        let task_id = storage.add(&task, "TP", None);
        assert!(!task_id.is_empty(), "Task ID should be assigned");

        // Get the actual project prefix that was generated
        let actual_project = utils::get_project_for_task(&task_id).unwrap();
        assert!(task_id.starts_with(&format!("{}-", actual_project)), "Task should have generated prefix");

        // Verify task file was created
        assertions::assert_task_exists(&fixtures.tasks_root, &actual_project, &task_id);
        assertions::assert_metadata_updated(&fixtures.tasks_root, &actual_project, 1, 1);
    }

    #[test]
    fn test_task_retrieval() {
        let fixtures = TestFixtures::new();
        let mut storage = fixtures.create_storage();

        let original_task = Task::new(fixtures.tasks_root.clone(), "Retrievable Task".to_string(), Priority::Medium
        );

        let task_id = storage.add(&original_task, "TP", None);
        let actual_project = utils::get_project_for_task(&task_id).unwrap();

        // Test successful retrieval
        let retrieved_task = storage.get(&task_id, actual_project.clone());
        assert!(retrieved_task.is_some(), "Task should be retrievable");

        let task = retrieved_task.unwrap();
        assert_eq!(task.title, "Retrievable Task");
        // Note: project field no longer exists
        assert_eq!(task.priority, Priority::Medium);
    }

    #[test]
    fn test_task_deletion() {
        let fixtures = TestFixtures::new();
        let mut storage = fixtures.create_storage();

        let task = Task::new(fixtures.tasks_root.clone(), "Task to Delete".to_string(), Priority::Low
        );

        let task_id = storage.add(&task, "TP", None);
        let actual_project = utils::get_project_for_task(&task_id).unwrap();

        // Verify task exists
        assert!(storage.get(&task_id, actual_project.clone()).is_some());

        // Delete the task
        let deleted = storage.delete(&task_id, actual_project.clone());
        assert!(deleted, "Task should be successfully deleted");

        // Verify task no longer exists
        assert!(storage.get(&task_id, actual_project.clone()).is_none());
        assertions::assert_metadata_updated(&fixtures.tasks_root, &actual_project, 0, 0);
    }

    #[test]
    fn test_sequential_task_ids() {
        let fixtures = TestFixtures::new();
        let mut storage = fixtures.create_storage();

        let task1 = Task::new(fixtures.tasks_root.clone(), "First Task".to_string(), Priority::Medium
        );

        let task2 = Task::new(fixtures.tasks_root.clone(), "Second Task".to_string(), Priority::High
        );

        let id1 = storage.add(&task1, "TP", None);
        let actual_project = utils::get_project_for_task(&id1).unwrap();

        // Use the same project prefix for the second task
        let task2_updated = task2.clone();
        // Note: project field no longer exists
        let id2 = storage.add(&task2_updated, "TP", None);

        // IDs should be different and sequential
        assert_ne!(id1, id2, "Task IDs should be different");
        assert_eq!(id1, format!("{}-1", actual_project), "First task should use natural numbering");
        assert_eq!(id2, format!("{}-2", actual_project), "Second task should use natural numbering");
    }

    #[test]
    fn test_task_id_increment() {
        let fixtures = TestFixtures::new();
        let mut storage = fixtures.create_storage();

        // Create tasks with unique titles to avoid file conflicts
        let mut task1 = fixtures.create_sample_task("increment-test");
        task1.title = "First Task".to_string();

        let id1 = storage.add(&task1, "TP", None);
        let actual_project = utils::get_project_for_task(&id1).unwrap();

        // Create second and third tasks using the same actual project prefix
        let mut task2 = fixtures.create_sample_task(&actual_project);
        task2.title = "Second Task".to_string();
        let mut task3 = fixtures.create_sample_task(&actual_project);
        task3.title = "Third Task".to_string();

        let id2 = storage.add(&task2, "TP", None);
        let id3 = storage.add(&task3, "TP", None);

        // IDs should increment with natural numbering
        assert_eq!(id1, format!("{}-1", actual_project), "First task should use natural numbering");
        assert_eq!(id2, format!("{}-2", actual_project), "Second task should use natural numbering");
        assert_eq!(id3, format!("{}-3", actual_project), "Third task should use natural numbering");
    }
}

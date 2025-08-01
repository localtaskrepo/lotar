use local_task_repo::storage::Task;
use local_task_repo::types::Priority;

mod common;
use common::{TestFixtures, utils};

/// Task validation and serialization tests
#[cfg(test)]
mod task_validation_tests {
    use super::*;

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
    fn test_task_with_all_fields() {
        let fixtures = TestFixtures::new();
        let mut task = Task::new(fixtures.tasks_root.clone(), "Complete Task".to_string(), Priority::High
        );

        // Set all optional fields
        task.subtitle = Some("Full task testing".to_string());
        task.description = Some("This task tests all possible fields".to_string());
        task.category = Some("testing".to_string());
        task.due_date = Some("2025-12-31".to_string());
        task.tags = vec!["comprehensive".to_string(), "testing".to_string(), "complete".to_string()];

        let mut storage = fixtures.create_storage();
        let task_id = storage.add(&task, "VT", None);
        let actual_project = utils::get_project_for_task(&task_id).unwrap();

        let retrieved = storage.get(&task_id, actual_project.clone()).unwrap();

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
    fn test_task_priority_validation() {
        let fixtures = TestFixtures::new();

        // Test all valid Priority enum values
        let priorities = vec![Priority::Low, Priority::Medium, Priority::High, Priority::Critical];

        for priority in priorities {
            let task = Task::new(
                fixtures.tasks_root.clone(),
                format!("Priority {:?} Task", priority),
                priority.clone()
            );
            assert_eq!(task.priority, priority);
        }
    }

    #[test]
    fn test_special_characters_in_task_fields() {
        let fixtures = TestFixtures::new();
        let mut task = Task::new(fixtures.tasks_root.clone(), "Task with 特殊字符 and émojis 🚀".to_string(), Priority::Medium
        );

        task.description = Some("Description with\nnewlines and\ttabs".to_string());
        task.tags = vec!["tag-with-dashes".to_string(), "tag_with_underscores".to_string()];

        let mut storage = fixtures.create_storage();
        let task_id = storage.add(&task, "VT", None);
        let actual_project = utils::get_project_for_task(&task_id).unwrap();

        let retrieved = storage.get(&task_id, actual_project).unwrap();
        assert_eq!(retrieved.title, "Task with 特殊字符 and émojis 🚀");
        assert!(retrieved.description.unwrap().contains("newlines"));
        assert!(retrieved.tags.contains(&"tag-with-dashes".to_string()));
    }

    #[test]
    fn test_empty_project_name_handling() {
        let fixtures = TestFixtures::new();

        let task = Task::new(fixtures.tasks_root.clone(), "Empty Project Task".to_string(), // Empty project name
            Priority::Medium
        );

        // Should handle empty project gracefully
        // Note: project field no longer exists

        let mut storage = fixtures.create_storage();
        let task_id = storage.add(&task, "VT", None);
        assert!(!task_id.is_empty(), "Should still create task with empty project");
    }
}

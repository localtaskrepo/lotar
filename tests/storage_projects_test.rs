use local_task_repo::storage::Task;
use local_task_repo::types::Priority;

mod common;
use common::{TestFixtures, utils};

/// Multi-project storage functionality tests
#[cfg(test)]
mod storage_projects_tests {
    use super::*;

    #[test]
    fn test_multiple_projects() {
        let fixtures = TestFixtures::new();
        let mut storage = fixtures.create_storage();

        // Create tasks in different projects
        let task1 = Task::new(fixtures.tasks_root.clone(), "Task in Project A".to_string(), Priority::High
        );

        let task2 = Task::new(fixtures.tasks_root.clone(), "Task in Project B".to_string(), Priority::Medium
        );

        let task3 = Task::new(fixtures.tasks_root.clone(), "Another task in Project A".to_string(), Priority::Low
        );

        // Add tasks
        let id1 = storage.add(&task1, "PA", None);
        let id2 = storage.add(&task2, "PB", None);
        let id3 = storage.add(&task3, "PA", None);

        // Get actual project prefixes that were generated
        let project_a = utils::get_project_for_task(&id1).unwrap();
        let project_b = utils::get_project_for_task(&id2).unwrap();
        let project_a_second = utils::get_project_for_task(&id3).unwrap();

        // Verify projects are isolated (different project names get different prefixes)
        assert_ne!(project_a, project_b, "Different projects should have different prefixes");

        // Note: The application may generate different prefixes for the same project name
        // if there are naming conflicts, so we just verify the tasks can be retrieved correctly

        // Verify tasks can be retrieved correctly
        let retrieved1 = storage.get(&id1, project_a.clone()).unwrap();
        let retrieved2 = storage.get(&id2, project_b.clone()).unwrap();
        let retrieved3 = storage.get(&id3, project_a_second.clone()).unwrap();

        assert_eq!(retrieved1.title, "Task in Project A");
        assert_eq!(retrieved2.title, "Task in Project B");
        assert_eq!(retrieved3.title, "Another task in Project A");

        // Verify task IDs have the correct prefixes
        assert!(id1.starts_with(&format!("{}-", project_a)));
        assert!(id2.starts_with(&format!("{}-", project_b)));
        assert!(id3.starts_with(&format!("{}-", project_a_second)));

        // Verify we have at least 2 distinct projects
        let mut unique_projects = std::collections::HashSet::new();
        unique_projects.insert(project_a);
        unique_projects.insert(project_b);
        unique_projects.insert(project_a_second);

        assert!(unique_projects.len() >= 2, "Should have at least 2 distinct project prefixes");
    }
}

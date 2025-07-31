use local_task_repo::storage::Task;
use local_task_repo::types::{TaskStatus, Priority};
use local_task_repo::index::TaskFilter;

mod common;
use common::{TestFixtures, utils};

#[test]
fn test_basic_task_status_update() {
    let fixtures = TestFixtures::new();
    let mut storage = fixtures.create_storage();
    
    let task = fixtures.create_sample_task("status-update-test");
    let task_id = storage.add(&task);
    
    // Get the actual project name that was created
    let actual_project = utils::get_project_for_task(&task_id).unwrap();

    // Initial status should be default (TODO)
    let task = storage.get(&task_id, actual_project.clone()).unwrap();
    assert_eq!(task.status, TaskStatus::Todo);
    
    // Update status
    let mut updated_task = task.clone();
    updated_task.status = TaskStatus::InProgress;
    storage.edit(&task_id, &updated_task);
    
    // Verify status was updated
    let final_task = storage.get(&task_id, actual_project.clone()).unwrap();
    assert_eq!(final_task.status, TaskStatus::InProgress);
}

#[test]
fn test_task_search_by_text() {
    let fixtures = TestFixtures::new();
    let mut storage = fixtures.create_storage();

    // Create test tasks with different content
    let task1 = Task::new(fixtures.tasks_root.clone(), "Authentication system".to_string(), "search-test".to_string(), Priority::High);
    let task1_id = storage.add(&task1);
    let actual_project = utils::get_project_for_task(&task1_id).unwrap();

    let mut task2 = Task::new(fixtures.tasks_root.clone(), "User interface design".to_string(), actual_project.clone(), Priority::Medium);
    let mut task3 = Task::new(fixtures.tasks_root.clone(), "Database authentication".to_string(), actual_project.clone(), Priority::High);

    task2.description = Some("Design responsive UI components".to_string());
    task3.description = Some("Setup database connection and authentication".to_string());

    storage.add(&task2);
    storage.add(&task3);

    // Search for tasks containing "authentication" using the correct project name
    let filter = TaskFilter {
        text_query: Some("authentication".to_string()),
        project: Some(actual_project.clone()),
        ..Default::default()
    };

    let results = storage.search(&filter);
    assert_eq!(results.len(), 2, "Should find 2 tasks with 'authentication'");

    // Verify correct tasks were found - fix tuple destructuring
    let titles: Vec<&str> = results.iter().map(|(_, task)| task.title.as_str()).collect();
    assert!(titles.contains(&"Authentication system"));
    assert!(titles.contains(&"Database authentication"));
    assert!(!titles.contains(&"User interface design"));
}

#[test]
fn test_task_filtering_by_status() {
    let fixtures = TestFixtures::new();
    let mut storage = fixtures.create_storage();
    
    let task1 = fixtures.create_sample_task("filter-test");
    let task1_id = storage.add(&task1);
    let actual_project = utils::get_project_for_task(&task1_id).unwrap();

    let mut task2 = fixtures.create_sample_task(&actual_project);
    task2.title = "In Progress Task".to_string();
    task2.status = TaskStatus::InProgress;
    
    let mut task3 = fixtures.create_sample_task(&actual_project);
    task3.title = "Done Task".to_string();
    task3.status = TaskStatus::Done;

    storage.add(&task2);
    storage.add(&task3);

    // Filter by TODO status using the correct project name
    let filter = TaskFilter {
        status: Some(TaskStatus::Todo),
        project: Some(actual_project.clone()),
        ..Default::default()
    };

    let results = storage.search(&filter);
    assert_eq!(results.len(), 1, "Should find 1 TODO task");
    assert_eq!(results[0].1.status, TaskStatus::Todo); // Fix: access task from tuple
}

#[test]
fn test_task_filtering_by_priority() {
    let fixtures = TestFixtures::new();
    let mut storage = fixtures.create_storage();

    let task1 = Task::new(fixtures.tasks_root.clone(), "High priority".to_string(), "priority-test".to_string(), Priority::High);
    let task1_id = storage.add(&task1);
    let actual_project = utils::get_project_for_task(&task1_id).unwrap();

    let task2 = Task::new(fixtures.tasks_root.clone(), "Low priority".to_string(), actual_project.clone(), Priority::Low);
    storage.add(&task2);

    // Filter by high priority using the correct project name
    let filter = TaskFilter {
        priority: Some(Priority::High),
        project: Some(actual_project.clone()),
        ..Default::default()
    };

    let results = storage.search(&filter);
    assert_eq!(results.len(), 1, "Should find 1 high priority task");
    assert_eq!(results[0].1.priority, Priority::High); // Fix: access task from tuple
}

#[test]
fn test_task_filtering_by_tags() {
    let fixtures = TestFixtures::new();
    let mut storage = fixtures.create_storage();

    let mut task1 = fixtures.create_sample_task("tag-test");
    task1.title = "Backend Task".to_string();
    task1.tags = vec!["backend".to_string(), "api".to_string()];
    let task1_id = storage.add(&task1);
    let actual_project = utils::get_project_for_task(&task1_id).unwrap();

    let mut task2 = fixtures.create_sample_task(&actual_project);
    task2.title = "Frontend Task".to_string();
    task2.tags = vec!["frontend".to_string(), "ui".to_string()];

    let mut task3 = fixtures.create_sample_task(&actual_project);
    task3.title = "Fullstack Task".to_string();
    task3.tags = vec!["backend".to_string(), "frontend".to_string()];

    storage.add(&task2);
    storage.add(&task3);

    // Filter by "backend" tag using the correct project name
    let filter = TaskFilter {
        tags: vec!["backend".to_string()],
        project: Some(actual_project.clone()),
        ..Default::default()
    };

    let results = storage.search(&filter);
    assert_eq!(results.len(), 2, "Should find 2 tasks with 'backend' tag");

    let titles: Vec<&str> = results.iter().map(|(_, task)| task.title.as_str()).collect(); // Fix: access task from tuple
    assert!(titles.contains(&"Backend Task"));
    assert!(titles.contains(&"Fullstack Task"));
    assert!(!titles.contains(&"Frontend Task"));
}

#[test]
fn test_complex_task_filtering() {
    let fixtures = TestFixtures::new();
    let mut storage = fixtures.create_storage();

    let mut task = Task::new(fixtures.tasks_root.clone(), "Authentication API".to_string(), "complex-test".to_string(), Priority::High);
    task.status = TaskStatus::InProgress;
    task.tags = vec!["backend".to_string(), "security".to_string()];
    task.description = Some("Implement secure authentication API endpoint".to_string());

    let task_id = storage.add(&task);
    let actual_project = utils::get_project_for_task(&task_id).unwrap();

    // Complex filter: text + status + tags using the correct project name
    let filter = TaskFilter {
        text_query: Some("authentication".to_string()),
        status: Some(TaskStatus::InProgress),
        tags: vec!["backend".to_string()],
        project: Some(actual_project.clone()),
        ..Default::default()
    };

    let results = storage.search(&filter);
    assert_eq!(results.len(), 1, "Should find 1 task matching all criteria");
    assert_eq!(results[0].1.title, "Authentication API"); // Fix: access task from tuple
}

#[test]
fn test_empty_search_results() {
    let fixtures = TestFixtures::new();
    let storage = fixtures.create_storage();

    // Search in empty storage
    let filter = TaskFilter {
        text_query: Some("nonexistent".to_string()),
        project: Some("empty-test".to_string()),
        ..Default::default()
    };

    let results = storage.search(&filter);
    assert_eq!(results.len(), 0, "Should return empty results for nonexistent content");
}

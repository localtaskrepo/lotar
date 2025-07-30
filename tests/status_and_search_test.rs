use local_task_repo::store::{Task, Storage};
use local_task_repo::types::{TaskStatus, Priority};
use local_task_repo::index::TaskFilter;

mod common;
use common::TestFixtures;

#[test]
fn test_basic_task_status_update() {
    let fixtures = TestFixtures::new();
    let mut storage = fixtures.create_storage();
    
    let task = fixtures.create_sample_task("status-update-test");
    let task_id = storage.add(&task);
    
    // Initial status should be default (TODO)
    let task = storage.get(&task_id, "status-update-test".to_string()).unwrap();
    assert_eq!(task.status, TaskStatus::Todo);
    
    // Update status
    let mut updated_task = task.clone();
    updated_task.status = TaskStatus::InProgress;
    storage.edit(&task_id, &updated_task);
    
    // Verify status was updated
    let final_task = storage.get(&task_id, "status-update-test".to_string()).unwrap();
    assert_eq!(final_task.status, TaskStatus::InProgress);
}

#[test]
fn test_task_search_by_text() {
    let fixtures = TestFixtures::new();
    let mut storage = fixtures.create_storage();

    // Create test tasks with different content
    let mut task1 = Task::new(fixtures.tasks_root.clone(), "Authentication system".to_string(), "search-test".to_string(), Priority::High);
    let mut task2 = Task::new(fixtures.tasks_root.clone(), "User interface design".to_string(), "search-test".to_string(), Priority::Medium);
    let mut task3 = Task::new(fixtures.tasks_root.clone(), "Database authentication".to_string(), "search-test".to_string(), Priority::High);

    task1.description = Some("Implement OAuth authentication for secure login".to_string());
    task2.description = Some("Design responsive UI components".to_string());
    task3.description = Some("Setup database connection and authentication".to_string());

    let _id1 = storage.add(&task1);
    let _id2 = storage.add(&task2);
    let _id3 = storage.add(&task3);

    // Search for tasks containing "authentication"
    let filter = TaskFilter {
        text_query: Some("authentication".to_string()),
        project: Some("search-test".to_string()),
        ..Default::default()
    };

    let results = storage.search(&filter);
    assert_eq!(results.len(), 2, "Should find 2 tasks with 'authentication'");

    // Verify correct tasks were found
    let titles: Vec<&str> = results.iter().map(|t| t.title.as_str()).collect();
    assert!(titles.contains(&"Authentication system"));
    assert!(titles.contains(&"Database authentication"));
    assert!(!titles.contains(&"User interface design"));
}

#[test]
fn test_task_filtering_by_status() {
    let fixtures = TestFixtures::new();
    let mut storage = fixtures.create_storage();
    
    let mut task1 = fixtures.create_sample_task("filter-test");
    task1.title = "Todo Task".to_string();
    task1.status = TaskStatus::Todo;
    
    let mut task2 = fixtures.create_sample_task("filter-test");
    task2.title = "In Progress Task".to_string();
    task2.status = TaskStatus::InProgress;
    
    let mut task3 = fixtures.create_sample_task("filter-test");
    task3.title = "Done Task".to_string();
    task3.status = TaskStatus::Done;

    storage.add(&task1);
    storage.add(&task2);
    storage.add(&task3);

    // Filter by TODO status
    let filter = TaskFilter {
        status: Some(TaskStatus::Todo),
        project: Some("filter-test".to_string()),
        ..Default::default()
    };

    let results = storage.search(&filter);
    assert_eq!(results.len(), 1, "Should find 1 TODO task");
    assert_eq!(results[0].title, "Todo Task");
    assert_eq!(results[0].status, TaskStatus::Todo);
}

#[test]
fn test_task_filtering_by_priority() {
    let fixtures = TestFixtures::new();
    let mut storage = fixtures.create_storage();

    let task1 = Task::new(fixtures.tasks_root.clone(), "High priority".to_string(), "priority-test".to_string(), Priority::High);
    let task2 = Task::new(fixtures.tasks_root.clone(), "Low priority".to_string(), "priority-test".to_string(), Priority::Low);

    storage.add(&task1);
    storage.add(&task2);

    // Filter by high priority (priority value 1 in the old system maps to High in new system)
    let filter = TaskFilter {
        priority: Some(1), // This maps to High priority in the filter system
        project: Some("priority-test".to_string()),
        ..Default::default()
    };

    let results = storage.search(&filter);
    assert_eq!(results.len(), 1, "Should find 1 high priority task");
    assert_eq!(results[0].priority, Priority::High);
}

#[test]
fn test_task_filtering_by_tags() {
    let fixtures = TestFixtures::new();
    let mut storage = fixtures.create_storage();

    let mut task1 = fixtures.create_sample_task("tag-test");
    task1.title = "Backend Task".to_string();
    task1.tags = vec!["backend".to_string(), "api".to_string()];

    let mut task2 = fixtures.create_sample_task("tag-test");
    task2.title = "Frontend Task".to_string(); 
    task2.tags = vec!["frontend".to_string(), "ui".to_string()];

    let mut task3 = fixtures.create_sample_task("tag-test");
    task3.title = "Fullstack Task".to_string();
    task3.tags = vec!["backend".to_string(), "frontend".to_string()];

    storage.add(&task1);
    storage.add(&task2);
    storage.add(&task3);

    // Filter by "backend" tag
    let filter = TaskFilter {
        tags: vec!["backend".to_string()],
        project: Some("tag-test".to_string()),
        ..Default::default()
    };

    let results = storage.search(&filter);
    assert_eq!(results.len(), 2, "Should find 2 tasks with 'backend' tag");

    let titles: Vec<&str> = results.iter().map(|t| t.title.as_str()).collect();
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

    storage.add(&task);

    // Complex filter: text + status + tags
    let filter = TaskFilter {
        text_query: Some("authentication".to_string()),
        status: Some(TaskStatus::InProgress),
        tags: vec!["backend".to_string()],
        project: Some("complex-test".to_string()),
        ..Default::default()
    };

    let results = storage.search(&filter);
    assert_eq!(results.len(), 1, "Should find 1 task matching all criteria");
    assert_eq!(results[0].title, "Authentication API");
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

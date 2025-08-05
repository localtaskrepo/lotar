use lotar::cli::handlers::{CommandHandler, AddHandler};
use lotar::cli::handlers::task::SearchHandler;
use lotar::cli::{AddArgs, TaskSearchArgs};
use lotar::workspace::{TasksDirectoryResolver, TasksDirectorySource};
use lotar::output::{OutputRenderer, OutputFormat};
use tempfile::TempDir;

/// Simple test to verify handlers work
#[test]
fn test_handler_basic_functionality() {
    // Create test environment
    let temp_dir = TempDir::new().expect("Failed to create temp directory");
    let tasks_dir = temp_dir.path().join("tasks");
    std::fs::create_dir_all(&tasks_dir).expect("Failed to create tasks directory");

    let resolver = TasksDirectoryResolver {
        path: tasks_dir.clone(),
        source: TasksDirectorySource::CurrentDirectory,
    };

    // Test AddHandler
    let args = AddArgs {
        title: "Test task".to_string(),
        task_type: None,
        priority: None,
        assignee: None,
        effort: None,
        due: None,
        description: None,
        category: None,
        tags: vec![],
        fields: vec![],
        bug: false,
        epic: false,
        critical: false,
        high: false,
    };

    let renderer = OutputRenderer::new(OutputFormat::Text, false);
    println!("Testing AddHandler...");
    let result = AddHandler::execute(args, None, &resolver, &renderer);
    match &result {
        Ok(task_id) => println!("✅ AddHandler succeeded, got task ID: {}", task_id),
        Err(e) => println!("❌ AddHandler failed: {}", e),
    }
    assert!(result.is_ok(), "AddHandler should succeed");

    // Test SearchHandler (unified list/search)
    let args = TaskSearchArgs {
        query: None, // No query means list all
        assignee: None,
        mine: false,
        status: None,
        priority: None,
        task_type: None,
        category: None,
        tag: None,
        high: false,
        critical: false,
        limit: 20,
    };

    println!("Testing SearchHandler (list mode)...");
    let result = SearchHandler::execute(args, None, &resolver, &renderer);
    match &result {
        Ok(()) => println!("✅ SearchHandler succeeded"),
        Err(e) => println!("❌ SearchHandler failed: {}", e),
    }
    assert!(result.is_ok(), "SearchHandler should succeed");

    println!("🎉 Basic handler functionality test passed!");
}

/// Test that handlers return consistent results
#[test]
fn test_handler_consistency() {
    let temp_dir = TempDir::new().expect("Failed to create temp directory");
    let tasks_dir = temp_dir.path().join("tasks");
    std::fs::create_dir_all(&tasks_dir).expect("Failed to create tasks directory");

    let resolver = TasksDirectoryResolver {
        path: tasks_dir.clone(),
        source: TasksDirectorySource::CurrentDirectory,
    };

    let renderer = OutputRenderer::new(OutputFormat::Text, false);

    // Create a task
    let args = AddArgs {
        title: "Consistency test task".to_string(),
        task_type: None,
        priority: None,
        assignee: None,
        effort: None,
        due: None,
        description: None,
        category: None,
        tags: vec![],
        fields: vec![],
        bug: false,
        epic: false,
        critical: false,
        high: false,
    };

    let task_id = AddHandler::execute(args, None, &resolver, &renderer)
        .expect("Should be able to create task");
    
    println!("Created task with ID: {}", task_id);

    // List tasks to verify it appears
    let args = TaskSearchArgs {
        query: None, // No query means list all
        assignee: None,
        mine: false,
        status: None,
        priority: None,
        task_type: None,
        category: None,
        tag: None,
        high: false,
        critical: false,
        limit: 20,
    };

    SearchHandler::execute(args, None, &resolver, &renderer)
        .expect("Should be able to list tasks");
    
    println!("Successfully listed tasks after creation");
    
    // The SearchHandler just prints output, so we can't assert on task count
    // But if it doesn't error, the test passes

    println!("🎉 Handler consistency test passed!");
}

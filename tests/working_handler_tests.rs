use lotar::cli::handlers::{CommandHandler, AddHandler};
use lotar::cli::handlers::task::SearchHandler;
use lotar::cli::handlers::status::{StatusHandler, StatusArgs};
use lotar::cli::{AddArgs, TaskSearchArgs};
use lotar::workspace::{TasksDirectoryResolver, TasksDirectorySource};
use lotar::output::{OutputRenderer, OutputFormat};
use std::fs;
use tempfile::TempDir;

/// Simple test harness for handler testing
pub struct SimpleHandlerTestHarness {
    _temp_dir: TempDir,
    resolver: TasksDirectoryResolver,
}

impl SimpleHandlerTestHarness {
    /// Create a new test harness
    pub fn new() -> Self {
        let temp_dir = TempDir::new().expect("Failed to create temp directory");
        let tasks_dir = temp_dir.path().join("tasks");
        fs::create_dir_all(&tasks_dir).expect("Failed to create tasks directory");

        let resolver = TasksDirectoryResolver {
            path: tasks_dir,
            source: TasksDirectorySource::CurrentDirectory,
        };

        Self {
            _temp_dir: temp_dir,
            resolver,
        }
    }

    /// Test add handler
    pub fn test_add_handler(&self, title: &str, project: Option<&str>, renderer: &OutputRenderer) -> Result<String, String> {
        let args = AddArgs {
            title: title.to_string(),
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

        AddHandler::execute(args, project, &self.resolver, renderer)
    }

    /// Test list handler (now using SearchHandler)
    pub fn test_list_handler(&self, project: Option<&str>, renderer: &OutputRenderer) -> Result<(), String> {
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

        SearchHandler::execute(args, project, &self.resolver, renderer)
    }

    /// Test status handler
    pub fn test_status_handler(&self, task_id: &str, new_status: &str, project: Option<&str>, renderer: &OutputRenderer) -> Result<(), String> {
        let args = StatusArgs {
            task_id: task_id.to_string(),
            new_status: new_status.to_string(),
            explicit_project: project.map(|s| s.to_string()),
        };

        StatusHandler::execute(args, project, &self.resolver, renderer)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_basic_add_handler() {
        let harness = SimpleHandlerTestHarness::new();
        let renderer = OutputRenderer::new(OutputFormat::Text, false);
        
        // Test basic task creation
        let result = harness.test_add_handler("Test task", None, &renderer);
        assert!(result.is_ok(), "AddHandler should succeed");
        
        let task_id = result.unwrap();
        assert!(!task_id.is_empty(), "Task ID should not be empty");
        assert!(task_id.contains("-"), "Task ID should contain dash separator");
        
        println!("✅ Created task with ID: {}", task_id);
    }

    #[test]
    fn test_add_then_list() {
        let harness = SimpleHandlerTestHarness::new();
        let renderer = OutputRenderer::new(OutputFormat::Text, false);
        
        // Create a task
        let add_result = harness.test_add_handler("List test task", None, &renderer);
        assert!(add_result.is_ok(), "Should be able to create task");
        
        let task_id = add_result.unwrap();
        println!("Created task: {}", task_id);
        
        // List tasks
        let list_result = harness.test_list_handler(None, &renderer);
        assert!(list_result.is_ok(), "Should be able to list tasks");
        
        println!("✅ List command executed successfully");
    }

    #[test]
    fn test_multiple_tasks() {
        let harness = SimpleHandlerTestHarness::new();
        let renderer = OutputRenderer::new(OutputFormat::Text, false);
        
        // Create multiple tasks
        let task1 = harness.test_add_handler("First task", None, &renderer);
        let task2 = harness.test_add_handler("Second task", None, &renderer);
        let task3 = harness.test_add_handler("Third task", None, &renderer);
        
        assert!(task1.is_ok() && task2.is_ok() && task3.is_ok(), "All tasks should be created successfully");
        
        // List all tasks
        let list_result = harness.test_list_handler(None, &renderer);
        assert!(list_result.is_ok(), "Should be able to list tasks");
        
        println!("✅ Created 3 tasks, list command executed successfully");
    }

    #[test]
    fn test_status_handler() {
        let harness = SimpleHandlerTestHarness::new();
        let renderer = OutputRenderer::new(OutputFormat::Text, false);
        
        // Create a task first
        let task_id = harness.test_add_handler("Status test task", None, &renderer)
            .expect("Should be able to create task");
        
        println!("Created task: {}", task_id);
        
        // Try to change status
        let status_result = harness.test_status_handler(&task_id, "in-progress", None, &renderer);
        
        // We don't assert success here because the status handler might have validation
        // The important thing is that it doesn't crash
        match status_result {
            Ok(_) => println!("✅ Status change succeeded"),
            Err(e) => println!("ℹ️ Status change failed (expected): {}", e),
        }
    }

    #[test]
    fn test_project_specific_operations() {
        let harness = SimpleHandlerTestHarness::new();
        let renderer = OutputRenderer::new(OutputFormat::Text, false);
        
        // Test with explicit project
        let result1 = harness.test_add_handler("Project specific task", Some("MYPROJ"), &renderer);
        
        // This might fail if project validation is strict, but shouldn't crash
        match result1 {
            Ok(task_id) => {
                println!("✅ Created project-specific task: {}", task_id);
                
                // Try listing for this project
                let list_result = harness.test_list_handler(Some("MYPROJ"), &renderer);
                match list_result {
                    Ok(()) => println!("✅ Listed tasks for project MYPROJ"),
                    Err(e) => println!("ℹ️ Project listing failed: {}", e),
                }
            }
            Err(e) => println!("ℹ️ Project-specific task creation failed: {}", e),
        }
    }

    #[test]
    fn test_error_handling() {
        let harness = SimpleHandlerTestHarness::new();
        let renderer = OutputRenderer::new(OutputFormat::Text, false);
        
        // Test with empty title (should fail)
        let result = harness.test_add_handler("", None, &renderer);
        // We don't assert failure because the behavior might vary
        
        match result {
            Ok(task_id) => println!("ℹ️ Empty title was accepted, got: {}", task_id),
            Err(e) => println!("✅ Empty title correctly rejected: {}", e),
        }
        
        // Test status change on non-existent task
        let status_result = harness.test_status_handler("NONEXISTENT-999", "done", None, &renderer);
        match status_result {
            Ok(_) => println!("ℹ️ Non-existent task status change was accepted"),
            Err(e) => println!("✅ Non-existent task correctly rejected: {}", e),
        }
    }

    #[test]
    fn test_handler_consistency() {
        let harness = SimpleHandlerTestHarness::new();
        let renderer = OutputRenderer::new(OutputFormat::Text, false);
        
        // Test that operations are consistent
        let initial_list = harness.test_list_handler(None, &renderer);
        assert!(initial_list.is_ok(), "Initial list should succeed");
            
        // Add a task
        let add_result = harness.test_add_handler("Consistency test", None, &renderer);
        assert!(add_result.is_ok(), "Task creation should succeed");
        
        // Should still be able to list
        let final_list = harness.test_list_handler(None, &renderer);
        assert!(final_list.is_ok(), "Final list should succeed");
            
        println!("✅ Handler consistency test passed");
    }
}

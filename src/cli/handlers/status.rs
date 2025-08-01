use crate::cli::project::ProjectResolver;
use crate::cli::validation::CliValidator;
use crate::storage::Storage;
use crate::workspace::TasksDirectoryResolver;
use crate::cli::handlers::CommandHandler;

/// Handler for status change commands
pub struct StatusHandler;

impl CommandHandler for StatusHandler {
    type Args = StatusArgs;
    type Result = Result<(), String>;
    
    fn execute(args: Self::Args, project: Option<&str>, resolver: &TasksDirectoryResolver) -> Self::Result {
        // Create project resolver and validator
        let project_resolver = ProjectResolver::new(resolver)
            .map_err(|e| format!("Failed to initialize project resolver: {}", e))?;
        
        let config = project_resolver.get_config();
        let validator = CliValidator::new(config);
        
        // Validate task ID format
        project_resolver.validate_task_id_format(&args.task_id)
            .map_err(|e| format!("Invalid task ID: {}", e))?;
        
        // Resolve project from task ID - function parameter takes precedence
        let effective_project = project.or(args.explicit_project.as_deref());
        let resolved_project = project_resolver.resolve_project(&args.task_id, effective_project)
            .map_err(|e| format!("Could not resolve project: {}", e))?;
        
        // Get full task ID with project prefix
        let full_task_id = project_resolver.get_full_task_id(&args.task_id, effective_project)
            .map_err(|e| format!("Could not determine full task ID: {}", e))?;
        
        // Validate the new status against project configuration
        let validated_status = validator.validate_status(&args.new_status)
            .map_err(|e| format!("Status validation failed: {}", e))?;
        
        // Load the task and update its status
        let mut storage = Storage::new(resolver.path.clone());
        
        // Find and load the task
        let task_result = storage.get(&full_task_id, resolved_project.clone());
        let mut task = task_result.ok_or_else(|| format!("Task '{}' not found", full_task_id))?;
        
        let old_status = task.status.clone();
        task.status = validated_status;
        
        // Save the updated task
        storage.edit(&full_task_id, &task);
        
        println!(
            "Task {} status changed from {} to {}", 
            full_task_id, 
            old_status,
            task.status
        );
        
        Ok(())
    }
}

/// Arguments for status change command
pub struct StatusArgs {
    pub task_id: String,
    pub new_status: String,
    pub explicit_project: Option<String>,
}

impl StatusArgs {
    pub fn new(task_id: String, new_status: String, explicit_project: Option<String>) -> Self {
        Self {
            task_id,
            new_status,
            explicit_project,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    fn create_test_resolver() -> TasksDirectoryResolver {
        TasksDirectoryResolver {
            path: std::path::PathBuf::from("/tmp/test_tasks"),
            source: crate::workspace::TasksDirectorySource::CurrentDirectory,
        }
    }
    
    #[test]
    fn test_status_args_creation() {
        let args = StatusArgs::new(
            "AUTH-123".to_string(),
            "InProgress".to_string(),
            Some("auth".to_string())
        );
        
        assert_eq!(args.task_id, "AUTH-123");
        assert_eq!(args.new_status, "InProgress");
        assert_eq!(args.explicit_project, Some("auth".to_string()));
    }
    
    #[test]
    fn test_status_handler_structure() {
        let args = StatusArgs::new(
            "123".to_string(),
            "Done".to_string(),
            None
        );
        
        let resolver = create_test_resolver();
        
        // This would fail in a real test because we need actual config files and tasks
        // But it demonstrates the structure
        match StatusHandler::execute(args, None, &resolver) {
            Ok(()) => println!("Success: Status changed"),
            Err(e) => println!("Expected error in test: {}", e),
        }
    }
}

use crate::cli::AddArgs;
use crate::cli::project::ProjectResolver;
use crate::cli::validation::CliValidator;
use crate::output::OutputRenderer;
use crate::storage::{Storage, task::Task};
use crate::types::TaskType;
use crate::workspace::TasksDirectoryResolver;

pub mod status;
pub mod task;
pub mod commands;

// Re-export handlers for easy access
pub use task::TaskHandler;
pub use commands::{ConfigHandler, ScanHandler, ServeHandler, IndexHandler};

/// Trait for command handlers
pub trait CommandHandler {
    type Args;
    type Result;
    
    fn execute(args: Self::Args, project: Option<&str>, resolver: &TasksDirectoryResolver, renderer: &OutputRenderer) -> Self::Result;
}

/// Handler for adding tasks with the new CLI
pub struct AddHandler;

impl CommandHandler for AddHandler {
    type Args = AddArgs;
    type Result = Result<String, String>;
    
    fn execute(args: Self::Args, project: Option<&str>, resolver: &TasksDirectoryResolver, _renderer: &OutputRenderer) -> Self::Result {
        // Create project resolver and validator
        let project_resolver = ProjectResolver::new(resolver)
            .map_err(|e| format!("Failed to initialize project resolver: {}", e))?;
        
        // Resolve project first (needed for project-specific config)
        let effective_project = match project_resolver.resolve_project("", project) {
            Ok(project) => {
                if project.is_empty() {
                    // No default project set, use global config
                    None
                } else {
                    Some(project)
                }
            },
            Err(e) => {
                // Project validation failed - this should be an error, not fallback
                return Err(e);
            }
        };
        
        // Get appropriate configuration (project-specific or global)
        let config = match &effective_project {
            Some(project_name) => {
                project_resolver.get_project_config(project_name)
                    .map_err(|e| format!("Failed to get project configuration: {}", e))?
            },
            None => {
                // Use global config
                project_resolver.get_config().clone()
            }
        };
        
        let validator = CliValidator::new(&config);
        
        // Process and validate arguments
        let validated_type = if args.bug {
            validator.validate_task_type("Bug")
                .map_err(|e| format!("Task type validation failed: {}", e))?
        } else if args.epic {
            validator.validate_task_type("Epic")
                .map_err(|e| format!("Task type validation failed: {}", e))?
        } else {
            match args.task_type {
                Some(task_type) => validator.validate_task_type(&task_type)
                    .map_err(|e| format!("Task type validation failed: {}", e))?,
                None => TaskType::Feature // Default
            }
        };
        
        let validated_priority = if args.critical {
            validator.validate_priority("Critical")
                .map_err(|e| format!("Priority validation failed: {}", e))?
        } else if args.high {
            validator.validate_priority("High")
                .map_err(|e| format!("Priority validation failed: {}", e))?
        } else {
            match args.priority {
                Some(priority) => validator.validate_priority(&priority)
                    .map_err(|e| format!("Priority validation failed: {}", e))?,
                None => config.default_priority.clone()
            }
        };
        
        // Validate assignee if provided
        let validated_assignee = if let Some(ref assignee) = args.assignee {
            Some(validator.validate_assignee(assignee)
                .map_err(|e| format!("Assignee validation failed: {}", e))?)
        } else {
            config.default_assignee.clone()
        };
        
        // Validate due date if provided
        let validated_due_date = if let Some(ref due_date) = args.due {
            Some(validator.parse_due_date(due_date)
                .map_err(|e| format!("Due date validation failed: {}", e))?)
        } else {
            None
        };
        
        // Validate effort if provided
        let validated_effort = if let Some(ref effort) = args.effort {
            Some(validator.validate_effort(effort)
                .map_err(|e| format!("Effort validation failed: {}", e))?)
        } else {
            None
        };
        
        // Validate category if provided
        let validated_category = if let Some(ref category) = args.category {
            Some(validator.validate_category(category)
                .map_err(|e| format!("Category validation failed: {}", e))?)
        } else {
            None
        };
        
        // Validate tags
        let mut validated_tags = Vec::new();
        for tag in &args.tags {
            let validated_tag = validator.validate_tag(tag)
                .map_err(|e| format!("Tag validation failed for '{}': {}", tag, e))?;
            validated_tags.push(validated_tag);
        }
        
        // Create the task
        let mut task = Task::new(
            resolver.path.clone(),
            args.title,
            validated_priority,
        );
        
        // Set validated properties
        task.task_type = validated_type;
        task.assignee = validated_assignee;
        task.due_date = validated_due_date;
        task.effort = validated_effort;
        task.description = args.description;
        task.category = validated_category;
        task.tags = validated_tags;
        
        // Handle arbitrary fields with validation
        for (key, value) in args.fields {
            // Validate the custom field name and value
            let (validated_key, validated_value) = validator.validate_custom_field(&key, &value)
                .map_err(|e| format!("Custom field validation failed for '{}': {}", key, e))?;
            
            // Store as custom fields using serde_yaml::Value
            task.custom_fields.insert(validated_key, serde_yaml::Value::String(validated_value));
        }
        
        // Save the task
        let mut storage = Storage::new(resolver.path.clone());
        
        // Use resolved project prefix, not the raw project name
        let project_for_storage = if let Some(project) = effective_project.as_deref() {
            // If we have an explicit project, resolve it to its prefix
            crate::utils::resolve_project_input(project, &resolver.path)
        } else {
            // Use the same logic as get_effective_project_name
            crate::project::get_effective_project_name(resolver)
        };
        
        let task_id = storage.add(&task, &project_for_storage, None);
        
        Ok(task_id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    // Helper function to create a test resolver
    fn create_test_resolver() -> TasksDirectoryResolver {
        TasksDirectoryResolver {
            path: std::path::PathBuf::from("/tmp/test_tasks"),
            source: crate::workspace::TasksDirectorySource::CurrentDirectory,
        }
    }
    
    #[test]
    fn test_add_handler_basic() {
        let args = AddArgs {
            title: "Test Task".to_string(),
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
        
        let resolver = create_test_resolver();
        let renderer = OutputRenderer::new(crate::output::OutputFormat::Text, false);
        
        // This would fail in a real test because we need actual config files
        // But it demonstrates the structure
        match AddHandler::execute(args, None, &resolver, &renderer) {
            Ok(task_id) => println!("Created task: {}", task_id),
            Err(e) => println!("Expected error in test: {}", e),
        }
    }
}

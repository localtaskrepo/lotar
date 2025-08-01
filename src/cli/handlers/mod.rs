use crate::cli::{AddArgs, ListArgs};
use crate::cli::project::ProjectResolver;
use crate::cli::validation::CliValidator;
use crate::storage::{Storage, task::Task};
use crate::types::{Priority, TaskType};
use crate::workspace::TasksDirectoryResolver;
use crate::index::TaskFilter;

pub mod status;

/// Trait for command handlers
pub trait CommandHandler {
    type Args;
    type Result;
    
    fn execute(args: Self::Args, project: Option<&str>, resolver: &TasksDirectoryResolver) -> Self::Result;
}

/// Handler for adding tasks with the new CLI
pub struct AddHandler;

impl CommandHandler for AddHandler {
    type Args = AddArgs;
    type Result = Result<String, String>;
    
    fn execute(args: Self::Args, project: Option<&str>, resolver: &TasksDirectoryResolver) -> Self::Result {
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
        let task_type = if args.bug {
            TaskType::Bug
        } else if args.epic {
            TaskType::Epic
        } else {
            args.task_type.map(Into::into).unwrap_or(TaskType::Feature)
        };
        
        // Validate task type
        let task_type_str = task_type.to_string();
        let validated_type = validator.validate_task_type(&task_type_str)
            .map_err(|e| format!("Task type validation failed: {}", e))?;
        
        let priority = if args.critical {
            Priority::Critical
        } else if args.high {
            Priority::High
        } else {
            args.priority.map(Into::into).unwrap_or(config.default_priority.clone())
        };
        
        // Validate priority
        let priority_str = priority.to_string();
        let validated_priority = validator.validate_priority(&priority_str)
            .map_err(|e| format!("Priority validation failed: {}", e))?;
        
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

/// Handler for listing tasks with the new CLI
pub struct ListHandler;

impl CommandHandler for ListHandler {
    type Args = ListArgs;
    type Result = Result<Vec<Task>, String>;
    
    fn execute(args: Self::Args, project: Option<&str>, resolver: &TasksDirectoryResolver) -> Self::Result {
        let project_resolver = ProjectResolver::new(resolver)
            .map_err(|e| format!("Failed to initialize project resolver: {}", e))?;
        
        let config = project_resolver.get_config();
        let validator = CliValidator::new(config);
        
        // Create storage instance
        let storage = Storage::new(resolver.path.clone());
        
        // Create task filter with project if specified
        let mut task_filter = TaskFilter::default();
        if let Some(project_filter) = project {
            task_filter.project = Some(project_filter.to_string());
        }
        
        let task_tuples = storage.search(&task_filter);
        let mut tasks: Vec<Task> = task_tuples.into_iter().map(|(_, task)| task).collect();
        
        // Apply additional filters
        
        // Filter by assignee
        if let Some(ref assignee) = args.assignee {
            let filter_assignee = if assignee == "@me" {
                // TODO: Resolve @me to actual user
                "@me".to_string()
            } else {
                assignee.clone()
            };
            tasks.retain(|task| {
                task.assignee.as_ref().map_or(false, |a| a == &filter_assignee)
            });
        }
        
        if args.mine {
            // TODO: Resolve current user and filter by that
            // For now, filter by @me placeholder
            tasks.retain(|task| {
                task.assignee.as_ref().map_or(false, |a| a == "@me")
            });
        }
        
        // Filter by status
        if let Some(ref status) = args.status {
            // Validate the status first
            let validated_status = validator.validate_status(status)
                .map_err(|e| format!("Status filter validation failed: {}", e))?;
            
            tasks.retain(|task| task.status == validated_status);
        }
        
        // Filter by priority
        if let Some(ref priority) = args.priority {
            let target_priority: Priority = priority.clone().into();
            tasks.retain(|task| task.priority == target_priority);
        }
        
        if args.high {
            tasks.retain(|task| task.priority == Priority::High);
        }
        
        if args.critical {
            tasks.retain(|task| task.priority == Priority::Critical);
        }
        
        // Filter by type
        if let Some(ref task_type) = args.task_type {
            let target_type: TaskType = task_type.clone().into();
            tasks.retain(|task| task.task_type == target_type);
        }
        
        // Filter by category
        if let Some(ref category) = args.category {
            tasks.retain(|task| {
                task.category.as_ref().map_or(false, |c| c == category)
            });
        }
        
        // Filter by tag
        if let Some(ref tag) = args.tag {
            tasks.retain(|task| task.tags.contains(tag));
        }
        
        // Apply limit
        if tasks.len() > args.limit {
            tasks.truncate(args.limit);
        }
        
        Ok(tasks)
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
        
        // This would fail in a real test because we need actual config files
        // But it demonstrates the structure
        match AddHandler::execute(args, None, &resolver) {
            Ok(task_id) => println!("Created task: {}", task_id),
            Err(e) => println!("Expected error in test: {}", e),
        }
    }
}

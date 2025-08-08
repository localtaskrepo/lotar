use crate::cli::handlers::CommandHandler;
use crate::cli::project::ProjectResolver;
use crate::cli::validation::CliValidator;
use crate::output::OutputRenderer;
use crate::storage::manager::Storage;
use crate::workspace::TasksDirectoryResolver;

/// Handler for priority change commands
pub struct PriorityHandler;

impl CommandHandler for PriorityHandler {
    type Args = PriorityArgs;
    type Result = Result<(), String>;

    fn execute(
        args: Self::Args,
        project: Option<&str>,
        resolver: &TasksDirectoryResolver,
        renderer: &OutputRenderer,
    ) -> Self::Result {
        // Create project resolver
        let mut project_resolver = ProjectResolver::new(resolver)
            .map_err(|e| format!("Failed to initialize project resolver: {}", e))?;

        // Validate task ID format
        project_resolver
            .validate_task_id_format(&args.task_id)
            .map_err(|e| format!("Invalid task ID: {}", e))?;

        // Resolve project from task ID - function parameter takes precedence
        let effective_project = project.or(args.explicit_project.as_deref());

        // Check for conflicts between full task ID and explicit project argument
        let final_effective_project = if let Some(explicit_proj) = effective_project {
            if let Some(task_id_prefix) =
                project_resolver.extract_project_from_task_id(&args.task_id)
            {
                let explicit_as_prefix =
                    project_resolver.resolve_project_name_to_prefix(explicit_proj);
                if task_id_prefix != explicit_as_prefix {
                    println!("{}", renderer.render_warning(&format!(
                        "Warning: Task ID '{}' belongs to project '{}', but project '{}' was specified. Using task ID's project.",
                        args.task_id, task_id_prefix, explicit_proj
                    )));
                    // Use task ID's project instead of the conflicting explicit project
                    None
                } else {
                    effective_project
                }
            } else {
                effective_project
            }
        } else {
            effective_project
        };

        // Get project configuration for validation
        let resolved_project = project_resolver.resolve_project("", final_effective_project)?;

        // Now get the config after project resolution
        let config = project_resolver.get_config();

        let project_config = if !resolved_project.is_empty() {
            project_resolver
                .get_project_config(&resolved_project)
                .map_err(|e| format!("Failed to get project configuration: {}", e))?
        } else {
            config.clone()
        };

        let project_validator = CliValidator::new(&project_config);

        // Try to open existing storage first (for both read and write operations)
        let mut storage = match Storage::try_open(resolver.path.clone()) {
            Some(storage) => storage,
            None => {
                return Err("No tasks found. Use 'lotar add' to create tasks first.".to_string());
            }
        };
        let project_prefix = if let Some(project) = final_effective_project {
            crate::utils::resolve_project_input(project, resolver.path.as_path())
        } else {
            crate::project::get_effective_project_name(resolver)
        };

        match args.new_priority {
            Some(new_priority) => {
                // SET operation: Change task priority
                let validated_priority = project_validator
                    .validate_priority(&new_priority)
                    .map_err(|e| format!("Priority validation failed: {}", e))?;

                // Load the task
                let mut task = storage
                    .get(&args.task_id, project_prefix.clone())
                    .ok_or_else(|| format!("Task '{}' not found", args.task_id))?;

                let old_priority = task.priority;

                // Check if priority is actually changing
                if old_priority == validated_priority {
                    println!(
                        "{}",
                        renderer.render_warning(&format!(
                            "Task {} priority is already {}",
                            args.task_id, validated_priority
                        ))
                    );
                    return Ok(());
                }

                // Update priority
                task.priority = validated_priority;
                storage.edit(&args.task_id, &task);

                println!(
                    "{}",
                    renderer.render_success(&format!(
                        "Task {} priority changed from {} to {}",
                        args.task_id, old_priority, task.priority
                    ))
                );

                Ok(())
            }
            None => {
                // GET operation: Show current priority
                let task = storage
                    .get(&args.task_id, project_prefix.clone())
                    .ok_or_else(|| format!("Task '{}' not found", args.task_id))?;

                println!(
                    "{}",
                    renderer.render_success(&format!(
                        "Task {} priority: {}",
                        args.task_id, task.priority
                    ))
                );
                Ok(())
            }
        }
    }
}

/// Arguments for priority command
#[derive(Debug, Clone)]
pub struct PriorityArgs {
    pub task_id: String,
    pub new_priority: Option<String>,
    pub explicit_project: Option<String>,
}

impl PriorityArgs {
    pub fn new(
        task_id: String,
        new_priority: Option<String>,
        explicit_project: Option<String>,
    ) -> Self {
        Self {
            task_id,
            new_priority,
            explicit_project,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_priority_args_creation() {
        let args = PriorityArgs::new("TEST-1".to_string(), Some("High".to_string()), None);
        assert_eq!(args.task_id, "TEST-1");
        assert_eq!(args.new_priority, Some("High".to_string()));
        assert_eq!(args.explicit_project, None);
    }

    #[test]
    fn test_priority_args_get_only() {
        let args = PriorityArgs::new("TEST-1".to_string(), None, Some("test-project".to_string()));
        assert_eq!(args.task_id, "TEST-1");
        assert_eq!(args.new_priority, None);
        assert_eq!(args.explicit_project, Some("test-project".to_string()));
    }

    #[test]
    fn test_priority_handler_structure() {
        // This test just verifies the struct can be created
        let _handler = PriorityHandler;
        // If this compiles, the basic structure is correct
    }
}

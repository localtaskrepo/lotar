use crate::cli::handlers::CommandHandler;
use crate::cli::project::ProjectResolver;
use crate::cli::validation::CliValidator;
use crate::output::OutputRenderer;
use crate::storage::manager::Storage;
use crate::workspace::TasksDirectoryResolver;

/// Handler for status change commands
pub struct StatusHandler;

impl CommandHandler for StatusHandler {
    type Args = StatusArgs;
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

        let resolved_project = project_resolver
            .resolve_project(&args.task_id, final_effective_project)
            .map_err(|e| format!("Could not resolve project: {}", e))?;

        // Get full task ID with project prefix
        let full_task_id = project_resolver
            .get_full_task_id(&args.task_id, final_effective_project)
            .map_err(|e| format!("Could not determine full task ID: {}", e))?;

        // Now that we have resolved the project, get the appropriate config
        let config = project_resolver.get_config();
        let validator = CliValidator::new(config);

        // Load the task
        // Try to open existing storage without creating directories
        let mut storage = match Storage::try_open(resolver.path.clone()) {
            Some(storage) => storage,
            None => {
                return Err("No tasks found. Use 'lotar add' to create tasks first.".to_string());
            }
        };
        let task_result = storage.get(&full_task_id, resolved_project.clone());
        let mut task = task_result.ok_or_else(|| format!("Task '{}' not found", full_task_id))?;

        match args.new_status {
            // Get current status
            None => {
                println!(
                    "{}",
                    renderer
                        .render_success(&format!("Task {} status: {}", full_task_id, task.status))
                );
                Ok(())
            }
            // Set new status
            Some(new_status) => {
                // Validate the new status against project configuration
                let validated_status = validator
                    .validate_status(&new_status)
                    .map_err(|e| format!("Status validation failed: {}", e))?;

                let old_status = task.status.clone();

                // Check if status is actually changing
                if old_status == validated_status {
                    println!(
                        "{}",
                        renderer.render_warning(&format!(
                            "Task {} already has status '{}'",
                            full_task_id, validated_status
                        ))
                    );
                    return Ok(());
                }

                task.status = validated_status.clone();

                // Save the updated task
                storage.edit(&full_task_id, &task);

                println!(
                    "{}",
                    renderer.render_success(&format!(
                        "Task {} status changed from {} to {}",
                        full_task_id, old_status, validated_status
                    ))
                );

                Ok(())
            }
        }
    }
}

/// Arguments for status command (get or set)
pub struct StatusArgs {
    pub task_id: String,
    pub new_status: Option<String>, // None = get status, Some = set status
    pub explicit_project: Option<String>,
}

impl StatusArgs {
    pub fn new(
        task_id: String,
        new_status: Option<String>,
        explicit_project: Option<String>,
    ) -> Self {
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
            Some("InProgress".to_string()),
            Some("auth".to_string()),
        );

        assert_eq!(args.task_id, "AUTH-123");
        assert_eq!(args.new_status, Some("InProgress".to_string()));
        assert_eq!(args.explicit_project, Some("auth".to_string()));
    }

    #[test]
    fn test_status_args_get_only() {
        let args = StatusArgs::new("AUTH-123".to_string(), None, Some("auth".to_string()));

        assert_eq!(args.task_id, "AUTH-123");
        assert_eq!(args.new_status, None);
        assert_eq!(args.explicit_project, Some("auth".to_string()));
    }

    #[test]
    fn test_status_handler_structure() {
        let args = StatusArgs::new("123".to_string(), Some("Done".to_string()), None);

        let resolver = create_test_resolver();
        let renderer = OutputRenderer::new(crate::output::OutputFormat::Text, false);

        // This would fail in a real test because we need actual config files and tasks
        // But it demonstrates the structure
        match StatusHandler::execute(args, None, &resolver, &renderer) {
            Ok(()) => println!("Success: Status changed"),
            Err(e) => println!("Expected error in test: {}", e),
        }
    }
}

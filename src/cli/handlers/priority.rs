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

        // Resolve project strictly: always honor explicit project if provided and validate
        // against any prefix in the task_id. This will error on mismatches.
        let final_effective_project = project.or(args.explicit_project.as_deref());

        // Load project configuration for validation
        let resolved_project = project_resolver
            .resolve_project("", final_effective_project)
            .map_err(|e| format!("Could not resolve project: {}", e))?;
        // Determine full task id (handles numeric IDs by prefixing with project)
        let full_task_id = project_resolver
            .get_full_task_id(&args.task_id, final_effective_project)
            .map_err(|e| format!("Could not determine full task ID: {}", e))?;
        let config = project_resolver.get_config();
        let project_config = if !resolved_project.is_empty() {
            project_resolver
                .get_project_config(&resolved_project)
                .map_err(|e| format!("Failed to get project configuration: {}", e))?
        } else {
            config.clone()
        };
        let project_validator = CliValidator::new(&project_config);

        // Try to open existing storage
        let mut storage = match Storage::try_open(resolver.path.clone()) {
            Some(storage) => storage,
            None => return Err("No tasks found. Use 'lotar add' to create tasks first.".into()),
        };
        let project_prefix = if let Some(project) = final_effective_project {
            crate::utils::resolve_project_input(project, resolver.path.as_path())
        } else {
            crate::project::get_effective_project_name(resolver)
        };

        match args.new_priority {
            Some(new_priority) => {
                // SET operation
                let validated_priority = project_validator
                    .validate_priority(&new_priority)
                    .map_err(|e| format!("Priority validation failed: {}", e))?;

                let mut task = storage
                    .get(&full_task_id, project_prefix.clone())
                    .ok_or_else(|| format!("Task '{}' not found", full_task_id))?;

                let old_priority = task.priority;
                if old_priority == validated_priority {
                    match renderer.format {
                        crate::output::OutputFormat::Json => {
                            let obj = serde_json::json!({
                                "status": "success",
                                "message": format!("Task {} priority unchanged", full_task_id),
                                "task_id": full_task_id,
                                "priority": validated_priority.to_string()
                            });
                            renderer.emit_raw_stdout(&obj.to_string());
                        }
                        _ => {
                            renderer.emit_warning(&format!(
                                "Task {} priority is already {}",
                                full_task_id, validated_priority
                            ));
                        }
                    }
                    return Ok(());
                }

                task.priority = validated_priority;
                storage.edit(&full_task_id, &task);

                match renderer.format {
                    crate::output::OutputFormat::Json => {
                        let obj = serde_json::json!({
                            "status": "success",
                            "message": format!(
                                "Task {} priority changed from {} to {}",
                                full_task_id, old_priority, task.priority
                            ),
                            "task_id": full_task_id,
                            "old_priority": old_priority.to_string(),
                            "new_priority": task.priority.to_string()
                        });
                        renderer.emit_raw_stdout(&obj.to_string());
                    }
                    _ => {
                        renderer.emit_success(&format!(
                            "Task {} priority changed from {} to {}",
                            full_task_id, old_priority, task.priority
                        ));
                    }
                }
                Ok(())
            }
            None => {
                // GET operation
                let task = storage
                    .get(&full_task_id, project_prefix.clone())
                    .ok_or_else(|| format!("Task '{}' not found", full_task_id))?;

                match renderer.format {
                    crate::output::OutputFormat::Json => {
                        let obj = serde_json::json!({
                            "status": "success",
                            "task_id": full_task_id,
                            "priority": task.priority.to_string()
                        });
                        renderer.emit_raw_stdout(&obj.to_string());
                    }
                    _ => {
                        renderer.emit_success(&format!(
                            "Task {} priority: {}",
                            full_task_id, task.priority
                        ));
                    }
                }
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

// inline tests moved to tests/cli_priority_unit_test.rs

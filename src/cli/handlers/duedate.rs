use crate::cli::handlers::CommandHandler;
use crate::cli::project::ProjectResolver;
use crate::cli::validation::CliValidator;
use crate::output::OutputRenderer;
use crate::workspace::TasksDirectoryResolver;

use crate::api_types::TaskUpdate;
use crate::services::task_service::TaskService;
use crate::storage::manager::Storage;

/// Handler for due date get/set
pub struct DueDateHandler;

pub struct DueDateArgs {
    pub task_id: String,
    pub new_due_date: Option<String>, // None = get current
}

impl CommandHandler for DueDateHandler {
    type Args = DueDateArgs;
    type Result = Result<(), String>;

    fn execute(
        args: Self::Args,
        project: Option<&str>,
        resolver: &TasksDirectoryResolver,
        renderer: &OutputRenderer,
    ) -> Self::Result {
        renderer.log_info(&format!(
            "duedate: resolving project for task_id={} explicit_project={:?}",
            args.task_id, project
        ));

        // Create project resolver
        let mut project_resolver = ProjectResolver::new(resolver)
            .map_err(|e| format!("Failed to initialize project resolver: {}", e))?;

        // Validate task ID format and resolve project
        project_resolver
            .validate_task_id_format(&args.task_id)
            .map_err(|e| format!("Invalid task ID: {}", e))?;

        let resolved_project = project_resolver
            .resolve_project(&args.task_id, project)
            .map_err(|e| format!("Could not resolve project: {}", e))?;

        // Determine full task id
        let full_task_id = project_resolver
            .get_full_task_id(&args.task_id, project)
            .map_err(|e| format!("Could not determine full task ID: {}", e))?;

        // Config for validation
        let config = project_resolver.get_config();
        let validator = CliValidator::new(config);

        // Open storage (read-only for get, read-write for set)
        let mut storage = match Storage::try_open(resolver.path.clone()) {
            Some(s) => s,
            None => return Err("No tasks found. Use 'lotar add' to create tasks first.".into()),
        };

        // Load task
        let task = storage
            .get(&full_task_id, resolved_project.clone())
            .ok_or_else(|| format!("Task '{}' not found", full_task_id))?;

        match args.new_due_date {
            // Show current due date
            None => {
                match renderer.format {
                    crate::output::OutputFormat::Json => {
                        renderer.emit_raw_stdout(
                            &serde_json::json!({
                                "status": "success",
                                "task_id": full_task_id,
                                "due_date": task.due_date
                            })
                            .to_string(),
                        );
                    }
                    _ => {
                        let val = task.due_date.as_deref().unwrap_or("-");
                        renderer.emit_success(&format!("Task {} due date: {}", full_task_id, val));
                    }
                }
                Ok(())
            }
            // Set new due date
            Some(new_val) => {
                // Parse/normalize due date via validator
                let normalized = validator
                    .parse_due_date(&new_val)
                    .map_err(|e| format!("Due date validation failed: {}", e))?;

                let old = task.due_date.clone();
                // Persist via service
                let patch = TaskUpdate {
                    title: None,
                    status: None,
                    priority: None,
                    task_type: None,
                    reporter: None,
                    assignee: None,
                    due_date: Some(normalized),
                    effort: None,
                    description: None,
                    category: None,
                    tags: None,
                    relationships: None,
                    custom_fields: None,
                };
                let updated = TaskService::update(&mut storage, &full_task_id, patch)
                    .map_err(|e| e.to_string())?;

                match renderer.format {
                    crate::output::OutputFormat::Json => {
                        renderer.emit_raw_stdout(
                            &serde_json::json!({
                                "status": "success",
                                "message": format!("Task {} due date updated", full_task_id),
                                "task_id": full_task_id,
                                "old_due_date": old,
                                "new_due_date": updated.due_date
                            })
                            .to_string(),
                        );
                    }
                    _ => {
                        let old_disp = old.as_deref().unwrap_or("-");
                        let new_disp = updated.due_date.as_deref().unwrap_or("-");
                        renderer.emit_success(&format!(
                            "Task {} due date changed: {} -> {}",
                            full_task_id, old_disp, new_disp
                        ));
                    }
                }
                Ok(())
            }
        }
    }
}

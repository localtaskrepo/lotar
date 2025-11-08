use crate::cli::handlers::CommandHandler;
use crate::cli::project::ProjectResolver;
use crate::cli::validation::CliValidator;
use crate::output::OutputRenderer;
use crate::workspace::TasksDirectoryResolver;

use crate::api_types::TaskUpdate;
use crate::services::task_service::TaskService;
use crate::storage::manager::Storage;

/// Handler for assignee get/set
pub struct AssigneeHandler;

pub struct AssigneeArgs {
    pub task_id: String,
    pub new_assignee: Option<String>, // None = get current
}

impl CommandHandler for AssigneeHandler {
    type Args = AssigneeArgs;
    type Result = Result<(), String>;

    fn execute(
        args: Self::Args,
        project: Option<&str>,
        resolver: &TasksDirectoryResolver,
        renderer: &OutputRenderer,
    ) -> Self::Result {
        renderer.log_info(&format!(
            "assignee: resolving project for task_id={} explicit_project={:?}",
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
        let autop_members_enabled = config.auto_populate_members;

        // Open storage (read-only for get, read-write for set)
        let mut storage = match Storage::try_open(resolver.path.clone()) {
            Some(s) => s,
            None => return Err("No tasks found. Use 'lotar add' to create tasks first.".into()),
        };

        // Load task
        let task = storage
            .get(&full_task_id, resolved_project.clone())
            .ok_or_else(|| format!("Task '{}' not found", full_task_id))?;

        match args.new_assignee {
            // Show current assignee
            None => {
                match renderer.format {
                    crate::output::OutputFormat::Json => {
                        renderer.emit_raw_stdout(
                            &serde_json::json!({
                                "status": "success",
                                "task_id": full_task_id,
                                "assignee": task.assignee
                            })
                            .to_string(),
                        );
                    }
                    _ => {
                        let val = task.assignee.as_deref().unwrap_or("-");
                        renderer.emit_success(&format!("Task {} assignee: {}", full_task_id, val));
                    }
                }
                Ok(())
            }
            // Set new assignee
            Some(new_val) => {
                // Validate format (emails, @username, @me)
                let validation = if autop_members_enabled {
                    validator.validate_assignee_allow_unknown(&new_val)
                } else {
                    validator.validate_assignee(&new_val)
                };
                let validated =
                    validation.map_err(|e| format!("Assignee validation failed: {}", e))?;

                let old = task.assignee.clone();
                // Persist via service so @me is resolved consistently
                let patch = TaskUpdate {
                    title: None,
                    status: None,
                    priority: None,
                    task_type: None,
                    reporter: None,
                    assignee: Some(validated),
                    due_date: None,
                    effort: None,
                    description: None,
                    tags: None,
                    relationships: None,
                    custom_fields: None,
                    sprints: None,
                };
                let updated = TaskService::update(&mut storage, &full_task_id, patch)
                    .map_err(|e| e.to_string())?;

                match renderer.format {
                    crate::output::OutputFormat::Json => {
                        renderer.emit_raw_stdout(
                            &serde_json::json!({
                                "status": "success",
                                "message": format!("Task {} assignee updated", full_task_id),
                                "task_id": full_task_id,
                                "old_assignee": old,
                                "new_assignee": updated.assignee
                            })
                            .to_string(),
                        );
                    }
                    _ => {
                        let old_disp = old.as_deref().unwrap_or("-");
                        let new_disp = updated.assignee.as_deref().unwrap_or("-");
                        renderer.emit_success(&format!(
                            "Task {} assignee changed: {} -> {}",
                            full_task_id, old_disp, new_disp
                        ));
                    }
                }
                Ok(())
            }
        }
    }
}

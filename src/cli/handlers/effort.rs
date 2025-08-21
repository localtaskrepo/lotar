use crate::cli::handlers::CommandHandler;
use crate::cli::project::ProjectResolver;
use crate::output::OutputRenderer;
use crate::workspace::TasksDirectoryResolver;

use crate::api_types::TaskUpdate;
use crate::services::task_service::TaskService;
use crate::storage::manager::Storage;

/// Handler for effort get/set/clear
pub struct EffortHandler;

pub struct EffortArgs {
    pub task_id: String,
    pub new_effort: Option<String>, // None = get current
    pub clear: bool,
    pub dry_run: bool,
    pub explain: bool,
}

impl CommandHandler for EffortHandler {
    type Args = EffortArgs;
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

        // Open storage
        let mut storage = match Storage::try_open(resolver.path.clone()) {
            Some(s) => s,
            None => return Err("No tasks found. Use 'lotar add' to create tasks first.".into()),
        };

        // Load task
        let task = storage
            .get(&full_task_id, resolved_project.clone())
            .ok_or_else(|| format!("Task '{}' not found", full_task_id))?;

        // GET path (no new value, no clear)
        if args.new_effort.is_none() && !args.clear {
            match renderer.format {
                crate::output::OutputFormat::Json => {
                    let mut obj = serde_json::json!({
                        "status": "success",
                        "task_id": full_task_id,
                        "effort": task.effort
                    });
                    if args.explain {
                        obj["explain"] = serde_json::Value::String(
                            "Displays the current effort value if set; values are normalized on write (e.g., days→hours).".to_string(),
                        );
                    }
                    renderer.emit_raw_stdout(&obj.to_string());
                }
                _ => {
                    let val = task.effort.as_deref().unwrap_or("-");
                    if args.explain {
                        renderer.emit_info("Effort values are normalized on write using time units (m/h/d/w) or points.");
                    }
                    renderer.emit_success(&format!("Task {} effort: {}", full_task_id, val));
                }
            }
            return Ok(());
        }

        // CLEAR path
        if args.clear {
            let old = task.effort.clone();
            if args.dry_run {
                match renderer.format {
                    crate::output::OutputFormat::Json => {
                        let mut obj = serde_json::json!({
                            "status": "success",
                            "message": format!("Would clear task {} effort", full_task_id),
                            "task_id": full_task_id,
                            "old_effort": old,
                            "new_effort": serde_json::Value::Null
                        });
                        if args.explain {
                            obj["explain"] = serde_json::Value::String(
                                "No write performed due to --dry-run".to_string(),
                            );
                        }
                        renderer.emit_raw_stdout(&obj.to_string());
                    }
                    _ => {
                        if args.explain {
                            renderer.emit_info("No write performed due to --dry-run");
                        }
                        let old_disp = old.as_deref().unwrap_or("-");
                        renderer.emit_success(&format!(
                            "Would clear effort for task {} (old: {})",
                            full_task_id, old_disp
                        ));
                    }
                }
                return Ok(());
            }
            // Perform clear by directly editing the task
            let mut t = task.clone();
            t.effort = None;
            storage.edit(&full_task_id, &t);
            match renderer.format {
                crate::output::OutputFormat::Json => {
                    renderer.emit_raw_stdout(
                        &serde_json::json!({
                            "status": "success",
                            "message": format!("Task {} effort cleared", full_task_id),
                            "task_id": full_task_id,
                            "old_effort": old,
                            "new_effort": serde_json::Value::Null
                        })
                        .to_string(),
                    );
                }
                _ => {
                    let old_disp = old.as_deref().unwrap_or("-");
                    renderer.emit_success(&format!(
                        "Task {} effort cleared (was: {})",
                        full_task_id, old_disp
                    ));
                }
            }
            return Ok(());
        }

        // SET path
        if let Some(new_val) = args.new_effort {
            // Normalize effort using util
            let normalized = match crate::utils::effort::parse_effort(&new_val) {
                Ok(parsed) => parsed.canonical,
                Err(e) => return Err(format!("Effort validation failed: {}", e)),
            };

            if args.dry_run {
                match renderer.format {
                    crate::output::OutputFormat::Json => {
                        let mut obj = serde_json::json!({
                            "status": "success",
                            "message": format!("Would update task {} effort", full_task_id),
                            "task_id": full_task_id,
                            "old_effort": task.effort,
                            "new_effort": normalized,
                        });
                        if args.explain {
                            obj["explain"] = serde_json::Value::String(
                                "Effort normalized to canonical units (hours for time; 'pt' for points). No write performed.".to_string(),
                            );
                        }
                        renderer.emit_raw_stdout(&obj.to_string());
                    }
                    _ => {
                        if args.explain {
                            renderer.emit_info(
                                "Effort will be normalized on write (e.g., 1d -> 8.00h)",
                            );
                        }
                        let old_disp = task.effort.as_deref().unwrap_or("-");
                        renderer.emit_success(&format!(
                            "Would change task {} effort: {} -> {}",
                            full_task_id, old_disp, normalized
                        ));
                    }
                }
                return Ok(());
            }

            // Persist via service for consistent normalization/logging
            let patch = TaskUpdate {
                title: None,
                status: None,
                priority: None,
                task_type: None,
                reporter: None,
                assignee: None,
                due_date: None,
                effort: Some(normalized.clone()),
                description: None,
                category: None,
                tags: None,
                custom_fields: None,
            };
            let updated = TaskService::update(&mut storage, &full_task_id, patch)
                .map_err(|e| e.to_string())?;

            match renderer.format {
                crate::output::OutputFormat::Json => {
                    renderer.emit_raw_stdout(
                        &serde_json::json!({
                            "status": "success",
                            "message": format!("Task {} effort updated", full_task_id),
                            "task_id": full_task_id,
                            "old_effort": task.effort,
                            "new_effort": updated.effort
                        })
                        .to_string(),
                    );
                }
                _ => {
                    let old_disp = task.effort.as_deref().unwrap_or("-");
                    let new_disp = updated.effort.as_deref().unwrap_or("-");
                    renderer.emit_success(&format!(
                        "Task {} effort changed: {} -> {}",
                        full_task_id, old_disp, new_disp
                    ));
                }
            }
            return Ok(());
        }

        // Should be unreachable due to earlier branches
        Err("Invalid effort command state".into())
    }
}

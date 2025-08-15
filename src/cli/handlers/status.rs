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
        renderer.log_info(&format!(
            "status: resolving project for task_id={} explicit_project={:?}",
            args.task_id, project
        ));
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
                    renderer.emit_warning(&format!(
                        "Warning: Task ID '{}' belongs to project '{}', but project '{}' was specified. Using task ID's project.",
                        args.task_id, task_id_prefix, explicit_proj
                    ));
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
        renderer.log_debug(&format!(
            "status: loading task full_id={} project={}",
            full_task_id, resolved_project
        ));
        let task_result = storage.get(&full_task_id, resolved_project.clone());
        let mut task = task_result.ok_or_else(|| format!("Task '{}' not found", full_task_id))?;

        match args.new_status {
            // Get current status
            None => {
                match renderer.format {
                    crate::output::OutputFormat::Json => {
                        renderer.emit_raw_stdout(
                            &serde_json::json!({
                                "status": "success",
                                "task_id": full_task_id,
                                "status_value": task.status.to_string()
                            })
                            .to_string(),
                        );
                    }
                    _ => {
                        renderer.emit_success(&format!(
                            "Task {} status: {}",
                            full_task_id, task.status
                        ));
                    }
                }
                Ok(())
            }
            // Set new status
            Some(new_status) => {
                // Validate the new status against project configuration
                renderer.log_debug(&format!(
                    "status: validating new_status candidate='{}'",
                    new_status
                ));
                let validated_status = validator
                    .validate_status(&new_status)
                    .map_err(|e| format!("Status validation failed: {}", e))?;

                let old_status = task.status.clone();

                // Check if status is actually changing
                if old_status == validated_status {
                    renderer.log_info("status: no-op (old == new)");
                    #[cfg(not(test))]
                    {
                        renderer.emit_warning(&format!(
                            "Task {} already has status '{}'",
                            full_task_id, validated_status
                        ));
                        // Emit a small stdout notice so --format=table/json/markdown flows have output
                        // Notice prints in JSON mode too (info is suppressed there)
                        renderer.emit_notice(&format!("Task {} status unchanged", full_task_id));
                    }
                    return Ok(());
                }

                // Prepare preview message if dry-run
                if args.dry_run {
                    // First-change semantics: only if moving away from project default
                    let cfg = project_resolver.get_config();
                    let project_default_status = cfg
                        .default_status
                        .clone()
                        .unwrap_or_else(|| cfg.issue_states.values[0].clone());
                    let would_assign = task.assignee.is_none()
                        && project_resolver.get_config().auto_assign_on_status
                        && task.status == project_default_status
                        && task.status != validated_status;
                    let resolved_assignee = if would_assign {
                        crate::utils::identity::resolve_current_user(Some(resolver.path.as_path()))
                    } else {
                        None
                    };

                    match renderer.format {
                        crate::output::OutputFormat::Json => {
                            let mut obj = serde_json::json!({
                                "status": "preview",
                                "action": "status_change",
                                "task_id": full_task_id,
                                "old_status": old_status,
                                "new_status": validated_status,
                            });
                            if let Some(me) = resolved_assignee.clone() {
                                obj["would_set_assignee"] = serde_json::Value::String(me);
                            }
                            if args.explain {
                                obj["explain"] = serde_json::Value::String("status validated against project config; auto-assign uses default_reporter→git user.name/email→system username.".to_string());
                            }
                            renderer.emit_raw_stdout(&obj.to_string());
                        }
                        _ => {
                            let mut preview = format!(
                                "DRY RUN: Would change {} status from {} to {}",
                                full_task_id, old_status, validated_status
                            );
                            if let Some(me) = resolved_assignee {
                                preview.push_str(&format!("; would set assignee = {}", me));
                            }
                            renderer.emit_info(&preview);
                            if args.explain {
                                renderer.emit_info("Explanation: status validated against project config; auto-assign uses default_reporter→git user.name/email→system username.");
                            }
                        }
                    }
                    return Ok(());
                }

                task.status = validated_status.clone();
                // Auto-assign assignee if none is set (configurable)
                if task.assignee.is_none() && project_resolver.get_config().auto_assign_on_status {
                    let cfg = project_resolver.get_config();
                    let project_default_status = cfg
                        .default_status
                        .clone()
                        .unwrap_or_else(|| cfg.issue_states.values[0].clone());
                    if old_status == project_default_status && old_status != validated_status {
                        // Try CODEOWNERS first (if enabled), then fall back to identity
                        let mut assigned = false;
                        if cfg.auto_codeowners_assign {
                            let owner_from_codeowners = (|| {
                                let repo_root =
                                    crate::utils::codeowners::repo_root_from_tasks_root(
                                        &resolver.path,
                                    )?;
                                let codeowners =
                                    crate::utils::codeowners::CodeOwners::load_from_repo(
                                        &repo_root,
                                    )?;
                                codeowners.default_owner()
                            })();

                            if let Some(owner) = owner_from_codeowners {
                                task.assignee = Some(owner);
                                assigned = true;
                            }
                        }

                        if !assigned {
                            if let Some(me) = crate::utils::identity::resolve_current_user(Some(
                                resolver.path.as_path(),
                            )) {
                                task.assignee = Some(me);
                            }
                        }
                    }
                }

                // Save the updated task
                renderer.log_debug("status: persisting change to storage");
                storage.edit(&full_task_id, &task);

                match renderer.format {
                    crate::output::OutputFormat::Json => {
                        let mut obj = serde_json::json!({
                            "status": "success",
                            "message": format!("Task {} status changed from {} to {}", full_task_id, old_status, validated_status),
                            "task_id": full_task_id,
                            "old_status": old_status,
                            "new_status": validated_status,
                        });
                        if let Some(assignee) = &task.assignee {
                            obj["assignee"] = serde_json::Value::String(assignee.clone());
                        }
                        renderer.emit_raw_stdout(&obj.to_string());
                    }
                    _ => {
                        renderer.emit_success(&format!(
                            "Task {} status changed from {} to {}",
                            full_task_id, old_status, validated_status
                        ));
                    }
                }
                renderer.log_info("status: updated successfully");

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
    pub dry_run: bool,
    pub explain: bool,
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
            dry_run: false,
            explain: false,
        }
    }
}

// inline tests moved to tests/cli_status_unit_test.rs

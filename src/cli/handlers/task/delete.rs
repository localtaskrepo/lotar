use crate::cli::TaskDeleteArgs;
use crate::cli::handlers::CommandHandler;
use crate::cli::handlers::task::context::TaskCommandContext;
use crate::cli::handlers::task::errors::TaskStorageAction;
use crate::cli::handlers::task::mutation::{LoadedDeletedAwareTask, load_task_including_deleted};
use crate::services::task_service::TaskService;
use crate::workspace::TasksDirectoryResolver;

/// Handler for deleting tasks.
///
/// Default is a soft delete (`deleted_at` marker, hidden from active
/// views, restorable via `task restore`). `--hard` physically removes the
/// file and reports retained attachment references and incoming
/// relationships as warnings. `--force`/`--yes` skips the confirmation
/// prompt only — it never implies `--hard`.
pub struct DeleteHandler;

impl CommandHandler for DeleteHandler {
    type Args = TaskDeleteArgs;
    type Result = Result<(), String>;

    fn execute(
        args: Self::Args,
        project: Option<&str>,
        resolver: &TasksDirectoryResolver,
        renderer: &crate::output::OutputRenderer,
    ) -> Self::Result {
        renderer.log_info("delete: begin");
        let TaskDeleteArgs {
            id,
            force,
            hard,
            dry_run,
        } = args;
        let mut ctx = TaskCommandContext::new(resolver, project, Some(id.as_str()))?;
        let LoadedDeletedAwareTask {
            full_id,
            project_prefix,
            task: loaded_task,
        } = load_task_including_deleted(&mut ctx, &id, project)?;

        // Confirm deletion if not forced (skip prompt in dry-run)
        if !force && !dry_run {
            let action = if hard {
                "permanently delete (hard)"
            } else {
                "soft delete"
            };
            print!("Are you sure you want to {action} task '{}'? (y/N): ", id);
            use std::io::{self, Write};
            let _ = io::stdout().flush();

            let mut input = String::new();
            if io::stdin().read_line(&mut input).is_err() {
                renderer.emit_error("Failed to read input. Aborting.");
                return Ok(());
            }
            let decision = input.trim().to_lowercase();

            if decision != "y" && decision != "yes" {
                renderer.emit_warning("Deletion cancelled.");
                return Ok(());
            }
        }

        if dry_run {
            let preview = TaskService::preview_delete(
                &ctx.storage,
                &full_id,
                Some(project_prefix.as_str()),
                hard,
            )
            .map_err(|e| format!("Failed to preview deletion of task '{}': {}", id, e))?;
            let warnings = preview.warnings;
            match renderer.format {
                crate::output::OutputFormat::Json => {
                    let obj = serde_json::json!({
                        "status": "preview",
                        "action": "delete",
                        "mode": if hard { "hard" } else { "soft" },
                        "task_id": id,
                        "project": project_prefix,
                        "warnings": warnings,
                    });
                    renderer.emit_json(&obj);
                }
                _ => {
                    renderer.emit_info(format_args!(
                        "DRY RUN: Would delete task '{}' from project {} ({})",
                        id,
                        project_prefix,
                        if hard {
                            "hard delete: physically removes the task file"
                        } else {
                            "soft delete: recoverable via 'lotar task restore'"
                        }
                    ));
                    for warning in &warnings {
                        renderer.emit_warning(format_args!("{}", warning));
                    }
                }
            }
            return Ok(());
        }

        // Delete the task (soft by default, physical with --hard)
        let outcome = TaskService::delete_with_options(
            &mut ctx.storage,
            &full_id,
            Some(project_prefix.as_str()),
            hard,
        )
        .map_err(TaskStorageAction::Delete.map_err(&full_id))?;
        if outcome.deleted {
            match renderer.format {
                crate::output::OutputFormat::Json => {
                    let obj = serde_json::json!({
                        "status": "success",
                        "message": if outcome.hard {
                            format!("Task '{}' permanently deleted", id)
                        } else {
                            format!("Task '{}' deleted (soft; restore with 'lotar task restore {}')", id, id)
                        },
                        "task_id": id,
                        "hard": outcome.hard,
                        "warnings": outcome.warnings,
                    });
                    renderer.emit_json(&obj);
                }
                _ => {
                    if outcome.hard {
                        renderer.emit_success(format_args!("Task '{}' permanently deleted", id));
                    } else {
                        renderer.emit_success(format_args!(
                            "Task '{}' deleted (soft; restore with 'lotar task restore {}')",
                            id, id
                        ));
                    }
                    for warning in &outcome.warnings {
                        renderer.emit_warning(format_args!("{}", warning));
                    }
                }
            }
            Ok(())
        } else if !hard && loaded_task.deleted_at.is_some() {
            // Already soft-deleted: a successful no-op. The service keeps
            // the tombstone untouched (no rewrite, no extra history entry)
            // and a repeated delete never escalates to a hard deletion.
            match renderer.format {
                crate::output::OutputFormat::Json => {
                    let obj = serde_json::json!({
                        "status": "success",
                        "message": format!(
                            "Task '{}' already deleted (soft); no changes",
                            id
                        ),
                        "task_id": id,
                        "hard": false,
                        "warnings": outcome.warnings,
                    });
                    renderer.emit_json(&obj);
                }
                _ => {
                    renderer.emit_success(format_args!(
                        "Task '{}' already deleted (soft); no changes",
                        id
                    ));
                }
            }
            Ok(())
        } else {
            Err(format!("Failed to delete task '{}'", id))
        }
    }
}

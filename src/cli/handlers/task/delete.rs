use crate::cli::TaskDeleteArgs;
use crate::cli::handlers::CommandHandler;
use crate::cli::handlers::task::context::TaskCommandContext;
use crate::cli::handlers::task::errors::TaskStorageAction;
use crate::cli::handlers::task::mutation::{LoadedTask, load_task};
use crate::workspace::TasksDirectoryResolver;

/// Handler for deleting tasks
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
        let TaskDeleteArgs { id, force, dry_run } = args;
        let mut ctx = TaskCommandContext::new(resolver, project, Some(id.as_str()))?;
        let LoadedTask {
            full_id,
            project_prefix,
            ..
        } = load_task(&mut ctx, &id, project)?;

        // Confirm deletion if not forced (skip prompt in dry-run)
        if !force && !dry_run {
            print!("Are you sure you want to delete task '{}'? (y/N): ", id);
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
            match renderer.format {
                crate::output::OutputFormat::Json => {
                    let obj = serde_json::json!({
                        "status": "preview",
                        "action": "delete",
                        "task_id": id,
                        "project": project_prefix,
                    });
                    renderer.emit_json(&obj);
                }
                _ => {
                    renderer.emit_info(&format!(
                        "DRY RUN: Would delete task '{}' from project {}",
                        id, project_prefix
                    ));
                }
            }
            return Ok(());
        }

        // Delete the task
        let deleted = ctx
            .storage
            .delete(&full_id, project_prefix.clone())
            .map_err(TaskStorageAction::Delete.map_err(&full_id))?;
        if deleted {
            match renderer.format {
                crate::output::OutputFormat::Json => {
                    let obj = serde_json::json!({
                        "status": "success",
                        "message": format!("Task '{}' deleted", id),
                        "task_id": id
                    });
                    renderer.emit_json(&obj);
                }
                _ => {
                    renderer.emit_success(&format!("Task '{}' deleted successfully", id));
                }
            }
            Ok(())
        } else {
            Err(format!("Failed to delete task '{}'", id))
        }
    }
}

use crate::cli::handlers::CommandHandler;
use crate::cli::project::ProjectResolver;
use crate::output::OutputRenderer;
use crate::storage::manager::Storage;
use crate::workspace::TasksDirectoryResolver;

/// Handler for adding a comment to a task
pub struct CommentHandler;

pub struct CommentArgs {
    pub task_id: String,
    pub text: String,
}

impl CommandHandler for CommentHandler {
    type Args = CommentArgs;
    type Result = Result<(), String>;

    fn execute(
        args: Self::Args,
        project: Option<&str>,
        resolver: &TasksDirectoryResolver,
        renderer: &OutputRenderer,
    ) -> Self::Result {
        renderer.log_info(&format!(
            "comment: resolving project for task_id={} explicit_project={:?}",
            args.task_id, project
        ));

        // Project resolution and validation
        let mut project_resolver = ProjectResolver::new(resolver)
            .map_err(|e| format!("Failed to initialize project resolver: {}", e))?;

        project_resolver
            .validate_task_id_format(&args.task_id)
            .map_err(|e| format!("Invalid task ID: {}", e))?;

        let resolved_project = project_resolver
            .resolve_project(&args.task_id, project)
            .map_err(|e| format!("Could not resolve project: {}", e))?;

        let full_task_id = project_resolver
            .get_full_task_id(&args.task_id, project)
            .map_err(|e| format!("Could not determine full task ID: {}", e))?;

        // Open storage
        let mut storage = match Storage::try_open(resolver.path.clone()) {
            Some(s) => s,
            None => return Err("No tasks found. Use 'lotar add' to create tasks first.".into()),
        };

        // Load task
        let mut task = storage
            .get(&full_task_id, resolved_project.clone())
            .ok_or_else(|| format!("Task '{}' not found", full_task_id))?;

        // Build comment
        let author = crate::utils::identity::resolve_current_user(Some(resolver.path.as_path()))
            .unwrap_or_else(|| "unknown".to_string());
        let when = chrono::Utc::now().to_rfc3339();
        let comment = crate::types::TaskComment {
            author,
            date: when,
            text: args.text,
        };

        // Append and persist
        task.comments.push(comment);
        task.modified = chrono::Utc::now().to_rfc3339();
        storage.edit(&full_task_id, &task);

        match renderer.format {
            crate::output::OutputFormat::Json => {
                renderer.emit_raw_stdout(
                    &serde_json::json!({
                        "status": "success",
                        "action": "task.comment",
                        "task_id": full_task_id,
                        "comments": task.comments.len()
                    })
                    .to_string(),
                );
            }
            _ => {
                renderer.emit_success(&format!(
                    "Comment added to {} ({} total)",
                    full_task_id,
                    task.comments.len()
                ));
            }
        }

        Ok(())
    }
}

use crate::cli::handlers::CommandHandler;
use crate::cli::project::ProjectResolver;
use crate::output::OutputRenderer;
use crate::storage::manager::Storage;
use crate::workspace::TasksDirectoryResolver;

/// Handler for adding a comment to a task
pub struct CommentHandler;

pub struct CommentArgs {
    pub task_id: String,
    pub text: Option<String>,
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

        if let Some(text) = args.text.filter(|t| !t.trim().is_empty()) {
            // Build comment and append
            let when = chrono::Utc::now().to_rfc3339();
            let comment = crate::types::TaskComment { date: when, text };
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
                            "comments": task.comments.len(),
                            "added_comment": {
                                "date": task
                                    .comments
                                    .last()
                                    .map(|c| c.date.clone())
                                    .unwrap_or_default(),
                                "text": task
                                    .comments
                                    .last()
                                    .map(|c| c.text.clone())
                                    .unwrap_or_default()
                            }
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
        } else {
            // List comments (no mutation)
            match renderer.format {
                crate::output::OutputFormat::Json => {
                    let items: Vec<_> = task
                        .comments
                        .iter()
                        .map(|c| serde_json::json!({"date": c.date, "text": c.text}))
                        .collect();
                    renderer.emit_raw_stdout(
                        &serde_json::json!({
                            "status": "ok",
                            "action": "task.comment.list",
                            "task_id": full_task_id,
                            "comments": items.len(),
                            "items": items
                        })
                        .to_string(),
                    );
                }
                _ => {
                    if task.comments.is_empty() {
                        renderer.emit_success(&format!("No comments for {}.", full_task_id));
                    } else {
                        for c in &task.comments {
                            renderer.emit_raw_stdout(&format!("{}  {}", c.date, c.text));
                        }
                    }
                }
            }
            Ok(())
        }
    }
}

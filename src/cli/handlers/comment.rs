use crate::cli::handlers::CommandHandler;
use crate::cli::handlers::task::context::TaskCommandContext;
use crate::cli::handlers::task::mutation::{LoadedTask, load_task};
use crate::output::{OutputFormat, OutputRenderer};
use crate::types::TaskComment;
use crate::workspace::TasksDirectoryResolver;

const COMMENT_PREVIEW_EXPLANATION: &str =
    "comments append to the task log with the current UTC timestamp; dry-run skips persistence";
const COMMENT_LIST_EXPLANATION: &str = "Displays stored comments in chronological order; use --dry-run with text to preview an addition.";

/// Handler for adding or listing task comments
pub struct CommentHandler;

pub struct CommentArgs {
    pub task_id: String,
    pub text: Option<String>,
    pub explicit_project: Option<String>,
    pub dry_run: bool,
    pub explain: bool,
}

impl CommentArgs {
    pub fn new(
        task_id: String,
        text: Option<String>,
        explicit_project: Option<String>,
        dry_run: bool,
        explain: bool,
    ) -> Self {
        Self {
            task_id,
            text,
            explicit_project,
            dry_run,
            explain,
        }
    }
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
        let CommentArgs {
            task_id,
            text,
            explicit_project,
            dry_run,
            explain,
        } = args;

        let project_hint = explicit_project.as_deref().or(project);
        renderer.log_info(&format!(
            "comment: begin task_id={} explicit_project={:?}",
            task_id, project_hint
        ));

        let comment_text = text.and_then(|candidate| {
            if candidate.trim().is_empty() {
                None
            } else {
                Some(candidate)
            }
        });

        let mut ctx = if comment_text.is_some() {
            TaskCommandContext::new(resolver, project_hint, Some(task_id.as_str()))?
        } else {
            TaskCommandContext::new_read_only(resolver, project_hint, Some(task_id.as_str()))?
        };

        let LoadedTask {
            full_id,
            project_prefix,
            task,
        } = load_task(&mut ctx, &task_id, project_hint)?;

        renderer.log_debug(&format!(
            "comment: resolved task={} project={}",
            full_id,
            if project_prefix.trim().is_empty() {
                "-"
            } else {
                project_prefix.as_str()
            }
        ));

        let resolved_project = ctx
            .resolved_project_name()
            .map(|value| value.to_string())
            .or_else(|| {
                if project_prefix.trim().is_empty() {
                    None
                } else {
                    Some(project_prefix.clone())
                }
            });

        match comment_text {
            Some(body) => {
                let add_inputs = CommentAddInputs {
                    project: resolved_project.clone(),
                    task,
                    text: body,
                    dry_run,
                    explain,
                };
                handle_add_comment(&mut ctx, renderer, full_id, add_inputs)
            }
            None => {
                render_comment_list(
                    renderer,
                    &full_id,
                    resolved_project.as_deref(),
                    &task,
                    explain,
                );
                Ok(())
            }
        }
    }
}

struct CommentAddInputs {
    project: Option<String>,
    task: crate::storage::task::Task,
    text: String,
    dry_run: bool,
    explain: bool,
}

fn handle_add_comment(
    ctx: &mut TaskCommandContext,
    renderer: &OutputRenderer,
    full_id: String,
    mut inputs: CommentAddInputs,
) -> Result<(), String> {
    renderer.log_debug("comment: preparing new comment entry");
    let timestamp = chrono::Utc::now().to_rfc3339();
    let entry = TaskComment {
        date: timestamp,
        text: inputs.text,
    };

    if inputs.dry_run {
        render_comment_preview(
            renderer,
            &full_id,
            inputs.project.as_deref(),
            &entry,
            inputs.task.comments.len() + 1,
            inputs.explain,
        );
        return Ok(());
    }

    inputs.task.comments.push(entry.clone());
    inputs.task.modified = chrono::Utc::now().to_rfc3339();

    ctx.storage.edit(&full_id, &inputs.task);
    renderer.log_info("comment: comment persisted");

    render_comment_success(
        renderer,
        &full_id,
        inputs.project.as_deref(),
        &entry,
        inputs.task.comments.len(),
    );
    Ok(())
}

fn render_comment_preview(
    renderer: &OutputRenderer,
    task_id: &str,
    project: Option<&str>,
    comment: &TaskComment,
    total_after: usize,
    explain: bool,
) {
    match renderer.format {
        OutputFormat::Json => {
            let mut payload = serde_json::json!({
                "status": "preview",
                "action": "task.comment",
                "task_id": task_id,
                "comments": total_after,
                "added_comment": {
                    "date": comment.date,
                    "text": comment.text,
                }
            });
            if let Some(name) = project {
                if !name.trim().is_empty() {
                    payload["project"] = serde_json::Value::String(name.to_string());
                }
            }
            if explain {
                payload["explain"] =
                    serde_json::Value::String(COMMENT_PREVIEW_EXPLANATION.to_string());
            }
            renderer.emit_raw_stdout(&payload.to_string());
        }
        _ => {
            renderer.emit_info(&format!(
                "DRY RUN: Would add comment to {} ({} total)",
                task_id, total_after
            ));
            if explain {
                renderer.emit_info(COMMENT_PREVIEW_EXPLANATION);
            }
        }
    }
}

fn render_comment_success(
    renderer: &OutputRenderer,
    task_id: &str,
    project: Option<&str>,
    comment: &TaskComment,
    total: usize,
) {
    match renderer.format {
        OutputFormat::Json => {
            let mut payload = serde_json::json!({
                "status": "success",
                "action": "task.comment",
                "task_id": task_id,
                "comments": total,
                "added_comment": {
                    "date": comment.date,
                    "text": comment.text,
                }
            });
            if let Some(name) = project {
                if !name.trim().is_empty() {
                    payload["project"] = serde_json::Value::String(name.to_string());
                }
            }
            renderer.emit_raw_stdout(&payload.to_string());
        }
        _ => {
            renderer.emit_success(&format!("Comment added to {} ({} total)", task_id, total));
        }
    }
}

fn render_comment_list(
    renderer: &OutputRenderer,
    task_id: &str,
    project: Option<&str>,
    task: &crate::storage::task::Task,
    explain: bool,
) {
    match renderer.format {
        OutputFormat::Json => {
            let items: Vec<_> = task
                .comments
                .iter()
                .map(|c| {
                    serde_json::json!({
                        "date": c.date,
                        "text": c.text,
                    })
                })
                .collect();
            let mut payload = serde_json::json!({
                "status": "ok",
                "action": "task.comment.list",
                "task_id": task_id,
                "comments": items.len(),
                "items": items,
            });
            if let Some(name) = project {
                if !name.trim().is_empty() {
                    payload["project"] = serde_json::Value::String(name.to_string());
                }
            }
            if explain {
                payload["explain"] =
                    serde_json::Value::String(COMMENT_LIST_EXPLANATION.to_string());
            }
            renderer.emit_raw_stdout(&payload.to_string());
        }
        _ => {
            if task.comments.is_empty() {
                renderer.emit_success(&format!("No comments for {}.", task_id));
            } else {
                for c in &task.comments {
                    renderer.emit_raw_stdout(&format!("{}  {}", c.date, c.text));
                }
            }
            if explain {
                renderer.emit_info(COMMENT_LIST_EXPLANATION);
            }
        }
    }
}

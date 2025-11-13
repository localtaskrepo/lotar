use crate::cli::handlers::CommandHandler;
use crate::cli::handlers::task::context::TaskCommandContext;
use crate::cli::handlers::task::errors::TaskStorageAction;
use crate::cli::handlers::task::mutation::{LoadedTask, load_task};
use crate::output::{OutputFormat, OutputRenderer};
use crate::types::TaskComment;
use crate::workspace::TasksDirectoryResolver;
use serde_json::{Map, Value};

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

    ctx.storage
        .edit(&full_id, &inputs.task)
        .map_err(TaskStorageAction::Update.map_err(&full_id))?;
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
            let mut payload =
                build_comment_payload("preview", "task.comment", task_id, total_after);
            payload.insert("added_comment".to_string(), comment_to_json(comment));
            insert_project(&mut payload, project);
            insert_explain(&mut payload, explain.then_some(COMMENT_PREVIEW_EXPLANATION));
            renderer.emit_json(&Value::Object(payload));
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
            let mut payload = build_comment_payload("success", "task.comment", task_id, total);
            payload.insert("added_comment".to_string(), comment_to_json(comment));
            insert_project(&mut payload, project);
            renderer.emit_json(&Value::Object(payload));
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
            let items: Vec<_> = task.comments.iter().map(comment_to_json).collect();
            let mut payload =
                build_comment_payload("ok", "task.comment.list", task_id, items.len());
            payload.insert("items".to_string(), Value::Array(items));
            insert_project(&mut payload, project);
            insert_explain(&mut payload, explain.then_some(COMMENT_LIST_EXPLANATION));
            renderer.emit_json(&Value::Object(payload));
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

fn build_comment_payload(
    status: &str,
    action: &str,
    task_id: &str,
    count: usize,
) -> Map<String, Value> {
    let mut payload = Map::with_capacity(4);
    payload.insert("status".to_string(), Value::String(status.to_string()));
    payload.insert("action".to_string(), Value::String(action.to_string()));
    payload.insert("task_id".to_string(), Value::String(task_id.to_string()));
    payload.insert("comments".to_string(), Value::from(count));
    payload
}

fn comment_to_json(comment: &TaskComment) -> Value {
    serde_json::json!({
        "date": comment.date,
        "text": comment.text,
    })
}

fn insert_project(payload: &mut Map<String, Value>, project: Option<&str>) {
    let trimmed = project.and_then(|candidate| {
        let value = candidate.trim();
        if value.is_empty() { None } else { Some(value) }
    });
    insert_optional_string(payload, "project", trimmed);
}

fn insert_explain(payload: &mut Map<String, Value>, explain: Option<&str>) {
    insert_optional_string(payload, "explain", explain);
}

fn insert_optional_string(payload: &mut Map<String, Value>, key: &str, value: Option<&str>) {
    if let Some(content) = value {
        payload.insert(key.to_string(), Value::String(content.to_string()));
    }
}

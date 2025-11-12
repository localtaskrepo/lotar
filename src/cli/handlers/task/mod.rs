use crate::cli::TaskAction;
use crate::cli::handlers::comment::{CommentArgs, CommentHandler};
use crate::cli::handlers::priority::{PriorityArgs, PriorityHandler};
use crate::cli::handlers::relationships::{
    RelationshipsArgs as RelationshipsHandlerArgs, RelationshipsHandler,
};
use crate::cli::handlers::status::{StatusArgs as StatusHandlerArgs, StatusHandler};
use crate::cli::handlers::{AddHandler, CommandHandler, emit_subcommand_overview};
use crate::cli::handlers::{
    assignee::{AssigneeArgs, AssigneeHandler},
    duedate::{DueDateArgs, DueDateHandler},
};
use crate::workspace::TasksDirectoryResolver;

pub(crate) mod context;
mod delete;
mod edit;
mod history;
pub(crate) mod mutation;
pub(crate) mod render;
mod search;

use delete::DeleteHandler;
use edit::EditHandler;
use history::{handle_at, handle_diff, handle_history, handle_history_by_field};
use search::SearchHandler;

/// Handler for all task subcommands
pub struct TaskHandler;

impl CommandHandler for TaskHandler {
    type Args = Option<TaskAction>;
    type Result = Result<(), String>;

    fn execute(
        args: Self::Args,
        project: Option<&str>,
        resolver: &TasksDirectoryResolver,
        renderer: &crate::output::OutputRenderer,
    ) -> Self::Result {
        let Some(action) = args else {
            emit_subcommand_overview(renderer, &["task"]);
            return Ok(());
        };

        match action {
            TaskAction::Effort(effort_args) => {
                let args = crate::cli::handlers::effort::EffortArgs::new(
                    effort_args.id,
                    effort_args.effort,
                    effort_args.clear,
                    project.map(|s| s.to_string()),
                    effort_args.dry_run,
                    effort_args.explain,
                );
                crate::cli::handlers::effort::EffortHandler::execute(
                    args, project, resolver, renderer,
                )
            }
            TaskAction::Add(add_args) => {
                let cli_add_args = crate::cli::AddArgs {
                    title: add_args.title,
                    task_type: add_args.task_type,
                    priority: add_args.priority,
                    reporter: add_args.reporter,
                    assignee: add_args.assignee,
                    effort: add_args.effort,
                    due: add_args.due,
                    description: add_args.description,
                    tags: add_args.tags,
                    fields: add_args.fields,
                    bug: false,
                    epic: false,
                    critical: false,
                    high: false,
                    dry_run: false,
                    explain: false,
                };

                match AddHandler::execute(cli_add_args, project, resolver, renderer) {
                    Ok(task_id) => {
                        // Use the shared output rendering function
                        AddHandler::render_add_success(&task_id, project, resolver, renderer);
                        Ok(())
                    }
                    Err(e) => Err(e),
                }
            }
            TaskAction::List(args) => SearchHandler::execute(args, project, resolver, renderer),
            TaskAction::Edit(edit_args) => {
                EditHandler::execute(edit_args, project, resolver, renderer)
            }
            TaskAction::History { id, limit } => {
                handle_history(id, limit, project, resolver, renderer)
            }
            TaskAction::HistoryByField { field, id, limit } => {
                handle_history_by_field(field, id, limit, project, resolver, renderer)
            }
            TaskAction::Diff { id, commit, fields } => {
                handle_diff(id, commit, fields, project, resolver, renderer)
            }
            TaskAction::At { id, commit } => handle_at(id, commit, project, resolver, renderer),
            TaskAction::Status(status_args) => {
                let handler_args = StatusHandlerArgs::new(
                    status_args.id,
                    Some(status_args.status), // Task subcommand always sets status
                    project.map(|s| s.to_string()),
                );
                StatusHandler::execute(handler_args, project, resolver, renderer)
            }
            TaskAction::Priority { id, priority } => {
                let priority_args = PriorityArgs::new(id, priority, project.map(|s| s.to_string()));
                PriorityHandler::execute(priority_args, project, resolver, renderer)
            }
            TaskAction::Assignee { id, assignee } => {
                let args =
                    AssigneeArgs::new(id, assignee, project.map(|s| s.to_string()), false, false);
                AssigneeHandler::execute(args, project, resolver, renderer)
            }
            TaskAction::DueDate { id, due_date } => {
                let args =
                    DueDateArgs::new(id, due_date, project.map(|s| s.to_string()), false, false);
                DueDateHandler::execute(args, project, resolver, renderer)
            }
            TaskAction::Relationships(rel_args) => {
                let handler_args = RelationshipsHandlerArgs {
                    task_id: rel_args.id,
                    kinds: rel_args.kinds,
                    explicit_project: project.map(|s| s.to_string()),
                };
                RelationshipsHandler::execute(handler_args, project, resolver, renderer)
            }
            TaskAction::Delete(delete_args) => {
                DeleteHandler::execute(delete_args, project, resolver, renderer)
            }
            TaskAction::Comment {
                id,
                text,
                message,
                file,
                dry_run,
                explain,
            } => {
                // Resolve comment content from args: file > message > text > stdin
                let resolved_text = if let Some(path) = file {
                    std::fs::read_to_string(&path)
                        .map(|s| s.trim_end_matches(['\n', '\r']).to_string())
                        .unwrap_or_default()
                } else if let Some(m) = message {
                    m
                } else if let Some(t) = text {
                    t
                } else {
                    use std::io::{IsTerminal, Read};
                    let mut buffer = String::new();
                    if !std::io::stdin().is_terminal() {
                        if std::io::stdin().read_to_string(&mut buffer).is_ok() {
                            buffer.trim_end_matches(['\n', '\r']).to_string()
                        } else {
                            String::new()
                        }
                    } else {
                        String::new()
                    }
                };
                let text_value = if resolved_text.trim().is_empty() {
                    None
                } else {
                    Some(resolved_text)
                };
                let args = CommentArgs::new(
                    id,
                    text_value,
                    project.map(|s| s.to_string()),
                    dry_run,
                    explain,
                );
                CommentHandler::execute(args, project, resolver, renderer)
            }
        }
    }
}

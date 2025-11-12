mod assignment;
mod operations;
mod reporting;
mod shared;

use crate::cli::args::sprint::{SprintAction, SprintArgs};
use crate::cli::handlers::{CommandHandler, emit_subcommand_overview};
use crate::output::OutputRenderer;
use crate::workspace::TasksDirectoryResolver;

pub struct SprintHandler;

impl CommandHandler for SprintHandler {
    type Args = SprintArgs;
    type Result = Result<(), String>;

    fn execute(
        args: Self::Args,
        _project: Option<&str>,
        resolver: &TasksDirectoryResolver,
        renderer: &OutputRenderer,
    ) -> Self::Result {
        let tasks_root = resolver.path.clone();
        let Some(action) = args.action else {
            emit_subcommand_overview(renderer, &["sprint"]);
            return Ok(());
        };

        match action {
            SprintAction::Create(create_args) => {
                operations::handle_create(create_args, tasks_root.clone(), renderer)
            }
            SprintAction::Update(update_args) => {
                operations::handle_update(update_args, tasks_root.clone(), renderer)
            }
            SprintAction::Start(start_args) => {
                operations::handle_start(start_args, tasks_root.clone(), renderer)
            }
            SprintAction::Close(close_args) => {
                operations::handle_close(close_args, tasks_root.clone(), renderer)
            }
            SprintAction::List(list_args) => {
                reporting::handle_list(list_args, tasks_root.clone(), renderer)
            }
            SprintAction::Calendar(calendar_args) => {
                reporting::handle_calendar(calendar_args, tasks_root.clone(), renderer)
            }
            SprintAction::Velocity(velocity_args) => {
                reporting::handle_velocity(velocity_args, tasks_root.clone(), renderer)
            }
            SprintAction::Show(show_args) => {
                reporting::handle_show(show_args, tasks_root.clone(), renderer)
            }
            SprintAction::Review(review_args) => {
                reporting::handle_review(review_args, tasks_root.clone(), renderer)
            }
            SprintAction::Stats(stats_args) => {
                reporting::handle_stats(stats_args, tasks_root.clone(), renderer)
            }
            SprintAction::Summary(summary_args) => {
                reporting::handle_summary(summary_args, tasks_root.clone(), renderer)
            }
            SprintAction::Burndown(burndown_args) => {
                reporting::handle_burndown(burndown_args, tasks_root.clone(), renderer)
            }
            SprintAction::CleanupRefs(cleanup_args) => {
                reporting::handle_cleanup_refs(cleanup_args, tasks_root.clone(), renderer)
            }
            SprintAction::Normalize(normalize_args) => {
                operations::handle_normalize(normalize_args, tasks_root.clone(), renderer)
            }
            SprintAction::Add(add_args) => {
                assignment::handle_add(add_args, tasks_root.clone(), renderer)
            }
            SprintAction::Move(move_args) => {
                assignment::handle_move(move_args, tasks_root.clone(), renderer)
            }
            SprintAction::Remove(remove_args) => {
                assignment::handle_remove(remove_args, tasks_root.clone(), renderer)
            }
            SprintAction::Delete(delete_args) => {
                operations::handle_delete(delete_args, tasks_root.clone(), renderer)
            }
            SprintAction::Backlog(backlog_args) => {
                assignment::handle_backlog(backlog_args, tasks_root.clone(), renderer)
            }
        }
    }
}

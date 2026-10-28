mod activity;
mod age;
mod authors;
mod comments;
pub(crate) mod common;
mod custom;
mod distribution;
mod due;
mod effort;
mod history;
mod stale;
mod status;
mod tags;

use crate::cli::args::stats::{StatsAction, StatsArgs};
use crate::cli::handlers::CommandHandler;
use crate::output::OutputRenderer;
use crate::workspace::TasksDirectoryResolver;

pub struct StatsHandler;

impl CommandHandler for StatsHandler {
    type Args = StatsArgs;
    type Result = Result<(), String>;

    fn execute(
        args: Self::Args,
        project: Option<&str>,
        resolver: &TasksDirectoryResolver,
        renderer: &OutputRenderer,
    ) -> Self::Result {
        match args.action {
            StatsAction::Age {
                distribution,
                limit,
                global,
                filter,
            } => age::run(
                distribution,
                limit,
                global,
                &filter.join(" "),
                project,
                resolver,
                renderer,
            ),
            StatsAction::Status {
                id,
                time_in_status,
                since,
                until,
            } => status::run_status(
                &id,
                time_in_status,
                since.as_deref(),
                until.as_deref(),
                project,
                resolver,
                renderer,
            ),
            StatsAction::TimeInStatus {
                since,
                until,
                limit,
                global,
                filter,
            } => status::run_time_in_status(
                since.as_deref(),
                until.as_deref(),
                limit,
                global,
                &filter.join(" "),
                project,
                resolver,
                renderer,
            ),
            StatsAction::Effort {
                by,
                filter,
                unit,
                limit,
                global,
                since,
                until,
                transitions,
            } => effort::run(
                &by,
                &filter.join(" "),
                unit,
                limit,
                global,
                since.as_deref(),
                until.as_deref(),
                transitions.as_deref(),
                project,
                resolver,
                renderer,
            ),
            StatsAction::CommentsTop {
                limit,
                global,
                filter,
            } => comments::run_top(
                limit,
                global,
                &filter.join(" "),
                project,
                resolver,
                renderer,
            ),
            StatsAction::CommentsByAuthor {
                limit,
                global,
                filter,
            } => comments::run_by_author(
                limit,
                global,
                &filter.join(" "),
                project,
                resolver,
                renderer,
            ),
            StatsAction::CustomKeys {
                limit,
                global,
                filter,
            } => custom::run_keys(
                limit,
                global,
                &filter.join(" "),
                project,
                resolver,
                renderer,
            ),
            StatsAction::CustomField {
                name,
                limit,
                global,
                filter,
            } => custom::run_field(
                &name,
                limit,
                global,
                &filter.join(" "),
                project,
                resolver,
                renderer,
            ),
            StatsAction::Changed {
                since,
                until,
                author,
                limit,
                global,
                filter,
            } => history::run_changed(
                since.as_deref(),
                until.as_deref(),
                author.as_deref(),
                limit,
                global,
                &filter.join(" "),
                project,
                resolver,
                renderer,
            ),
            StatsAction::Churn {
                since,
                until,
                author,
                limit,
                global,
                filter,
            } => history::run_churn(
                since.as_deref(),
                until.as_deref(),
                author.as_deref(),
                limit,
                global,
                &filter.join(" "),
                project,
                resolver,
                renderer,
            ),
            StatsAction::Authors {
                since,
                until,
                limit,
                global,
            } => authors::run(
                since.as_deref(),
                until.as_deref(),
                limit,
                global,
                project,
                resolver,
                renderer,
            ),
            StatsAction::Activity {
                since,
                until,
                group_by,
                limit,
                global,
            } => activity::run(
                since.as_deref(),
                until.as_deref(),
                &group_by,
                limit,
                global,
                project,
                resolver,
                renderer,
            ),
            StatsAction::Stale {
                threshold,
                limit,
                global,
                filter,
            } => stale::run(
                &threshold,
                limit,
                global,
                &filter.join(" "),
                project,
                resolver,
                renderer,
            ),
            StatsAction::Tags {
                limit,
                global,
                filter,
            } => tags::run(
                limit,
                global,
                &filter.join(" "),
                project,
                resolver,
                renderer,
            ),
            StatsAction::Distribution {
                field,
                limit,
                global,
                filter,
            } => distribution::run(
                field,
                limit,
                global,
                &filter.join(" "),
                project,
                resolver,
                renderer,
            ),
            StatsAction::Due {
                buckets,
                overdue,
                threshold,
                global,
                filter,
            } => due::run(
                buckets,
                overdue,
                threshold,
                global,
                &filter.join(" "),
                project,
                resolver,
                renderer,
            ),
        }
    }
}

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
            } => age::run(distribution, limit, global, project, resolver, renderer),
            StatsAction::Status {
                id,
                time_in_status,
                since,
                until,
            } => status::run_status(
                id,
                time_in_status,
                since,
                until,
                project,
                resolver,
                renderer,
            ),
            StatsAction::TimeInStatus {
                since,
                until,
                limit,
                global,
            } => {
                status::run_time_in_status(since, until, limit, global, project, resolver, renderer)
            }
            StatsAction::Effort {
                by,
                r#where,
                unit,
                limit,
                global,
                since,
                until,
                transitions,
            } => effort::run(
                by,
                r#where,
                unit,
                limit,
                global,
                since,
                until,
                transitions,
                project,
                resolver,
                renderer,
            ),
            StatsAction::CommentsTop { limit, global } => {
                comments::run_top(limit, global, project, resolver, renderer)
            }
            StatsAction::CommentsByAuthor { limit, global } => {
                comments::run_by_author(limit, global, project, resolver, renderer)
            }
            StatsAction::CustomKeys { limit, global } => {
                custom::run_keys(limit, global, project, resolver, renderer)
            }
            StatsAction::CustomField {
                name,
                limit,
                global,
            } => custom::run_field(name, limit, global, project, resolver, renderer),
            StatsAction::Changed {
                since,
                until,
                author,
                limit,
                global,
            } => history::run_changed(
                since, until, author, limit, global, project, resolver, renderer,
            ),
            StatsAction::Churn {
                since,
                until,
                author,
                limit,
                global,
            } => history::run_churn(
                since, until, author, limit, global, project, resolver, renderer,
            ),
            StatsAction::Authors {
                since,
                until,
                limit,
                global,
            } => authors::run(since, until, limit, global, project, resolver, renderer),
            StatsAction::Activity {
                since,
                until,
                group_by,
                limit,
                global,
            } => activity::run(
                since, until, group_by, limit, global, project, resolver, renderer,
            ),
            StatsAction::Stale {
                threshold,
                limit,
                global,
            } => stale::run(threshold, limit, global, project, resolver, renderer),
            StatsAction::Tags { limit, global } => {
                tags::run(limit, global, project, resolver, renderer)
            }
            StatsAction::Distribution {
                field,
                limit,
                global,
            } => distribution::run(field, limit, global, project, resolver, renderer),
            StatsAction::Due {
                buckets,
                overdue,
                threshold,
                global,
            } => due::run(
                buckets, overdue, threshold, global, project, resolver, renderer,
            ),
        }
    }
}

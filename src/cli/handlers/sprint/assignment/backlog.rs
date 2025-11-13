use std::path::PathBuf;

use serde::Serialize;

use crate::cli::args::sprint::SprintBacklogArgs;
use crate::cli::handlers::sprint::shared::{
    SprintAssignmentIntegrityPayload, build_assignment_integrity, truncate,
};
use crate::output::{OutputFormat, OutputRenderer};
use crate::services::sprint_assignment::{self, SprintBacklogEntry};
use crate::types::TaskStatus;

use super::context::AssignmentContext;

pub(crate) fn handle_backlog(
    backlog_args: SprintBacklogArgs,
    tasks_root: PathBuf,
    renderer: &OutputRenderer,
) -> Result<(), String> {
    if backlog_args.limit == 0 {
        return Err("--limit must be greater than zero".to_string());
    }

    let mut context = match AssignmentContext::try_open(tasks_root)? {
        Some(context) => context,
        None => {
            match renderer.format {
                OutputFormat::Json => {
                    let payload = SprintBacklogResponse {
                        status: "ok",
                        count: 0,
                        truncated: false,
                        tasks: Vec::new(),
                        missing_sprints: Vec::new(),
                        integrity: None,
                    };
                    renderer.emit_json(&payload);
                }
                _ => renderer.emit_success("No backlog tasks found."),
            }
            return Ok(());
        }
    };

    let cleanup_summary = context.reconcile_missing(
        backlog_args.cleanup_missing,
        renderer,
        "loading the sprint backlog",
    )?;

    let options = sprint_assignment::SprintBacklogOptions {
        project: backlog_args.project.clone(),
        tags: backlog_args.tag.clone(),
        statuses: backlog_args
            .status
            .iter()
            .map(|value| TaskStatus::from(value.as_str()))
            .collect(),
        assignee: backlog_args.assignee.clone(),
        limit: backlog_args.limit,
    };

    let result = sprint_assignment::fetch_backlog(&context.storage, options)?;
    let entries = result.entries;
    let truncated = result.truncated;

    if entries.is_empty() {
        match renderer.format {
            OutputFormat::Json => {
                let payload = SprintBacklogResponse {
                    status: "ok",
                    count: 0,
                    truncated: false,
                    tasks: Vec::new(),
                    missing_sprints: context.integrity().missing_sprints.clone(),
                    integrity: build_assignment_integrity(
                        context.baseline_integrity(),
                        context.integrity(),
                        cleanup_summary.as_ref(),
                    ),
                };
                renderer.emit_json(&payload);
            }
            _ => renderer.emit_success("No backlog tasks matched the provided filters."),
        }
        return Ok(());
    }

    match renderer.format {
        OutputFormat::Json => {
            let payload = SprintBacklogResponse {
                status: "ok",
                count: entries.len(),
                truncated,
                tasks: entries.clone(),
                missing_sprints: context.integrity().missing_sprints.clone(),
                integrity: build_assignment_integrity(
                    context.baseline_integrity(),
                    context.integrity(),
                    cleanup_summary.as_ref(),
                ),
            };
            renderer.emit_json(&payload);
        }
        _ => {
            renderer.emit_raw_stdout(
                "ID          Title                          Status      Assignee        Priority    Due",
            );
            renderer.emit_raw_stdout(
                "--------------------------------------------------------------------------------",
            );
            for entry in &entries {
                let title = truncate(&entry.title, 30);
                let status = truncate(&entry.status, 12);
                let assignee = truncate(entry.assignee.as_deref().unwrap_or("-"), 14);
                let priority = truncate(&entry.priority, 10);
                let due = truncate(entry.due_date.as_deref().unwrap_or("-"), 16);
                renderer.emit_raw_stdout(format_args!(
                    "{:<10} {:<30} {:<12} {:<14} {:<10} {}",
                    truncate(&entry.id, 10),
                    title,
                    status,
                    assignee,
                    priority,
                    due
                ));
            }
            if truncated {
                renderer.emit_info("Results truncated; raise --limit to see more backlog tasks.");
            }
        }
    }

    Ok(())
}

#[derive(Debug, Serialize)]
struct SprintBacklogResponse {
    status: &'static str,
    count: usize,
    truncated: bool,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    tasks: Vec<SprintBacklogEntry>,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    missing_sprints: Vec<u32>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    integrity: Option<SprintAssignmentIntegrityPayload>,
}

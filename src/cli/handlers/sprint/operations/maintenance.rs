use std::io::{self, IsTerminal, Write};
use std::path::Path;

use crate::cli::args::sprint::SprintDeleteArgs;
use crate::output::{OutputFormat, OutputRenderer};
use crate::services::sprint_assignment;
use crate::services::sprint_integrity;
use crate::services::sprint_service::SprintService;
use crate::storage::manager::Storage;
use serde::Serialize;

use super::super::shared::{
    SprintAssignmentIntegrityPayload, build_assignment_integrity, emit_cleanup_summary,
    emit_missing_report,
};

pub(crate) fn handle_delete(
    delete_args: &SprintDeleteArgs,
    tasks_root: &Path,
    renderer: &OutputRenderer,
) -> Result<(), String> {
    let mut storage = Storage::new(tasks_root);

    let sprint_id = delete_args.resolved_sprint_id();
    let existing = SprintService::get(&storage, sprint_id)
        .map_err(|err| format!("Sprint #{} not found: {}", sprint_id, err))?;
    let display_name = sprint_assignment::sprint_display_name(&existing);
    let sprint_label = existing
        .sprint
        .plan
        .as_ref()
        .and_then(|plan| plan.label.clone());

    if !delete_args.force {
        if matches!(renderer.format, OutputFormat::Json) || !io::stdin().is_terminal() {
            return Err(
                "Sprint deletion requires --force with JSON output or non-interactive input."
                    .to_string(),
            );
        }

        print!(
            "Are you sure you want to delete sprint '{}'? (y/N): ",
            display_name
        );
        let _ = io::stdout().flush();

        let mut input = String::new();
        io::stdin()
            .read_line(&mut input)
            .map_err(|err| format!("Failed to read confirmation. Aborting: {err}"))?;
        let input = input.trim().to_lowercase();
        if input != "y" && input != "yes" {
            renderer.emit_warning("Deletion cancelled.");
            return Ok(());
        }
    }

    match SprintService::delete(&mut storage, sprint_id) {
        Ok(true) => {}
        Ok(false) => return Err(format!("Sprint #{} not found.", sprint_id)),
        Err(err) => return Err(format!("Failed to delete sprint: {}", err)),
    }

    let mut records = SprintService::list(&storage).map_err(|err| err.to_string())?;
    let integrity = sprint_integrity::check_missing_sprints(
        &mut storage,
        &mut records,
        delete_args.cleanup_missing,
        Some(sprint_id),
    )
    .map_err(|err| format!("Failed to clean up missing sprint references: {}", err))?;
    let cleanup_outcome = integrity.cleanup.as_ref();

    match renderer.format {
        OutputFormat::Json => {
            let payload = SprintDeleteResponsePayload {
                status: "ok",
                deleted: true,
                sprint_id,
                sprint_label,
                removed_references: cleanup_outcome
                    .map(|outcome| outcome.removed_references)
                    .unwrap_or(0),
                updated_tasks: cleanup_outcome
                    .map(|outcome| outcome.updated_tasks)
                    .unwrap_or(0),
                integrity: build_assignment_integrity(
                    &integrity.before,
                    &integrity.after,
                    cleanup_outcome,
                ),
            };
            renderer.emit_json(&payload);
        }
        _ => {
            renderer.emit_success(format_args!("Deleted {}.", display_name));
            if let Some(outcome) = cleanup_outcome {
                emit_cleanup_summary(renderer, outcome, "deleting the sprint");
            } else {
                emit_missing_report(renderer, &integrity.after, "deleting the sprint");
            }
        }
    }

    Ok(())
}

#[derive(Debug, Serialize)]
struct SprintDeleteResponsePayload {
    status: &'static str,
    deleted: bool,
    sprint_id: u32,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    sprint_label: Option<String>,
    removed_references: usize,
    updated_tasks: usize,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    integrity: Option<SprintAssignmentIntegrityPayload>,
}

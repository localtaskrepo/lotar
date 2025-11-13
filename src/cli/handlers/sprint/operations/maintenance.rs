use std::io::{self, Write};
use std::path::PathBuf;

use crate::cli::args::sprint::SprintDeleteArgs;
use crate::output::{OutputFormat, OutputRenderer};
use crate::services::sprint_assignment;
use crate::services::sprint_integrity::{self, SprintCleanupOutcome};
use crate::services::sprint_service::SprintService;
use crate::storage::manager::Storage;
use serde::Serialize;

use super::super::shared::{
    SprintAssignmentIntegrityPayload, build_assignment_integrity, emit_cleanup_summary,
    emit_missing_report,
};

pub(crate) fn handle_delete(
    delete_args: SprintDeleteArgs,
    tasks_root: PathBuf,
    renderer: &OutputRenderer,
) -> Result<(), String> {
    let mut storage = Storage::new(tasks_root.clone());

    let sprint_id = delete_args.resolved_sprint_id();
    let existing = SprintService::get(&storage, sprint_id)
        .map_err(|err| format!("Sprint #{} not found: {}", sprint_id, err))?;
    let display_name = sprint_assignment::sprint_display_name(&existing);
    let sprint_label = existing
        .sprint
        .plan
        .as_ref()
        .and_then(|plan| plan.label.clone());

    if !delete_args.force && !matches!(renderer.format, OutputFormat::Json) {
        print!(
            "Are you sure you want to delete sprint '{}'? (y/N): ",
            display_name
        );
        let _ = io::stdout().flush();

        let mut input = String::new();
        if io::stdin().read_line(&mut input).is_err() {
            renderer.emit_error("Failed to read input. Aborting.");
            return Ok(());
        }
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
    let mut integrity_report = sprint_integrity::detect_missing_sprints(&storage, &records);
    let baseline_report = integrity_report.clone();
    let mut cleanup_outcome: Option<SprintCleanupOutcome> = None;

    if delete_args.cleanup_missing {
        match sprint_integrity::cleanup_missing_sprint_refs(
            &mut storage,
            &mut records,
            Some(sprint_id),
        ) {
            Ok(outcome) => {
                integrity_report = sprint_integrity::detect_missing_sprints(&storage, &records);
                cleanup_outcome = Some(outcome);
            }
            Err(err) => {
                return Err(format!(
                    "Failed to clean up missing sprint references: {}",
                    err
                ));
            }
        }
    }

    match renderer.format {
        OutputFormat::Json => {
            let payload = SprintDeleteResponsePayload {
                status: "ok",
                deleted: true,
                sprint_id,
                sprint_label,
                removed_references: cleanup_outcome
                    .as_ref()
                    .map(|outcome| outcome.removed_references)
                    .unwrap_or(0),
                updated_tasks: cleanup_outcome
                    .as_ref()
                    .map(|outcome| outcome.updated_tasks)
                    .unwrap_or(0),
                integrity: build_assignment_integrity(
                    &baseline_report,
                    &integrity_report,
                    cleanup_outcome.as_ref(),
                ),
            };
            renderer.emit_json(&payload);
        }
        _ => {
            renderer.emit_success(format_args!("Deleted {}.", display_name));
            if let Some(outcome) = cleanup_outcome.as_ref() {
                emit_cleanup_summary(renderer, outcome, "deleting the sprint");
            } else {
                emit_missing_report(renderer, &integrity_report, "deleting the sprint");
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

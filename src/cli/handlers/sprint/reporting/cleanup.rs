use std::path::PathBuf;

use serde::Serialize;

use crate::cli::args::sprint::SprintCleanupRefsArgs;
use crate::cli::handlers::sprint::shared::{
    SprintCleanupRefsRemoved, cleanup_summary_payload, format_missing_ids,
};
use crate::output::{OutputFormat, OutputRenderer};
use crate::services::sprint_integrity;
use crate::services::sprint_service::SprintService;
use crate::storage::manager::Storage;

pub(crate) fn handle_cleanup_refs(
    cleanup_args: SprintCleanupRefsArgs,
    tasks_root: PathBuf,
    renderer: &OutputRenderer,
) -> Result<(), String> {
    let mut storage = Storage::new(tasks_root.clone());

    let mut records = SprintService::list(&storage).map_err(|err| err.to_string())?;
    let outcome = sprint_integrity::cleanup_missing_sprint_refs(
        &mut storage,
        &mut records,
        cleanup_args.sprint_id,
    )
    .map_err(|err| err.to_string())?;

    if let Some(id) = cleanup_args.sprint_id
        && records.iter().any(|record| record.id == id)
        && !matches!(renderer.format, OutputFormat::Json)
    {
        renderer.emit_warning(format_args!(
            "Sprint #{} still exists; removing references per request.",
            id
        ));
    }

    let cleanup_payload = cleanup_summary_payload(&outcome);

    let payload = SprintCleanupRefsResponse {
        status: "ok",
        scanned_tasks: outcome.scanned_tasks,
        updated_tasks: outcome.updated_tasks,
        removed_references: outcome.removed_references,
        removed_by_sprint: cleanup_payload.removed_by_sprint.clone(),
        targeted: cleanup_args.sprint_id,
        missing_sprints: outcome.missing_sprints.clone(),
        remaining_missing: cleanup_payload.remaining_missing.clone(),
    };

    match renderer.format {
        OutputFormat::Json => {
            renderer.emit_json(&payload);
        }
        _ => {
            if outcome.removed_references == 0 {
                if cleanup_args.sprint_id.is_some() {
                    renderer.emit_success("No tasks reference the specified sprint.");
                } else if outcome.missing_sprints.is_empty() {
                    renderer.emit_success("No dangling sprint references detected.");
                } else {
                    renderer.emit_success("No sprint references required cleanup.");
                }
            } else {
                renderer.emit_success(format_args!(
                    "Removed {} sprint reference(s) across {} task(s).",
                    outcome.removed_references, outcome.updated_tasks
                ));
                for metric in &cleanup_payload.removed_by_sprint {
                    renderer.emit_info(format_args!(
                        "Sprint #{}: removed {} reference(s).",
                        metric.sprint_id, metric.count
                    ));
                }

                if cleanup_args.sprint_id.is_none() && !outcome.remaining_missing.is_empty() {
                    let formatted = format_missing_ids(&outcome.remaining_missing);
                    renderer.emit_warning(format_args!(
                        "Additional missing sprint IDs still referenced: {}",
                        formatted
                    ));
                }
            }
        }
    }

    Ok(())
}

#[derive(Debug, Serialize)]
struct SprintCleanupRefsResponse {
    status: &'static str,
    scanned_tasks: usize,
    updated_tasks: usize,
    removed_references: usize,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    removed_by_sprint: Vec<SprintCleanupRefsRemoved>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    targeted: Option<u32>,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    missing_sprints: Vec<u32>,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    remaining_missing: Vec<u32>,
}

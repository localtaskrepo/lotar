use std::path::PathBuf;

use serde::Serialize;

use crate::cli::args::sprint::{SprintAddArgs, SprintMoveArgs, SprintRemoveArgs};
use crate::cli::handlers::sprint::shared::{
    SprintAssignmentIntegrityPayload, build_assignment_integrity,
};
use crate::output::{OutputFormat, OutputRenderer};
use crate::services::sprint_assignment;
use crate::services::sprint_service::SprintRecord;
use crate::storage::manager::Storage;

use super::context::AssignmentContext;

pub(crate) fn handle_add(
    add_args: SprintAddArgs,
    tasks_root: PathBuf,
    renderer: &OutputRenderer,
) -> Result<(), String> {
    let mut context = AssignmentContext::for_mutation(tasks_root)?;

    let cleanup_summary = context.reconcile_missing(
        add_args.cleanup_missing,
        renderer,
        "assigning sprint memberships",
    )?;

    let (explicit, tasks) = split_assignment_inputs(
        &context.storage,
        &context.records,
        &add_args.sprint,
        &add_args.items,
    )?;

    let outcome = sprint_assignment::assign_tasks(
        &mut context.storage,
        &context.records,
        &tasks,
        explicit.as_deref(),
        add_args.allow_closed,
        add_args.force,
    )?;

    let reassignment_messages: Vec<String> = outcome
        .replaced
        .iter()
        .filter_map(|info| info.describe())
        .collect();

    match renderer.format {
        OutputFormat::Json => {
            let payload = SprintAssignmentResponse {
                status: "ok",
                action: outcome.action.as_str(),
                sprint_id: outcome.sprint_id,
                sprint_label: outcome.sprint_label.clone(),
                modified: outcome.modified.clone(),
                unchanged: outcome.unchanged.clone(),
                replaced: outcome
                    .replaced
                    .iter()
                    .map(|info| SprintReassignment {
                        task_id: info.task_id.clone(),
                        previous: info.previous.clone(),
                    })
                    .collect(),
                messages: reassignment_messages.clone(),
                integrity: build_assignment_integrity(
                    context.baseline_integrity(),
                    context.integrity(),
                    cleanup_summary.as_ref(),
                ),
            };
            renderer.emit_json(&payload);
        }
        _ => {
            renderer.emit_success(format_args!(
                "Attached sprint #{} ({}) to {} task(s).",
                outcome.sprint_id,
                outcome.sprint_display_name,
                outcome.modified.len()
            ));
            if !outcome.unchanged.is_empty() {
                renderer.emit_info(format_args!(
                    "Already assigned (skipped): {}",
                    outcome.unchanged.join(", ")
                ));
            }
            if outcome.modified.is_empty() {
                renderer.emit_warning(
                    "No tasks gained the sprint membership; all provided tasks were already assigned.",
                );
            }
            if add_args.force {
                for message in &reassignment_messages {
                    renderer.emit_info(message);
                }
            }
        }
    }

    Ok(())
}

pub(crate) fn handle_move(
    move_args: SprintMoveArgs,
    tasks_root: PathBuf,
    renderer: &OutputRenderer,
) -> Result<(), String> {
    let mut context = AssignmentContext::for_mutation(tasks_root)?;

    let cleanup_summary = context.reconcile_missing(
        move_args.cleanup_missing,
        renderer,
        "moving sprint memberships",
    )?;

    let (explicit, tasks) = split_assignment_inputs(
        &context.storage,
        &context.records,
        &move_args.sprint,
        &move_args.items,
    )?;

    let outcome = sprint_assignment::assign_tasks(
        &mut context.storage,
        &context.records,
        &tasks,
        explicit.as_deref(),
        move_args.allow_closed,
        true,
    )?;

    let reassignment_messages: Vec<String> = outcome
        .replaced
        .iter()
        .filter_map(|info| info.describe())
        .collect();

    match renderer.format {
        OutputFormat::Json => {
            let payload = SprintAssignmentResponse {
                status: "ok",
                action: outcome.action.as_str(),
                sprint_id: outcome.sprint_id,
                sprint_label: outcome.sprint_label.clone(),
                modified: outcome.modified.clone(),
                unchanged: outcome.unchanged.clone(),
                replaced: outcome
                    .replaced
                    .iter()
                    .map(|info| SprintReassignment {
                        task_id: info.task_id.clone(),
                        previous: info.previous.clone(),
                    })
                    .collect(),
                messages: reassignment_messages.clone(),
                integrity: build_assignment_integrity(
                    context.baseline_integrity(),
                    context.integrity(),
                    cleanup_summary.as_ref(),
                ),
            };
            renderer.emit_json(&payload);
        }
        _ => {
            renderer.emit_success(format_args!(
                "Moved {} task(s) to sprint #{} ({}).",
                outcome.modified.len(),
                outcome.sprint_id,
                outcome.sprint_display_name
            ));
            if !outcome.replaced.is_empty() {
                for message in &reassignment_messages {
                    renderer.emit_info(message);
                }
            }
            if !outcome.unchanged.is_empty() {
                renderer.emit_info(format_args!(
                    "Already assigned to target sprint (skipped): {}",
                    outcome.unchanged.join(", ")
                ));
            }
            if outcome.modified.is_empty() {
                renderer.emit_warning(
                    "No tasks changed sprint membership; all provided tasks already belonged to the target sprint.",
                );
            }
        }
    }

    Ok(())
}

pub(crate) fn handle_remove(
    remove_args: SprintRemoveArgs,
    tasks_root: PathBuf,
    renderer: &OutputRenderer,
) -> Result<(), String> {
    let mut context = AssignmentContext::for_mutation(tasks_root)?;

    let cleanup_summary = context.reconcile_missing(
        remove_args.cleanup_missing,
        renderer,
        "removing sprint memberships",
    )?;

    let (explicit, tasks) = split_assignment_inputs(
        &context.storage,
        &context.records,
        &remove_args.sprint,
        &remove_args.items,
    )?;

    let outcome = sprint_assignment::remove_tasks(
        &mut context.storage,
        &context.records,
        &tasks,
        explicit.as_deref(),
    )?;

    match renderer.format {
        OutputFormat::Json => {
            let payload = SprintAssignmentResponse {
                status: "ok",
                action: outcome.action.as_str(),
                sprint_id: outcome.sprint_id,
                sprint_label: outcome.sprint_label.clone(),
                modified: outcome.modified.clone(),
                unchanged: outcome.unchanged.clone(),
                replaced: Vec::new(),
                messages: Vec::new(),
                integrity: build_assignment_integrity(
                    context.baseline_integrity(),
                    context.integrity(),
                    cleanup_summary.as_ref(),
                ),
            };
            renderer.emit_json(&payload);
        }
        _ => {
            renderer.emit_success(format_args!(
                "Removed sprint #{} ({}) from {} task(s).",
                outcome.sprint_id,
                outcome.sprint_display_name,
                outcome.modified.len()
            ));
            if !outcome.unchanged.is_empty() {
                renderer.emit_info(format_args!(
                    "Tasks without that sprint membership: {}",
                    outcome.unchanged.join(", ")
                ));
            }
            if outcome.modified.is_empty() {
                renderer.emit_warning(
                    "No sprint memberships were removed because none of the provided tasks were linked to the sprint.",
                );
            }
        }
    }

    Ok(())
}

fn split_assignment_inputs(
    storage: &Storage,
    records: &[SprintRecord],
    explicit: &Option<String>,
    items: &[String],
) -> Result<(Option<String>, Vec<String>), String> {
    if items.is_empty() {
        return Ok((explicit.clone(), Vec::new()));
    }

    if let Some(sprint_ref) = explicit.as_ref() {
        return Ok((Some(sprint_ref.clone()), items.to_vec()));
    }

    if items.len() == 1 {
        return Ok((None, items.to_vec()));
    }

    let first = items[0].trim();
    if sprint_assignment::likely_sprint_reference(storage, records, first) {
        let remaining = items[1..].to_vec();
        if remaining.is_empty() {
            Err("Provide at least one task identifier after the sprint reference.".to_string())
        } else {
            Ok((Some(items[0].clone()), remaining))
        }
    } else {
        Ok((None, items.to_vec()))
    }
}

#[derive(Debug, Serialize)]
struct SprintAssignmentResponse {
    status: &'static str,
    action: &'static str,
    sprint_id: u32,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    sprint_label: Option<String>,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    modified: Vec<String>,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    unchanged: Vec<String>,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    replaced: Vec<SprintReassignment>,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    messages: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    integrity: Option<SprintAssignmentIntegrityPayload>,
}

#[derive(Debug, Serialize)]
struct SprintReassignment {
    task_id: String,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    previous: Vec<u32>,
}

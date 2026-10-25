use std::path::Path;

use crate::api_types::SprintCreateRequest;
use crate::cli::args::sprint::{SprintCreateArgs, SprintUpdateArgs};
use crate::output::OutputRenderer;
use crate::services::sprint_service::{
    SprintMetadataPatch, SprintService, sprint_from_create_request,
};
use crate::storage::manager::Storage;
use crate::storage::sprint::Sprint;

use super::support::{SprintOperationKind, render_operation_response};

pub(crate) fn handle_create(
    create_args: &SprintCreateArgs,
    tasks_root: &Path,
    renderer: &OutputRenderer,
) -> Result<(), String> {
    let mut storage = Storage::new(tasks_root);

    let resolved_config = crate::config::resolution::load_and_merge_configs(Some(tasks_root))
        .map_err(|err| err.to_string())?;

    let sprint = build_sprint_from_args(create_args);
    let defaults = if create_args.no_defaults {
        None
    } else {
        Some(&resolved_config.sprint_defaults)
    };

    let warnings_enabled = resolved_config.sprint_notifications.enabled;

    let outcome = SprintService::create(&mut storage, sprint, defaults)
        .map_err(|err| format!("Failed to create sprint: {}", err))?;

    render_operation_response(
        SprintOperationKind::Create,
        outcome.record,
        outcome.warnings,
        outcome.applied_defaults,
        renderer,
        warnings_enabled,
        &resolved_config,
    );

    Ok(())
}

pub(crate) fn handle_update(
    update_args: &SprintUpdateArgs,
    tasks_root: &Path,
    renderer: &OutputRenderer,
) -> Result<(), String> {
    if !update_args.has_mutations() {
        return Err("No updates provided; specify fields to mutate.".to_string());
    }

    let mut storage = Storage::new(tasks_root);

    let resolved_config = crate::config::resolution::load_and_merge_configs(Some(tasks_root))
        .map_err(|err| err.to_string())?;

    let sprint_id = update_args.resolved_sprint_id();

    let patch = SprintMetadataPatch {
        label: update_args.label.clone().map(Some),
        goal: update_args.goal.clone().map(Some),
        plan_length: update_args.plan_length.clone().map(Some),
        ends_at: update_args.ends_at.clone().map(Some),
        starts_at: update_args.starts_at.clone().map(Some),
        overdue_after: update_args.overdue_after.clone().map(Some),
        notes: update_args.notes.clone().map(Some),
        capacity_points: update_args.capacity_points.map(Some),
        capacity_hours: update_args.capacity_hours.map(Some),
        actual_started_at: update_args.actual_started_at.clone().map(Some),
        actual_closed_at: update_args.actual_closed_at.clone().map(Some),
    };

    let warnings_enabled = resolved_config.sprint_notifications.enabled;

    let outcome = SprintService::update_with_patch(&mut storage, sprint_id, &patch).map_err(
        |err| match err {
            crate::errors::LoTaRError::SprintNotFound(_) => err.to_string(),
            other => format!("Failed to update sprint: {}", other),
        },
    )?;

    render_operation_response(
        SprintOperationKind::Update,
        outcome.record,
        outcome.warnings,
        outcome.applied_defaults,
        renderer,
        warnings_enabled,
        &resolved_config,
    );

    Ok(())
}

fn build_sprint_from_args(create_args: &SprintCreateArgs) -> Sprint {
    sprint_from_create_request(&SprintCreateRequest {
        label: create_args.label.clone(),
        goal: create_args.goal.clone(),
        plan_length: create_args.plan_length.clone(),
        ends_at: create_args.ends_at.clone(),
        starts_at: create_args.starts_at.clone(),
        capacity_points: create_args.capacity_points,
        capacity_hours: create_args.capacity_hours,
        overdue_after: create_args.overdue_after.clone(),
        notes: create_args.notes.clone(),
        skip_defaults: create_args.no_defaults,
    })
}

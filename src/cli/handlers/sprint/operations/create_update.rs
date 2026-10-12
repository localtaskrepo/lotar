use std::path::Path;

use crate::cli::args::sprint::{SprintCreateArgs, SprintUpdateArgs};
use crate::output::OutputRenderer;
use crate::services::sprint_service::{SprintMetadataPatch, SprintService};
use crate::storage::manager::Storage;
use crate::storage::sprint::{Sprint, SprintCapacity, SprintPlan};

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
    let mut plan = SprintPlan::default();

    if let Some(label) = clean_opt_string(create_args.label.clone()) {
        plan.label = Some(label);
    }
    if let Some(goal) = clean_opt_string(create_args.goal.clone()) {
        plan.goal = Some(goal);
    }
    if let Some(length) = clean_opt_string(create_args.plan_length.clone()) {
        plan.length = Some(length);
    }
    if let Some(ends_at) = clean_opt_string(create_args.ends_at.clone()) {
        plan.ends_at = Some(ends_at);
    }
    if let Some(starts_at) = clean_opt_string(create_args.starts_at.clone()) {
        plan.starts_at = Some(starts_at);
    }
    if let Some(points) = create_args.capacity_points {
        plan.capacity
            .get_or_insert_with(SprintCapacity::default)
            .points = Some(points);
    }
    if let Some(hours) = create_args.capacity_hours {
        plan.capacity
            .get_or_insert_with(SprintCapacity::default)
            .hours = Some(hours);
    }
    if let Some(overdue) = clean_opt_string(create_args.overdue_after.clone()) {
        plan.overdue_after = Some(overdue);
    }
    if let Some(notes) = create_args
        .notes
        .clone()
        .filter(|value| !value.trim().is_empty())
    {
        plan.notes = Some(notes);
    }

    let mut sprint = Sprint::default();
    if plan_has_values(&plan) {
        sprint.plan = Some(plan);
    }
    sprint
}

fn plan_has_values(plan: &SprintPlan) -> bool {
    plan.label.is_some()
        || plan.goal.is_some()
        || plan.length.is_some()
        || plan.ends_at.is_some()
        || plan.starts_at.is_some()
        || plan.capacity.is_some()
        || plan.overdue_after.is_some()
        || plan.notes.is_some()
}

fn clean_opt_string(input: Option<String>) -> Option<String> {
    input.and_then(|value| {
        let trimmed = value.trim();
        if trimmed.is_empty() {
            None
        } else {
            Some(trimmed.to_string())
        }
    })
}

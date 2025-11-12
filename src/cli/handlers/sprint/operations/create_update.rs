use std::path::PathBuf;

use crate::cli::args::sprint::{SprintCreateArgs, SprintUpdateArgs};
use crate::output::OutputRenderer;
use crate::services::sprint_service::SprintService;
use crate::storage::manager::Storage;
use crate::storage::sprint::{Sprint, SprintActual, SprintCapacity, SprintPlan};

use super::support::{SprintOperationKind, render_operation_response};

pub(crate) fn handle_create(
    create_args: SprintCreateArgs,
    tasks_root: PathBuf,
    renderer: &OutputRenderer,
) -> Result<(), String> {
    let mut storage = Storage::new(tasks_root.clone());

    let resolved_config =
        crate::config::resolution::load_and_merge_configs(Some(tasks_root.as_path()))
            .map_err(|err| err.to_string())?;

    let sprint = build_sprint_from_args(&create_args);
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
    update_args: SprintUpdateArgs,
    tasks_root: PathBuf,
    renderer: &OutputRenderer,
) -> Result<(), String> {
    if !update_args.has_mutations() {
        return Err("No updates provided; specify fields to mutate.".to_string());
    }

    let mut storage = Storage::new(tasks_root.clone());

    let resolved_config =
        crate::config::resolution::load_and_merge_configs(Some(tasks_root.as_path()))
            .map_err(|err| err.to_string())?;

    let sprint_id = update_args.resolved_sprint_id();

    let existing = SprintService::get(&storage, sprint_id).map_err(|err| err.to_string())?;

    let mut sprint = existing.sprint.clone();
    apply_update_to_sprint(&mut sprint, &update_args);

    let warnings_enabled = resolved_config.sprint_notifications.enabled;

    let outcome = SprintService::update(&mut storage, sprint_id, sprint)
        .map_err(|err| format!("Failed to update sprint: {}", err))?;

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

fn apply_update_to_sprint(sprint: &mut Sprint, update: &SprintUpdateArgs) {
    if update.label.is_some()
        || update.goal.is_some()
        || update.plan_length.is_some()
        || update.ends_at.is_some()
        || update.starts_at.is_some()
        || update.overdue_after.is_some()
        || update.notes.is_some()
    {
        let plan = sprint.plan.get_or_insert_with(SprintPlan::default);

        if update.label.is_some() {
            plan.label = clean_opt_string(update.label.clone());
        }
        if update.goal.is_some() {
            plan.goal = clean_opt_string(update.goal.clone());
        }
        if update.plan_length.is_some() {
            plan.length = clean_opt_string(update.plan_length.clone());
        }
        if update.ends_at.is_some() {
            plan.ends_at = clean_opt_string(update.ends_at.clone());
        }
        if update.starts_at.is_some() {
            plan.starts_at = clean_opt_string(update.starts_at.clone());
        }
        if update.overdue_after.is_some() {
            plan.overdue_after = clean_opt_string(update.overdue_after.clone());
        }
        if update.notes.is_some() {
            plan.notes = clean_opt_string(update.notes.clone());
        }
    }

    if update.capacity_points.is_some() || update.capacity_hours.is_some() {
        let plan = sprint.plan.get_or_insert_with(SprintPlan::default);
        let capacity = plan.capacity.get_or_insert_with(SprintCapacity::default);
        if let Some(points) = update.capacity_points {
            capacity.points = Some(points);
        }
        if let Some(hours) = update.capacity_hours {
            capacity.hours = Some(hours);
        }
    }

    if update.actual_started_at.is_some() || update.actual_closed_at.is_some() {
        let actual = sprint.actual.get_or_insert_with(SprintActual::default);
        if update.actual_started_at.is_some() {
            actual.started_at = clean_opt_string(update.actual_started_at.clone());
        }
        if update.actual_closed_at.is_some() {
            actual.closed_at = clean_opt_string(update.actual_closed_at.clone());
        }
    }
}

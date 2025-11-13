use std::path::PathBuf;

use chrono::{DateTime, Duration, Utc};

use crate::cli::args::sprint::{SprintCloseArgs, SprintStartArgs};
use crate::output::{OutputFormat, OutputRenderer};
use crate::services::sprint_assignment;
use crate::services::sprint_service::{SprintRecord, SprintService};
use crate::services::sprint_status::{self, SprintLifecycleState, SprintLifecycleStatus};
use crate::storage::sprint::{Sprint, SprintActual};
use crate::utils::time;

use super::support::{
    SprintOperationKind, render_operation_response, resolve_sprint_records_context,
};

pub(crate) fn handle_start(
    start_args: SprintStartArgs,
    tasks_root: PathBuf,
    renderer: &OutputRenderer,
) -> Result<(), String> {
    let start_instant = determine_start_timestamp(&start_args)?;

    let mut context = resolve_sprint_records_context(tasks_root, "starting")?;

    let target_id = match start_args.sprint_id {
        Some(id) => id,
        None => select_sprint_id_for_start(&context.records, start_instant)
            .ok_or_else(|| "No pending sprints ready to start.".to_string())?,
    };

    if start_args.sprint_id.is_none() && !matches!(renderer.format, OutputFormat::Json) {
        renderer.emit_info(format_args!(
            "Auto-selected sprint #{} to start.",
            target_id
        ));
    }

    let existing =
        SprintService::get(&context.storage, target_id).map_err(|err| err.to_string())?;
    let lifecycle = sprint_status::derive_status(&existing.sprint, start_instant);

    if lifecycle.actual_end.is_some() && !start_args.force {
        return Err(
            "Sprint is already closed. Clear actual.closed_at or pass --force to restart."
                .to_string(),
        );
    }

    if lifecycle.actual_start.is_some() && !start_args.force {
        return Err(
            "Sprint already has actual.started_at; use --force to override the recorded start time.".to_string(),
        );
    }

    let mut sprint = existing.sprint.clone();

    let warnings_enabled =
        context.resolved_config.sprint_notifications.enabled && !start_args.no_warn;

    warn_about_overdue_start(renderer, warnings_enabled, &existing, &lifecycle);
    warn_about_future_start(renderer, warnings_enabled, start_instant, start_args.force);

    apply_start_to_sprint(&mut sprint, start_instant, start_args.force);

    warn_about_parallel_active_sprints(
        renderer,
        warnings_enabled,
        &context.records,
        target_id,
        start_instant,
        ParallelWarningContext::Start,
    );

    let outcome = SprintService::update(&mut context.storage, target_id, sprint)
        .map_err(|err| format!("Failed to start sprint: {}", err))?;

    render_operation_response(
        SprintOperationKind::Start,
        outcome.record,
        outcome.warnings,
        outcome.applied_defaults,
        renderer,
        warnings_enabled,
        &context.resolved_config,
    );

    Ok(())
}

pub(crate) fn handle_close(
    close_args: SprintCloseArgs,
    tasks_root: PathBuf,
    renderer: &OutputRenderer,
) -> Result<(), String> {
    let close_instant = determine_close_timestamp(&close_args)?;

    let mut context = resolve_sprint_records_context(tasks_root, "closing")?;

    let target_id = match close_args.sprint_id {
        Some(id) => id,
        None => select_sprint_id_for_close(&context.records)
            .ok_or_else(|| "No active sprints ready to close.".to_string())?,
    };

    if close_args.sprint_id.is_none() && !matches!(&renderer.format, OutputFormat::Json) {
        renderer.emit_info(format_args!(
            "Auto-selected sprint #{} to close.",
            target_id
        ));
    }

    let existing =
        SprintService::get(&context.storage, target_id).map_err(|err| err.to_string())?;
    let lifecycle = sprint_status::derive_status(&existing.sprint, close_instant);

    if lifecycle.actual_end.is_some() && !close_args.force {
        return Err(
            "Sprint already has actual.closed_at; use --force to override the recorded close time."
                .to_string(),
        );
    }

    if lifecycle.actual_start.is_none() && !close_args.force {
        return Err(
            "Sprint has not been started yet; use --force to close without a recorded start."
                .to_string(),
        );
    }

    let mut sprint = existing.sprint.clone();

    let warnings_enabled =
        context.resolved_config.sprint_notifications.enabled && !close_args.no_warn;

    warn_about_overdue_close(
        renderer,
        warnings_enabled,
        &existing,
        &lifecycle,
        close_instant,
    );

    apply_close_to_sprint(&mut sprint, close_instant, close_args.force);

    warn_about_parallel_active_sprints(
        renderer,
        warnings_enabled,
        &context.records,
        target_id,
        close_instant,
        ParallelWarningContext::Close,
    );

    let outcome = SprintService::update(&mut context.storage, target_id, sprint)
        .map_err(|err| format!("Failed to close sprint: {}", err))?;

    let review_record = if close_args.review {
        Some(outcome.record.clone())
    } else {
        None
    };

    render_operation_response(
        SprintOperationKind::Close,
        outcome.record,
        outcome.warnings,
        outcome.applied_defaults,
        renderer,
        warnings_enabled,
        &context.resolved_config,
    );

    if let Some(record) = review_record.as_ref() {
        super::super::reporting::render_sprint_review(
            &context.storage,
            record,
            &context.resolved_config,
            renderer,
        );
    }

    Ok(())
}

fn determine_start_timestamp(args: &SprintStartArgs) -> Result<DateTime<Utc>, String> {
    if let Some(ref at) = args.at {
        time::parse_human_datetime_to_utc(at).map_err(|err| format!("Invalid --at value: {}", err))
    } else {
        Ok(Utc::now())
    }
}

fn determine_close_timestamp(args: &SprintCloseArgs) -> Result<DateTime<Utc>, String> {
    if let Some(ref at) = args.at {
        time::parse_human_datetime_to_utc(at).map_err(|err| format!("Invalid --at value: {}", err))
    } else {
        Ok(Utc::now())
    }
}

fn select_sprint_id_for_start(
    records: &[SprintRecord],
    evaluation_time: DateTime<Utc>,
) -> Option<u32> {
    let mut ready: Vec<(DateTime<Utc>, u32)> = Vec::new();
    let mut fallback: Option<u32> = None;

    for record in records {
        let lifecycle = sprint_status::derive_status(&record.sprint, evaluation_time);
        if lifecycle.actual_end.is_some() {
            continue;
        }
        if lifecycle.actual_start.is_some() {
            continue;
        }

        if let Some(planned_start) = lifecycle.planned_start
            && planned_start <= evaluation_time
        {
            ready.push((planned_start, record.id));
            continue;
        }

        if fallback.is_none() {
            fallback = Some(record.id);
        }
    }

    if let Some((_, id)) = ready
        .into_iter()
        .min_by(|a, b| a.0.cmp(&b.0).then(a.1.cmp(&b.1)))
    {
        Some(id)
    } else {
        fallback
    }
}

fn select_sprint_id_for_close(records: &[SprintRecord]) -> Option<u32> {
    let mut active = records
        .iter()
        .filter_map(|record| {
            let lifecycle = sprint_status::derive_status(&record.sprint, Utc::now());
            if lifecycle.actual_end.is_some() {
                None
            } else if lifecycle.actual_start.is_some() {
                Some(record.id)
            } else {
                None
            }
        })
        .collect::<Vec<u32>>();

    if active.is_empty() {
        records
            .iter()
            .rev()
            .find(|record| {
                let lifecycle = sprint_status::derive_status(&record.sprint, Utc::now());
                lifecycle.actual_end.is_none()
            })
            .map(|record| record.id)
    } else {
        active.sort_unstable();
        active.pop()
    }
}

#[derive(Copy, Clone)]
enum ParallelWarningContext {
    Start,
    Close,
}

fn warn_about_parallel_active_sprints(
    renderer: &OutputRenderer,
    notifications_enabled: bool,
    records: &[SprintRecord],
    target_id: u32,
    reference_time: DateTime<Utc>,
    context: ParallelWarningContext,
) {
    if !notifications_enabled {
        return;
    }

    let mut others = Vec::new();
    for record in records {
        if record.id == target_id {
            continue;
        }
        let lifecycle = sprint_status::derive_status(&record.sprint, reference_time);
        if matches!(
            lifecycle.state,
            SprintLifecycleState::Active | SprintLifecycleState::Overdue
        ) {
            others.push((
                record.id,
                sprint_assignment::sprint_display_name(record),
                lifecycle.label().to_string(),
            ));
        }
    }

    if others.is_empty() {
        return;
    }

    let prefix = match context {
        ParallelWarningContext::Start => "Another sprint is still running",
        ParallelWarningContext::Close => "Additional sprints remain active",
    };

    let list = others
        .into_iter()
        .map(|(id, label, status)| format!("#{} ({}) is {}", id, label, status))
        .collect::<Vec<_>>()
        .join("; ");

    let guidance = match context {
        ParallelWarningContext::Start => {
            " Close it before starting another sprint or pass --force if you intend to overlap."
        }
        ParallelWarningContext::Close => "",
    };

    renderer.emit_warning(format_args!("{}: {}.{}", prefix, list, guidance));
}

fn warn_about_future_start(
    renderer: &OutputRenderer,
    notifications_enabled: bool,
    start_instant: DateTime<Utc>,
    force: bool,
) {
    if !notifications_enabled || force {
        return;
    }

    let now = Utc::now();
    if start_instant > now + Duration::hours(12) {
        renderer.emit_warning(format_args!(
            "The requested start time {} is more than 12 hours in the future; pass --force to proceed.",
            start_instant.to_rfc3339()
        ))
    }
}

fn warn_about_overdue_start(
    renderer: &OutputRenderer,
    notifications_enabled: bool,
    record: &SprintRecord,
    lifecycle: &SprintLifecycleStatus,
) {
    if !notifications_enabled {
        return;
    }

    if lifecycle.actual_start.is_some() {
        return;
    }

    if let Some(planned_start) = lifecycle.planned_start
        && Utc::now() > planned_start
    {
        renderer.emit_warning(format_args!(
            "Sprint #{} ({}) was scheduled to start at {} and is now overdue to begin.",
            record.id,
            sprint_assignment::sprint_display_name(record),
            planned_start.to_rfc3339()
        ));
    }
}

fn warn_about_overdue_close(
    renderer: &OutputRenderer,
    notifications_enabled: bool,
    record: &SprintRecord,
    lifecycle: &SprintLifecycleStatus,
    close_instant: DateTime<Utc>,
) {
    if !notifications_enabled {
        return;
    }

    if let Some(computed_end) = lifecycle.computed_end
        && close_instant > computed_end
    {
        renderer.emit_warning(format_args!(
            "Sprint #{} ({}) was scheduled to end by {} and is overdue to close.",
            record.id,
            sprint_assignment::sprint_display_name(record),
            computed_end.to_rfc3339()
        ));
    }
}

fn apply_start_to_sprint(sprint: &mut Sprint, start_instant: DateTime<Utc>, force: bool) {
    let actual = sprint.actual.get_or_insert_with(SprintActual::default);
    if actual.started_at.is_some() && !force {
        return;
    }
    actual.started_at = Some(start_instant.to_rfc3339());
}

fn apply_close_to_sprint(sprint: &mut Sprint, close_instant: DateTime<Utc>, _force: bool) {
    let actual = sprint.actual.get_or_insert_with(SprintActual::default);
    actual.closed_at = Some(close_instant.to_rfc3339());
}

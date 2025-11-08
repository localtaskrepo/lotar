use crate::cli::args::sprint::{
    SprintAction, SprintAddArgs, SprintArgs, SprintBacklogArgs, SprintBurndownArgs,
    SprintCalendarArgs, SprintCleanupRefsArgs, SprintCloseArgs, SprintCreateArgs, SprintDeleteArgs,
    SprintListArgs, SprintMoveArgs, SprintNormalizeArgs, SprintRemoveArgs, SprintReviewArgs,
    SprintShowArgs, SprintStartArgs, SprintStatsArgs, SprintSummaryArgs, SprintUpdateArgs,
    SprintVelocityArgs,
};
use crate::cli::handlers::{CommandHandler, emit_subcommand_overview};
use crate::config::types::ResolvedConfig;
use crate::output::{OutputFormat, OutputRenderer};
use crate::services::sprint_analytics::{SprintDetail, SprintSummary};
use crate::services::sprint_assignment::{self, SprintBacklogEntry};
use crate::services::sprint_integrity::{self, MissingSprintReport, SprintCleanupOutcome};
use crate::services::sprint_metrics::SprintBurndownMetric;
use crate::services::sprint_reports::{
    SprintBurndownContext, SprintCalendarContext, SprintReviewContext, SprintStatsContext,
    SprintSummaryContext, compute_sprint_burndown, compute_sprint_calendar, compute_sprint_review,
    compute_sprint_stats, compute_sprint_summary,
};
use crate::services::sprint_service::{SprintRecord, SprintService};
use crate::services::sprint_status::{self, SprintLifecycleState, SprintLifecycleStatus};
use crate::services::sprint_velocity::{
    DEFAULT_VELOCITY_WINDOW, VelocityComputation, VelocityOptions, compute_velocity,
};
use crate::storage::manager::Storage;
use crate::storage::sprint::{Sprint, SprintActual, SprintCapacity, SprintPlan};
use crate::types::TaskStatus;
use crate::utils::time;
use crate::workspace::TasksDirectoryResolver;
use chrono::{DateTime, Duration, Timelike, Utc};
use serde::Serialize;
use std::collections::HashSet;
use std::fs;
use std::io::{self, Write};
use std::path::PathBuf;

pub struct SprintHandler;

const MIN_ID_COL_WIDTH: usize = 4;
const MIN_STATUS_COL_WIDTH: usize = 11; // fits [complete!]
const MIN_LABEL_COL_WIDTH: usize = 18;
const MIN_WINDOW_COL_WIDTH: usize = 28;
const MIN_GOAL_COL_WIDTH: usize = 24;

const MAX_LABEL_COL_WIDTH: usize = 40;
const MAX_WINDOW_COL_WIDTH: usize = 42;
const MAX_GOAL_COL_WIDTH: usize = 48;

const COLUMN_GAP_COUNT: usize = 4; // spaces between the five table columns
const MIN_TABLE_TOTAL_WIDTH: usize = MIN_ID_COL_WIDTH
    + MIN_STATUS_COL_WIDTH
    + MIN_LABEL_COL_WIDTH
    + MIN_WINDOW_COL_WIDTH
    + MIN_GOAL_COL_WIDTH
    + COLUMN_GAP_COUNT;

impl CommandHandler for SprintHandler {
    type Args = SprintArgs;
    type Result = Result<(), String>;

    fn execute(
        args: Self::Args,
        _project: Option<&str>,
        resolver: &TasksDirectoryResolver,
        renderer: &OutputRenderer,
    ) -> Self::Result {
        let tasks_root = resolver.path.clone();
        let Some(action) = args.action else {
            emit_subcommand_overview(renderer, &["sprint"]);
            return Ok(());
        };

        match action {
            SprintAction::Create(create_args) => {
                handle_create(create_args, tasks_root.clone(), renderer)
            }
            SprintAction::Update(update_args) => {
                handle_update(update_args, tasks_root.clone(), renderer)
            }
            SprintAction::Start(start_args) => {
                handle_start(start_args, tasks_root.clone(), renderer)
            }
            SprintAction::Close(close_args) => {
                handle_close(close_args, tasks_root.clone(), renderer)
            }
            SprintAction::List(list_args) => handle_list(list_args, tasks_root.clone(), renderer),
            SprintAction::Calendar(calendar_args) => {
                handle_calendar(calendar_args, tasks_root.clone(), renderer)
            }
            SprintAction::Velocity(velocity_args) => {
                handle_velocity(velocity_args, tasks_root.clone(), renderer)
            }
            SprintAction::Show(show_args) => handle_show(show_args, tasks_root.clone(), renderer),
            SprintAction::Review(review_args) => {
                handle_review(review_args, tasks_root.clone(), renderer)
            }
            SprintAction::Stats(stats_args) => {
                handle_stats(stats_args, tasks_root.clone(), renderer)
            }
            SprintAction::Summary(summary_args) => {
                handle_summary(summary_args, tasks_root.clone(), renderer)
            }
            SprintAction::Burndown(burndown_args) => {
                handle_burndown(burndown_args, tasks_root.clone(), renderer)
            }
            SprintAction::CleanupRefs(cleanup_args) => {
                handle_cleanup_refs(cleanup_args, tasks_root.clone(), renderer)
            }
            SprintAction::Normalize(normalize_args) => {
                handle_normalize(normalize_args, tasks_root.clone(), renderer)
            }
            SprintAction::Add(add_args) => handle_add(add_args, tasks_root.clone(), renderer),
            SprintAction::Move(move_args) => handle_move(move_args, tasks_root.clone(), renderer),
            SprintAction::Remove(remove_args) => {
                handle_remove(remove_args, tasks_root.clone(), renderer)
            }
            SprintAction::Delete(delete_args) => {
                handle_delete(delete_args, tasks_root.clone(), renderer)
            }
            SprintAction::Backlog(backlog_args) => {
                handle_backlog(backlog_args, tasks_root.clone(), renderer)
            }
        }
    }
}

fn handle_create(
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

fn handle_update(
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

fn handle_start(
    start_args: SprintStartArgs,
    tasks_root: PathBuf,
    renderer: &OutputRenderer,
) -> Result<(), String> {
    let mut storage = Storage::new(tasks_root.clone());

    let resolved_config =
        crate::config::resolution::load_and_merge_configs(Some(tasks_root.as_path()))
            .map_err(|err| err.to_string())?;

    let start_instant = determine_start_timestamp(&start_args)?;

    let records = SprintService::list(&storage).map_err(|err| err.to_string())?;
    if records.is_empty() {
        return Err("No sprints found. Create one before starting.".to_string());
    }

    let target_id = match start_args.sprint_id {
        Some(id) => id,
        None => select_sprint_id_for_start(&records, start_instant)
            .ok_or_else(|| "No pending sprints ready to start.".to_string())?,
    };

    if start_args.sprint_id.is_none() && !matches!(renderer.format, OutputFormat::Json) {
        renderer.emit_info(&format!("Auto-selected sprint #{} to start.", target_id));
    }

    let existing = SprintService::get(&storage, target_id).map_err(|err| err.to_string())?;
    let lifecycle = sprint_status::derive_status(&existing.sprint, start_instant);

    if lifecycle.actual_end.is_some() && !start_args.force {
        return Err(
            "Sprint is already closed. Clear actual.closed_at or pass --force to restart."
                .to_string(),
        );
    }

    if lifecycle.actual_start.is_some() && !start_args.force {
        return Err(
            "Sprint already has actual.started_at; use --force to override the recorded start time."
                .to_string(),
        );
    }

    let mut sprint = existing.sprint.clone();

    let warnings_enabled = resolved_config.sprint_notifications.enabled && !start_args.no_warn;

    warn_about_overdue_start(renderer, warnings_enabled, &existing, &lifecycle);
    warn_about_future_start(renderer, warnings_enabled, start_instant, start_args.force);

    apply_start_to_sprint(&mut sprint, start_instant, start_args.force);

    warn_about_parallel_active_sprints(
        renderer,
        warnings_enabled,
        &records,
        target_id,
        start_instant,
        ParallelWarningContext::Start,
    );

    let outcome = SprintService::update(&mut storage, target_id, sprint)
        .map_err(|err| format!("Failed to start sprint: {}", err))?;

    render_operation_response(
        SprintOperationKind::Start,
        outcome.record,
        outcome.warnings,
        outcome.applied_defaults,
        renderer,
        warnings_enabled,
        &resolved_config,
    );

    Ok(())
}

fn handle_close(
    close_args: SprintCloseArgs,
    tasks_root: PathBuf,
    renderer: &OutputRenderer,
) -> Result<(), String> {
    let mut storage = Storage::new(tasks_root.clone());

    let resolved_config =
        crate::config::resolution::load_and_merge_configs(Some(tasks_root.as_path()))
            .map_err(|err| err.to_string())?;

    let close_instant = determine_close_timestamp(&close_args)?;

    let records = SprintService::list(&storage).map_err(|err| err.to_string())?;
    if records.is_empty() {
        return Err("No sprints found. Create one before closing.".to_string());
    }

    let target_id = match close_args.sprint_id {
        Some(id) => id,
        None => select_sprint_id_for_close(&records)
            .ok_or_else(|| "No active sprints ready to close.".to_string())?,
    };

    if close_args.sprint_id.is_none() && !matches!(&renderer.format, OutputFormat::Json) {
        renderer.emit_info(&format!("Auto-selected sprint #{} to close.", target_id));
    }

    let existing = SprintService::get(&storage, target_id).map_err(|err| err.to_string())?;
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

    let warnings_enabled = resolved_config.sprint_notifications.enabled && !close_args.no_warn;

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
        &records,
        target_id,
        close_instant,
        ParallelWarningContext::Close,
    );

    let outcome = SprintService::update(&mut storage, target_id, sprint)
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
        &resolved_config,
    );

    if let Some(record) = review_record.as_ref() {
        render_sprint_review(&storage, record, &resolved_config, renderer);
    }

    Ok(())
}

fn handle_list(
    list_args: SprintListArgs,
    tasks_root: PathBuf,
    renderer: &OutputRenderer,
) -> Result<(), String> {
    if let Some(limit) = list_args.limit {
        if limit == 0 {
            return Err("--limit must be greater than zero".to_string());
        }
    }

    let mut storage_opt = Storage::try_open(tasks_root.clone());
    let mut records: Vec<SprintRecord> = Vec::new();
    let mut integrity = MissingSprintReport::default();
    let mut baseline_integrity = integrity.clone();
    let mut cleanup_summary: Option<SprintCleanupOutcome> = None;

    if let Some(storage) = storage_opt.as_ref() {
        records = SprintService::list(storage).map_err(|err| err.to_string())?;
        integrity = sprint_integrity::detect_missing_sprints(storage, &records);
        baseline_integrity = integrity.clone();
    }

    if let Some(storage) = storage_opt.as_mut() {
        if !integrity.missing_sprints.is_empty() && list_args.cleanup_missing {
            let outcome =
                sprint_integrity::cleanup_missing_sprint_refs(storage, &mut records, None)
                    .map_err(|err| err.to_string())?;
            emit_cleanup_summary(renderer, &outcome, "listing sprints");
            integrity = sprint_integrity::detect_missing_sprints(storage, &records);
            cleanup_summary = Some(outcome);
        }
    } else if list_args.cleanup_missing {
        renderer.emit_warning(
            "Cannot clean up missing sprint references: no tasks workspace detected.",
        );
    }

    if !list_args.cleanup_missing && !integrity.missing_sprints.is_empty() {
        emit_missing_report(renderer, &integrity, "listing sprints");
    }

    if records.is_empty() {
        match renderer.format {
            OutputFormat::Json => {
                let payload = SprintListPayload {
                    status: "ok",
                    count: 0,
                    truncated: false,
                    sprints: Vec::new(),
                    missing_sprints: integrity.missing_sprints.clone(),
                    integrity: build_assignment_integrity(
                        &baseline_integrity,
                        &integrity,
                        cleanup_summary.as_ref(),
                    ),
                };
                renderer.emit_raw_stdout(&serde_json::to_string(&payload).unwrap_or_default());
            }
            _ => renderer.emit_success("No sprints found."),
        }
        return Ok(());
    }

    let mut truncated = false;
    if let Some(limit) = list_args.limit {
        if records.len() > limit {
            records.truncate(limit);
            truncated = true;
        }
    }

    let now = Utc::now();
    let summaries: Vec<SprintSummary> = records
        .iter()
        .map(|record| {
            let lifecycle = sprint_status::derive_status(&record.sprint, now);
            SprintSummary::from_record(record, &lifecycle)
        })
        .collect();

    match renderer.format {
        OutputFormat::Json => {
            let payload = SprintListPayload {
                status: "ok",
                count: summaries.len(),
                truncated,
                sprints: summaries,
                missing_sprints: integrity.missing_sprints.clone(),
                integrity: build_assignment_integrity(
                    &baseline_integrity,
                    &integrity,
                    cleanup_summary.as_ref(),
                ),
            };
            renderer.emit_raw_stdout(&serde_json::to_string(&payload).unwrap_or_default());
        }
        _ => {
            let rows: Vec<PreparedSummaryRow> = summaries
                .iter()
                .map(PreparedSummaryRow::from_summary)
                .collect();
            let widths = compute_column_widths(&rows);

            renderer.emit_raw_stdout(&format_table_header(&widths));
            let separator = "-".repeat(widths.total());
            renderer.emit_raw_stdout(&separator);
            for row in &rows {
                renderer.emit_raw_stdout(&format_summary_row(row, &widths));
            }
            if truncated {
                renderer.emit_warning(
                    "List truncated by --limit; rerun without --limit for the complete list.",
                );
            }
        }
    }

    Ok(())
}

fn handle_calendar(
    calendar_args: SprintCalendarArgs,
    tasks_root: PathBuf,
    renderer: &OutputRenderer,
) -> Result<(), String> {
    if let Some(limit) = calendar_args.limit {
        if limit == 0 {
            return Err("--limit must be greater than zero".to_string());
        }
    }

    let storage = Storage::try_open(tasks_root)
        .ok_or_else(|| "No sprints found. Create one before running calendar.".to_string())?;

    let records = SprintService::list(&storage).map_err(|err| err.to_string())?;
    let context = compute_sprint_calendar(
        &records,
        calendar_args.include_complete,
        calendar_args.limit,
        Utc::now(),
    );

    match renderer.format {
        OutputFormat::Json => {
            let mut value = serde_json::to_value(&context.payload).unwrap_or_default();
            if let Some(obj) = value.as_object_mut() {
                if obj.get("skipped_complete").and_then(|flag| flag.as_bool()) == Some(false) {
                    obj.remove("skipped_complete");
                }
            }
            renderer.emit_raw_stdout(&value.to_string());
        }
        _ => render_calendar_text(renderer, &context, calendar_args.include_complete),
    }

    Ok(())
}

fn handle_velocity(
    velocity_args: SprintVelocityArgs,
    tasks_root: PathBuf,
    renderer: &OutputRenderer,
) -> Result<(), String> {
    let limit = match velocity_args.limit {
        Some(0) => return Err("--limit must be greater than zero".to_string()),
        Some(value) => value,
        None => DEFAULT_VELOCITY_WINDOW,
    };

    let config_root = tasks_root.clone();
    let storage = Storage::try_open(tasks_root)
        .ok_or_else(|| "No sprints found. Create one before running velocity.".to_string())?;

    let resolved_config =
        crate::config::resolution::load_and_merge_configs(Some(config_root.as_path()))
            .map_err(|err| err.to_string())?;

    let records = SprintService::list(&storage).map_err(|err| err.to_string())?;
    let had_records = !records.is_empty();

    let options = VelocityOptions {
        limit,
        include_active: velocity_args.include_active,
        metric: velocity_args.metric,
    };

    let computation = compute_velocity(&storage, records, &resolved_config, options, Utc::now());

    match renderer.format {
        OutputFormat::Json => {
            let payload = computation.to_payload(velocity_args.include_active);
            renderer.emit_raw_stdout(&serde_json::to_string(&payload).unwrap_or_default());
        }
        _ => render_velocity_text(
            renderer,
            &computation,
            had_records,
            velocity_args.include_active,
        ),
    }

    Ok(())
}

fn render_velocity_text(
    renderer: &OutputRenderer,
    computation: &VelocityComputation,
    had_records: bool,
    include_active: bool,
) {
    if computation.entries.is_empty() && !had_records {
        renderer.emit_success("No sprints found.");
        return;
    }

    let metric = metric_label(computation.metric);
    renderer.emit_success(&format!(
        "Sprint velocity ({} metric, showing {} of {} sprint{}).",
        metric,
        computation.entries.len(),
        computation.total_matching,
        if computation.total_matching == 1 {
            ""
        } else {
            "s"
        }
    ));
    renderer.emit_raw_stdout(
        "ID   Label                 Closed      Committed  Completed  % Done  Capacity  Relative",
    );
    renderer.emit_raw_stdout(
        "-------------------------------------------------------------------------------------------",
    );

    for entry in &computation.entries {
        let label = entry
            .summary
            .label
            .clone()
            .unwrap_or_else(|| format!("Sprint {}", entry.sprint_id));
        let closed_display = entry
            .actual_end
            .or(entry.end)
            .map(|dt| dt.format("%Y-%m-%d").to_string())
            .unwrap_or_else(|| "-".to_string());
        let committed_display = format_velocity_value(computation.metric, entry.committed);
        let completed_display = format_velocity_value(computation.metric, entry.completed);
        let percentage_display = format_percentage(entry.completion_ratio);
        let capacity_display = entry
            .capacity
            .map(|cap| format_velocity_value(computation.metric, cap))
            .unwrap_or_else(|| "-".to_string());
        renderer.emit_raw_stdout(&format!(
            "#{:>3} {:<20} {:<11} {:>10} {:>10} {:>7} {:>9} {}",
            entry.sprint_id,
            truncate(&label, 20),
            closed_display,
            committed_display,
            completed_display,
            percentage_display,
            capacity_display,
            entry.relative
        ));
    }

    if let Some(avg_velocity) = computation.average_velocity {
        renderer.emit_raw_stdout(&format!(
            "Average completed: {} {}",
            format_velocity_value(computation.metric, avg_velocity),
            metric
        ));
    }

    if let Some(avg_ratio) = computation.average_completion_ratio {
        renderer.emit_raw_stdout(&format!(
            "Average completion ratio: {}",
            format_percentage(Some(avg_ratio))
        ));
    }

    if computation.truncated {
        renderer.emit_warning(
            "Velocity window truncated by --limit; rerun with a larger limit for additional history.",
        );
    }

    if computation.skipped_incomplete && !include_active {
        renderer.emit_info(
            "Active or pending sprints omitted; re-run with --include-active to include them.",
        );
    }

    for entry in &computation.entries {
        if entry.summary.has_warnings && !entry.warnings.is_empty() {
            for warning in &entry.warnings {
                renderer.emit_warning(&format!("Sprint #{}: {}", entry.sprint_id, warning.message));
            }
        }
    }
}

fn handle_show(
    show_args: SprintShowArgs,
    tasks_root: PathBuf,
    renderer: &OutputRenderer,
) -> Result<(), String> {
    let storage = Storage::try_open(tasks_root)
        .ok_or_else(|| "No sprints found. Create one before showing details.".to_string())?;

    let record =
        SprintService::get(&storage, show_args.sprint_id).map_err(|err| err.to_string())?;

    let lifecycle = sprint_status::derive_status(&record.sprint, Utc::now());
    let summary = SprintSummary::from_record(&record, &lifecycle);

    match renderer.format {
        OutputFormat::Json => {
            let detail = SprintDetail::from_record(&record, &summary, &lifecycle);
            let payload = serde_json::json!({
                "status": "ok",
                "sprint": detail,
            });
            renderer.emit_raw_stdout(&payload.to_string());
        }
        _ => {
            renderer.emit_raw_stdout(&format!(
                "Sprint {} - {} [{}]",
                record.id,
                summary
                    .label
                    .clone()
                    .unwrap_or_else(|| format!("Sprint {}", record.id)),
                summary.status
            ));

            if let Some(goal) = &summary.goal {
                renderer.emit_raw_stdout(&format!("Goal: {}", goal));
            }

            if let Some(plan) = record.sprint.plan.as_ref() {
                emit_plan(renderer, plan);
            }

            if let Some(actual) = record.sprint.actual.as_ref() {
                emit_actual(renderer, actual);
            }

            if let Some(computed_end) = lifecycle.computed_end.as_ref() {
                renderer.emit_raw_stdout(&format!(
                    "Computed end (inferred): {}",
                    computed_end.to_rfc3339()
                ));
            }

            if !record.sprint.history.is_empty() {
                renderer
                    .emit_raw_stdout(&format!("History entries: {}", record.sprint.history.len()));
            }

            if !lifecycle.warnings.is_empty() {
                for warning in &lifecycle.warnings {
                    renderer.emit_warning(&warning.message());
                }
            }
        }
    }

    Ok(())
}

fn handle_review(
    review_args: SprintReviewArgs,
    tasks_root: PathBuf,
    renderer: &OutputRenderer,
) -> Result<(), String> {
    let config_root = tasks_root.clone();
    let storage = Storage::try_open(tasks_root)
        .ok_or_else(|| "No sprints found. Create one before running a review.".to_string())?;

    let resolved_config =
        crate::config::resolution::load_and_merge_configs(Some(config_root.as_path()))
            .map_err(|err| err.to_string())?;

    let records = SprintService::list(&storage).map_err(|err| err.to_string())?;
    if records.is_empty() {
        return Err("No sprints found. Create one before running a review.".to_string());
    }

    let target_id = match review_args.sprint_id {
        Some(id) => id,
        None => select_sprint_id_for_review(&records)
            .ok_or_else(|| "No sprints available for review.".to_string())?,
    };

    if review_args.sprint_id.is_none() && !matches!(renderer.format, OutputFormat::Json) {
        renderer.emit_info(&format!("Auto-selected sprint #{} for review.", target_id));
    }

    let record = SprintService::get(&storage, target_id).map_err(|err| err.to_string())?;
    render_sprint_review(&storage, &record, &resolved_config, renderer);

    Ok(())
}

fn handle_stats(
    stats_args: SprintStatsArgs,
    tasks_root: PathBuf,
    renderer: &OutputRenderer,
) -> Result<(), String> {
    let config_root = tasks_root.clone();
    let storage = Storage::try_open(tasks_root)
        .ok_or_else(|| "No sprints found. Create one before running stats.".to_string())?;

    let resolved_config =
        crate::config::resolution::load_and_merge_configs(Some(config_root.as_path()))
            .map_err(|err| err.to_string())?;

    let records = SprintService::list(&storage).map_err(|err| err.to_string())?;
    if records.is_empty() {
        return Err("No sprints found. Create one before running stats.".to_string());
    }

    let target_id = match stats_args.sprint_id {
        Some(id) => id,
        None => select_sprint_id_for_review(&records)
            .ok_or_else(|| "No sprints available for stats.".to_string())?,
    };

    if stats_args.sprint_id.is_none() && !matches!(renderer.format, OutputFormat::Json) {
        renderer.emit_info(&format!("Auto-selected sprint #{} for stats.", target_id));
    }

    let record = SprintService::get(&storage, target_id).map_err(|err| err.to_string())?;
    render_sprint_stats(&storage, &record, &resolved_config, renderer);

    Ok(())
}

fn handle_summary(
    summary_args: SprintSummaryArgs,
    tasks_root: PathBuf,
    renderer: &OutputRenderer,
) -> Result<(), String> {
    let config_root = tasks_root.clone();
    let storage = Storage::try_open(tasks_root)
        .ok_or_else(|| "No sprints found. Create one before running summary.".to_string())?;

    let resolved_config =
        crate::config::resolution::load_and_merge_configs(Some(config_root.as_path()))
            .map_err(|err| err.to_string())?;

    let records = SprintService::list(&storage).map_err(|err| err.to_string())?;
    if records.is_empty() {
        return Err("No sprints found. Create one before running summary.".to_string());
    }

    let target_id = match summary_args.sprint_id {
        Some(id) => id,
        None => select_sprint_id_for_review(&records)
            .ok_or_else(|| "No sprints available for summary.".to_string())?,
    };

    if summary_args.sprint_id.is_none() && !matches!(renderer.format, OutputFormat::Json) {
        renderer.emit_info(&format!("Auto-selected sprint #{} for summary.", target_id));
    }

    let record = SprintService::get(&storage, target_id).map_err(|err| err.to_string())?;
    let context = compute_sprint_summary(&storage, &record, &resolved_config, Utc::now());

    match renderer.format {
        OutputFormat::Json => {
            renderer.emit_raw_stdout(&serde_json::to_string(&context.payload).unwrap_or_default());
        }
        _ => render_summary_text(renderer, &context),
    }

    Ok(())
}

fn handle_burndown(
    burndown_args: SprintBurndownArgs,
    tasks_root: PathBuf,
    renderer: &OutputRenderer,
) -> Result<(), String> {
    let config_root = tasks_root.clone();
    let storage = Storage::try_open(tasks_root)
        .ok_or_else(|| "No sprints found. Create one before running burndown.".to_string())?;

    let resolved_config =
        crate::config::resolution::load_and_merge_configs(Some(config_root.as_path()))
            .map_err(|err| err.to_string())?;

    let records = SprintService::list(&storage).map_err(|err| err.to_string())?;
    if records.is_empty() {
        return Err("No sprints found. Create one before running burndown.".to_string());
    }

    let target_id = match burndown_args.sprint_id {
        Some(id) => id,
        None => select_sprint_id_for_review(&records)
            .ok_or_else(|| "No sprints available for burndown.".to_string())?,
    };

    if burndown_args.sprint_id.is_none() && !matches!(renderer.format, OutputFormat::Json) {
        renderer.emit_info(&format!(
            "Auto-selected sprint #{} for burndown.",
            target_id
        ));
    }

    let record = SprintService::get(&storage, target_id).map_err(|err| err.to_string())?;
    let context = compute_sprint_burndown(&storage, &record, &resolved_config, Utc::now())?;

    match renderer.format {
        OutputFormat::Json => {
            renderer.emit_raw_stdout(&serde_json::to_string(&context.payload).unwrap_or_default());
        }
        _ => render_burndown_text(renderer, &context, burndown_args.metric),
    }

    Ok(())
}

fn handle_cleanup_refs(
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

    if let Some(id) = cleanup_args.sprint_id {
        if records.iter().any(|record| record.id == id)
            && !matches!(renderer.format, OutputFormat::Json)
        {
            renderer.emit_warning(&format!(
                "Sprint #{} still exists; removing references per request.",
                id
            ));
        }
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
            renderer.emit_raw_stdout(&serde_json::to_string(&payload).unwrap_or_default());
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
                renderer.emit_success(&format!(
                    "Removed {} sprint reference(s) across {} task(s).",
                    outcome.removed_references, outcome.updated_tasks
                ));
                for metric in &cleanup_payload.removed_by_sprint {
                    renderer.emit_info(&format!(
                        "Sprint #{}: removed {} reference(s).",
                        metric.sprint_id, metric.count
                    ));
                }

                if cleanup_args.sprint_id.is_none() && !outcome.remaining_missing.is_empty() {
                    let formatted = format_missing_ids(&outcome.remaining_missing);
                    renderer.emit_warning(&format!(
                        "Additional missing sprint IDs still referenced: {}",
                        formatted
                    ));
                }
            }
        }
    }

    Ok(())
}

fn cleanup_summary_payload(outcome: &SprintCleanupOutcome) -> SprintCleanupSummaryPayload {
    SprintCleanupSummaryPayload {
        removed_references: outcome.removed_references,
        updated_tasks: outcome.updated_tasks,
        removed_by_sprint: outcome
            .removed_by_sprint
            .iter()
            .map(|entry| SprintCleanupRefsRemoved {
                sprint_id: entry.sprint_id,
                count: entry.count,
            })
            .collect(),
        remaining_missing: outcome.remaining_missing.clone(),
    }
}

fn emit_cleanup_summary(renderer: &OutputRenderer, outcome: &SprintCleanupOutcome, context: &str) {
    if outcome.removed_references == 0 {
        renderer.emit_info(&format!(
            "No missing sprint references were removed while {}.",
            context
        ));
        return;
    }

    renderer.emit_info(&format!(
        "Removed {} missing sprint reference(s) while {}.",
        outcome.removed_references, context
    ));

    if !outcome.removed_by_sprint.is_empty() {
        renderer.emit_info("Removed references by sprint:");
        for entry in &outcome.removed_by_sprint {
            renderer.emit_info(&format!("  - Sprint #{}: {}", entry.sprint_id, entry.count));
        }
    }

    if !outcome.remaining_missing.is_empty() {
        let formatted = format_missing_ids(&outcome.remaining_missing);
        renderer.emit_warning(&format!(
            "Remaining missing sprint references detected: {}.",
            formatted
        ));
    }
}

fn emit_missing_report(renderer: &OutputRenderer, report: &MissingSprintReport, context: &str) {
    if report.missing_sprints.is_empty() {
        return;
    }

    let formatted = format_missing_ids(&report.missing_sprints);
    renderer.emit_warning(&format!(
        "Missing sprint references detected while {}: {}.",
        context, formatted
    ));

    if report.tasks_with_missing > 0 {
        renderer.emit_info(&format!(
            "{} task(s) currently reference missing sprints.",
            report.tasks_with_missing
        ));
    }

    renderer.emit_info(
        "Re-run with --cleanup-missing to remove stale sprint memberships automatically.",
    );
}

fn build_assignment_integrity(
    baseline: &MissingSprintReport,
    current: &MissingSprintReport,
    cleanup: Option<&SprintCleanupOutcome>,
) -> Option<SprintAssignmentIntegrityPayload> {
    let missing_sprints = current.missing_sprints.clone();
    let tasks_with_missing = baseline.tasks_with_missing.max(current.tasks_with_missing);

    let auto_cleanup = cleanup.map(cleanup_summary_payload);

    if missing_sprints.is_empty()
        && tasks_with_missing == 0
        && auto_cleanup
            .as_ref()
            .map(|summary| summary.removed_references == 0 && summary.remaining_missing.is_empty())
            .unwrap_or(true)
    {
        return None;
    }

    Some(SprintAssignmentIntegrityPayload {
        missing_sprints,
        tasks_with_missing: (tasks_with_missing > 0).then_some(tasks_with_missing),
        auto_cleanup,
    })
}

fn format_missing_ids(ids: &[u32]) -> String {
    ids.iter()
        .map(|id| format!("#{}", id))
        .collect::<Vec<_>>()
        .join(", ")
}

#[derive(Clone, Copy, Debug)]
struct ColumnWidths {
    id: usize,
    status: usize,
    label: usize,
    window: usize,
    goal: usize,
}

impl ColumnWidths {
    fn total(&self) -> usize {
        self.id + self.status + self.label + self.window + self.goal + COLUMN_GAP_COUNT
    }
}

impl Default for ColumnWidths {
    fn default() -> Self {
        Self {
            id: MIN_ID_COL_WIDTH,
            status: MIN_STATUS_COL_WIDTH,
            label: MIN_LABEL_COL_WIDTH,
            window: MIN_WINDOW_COL_WIDTH,
            goal: MIN_GOAL_COL_WIDTH,
        }
    }
}

struct PreparedSummaryRow {
    id: String,
    status: String,
    label: String,
    window: String,
    goal: String,
}

impl PreparedSummaryRow {
    fn from_summary(summary: &SprintSummary) -> Self {
        Self {
            id: format!("#{}", summary.id),
            status: build_status_display(summary),
            label: summary
                .label
                .clone()
                .unwrap_or_else(|| format!("Sprint {}", summary.id)),
            window: build_window_display(summary),
            goal: summary
                .goal
                .as_deref()
                .filter(|goal| !goal.trim().is_empty())
                .unwrap_or("-")
                .to_string(),
        }
    }
}

fn compute_column_widths(rows: &[PreparedSummaryRow]) -> ColumnWidths {
    let mut widths = ColumnWidths::default();

    widths.id = widths
        .id
        .max("ID".len())
        .max(rows.iter().map(|row| row.id.len()).max().unwrap_or(0));

    let status_target = rows.iter().map(|row| row.status.len()).max().unwrap_or(0);
    widths.status = widths
        .status
        .max("Status".len())
        .max(status_target)
        .max(max_possible_status_display_width());

    let label_target = rows.iter().map(|row| row.label.len()).max().unwrap_or(0);
    widths.label = widths
        .label
        .max("Label".len())
        .max(label_target)
        .min(MAX_LABEL_COL_WIDTH);

    let window_target = rows.iter().map(|row| row.window.len()).max().unwrap_or(0);
    widths.window = widths
        .window
        .max("Window".len())
        .max(window_target)
        .min(MAX_WINDOW_COL_WIDTH);

    let goal_target = rows.iter().map(|row| row.goal.len()).max().unwrap_or(0);
    widths.goal = widths
        .goal
        .max("Goal".len())
        .max(goal_target)
        .min(MAX_GOAL_COL_WIDTH);

    adjust_column_widths_for_terminal(widths)
}

fn adjust_column_widths_for_terminal(mut widths: ColumnWidths) -> ColumnWidths {
    let Some(columns) = detect_terminal_width() else {
        return widths;
    };

    if columns < MIN_TABLE_TOTAL_WIDTH {
        return widths;
    }

    if columns > widths.total() {
        let mut extra = columns - widths.total();

        let label_cap = MAX_LABEL_COL_WIDTH.saturating_sub(widths.label);
        let label_add = extra.min(label_cap);
        widths.label += label_add;
        extra -= label_add;

        if extra > 0 {
            let goal_cap = MAX_GOAL_COL_WIDTH.saturating_sub(widths.goal);
            let goal_add = extra.min(goal_cap);
            widths.goal += goal_add;
            extra -= goal_add;
        }

        if extra > 0 {
            let window_cap = MAX_WINDOW_COL_WIDTH.saturating_sub(widths.window);
            let window_add = extra.min(window_cap);
            widths.window += window_add;
        }
    } else if columns < widths.total() {
        let mut deficit = widths.total() - columns;

        if deficit > 0 {
            let goal_room = widths.goal.saturating_sub(MIN_GOAL_COL_WIDTH);
            let goal_cut = deficit.min(goal_room);
            widths.goal -= goal_cut;
            deficit -= goal_cut;
        }

        if deficit > 0 {
            let label_room = widths.label.saturating_sub(MIN_LABEL_COL_WIDTH);
            let label_cut = deficit.min(label_room);
            widths.label -= label_cut;
            deficit -= label_cut;
        }

        if deficit > 0 {
            let window_room = widths.window.saturating_sub(MIN_WINDOW_COL_WIDTH);
            let window_cut = deficit.min(window_room);
            widths.window -= window_cut;
        }
    }

    widths
}

fn detect_terminal_width() -> Option<usize> {
    std::env::var("COLUMNS")
        .ok()
        .and_then(|raw| raw.parse::<usize>().ok())
        .filter(|&cols| cols >= MIN_TABLE_TOTAL_WIDTH)
}

fn max_possible_status_display_width() -> usize {
    use SprintLifecycleState::*;

    [Pending, Active, Overdue, Complete]
        .into_iter()
        .map(|state| state.as_str().len() + 3) // [] plus optional '!'
        .max()
        .unwrap_or(MIN_STATUS_COL_WIDTH)
}

fn format_table_header(widths: &ColumnWidths) -> String {
    format!(
        "{:>id_width$} {:<status_width$} {:<label_width$} {:<window_width$} {:<goal_width$}",
        "ID",
        "Status",
        "Label",
        "Window",
        "Goal",
        id_width = widths.id,
        status_width = widths.status,
        label_width = widths.label,
        window_width = widths.window,
        goal_width = widths.goal,
    )
}

fn format_summary_row(row: &PreparedSummaryRow, widths: &ColumnWidths) -> String {
    format!(
        "{:>id_width$} {:<status_width$} {:<label_width$} {:<window_width$} {:<goal_width$}",
        truncate(&row.id, widths.id),
        truncate(&row.status, widths.status),
        truncate(&row.label, widths.label),
        truncate(&row.window, widths.window),
        truncate(&row.goal, widths.goal),
        id_width = widths.id,
        status_width = widths.status,
        label_width = widths.label,
        window_width = widths.window,
        goal_width = widths.goal,
    )
}

fn truncate(value: &str, max: usize) -> String {
    if value.len() <= max {
        return value.to_string();
    }

    if max == 0 {
        return String::new();
    }

    if max <= 3 {
        return "...".chars().take(max).collect();
    }

    let mut truncated = value.chars().take(max - 3).collect::<String>();
    truncated.push_str("...");
    truncated
}

fn build_status_display(summary: &SprintSummary) -> String {
    if summary.has_warnings {
        format!("[{}!]", summary.status)
    } else {
        format!("[{}]", summary.status)
    }
}

fn build_window_display(summary: &SprintSummary) -> String {
    let start = summary.starts_at.as_deref().map(format_short_datetime);
    let end_source = summary
        .ends_at
        .as_deref()
        .or(summary.computed_end.as_deref());
    let end = end_source.map(format_short_datetime);

    match (start, end) {
        (Some(start), Some(end)) => format!("{start} -> {end}"),
        (Some(start), None) => format!("{start} -> ??"),
        (None, Some(end)) => format!("?? -> {end}"),
        (None, None) => "-".to_string(),
    }
}

fn format_short_datetime(raw: &str) -> String {
    if raw.trim().is_empty() {
        return "-".to_string();
    }

    match time::parse_human_datetime_to_utc(raw) {
        Ok(dt) => {
            if dt.hour() == 0 && dt.minute() == 0 && dt.second() == 0 {
                dt.format("%b %-d").to_string()
            } else {
                dt.format("%b %-d %H:%M").to_string()
            }
        }
        Err(_) => raw.to_string(),
    }
}
fn handle_add(
    add_args: SprintAddArgs,
    tasks_root: PathBuf,
    renderer: &OutputRenderer,
) -> Result<(), String> {
    let mut storage = Storage::new(tasks_root.clone());

    let mut records = SprintService::list(&storage).map_err(|err| err.to_string())?;
    let mut integrity = sprint_integrity::detect_missing_sprints(&storage, &records);
    let baseline_integrity = integrity.clone();
    let mut cleanup_summary: Option<SprintCleanupOutcome> = None;

    if !integrity.missing_sprints.is_empty() {
        if add_args.cleanup_missing {
            let outcome =
                sprint_integrity::cleanup_missing_sprint_refs(&mut storage, &mut records, None)
                    .map_err(|err| err.to_string())?;
            emit_cleanup_summary(renderer, &outcome, "assigning sprint memberships");
            integrity = sprint_integrity::detect_missing_sprints(&storage, &records);
            cleanup_summary = Some(outcome);
        } else {
            emit_missing_report(renderer, &integrity, "assigning sprint memberships");
        }
    }

    let (explicit, tasks) =
        split_assignment_inputs(&storage, &records, &add_args.sprint, &add_args.items)?;

    let outcome = sprint_assignment::assign_tasks(
        &mut storage,
        &records,
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
                    &baseline_integrity,
                    &integrity,
                    cleanup_summary.as_ref(),
                ),
            };
            renderer.emit_raw_stdout(&serde_json::to_string(&payload).unwrap_or_default());
        }
        _ => {
            renderer.emit_success(&format!(
                "Attached sprint #{} ({}) to {} task(s).",
                outcome.sprint_id,
                outcome.sprint_display_name,
                outcome.modified.len()
            ));
            if !outcome.unchanged.is_empty() {
                renderer.emit_info(&format!(
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

fn handle_move(
    move_args: SprintMoveArgs,
    tasks_root: PathBuf,
    renderer: &OutputRenderer,
) -> Result<(), String> {
    let mut storage = Storage::new(tasks_root.clone());

    let mut records = SprintService::list(&storage).map_err(|err| err.to_string())?;
    let mut integrity = sprint_integrity::detect_missing_sprints(&storage, &records);
    let baseline_integrity = integrity.clone();
    let mut cleanup_summary: Option<SprintCleanupOutcome> = None;

    if !integrity.missing_sprints.is_empty() {
        if move_args.cleanup_missing {
            let outcome =
                sprint_integrity::cleanup_missing_sprint_refs(&mut storage, &mut records, None)
                    .map_err(|err| err.to_string())?;
            emit_cleanup_summary(renderer, &outcome, "moving sprint memberships");
            integrity = sprint_integrity::detect_missing_sprints(&storage, &records);
            cleanup_summary = Some(outcome);
        } else {
            emit_missing_report(renderer, &integrity, "moving sprint memberships");
        }
    }

    let (explicit, tasks) =
        split_assignment_inputs(&storage, &records, &move_args.sprint, &move_args.items)?;

    let outcome = sprint_assignment::assign_tasks(
        &mut storage,
        &records,
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
                    &baseline_integrity,
                    &integrity,
                    cleanup_summary.as_ref(),
                ),
            };
            renderer.emit_raw_stdout(&serde_json::to_string(&payload).unwrap_or_default());
        }
        _ => {
            renderer.emit_success(&format!(
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
                renderer.emit_info(&format!(
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

fn handle_remove(
    remove_args: SprintRemoveArgs,
    tasks_root: PathBuf,
    renderer: &OutputRenderer,
) -> Result<(), String> {
    let mut storage = Storage::new(tasks_root.clone());

    let mut records = SprintService::list(&storage).map_err(|err| err.to_string())?;
    let mut integrity = sprint_integrity::detect_missing_sprints(&storage, &records);
    let baseline_integrity = integrity.clone();
    let mut cleanup_summary: Option<SprintCleanupOutcome> = None;

    if !integrity.missing_sprints.is_empty() {
        if remove_args.cleanup_missing {
            let outcome =
                sprint_integrity::cleanup_missing_sprint_refs(&mut storage, &mut records, None)
                    .map_err(|err| err.to_string())?;
            emit_cleanup_summary(renderer, &outcome, "removing sprint memberships");
            integrity = sprint_integrity::detect_missing_sprints(&storage, &records);
            cleanup_summary = Some(outcome);
        } else {
            emit_missing_report(renderer, &integrity, "removing sprint memberships");
        }
    }

    let (explicit, tasks) =
        split_assignment_inputs(&storage, &records, &remove_args.sprint, &remove_args.items)?;

    let outcome =
        sprint_assignment::remove_tasks(&mut storage, &records, &tasks, explicit.as_deref())?;

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
                    &baseline_integrity,
                    &integrity,
                    cleanup_summary.as_ref(),
                ),
            };
            renderer.emit_raw_stdout(&serde_json::to_string(&payload).unwrap_or_default());
        }
        _ => {
            renderer.emit_success(&format!(
                "Removed sprint #{} ({}) from {} task(s).",
                outcome.sprint_id,
                outcome.sprint_display_name,
                outcome.modified.len()
            ));
            if !outcome.unchanged.is_empty() {
                renderer.emit_info(&format!(
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

fn handle_delete(
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
            renderer.emit_raw_stdout(&serde_json::to_string(&payload).unwrap_or_default());
        }
        _ => {
            renderer.emit_success(&format!("Deleted {}.", display_name));
            if let Some(outcome) = cleanup_outcome.as_ref() {
                emit_cleanup_summary(renderer, outcome, "deleting the sprint");
            } else {
                emit_missing_report(renderer, &integrity_report, "deleting the sprint");
            }
        }
    }

    Ok(())
}

fn handle_backlog(
    backlog_args: SprintBacklogArgs,
    tasks_root: PathBuf,
    renderer: &OutputRenderer,
) -> Result<(), String> {
    if backlog_args.limit == 0 {
        return Err("--limit must be greater than zero".to_string());
    }

    let mut storage = match Storage::try_open(tasks_root.clone()) {
        Some(storage) => storage,
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
                    renderer.emit_raw_stdout(&serde_json::to_string(&payload).unwrap_or_default());
                }
                _ => renderer.emit_success("No backlog tasks found."),
            }
            return Ok(());
        }
    };

    let mut records = SprintService::list(&storage).map_err(|err| err.to_string())?;
    let mut integrity = sprint_integrity::detect_missing_sprints(&storage, &records);
    let baseline_integrity = integrity.clone();
    let mut cleanup_summary: Option<SprintCleanupOutcome> = None;

    if !integrity.missing_sprints.is_empty() {
        if backlog_args.cleanup_missing {
            let outcome =
                sprint_integrity::cleanup_missing_sprint_refs(&mut storage, &mut records, None)
                    .map_err(|err| err.to_string())?;
            emit_cleanup_summary(renderer, &outcome, "loading the sprint backlog");
            integrity = sprint_integrity::detect_missing_sprints(&storage, &records);
            cleanup_summary = Some(outcome);
        } else {
            emit_missing_report(renderer, &integrity, "loading the sprint backlog");
        }
    }

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

    let result = sprint_assignment::fetch_backlog(&storage, options)?;
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
                    missing_sprints: integrity.missing_sprints.clone(),
                    integrity: build_assignment_integrity(
                        &baseline_integrity,
                        &integrity,
                        cleanup_summary.as_ref(),
                    ),
                };
                renderer.emit_raw_stdout(&serde_json::to_string(&payload).unwrap_or_default());
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
                missing_sprints: integrity.missing_sprints.clone(),
                integrity: build_assignment_integrity(
                    &baseline_integrity,
                    &integrity,
                    cleanup_summary.as_ref(),
                ),
            };
            renderer.emit_raw_stdout(&serde_json::to_string(&payload).unwrap_or_default());
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
                renderer.emit_raw_stdout(&format!(
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SprintNormalizeMode {
    Write,
    Check,
}

impl SprintNormalizeMode {
    fn as_label(&self) -> &'static str {
        match self {
            SprintNormalizeMode::Write => "write",
            SprintNormalizeMode::Check => "check",
        }
    }
}

fn handle_normalize(
    normalize_args: SprintNormalizeArgs,
    tasks_root: PathBuf,
    renderer: &OutputRenderer,
) -> Result<(), String> {
    let mode = resolve_normalize_mode(&normalize_args)?;
    let is_json = matches!(renderer.format, OutputFormat::Json);

    let storage = match Storage::try_open(tasks_root.clone()) {
        Some(storage) => storage,
        None => {
            if is_json {
                let payload = SprintNormalizeResponse {
                    status: "ok",
                    mode: mode.as_label(),
                    processed: 0,
                    updated: 0,
                    changes_required: Vec::new(),
                    updated_ids: Vec::new(),
                    warnings: Vec::new(),
                    skipped: Vec::new(),
                };
                renderer.emit_raw_stdout(&serde_json::to_string(&payload).unwrap_or_default());
            } else {
                renderer.emit_info("No sprints found. Nothing to normalize.");
            }
            return Ok(());
        }
    };

    let records = SprintService::list(&storage).map_err(|err| err.to_string())?;
    if records.is_empty() {
        if is_json {
            let payload = SprintNormalizeResponse {
                status: "ok",
                mode: mode.as_label(),
                processed: 0,
                updated: 0,
                changes_required: Vec::new(),
                updated_ids: Vec::new(),
                warnings: Vec::new(),
                skipped: Vec::new(),
            };
            renderer.emit_raw_stdout(&serde_json::to_string(&payload).unwrap_or_default());
        } else {
            renderer.emit_info("No sprints found. Nothing to normalize.");
        }
        return Ok(());
    }

    let available: HashSet<u32> = records.iter().map(|record| record.id).collect();
    let target_ids: Vec<u32> = if let Some(requested) = normalize_args.sprint_id {
        if !available.contains(&requested) {
            return Err(format!("Sprint #{} not found.", requested));
        }
        vec![requested]
    } else {
        records.iter().map(|record| record.id).collect()
    };

    let mut processed = 0usize;
    let mut updated = 0usize;
    let mut pending_ids = Vec::new();
    let mut updated_ids = Vec::new();
    let mut warning_payloads = Vec::new();
    let mut skipped = Vec::new();

    for sprint_id in target_ids {
        let path = Sprint::path_for_id(&storage.root_path, sprint_id);
        if !path.exists() {
            let message = format!(
                "Skipping sprint #{}; {} is missing.",
                sprint_id,
                path.display()
            );
            if is_json {
                skipped.push(SprintNormalizeSkipPayload {
                    sprint_id,
                    reason: message,
                });
            } else {
                renderer.emit_warning(&message);
            }
            continue;
        }

        let original = fs::read_to_string(&path)
            .map_err(|err| format!("Failed to read {}: {}", path.display(), err))?;
        let mut sprint: Sprint = serde_yaml::from_str(&original)
            .map_err(|err| format!("Failed to parse sprint #{}: {}", sprint_id, err))?;

        let warnings = sprint.canonicalize();
        let canonical = sprint
            .to_yaml()
            .map_err(|err| format!("Failed to serialize sprint #{}: {}", sprint_id, err))?;
        let dirty = canonical != original;

        match mode {
            SprintNormalizeMode::Write => {
                if dirty {
                    fs::write(&path, canonical.as_bytes())
                        .map_err(|err| format!("Failed to write {}: {}", path.display(), err))?;
                    updated += 1;
                    updated_ids.push(sprint_id);
                    if !is_json {
                        renderer.emit_success(&format!("Normalized sprint #{}.", sprint_id));
                    }
                } else if !is_json {
                    renderer.emit_info(&format!("Sprint #{} already canonical.", sprint_id));
                }
            }
            SprintNormalizeMode::Check => {
                if dirty {
                    pending_ids.push(sprint_id);
                    if !is_json {
                        renderer.emit_warning(&format!(
                            "Sprint #{} requires normalization (run with --write to update).",
                            sprint_id
                        ));
                    }
                } else if !is_json {
                    renderer.emit_info(&format!("Sprint #{} already canonical.", sprint_id));
                }
            }
        }

        if !warnings.is_empty() {
            if is_json {
                for warning in &warnings {
                    warning_payloads.push(SprintNormalizeWarningPayload {
                        sprint_id,
                        code: warning.code(),
                        message: warning.message(),
                    });
                }
            } else {
                let emit = match mode {
                    SprintNormalizeMode::Write => OutputRenderer::emit_info,
                    SprintNormalizeMode::Check => OutputRenderer::emit_warning,
                };
                for warning in warnings {
                    emit(
                        renderer,
                        &format!(
                            "Sprint #{} canonicalization notice: {}",
                            sprint_id,
                            warning.message()
                        ),
                    );
                }
            }
        }

        processed += 1;
    }

    if is_json {
        let status = match mode {
            SprintNormalizeMode::Write => "ok",
            SprintNormalizeMode::Check => {
                if pending_ids.is_empty() {
                    "ok"
                } else {
                    "needs_normalization"
                }
            }
        };

        let payload = SprintNormalizeResponse {
            status,
            mode: mode.as_label(),
            processed,
            updated,
            changes_required: pending_ids.clone(),
            updated_ids: updated_ids.clone(),
            warnings: warning_payloads,
            skipped,
        };
        renderer.emit_raw_stdout(&serde_json::to_string(&payload).unwrap_or_default());
    }

    match mode {
        SprintNormalizeMode::Write => {
            if !is_json {
                renderer.emit_success(&format!(
                    "Normalization complete ({} sprint(s) processed, {} updated).",
                    processed, updated
                ));
            }
            Ok(())
        }
        SprintNormalizeMode::Check => {
            if pending_ids.is_empty() {
                if !is_json {
                    renderer
                        .emit_success(&format!("All {} sprint(s) already canonical.", processed));
                }
                Ok(())
            } else {
                let summary = pending_ids
                    .iter()
                    .map(|id| format!("#{}", id))
                    .collect::<Vec<_>>()
                    .join(", ");
                if !is_json {
                    renderer.emit_warning("Sprint normalization required.");
                }
                Err(format!(
                    "Sprint normalization required for: {}. Run with --write to update.",
                    summary
                ))
            }
        }
    }
}

fn resolve_normalize_mode(args: &SprintNormalizeArgs) -> Result<SprintNormalizeMode, String> {
    match (args.check, args.write) {
        (true, true) => Err("--check and --write are mutually exclusive".to_string()),
        (false, false) | (true, false) => Ok(SprintNormalizeMode::Check),
        (false, true) => Ok(SprintNormalizeMode::Write),
    }
}

fn render_sprint_stats(
    storage: &Storage,
    record: &SprintRecord,
    resolved_config: &ResolvedConfig,
    renderer: &OutputRenderer,
) {
    let context = compute_sprint_stats(storage, record, resolved_config, Utc::now());

    match renderer.format {
        OutputFormat::Json => {
            renderer.emit_raw_stdout(&serde_json::to_string(&context.payload).unwrap_or_default());
        }
        _ => render_stats_text(renderer, &context),
    }
}

fn render_sprint_review(
    storage: &Storage,
    record: &SprintRecord,
    resolved_config: &ResolvedConfig,
    renderer: &OutputRenderer,
) {
    let context = compute_sprint_review(storage, record, resolved_config, Utc::now());

    match renderer.format {
        OutputFormat::Json => {
            renderer.emit_raw_stdout(&serde_json::to_string(&context.payload).unwrap_or_default());
        }
        _ => render_review_text(renderer, &context),
    }
}

fn render_review_text(renderer: &OutputRenderer, context: &SprintReviewContext) {
    let summary = &context.summary;
    let lifecycle = &context.lifecycle;
    let metrics = &context.metrics;

    renderer.emit_success(&format!(
        "Sprint review for #{}{}.",
        summary.id,
        summary
            .label
            .as_ref()
            .map(|label| format!(" ({})", label))
            .unwrap_or_default()
    ));
    renderer.emit_raw_stdout(&format!("Lifecycle status: {}", summary.status));

    if let Some(goal) = summary.goal.as_ref() {
        renderer.emit_raw_stdout(&format!("Goal: {}", goal));
    }

    if let Some(actual_start) = lifecycle.actual_start.as_ref() {
        renderer.emit_raw_stdout(&format!("Started at: {}", actual_start.to_rfc3339()));
    } else if let Some(planned_start) = lifecycle.planned_start.as_ref() {
        renderer.emit_raw_stdout(&format!("Planned start: {}", planned_start.to_rfc3339()));
    }

    if let Some(actual_end) = lifecycle.actual_end.as_ref() {
        renderer.emit_raw_stdout(&format!("Closed at: {}", actual_end.to_rfc3339()));
    } else if let Some(target_end) = lifecycle.computed_end.as_ref() {
        renderer.emit_raw_stdout(&format!("Target end: {}", target_end.to_rfc3339()));
    }

    if metrics.total_tasks == 0 {
        renderer.emit_info("No tasks are linked to this sprint.");
        return;
    }

    let remaining_count = metrics.remaining_tasks_count();

    renderer.emit_raw_stdout(&format!(
        "Tasks: {} total • {} done • {} remaining",
        metrics.total_tasks, metrics.done_tasks, remaining_count
    ));

    if !metrics.status_breakdown.is_empty() {
        renderer.emit_raw_stdout("Status breakdown:");
        for metric in &metrics.status_breakdown {
            let suffix = if metric.done { " (done)" } else { "" };
            renderer.emit_raw_stdout(&format!(
                "  - {}: {}{}",
                metric.status, metric.count, suffix
            ));
        }
    }

    if remaining_count == 0 {
        renderer.emit_success("All sprint tasks are complete.");
    } else {
        renderer.emit_raw_stdout("Remaining tasks:");
        for task in &metrics.remaining_tasks {
            if let Some(assignee) = task.assignee.as_ref() {
                renderer.emit_raw_stdout(&format!(
                    "  - {}: {} [{}] (assignee: {})",
                    task.id, task.title, task.status, assignee
                ));
            } else {
                renderer.emit_raw_stdout(&format!(
                    "  - {}: {} [{}]",
                    task.id, task.title, task.status
                ));
            }
        }
    }
}

fn render_summary_text(renderer: &OutputRenderer, context: &SprintSummaryContext) {
    let summary = &context.summary;
    let lifecycle = &context.lifecycle;
    let metrics = &context.metrics;
    let durations = &context.durations;
    let payload = &context.payload;

    renderer.emit_success(&format!(
        "Sprint summary for #{}{}.",
        summary.id,
        summary
            .label
            .as_ref()
            .map(|label| format!(" ({})", label))
            .unwrap_or_default()
    ));
    renderer.emit_raw_stdout(&format!("Status: {}", summary.status));

    if let Some(goal) = summary.goal.as_ref() {
        renderer.emit_raw_stdout(&format!("Goal: {}", goal));
    }

    if summary.has_warnings {
        for warning in &lifecycle.warnings {
            renderer.emit_warning(&warning.message());
        }
    }

    if metrics.total_tasks == 0 {
        renderer.emit_info("No tasks are linked to this sprint.");
    } else {
        renderer.emit_raw_stdout(&format!(
            "Progress: {} committed • {} done • {} remaining ({} complete)",
            metrics.total_tasks,
            metrics.done_tasks,
            metrics.remaining_tasks_count(),
            format_percentage(Some(payload.metrics.tasks.completion_ratio)),
        ));

        if !metrics.status_breakdown.is_empty() {
            renderer.emit_raw_stdout("Status highlights:");
            for metric in metrics.status_breakdown.iter().take(5) {
                let suffix = if metric.done { " (done)" } else { "" };
                renderer.emit_raw_stdout(&format!(
                    "  - {}: {}{}",
                    metric.status, metric.count, suffix
                ));
            }
            if metrics.status_breakdown.len() > 5 {
                renderer.emit_info(
                    "Additional statuses omitted; use --format json for full breakdown.",
                );
            }
        }
    }

    let blocked_tasks = &metrics.blocked_tasks;
    if !blocked_tasks.is_empty() {
        renderer.emit_warning(&format!(
            "{} blocked task(s) require attention:",
            blocked_tasks.len()
        ));
        for task in blocked_tasks.iter().take(10) {
            if let Some(assignee) = task.assignee.as_ref() {
                renderer.emit_raw_stdout(&format!(
                    "  - {}: {} [{}] (assignee: {})",
                    task.id, task.title, task.status, assignee
                ));
            } else {
                renderer.emit_raw_stdout(&format!(
                    "  - {}: {} [{}]",
                    task.id, task.title, task.status
                ));
            }
        }
        if blocked_tasks.len() > 10 {
            renderer
                .emit_warning("Blocked task list truncated; use --format json for full details.");
        }
    }

    if let Some(points) = payload.metrics.points.as_ref() {
        renderer.emit_raw_stdout(&format!(
            "Points: {} committed • {} done • {} remaining ({} complete)",
            format_float(points.committed),
            format_float(points.done),
            format_float(points.remaining),
            format_percentage(Some(points.completion_ratio)),
        ));

        if let Some(capacity) = points.capacity {
            renderer.emit_raw_stdout(&format!(
                "  Capacity: {} planned • {} committed • {} consumed",
                format_float(capacity),
                format_percentage(points.capacity_commitment_ratio),
                format_percentage(points.capacity_consumed_ratio),
            ));

            if points.committed > capacity + 0.000_1 {
                renderer.emit_warning(&format!(
                    "Points commitment exceeds capacity by {}.",
                    format_float(points.committed - capacity)
                ));
            }
        }
    }

    if let Some(hours) = payload.metrics.hours.as_ref() {
        renderer.emit_raw_stdout(&format!(
            "Hours: {} committed • {} done • {} remaining ({} complete)",
            format_float(hours.committed),
            format_float(hours.done),
            format_float(hours.remaining),
            format_percentage(Some(hours.completion_ratio)),
        ));

        if let Some(capacity) = hours.capacity {
            renderer.emit_raw_stdout(&format!(
                "  Capacity: {} planned • {} committed • {} consumed",
                format_float(capacity),
                format_percentage(hours.capacity_commitment_ratio),
                format_percentage(hours.capacity_consumed_ratio),
            ));

            if hours.committed > capacity + 0.000_1 {
                renderer.emit_warning(&format!(
                    "Hour commitment exceeds capacity by {}.",
                    format_float(hours.committed - capacity)
                ));
            }
        }
    }

    if let Some(start) = lifecycle.actual_start.as_ref() {
        renderer.emit_raw_stdout(&format!("Started: {}", start.to_rfc3339()));
    } else if let Some(start) = lifecycle.planned_start.as_ref() {
        renderer.emit_raw_stdout(&format!("Planned start: {}", start.to_rfc3339()));
    }

    if let Some(end) = lifecycle.actual_end.as_ref() {
        renderer.emit_raw_stdout(&format!("Closed: {}", end.to_rfc3339()));
    } else if let Some(end) = lifecycle.computed_end.as_ref() {
        renderer.emit_raw_stdout(&format!("Target end: {}", end.to_rfc3339()));
    }

    if let Some(duration) = durations.planned {
        renderer.emit_raw_stdout(&format!("Planned duration: {}", format_duration(duration)));
    }
    if let Some(duration) = durations.actual {
        renderer.emit_raw_stdout(&format!("Actual duration: {}", format_duration(duration)));
    } else if let Some(duration) = durations.elapsed {
        renderer.emit_raw_stdout(&format!("Elapsed so far: {}", format_duration(duration)));
    }
    if let Some(duration) = durations.remaining {
        renderer.emit_raw_stdout(&format!("Time remaining: {}", format_duration(duration)));
    }
    if let Some(duration) = durations.overdue {
        renderer.emit_warning(&format!("Overdue by: {}", format_duration(duration)));
    }
}

fn render_stats_text(renderer: &OutputRenderer, context: &SprintStatsContext) {
    let summary = &context.summary;
    let lifecycle = &context.lifecycle;
    let metrics = &context.metrics;
    let durations = &context.durations;
    let payload = &context.payload;

    renderer.emit_success(&format!(
        "Sprint stats for #{}{}.",
        summary.id,
        summary
            .label
            .as_ref()
            .map(|label| format!(" ({})", label))
            .unwrap_or_default()
    ));
    renderer.emit_raw_stdout(&format!("Lifecycle status: {}", summary.status));

    if metrics.total_tasks == 0 {
        renderer.emit_info("No tasks are linked to this sprint; only timeline metrics available.");
    } else {
        renderer.emit_raw_stdout(&format!(
            "Tasks: {} committed • {} done • {} remaining ({} complete)",
            metrics.total_tasks,
            metrics.done_tasks,
            metrics.remaining_tasks_count(),
            format_percentage(Some(payload.metrics.tasks.completion_ratio)),
        ));

        if !metrics.status_breakdown.is_empty() {
            renderer.emit_raw_stdout("Status breakdown:");
            for metric in &metrics.status_breakdown {
                let suffix = if metric.done { " (done)" } else { "" };
                renderer.emit_raw_stdout(&format!(
                    "  - {}: {}{}",
                    metric.status, metric.count, suffix
                ));
            }
        }
    }

    if let Some(points) = payload.metrics.points.as_ref() {
        renderer.emit_raw_stdout(&format!(
            "Points: {} committed • {} done • {} remaining ({} complete)",
            format_float(points.committed),
            format_float(points.done),
            format_float(points.remaining),
            format_percentage(Some(points.completion_ratio)),
        ));

        if let Some(capacity) = points.capacity {
            renderer.emit_raw_stdout(&format!(
                "  Capacity: {} planned • {} committed • {} consumed",
                format_float(capacity),
                format_percentage(points.capacity_commitment_ratio),
                format_percentage(points.capacity_consumed_ratio),
            ));

            if points.committed > capacity + 0.000_1 {
                renderer.emit_warning(&format!(
                    "Points commitment exceeds capacity by {}.",
                    format_float(points.committed - capacity)
                ));
            }
        }
    }

    if let Some(hours) = payload.metrics.hours.as_ref() {
        renderer.emit_raw_stdout(&format!(
            "Hours: {} committed • {} done • {} remaining ({} complete)",
            format_float(hours.committed),
            format_float(hours.done),
            format_float(hours.remaining),
            format_percentage(Some(hours.completion_ratio)),
        ));

        if let Some(capacity) = hours.capacity {
            renderer.emit_raw_stdout(&format!(
                "  Capacity: {} planned • {} committed • {} consumed",
                format_float(capacity),
                format_percentage(hours.capacity_commitment_ratio),
                format_percentage(hours.capacity_consumed_ratio),
            ));

            if hours.committed > capacity + 0.000_1 {
                renderer.emit_warning(&format!(
                    "Hour commitment exceeds capacity by {}.",
                    format_float(hours.committed - capacity)
                ));
            }
        }
    }

    renderer.emit_raw_stdout("Timeline:");
    if let Some(start) = lifecycle.planned_start.as_ref() {
        renderer.emit_raw_stdout(&format!("  Planned start: {}", start.to_rfc3339()));
    }
    if let Some(start) = lifecycle.actual_start.as_ref() {
        renderer.emit_raw_stdout(&format!("  Actual start: {}", start.to_rfc3339()));
    }
    if let Some(end) = lifecycle.planned_end.as_ref() {
        renderer.emit_raw_stdout(&format!("  Planned end: {}", end.to_rfc3339()));
    }
    if let Some(end) = lifecycle.computed_end.as_ref() {
        let differs = lifecycle
            .planned_end
            .as_ref()
            .map(|planned| planned != end)
            .unwrap_or(true);
        if lifecycle.actual_end.is_none() && differs {
            renderer.emit_raw_stdout(&format!("  Computed end: {}", end.to_rfc3339()));
        }
    }
    if let Some(end) = lifecycle.actual_end.as_ref() {
        renderer.emit_raw_stdout(&format!("  Actual end: {}", end.to_rfc3339()));
    }
    if let Some(duration) = durations.planned {
        renderer.emit_raw_stdout(&format!(
            "  Planned duration: {}",
            format_duration(duration)
        ));
    }
    if let Some(duration) = durations.actual {
        renderer.emit_raw_stdout(&format!("  Actual duration: {}", format_duration(duration)));
    } else if let Some(duration) = durations.elapsed {
        renderer.emit_raw_stdout(&format!("  Elapsed so far: {}", format_duration(duration)));
    }
    if let Some(duration) = durations.remaining {
        renderer.emit_raw_stdout(&format!("  Time remaining: {}", format_duration(duration)));
    }
    if let Some(duration) = durations.overdue {
        renderer.emit_raw_stdout(&format!("  Overdue by: {}", format_duration(duration)));
    }
}

fn render_burndown_text(
    renderer: &OutputRenderer,
    context: &SprintBurndownContext,
    requested_metric: SprintBurndownMetric,
) {
    let summary = &context.summary;
    let computation = &context.computation;

    let mut focus_metric = requested_metric;
    if matches!(focus_metric, SprintBurndownMetric::Points)
        && computation.totals.points.unwrap_or(0.0) <= 0.0
    {
        renderer
            .emit_warning("No point estimates recorded for this sprint; falling back to tasks.");
        focus_metric = SprintBurndownMetric::Tasks;
    }
    if matches!(focus_metric, SprintBurndownMetric::Hours)
        && computation.totals.hours.unwrap_or(0.0) <= 0.0
    {
        renderer.emit_warning("No hour estimates recorded for this sprint; falling back to tasks.");
        focus_metric = SprintBurndownMetric::Tasks;
    }

    renderer.emit_success(&format!(
        "Sprint burndown for #{}{}.",
        summary.id,
        summary
            .label
            .as_ref()
            .map(|label| format!(" ({})", label))
            .unwrap_or_default()
    ));
    renderer.emit_raw_stdout(&format!("Status: {}", summary.status));

    if computation.series.is_empty() {
        renderer.emit_info("No burndown samples available for this sprint.");
        return;
    }

    renderer.emit_raw_stdout("Date       Remaining  Ideal");
    renderer.emit_raw_stdout("--------------------------------");

    match focus_metric {
        SprintBurndownMetric::Tasks => {
            renderer.emit_raw_stdout(&format!(
                "Total tasks: {} | Ideal horizon: {} days",
                computation.totals.tasks, computation.day_span
            ));
            for point in &context.computation.series {
                let date_display = point.date.date_naive();
                renderer.emit_raw_stdout(&format!(
                    "{}  {:>9}  {:>5.1}",
                    date_display, point.remaining_tasks, point.ideal_tasks
                ));
            }
        }
        SprintBurndownMetric::Points => {
            renderer.emit_raw_stdout(&format!(
                "Total points: {} | Ideal horizon: {} days",
                format_float(computation.totals.points.unwrap_or(0.0)),
                computation.day_span
            ));
            for point in &context.computation.series {
                let date_display = point.date.date_naive();
                renderer.emit_raw_stdout(&format!(
                    "{}  {:>9}  {:>5.1}",
                    date_display,
                    format_float(point.remaining_points.unwrap_or(0.0)),
                    point.ideal_points.unwrap_or(0.0)
                ));
            }
        }
        SprintBurndownMetric::Hours => {
            renderer.emit_raw_stdout(&format!(
                "Total hours: {} | Ideal horizon: {} days",
                format_float(computation.totals.hours.unwrap_or(0.0)),
                computation.day_span
            ));
            for point in &context.computation.series {
                let date_display = point.date.date_naive();
                renderer.emit_raw_stdout(&format!(
                    "{}  {:>9}  {:>5.1}",
                    date_display,
                    format_float(point.remaining_hours.unwrap_or(0.0)),
                    point.ideal_hours.unwrap_or(0.0)
                ));
            }
        }
    }

    if computation.totals.points.unwrap_or(0.0) > 0.0
        && !matches!(focus_metric, SprintBurndownMetric::Points)
    {
        renderer.emit_info("Tip: run with --metric points for point-based burndown.");
    }
    if computation.totals.hours.unwrap_or(0.0) > 0.0
        && !matches!(focus_metric, SprintBurndownMetric::Hours)
    {
        renderer.emit_info("Tip: run with --metric hours for time-based burndown.");
    }
    renderer.emit_info("Use --format json to feed the series into custom dashboards.");
}

fn render_calendar_text(
    renderer: &OutputRenderer,
    context: &SprintCalendarContext,
    include_complete: bool,
) {
    if context.entries.is_empty() {
        renderer.emit_info("No sprints match the calendar filters.");
        return;
    }

    renderer.emit_success("Sprint calendar:");
    renderer.emit_raw_stdout(
        "ID   Label                 Status    Window                        Relative",
    );
    renderer.emit_raw_stdout(
        "--------------------------------------------------------------------------------",
    );

    for entry in &context.entries {
        let label = entry
            .summary
            .label
            .clone()
            .unwrap_or_else(|| format!("Sprint {}", entry.id));
        let status_display = if entry.summary.has_warnings {
            format!("{}*", entry.summary.status)
        } else {
            entry.summary.status.clone()
        };
        let window = format_calendar_window(entry.start, entry.end, entry.duration);
        renderer.emit_raw_stdout(&format!(
            "#{:>3} {:<20} {:<10} {:<30} {}",
            entry.id,
            truncate(&label, 20),
            truncate(&status_display, 10),
            truncate(&window, 30),
            entry.relative
        ));
    }

    if context.payload.truncated {
        renderer.emit_warning(
            "Calendar view truncated by --limit; rerun without --limit for the complete schedule.",
        );
    }
    if context.payload.skipped_complete && !include_complete {
        renderer.emit_info("Completed sprints hidden; use --include-complete to show them.");
    }

    for entry in &context.entries {
        if entry.summary.has_warnings && !entry.warnings.is_empty() {
            for warning in &entry.warnings {
                renderer.emit_warning(&format!("Sprint #{}: {}", entry.id, warning.message));
            }
        }
    }
}

fn format_percentage(value: Option<f64>) -> String {
    match value {
        Some(val) if val.is_finite() => format!("{:.1}%", val * 100.0),
        _ => "n/a".to_string(),
    }
}

fn format_float(value: f64) -> String {
    if !value.is_finite() {
        return "n/a".to_string();
    }
    if (value - value.round()).abs() < 0.05 {
        format!("{:.0}", value)
    } else {
        format!("{:.1}", value)
    }
}

fn format_velocity_value(metric: SprintBurndownMetric, value: f64) -> String {
    match metric {
        SprintBurndownMetric::Tasks => format!("{:.0}", value.round()),
        SprintBurndownMetric::Points | SprintBurndownMetric::Hours => format_float(value),
    }
}

fn metric_label(metric: SprintBurndownMetric) -> &'static str {
    match metric {
        SprintBurndownMetric::Tasks => "tasks",
        SprintBurndownMetric::Points => "points",
        SprintBurndownMetric::Hours => "hours",
    }
}

fn format_duration(duration: Duration) -> String {
    let mut seconds = duration.num_seconds();
    let negative = seconds < 0;
    if negative {
        seconds = -seconds;
    }

    let days = seconds / 86_400;
    let hours = (seconds % 86_400) / 3_600;
    let minutes = (seconds % 3_600) / 60;

    let mut parts = Vec::new();
    if days > 0 {
        parts.push(format!("{}d", days));
    }
    if hours > 0 {
        parts.push(format!("{}h", hours));
    }
    if minutes > 0 && days == 0 {
        parts.push(format!("{}m", minutes));
    }
    if parts.is_empty() {
        parts.push("0m".to_string());
    }

    let text = parts.join(" ");
    if negative { format!("-{}", text) } else { text }
}

fn format_calendar_window(
    start: Option<DateTime<Utc>>,
    end: Option<DateTime<Utc>>,
    duration: Option<Duration>,
) -> String {
    match (start, end) {
        (Some(begin), Some(finish)) => {
            let duration_text = duration
                .map(format_duration)
                .unwrap_or_else(|| "n/a".to_string());
            format!(
                "{} -> {} ({})",
                format_calendar_date(begin),
                format_calendar_date(finish),
                duration_text
            )
        }
        (Some(begin), None) => format!("{} -> ?", format_calendar_date(begin)),
        (None, Some(finish)) => format!("? -> {}", format_calendar_date(finish)),
        (None, None) => "window pending".to_string(),
    }
}

fn format_calendar_date(value: DateTime<Utc>) -> String {
    value.format("%Y-%m-%d").to_string()
}

fn select_sprint_id_for_review(records: &[SprintRecord]) -> Option<u32> {
    if records.is_empty() {
        return None;
    }

    let now = Utc::now();
    let mut completed = Vec::new();
    let mut active = Vec::new();
    let mut pending = Vec::new();

    for record in records {
        let lifecycle = sprint_status::derive_status(&record.sprint, now);
        match lifecycle.state {
            SprintLifecycleState::Complete => {
                let reference = lifecycle
                    .actual_end
                    .or(lifecycle.computed_end)
                    .unwrap_or(now);
                completed.push((reference, record.id));
            }
            SprintLifecycleState::Active | SprintLifecycleState::Overdue => {
                let reference = lifecycle
                    .actual_start
                    .or(lifecycle.planned_start)
                    .unwrap_or(now);
                active.push((reference, record.id));
            }
            SprintLifecycleState::Pending => pending.push(record.id),
        }
    }

    if let Some((_, id)) = completed
        .into_iter()
        .max_by(|a, b| a.0.cmp(&b.0).then(a.1.cmp(&b.1)))
    {
        return Some(id);
    }

    if let Some((_, id)) = active
        .into_iter()
        .max_by(|a, b| a.0.cmp(&b.0).then(a.1.cmp(&b.1)))
    {
        return Some(id);
    }

    pending.into_iter().max()
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

        if let Some(planned_start) = lifecycle.planned_start {
            if planned_start <= evaluation_time {
                ready.push((planned_start, record.id));
                continue;
            }
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

    renderer.emit_warning(&format!("{}: {}.{}", prefix, list, guidance));
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
        renderer.emit_warning(&format!(
            "The requested start time {} is more than 12 hours in the future; pass --force to proceed.",
            start_instant.to_rfc3339()
        ));
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

    if let Some(planned_start) = lifecycle.planned_start {
        if Utc::now() > planned_start {
            renderer.emit_warning(&format!(
                "Sprint #{} ({}) was scheduled to start at {} and is now overdue to begin.",
                record.id,
                sprint_assignment::sprint_display_name(record),
                planned_start.to_rfc3339()
            ));
        }
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

    if let Some(computed_end) = lifecycle.computed_end {
        if close_instant > computed_end {
            renderer.emit_warning(&format!(
                "Sprint #{} ({}) was scheduled to end by {} and is overdue to close.",
                record.id,
                sprint_assignment::sprint_display_name(record),
                computed_end.to_rfc3339()
            ));
        }
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

#[derive(Copy, Clone, Debug)]
enum SprintOperationKind {
    Create,
    Update,
    Start,
    Close,
}

fn render_operation_response(
    kind: SprintOperationKind,
    record: SprintRecord,
    warnings: Vec<crate::storage::sprint::SprintCanonicalizationWarning>,
    applied_defaults: Vec<String>,
    renderer: &OutputRenderer,
    warnings_enabled: bool,
    _resolved_config: &crate::config::types::ResolvedConfig,
) {
    let lifecycle = sprint_status::derive_status(&record.sprint, Utc::now());
    let summary = SprintSummary::from_record(&record, &lifecycle);
    let detail = SprintDetail::from_record(&record, &summary, &lifecycle);
    let actual_started_at = record
        .sprint
        .actual
        .as_ref()
        .and_then(|actual| actual.started_at.clone());
    let mut canonical_warnings = if warnings_enabled {
        warnings
    } else {
        Vec::new()
    };

    match renderer.format {
        OutputFormat::Json => {
            let payload = SprintOperationResponse {
                status: "ok",
                action: match kind {
                    SprintOperationKind::Create => "created",
                    SprintOperationKind::Update => "updated",
                    SprintOperationKind::Start => "started",
                    SprintOperationKind::Close => "closed",
                },
                sprint: detail,
                applied_defaults: applied_defaults.clone(),
                warnings: canonical_warnings
                    .iter()
                    .map(|warning| SprintWarningPayload {
                        code: warning.code(),
                        message: warning.message(),
                    })
                    .collect(),
            };
            renderer.emit_raw_stdout(&serde_json::to_string(&payload).unwrap_or_default());
        }
        _ => {
            let verb = match kind {
                SprintOperationKind::Create => "Created",
                SprintOperationKind::Update => "Updated",
                SprintOperationKind::Start => "Started",
                SprintOperationKind::Close => "Closed",
            };

            renderer.emit_success(&format!(
                "{} sprint #{}{}.",
                verb,
                record.id,
                summary
                    .label
                    .as_ref()
                    .map(|label| format!(" ({})", label))
                    .unwrap_or_default()
            ));

            if !applied_defaults.is_empty() {
                let defaults_list = applied_defaults.join(", ");
                renderer.emit_info(&format!("Applied sprint defaults: {}.", defaults_list));
            }

            if let Some(computed_end) = detail.computed_end.as_ref() {
                renderer.emit_info(&format!("Computed end: {}", computed_end));
            }

            if !canonical_warnings.is_empty() {
                for warning in canonical_warnings.drain(..) {
                    renderer.emit_warning(warning.message());
                }
            }

            if warnings_enabled && !lifecycle.warnings.is_empty() {
                for warning in &lifecycle.warnings {
                    renderer.emit_warning(&warning.message());
                }
            }

            if matches!(kind, SprintOperationKind::Start) {
                if let Some(started_at) = actual_started_at {
                    renderer.emit_info(&format!("Started at: {}", started_at));
                }
            }
            if matches!(kind, SprintOperationKind::Close) {
                if let Some(closed_at) = record
                    .sprint
                    .actual
                    .as_ref()
                    .and_then(|actual| actual.closed_at.clone())
                {
                    renderer.emit_info(&format!("Closed at: {}", closed_at));
                }
            }
        }
    }
}

#[derive(Debug, Serialize)]
struct SprintOperationResponse {
    status: &'static str,
    action: &'static str,
    sprint: SprintDetail,
    applied_defaults: Vec<String>,
    warnings: Vec<SprintWarningPayload>,
}

#[derive(Debug, Serialize)]
struct SprintWarningPayload {
    code: &'static str,
    message: &'static str,
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

#[derive(Debug, Serialize, Clone)]
struct SprintCleanupRefsRemoved {
    sprint_id: u32,
    count: usize,
}

#[derive(Debug, Serialize, Clone)]
struct SprintCleanupSummaryPayload {
    removed_references: usize,
    updated_tasks: usize,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    removed_by_sprint: Vec<SprintCleanupRefsRemoved>,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    remaining_missing: Vec<u32>,
}

#[derive(Debug, Serialize)]
struct SprintAssignmentIntegrityPayload {
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    missing_sprints: Vec<u32>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    tasks_with_missing: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    auto_cleanup: Option<SprintCleanupSummaryPayload>,
}

#[derive(Debug, Serialize)]
struct SprintListPayload {
    status: &'static str,
    count: usize,
    truncated: bool,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    sprints: Vec<SprintSummary>,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    missing_sprints: Vec<u32>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    integrity: Option<SprintAssignmentIntegrityPayload>,
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

#[derive(Debug, Serialize)]
struct SprintReassignment {
    task_id: String,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    previous: Vec<u32>,
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

#[derive(Debug, Serialize)]
struct SprintNormalizeResponse {
    status: &'static str,
    mode: &'static str,
    processed: usize,
    updated: usize,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    changes_required: Vec<u32>,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    updated_ids: Vec<u32>,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    warnings: Vec<SprintNormalizeWarningPayload>,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    skipped: Vec<SprintNormalizeSkipPayload>,
}

#[derive(Debug, Serialize)]
struct SprintNormalizeWarningPayload {
    sprint_id: u32,
    code: &'static str,
    message: &'static str,
}

#[derive(Debug, Serialize)]
struct SprintNormalizeSkipPayload {
    sprint_id: u32,
    reason: String,
}

fn emit_plan(renderer: &OutputRenderer, plan: &SprintPlan) {
    renderer.emit_raw_stdout("Plan:");
    let mut printed = false;
    if let Some(label) = plan.label.as_ref() {
        renderer.emit_raw_stdout(&format!("  label: {}", label));
        printed = true;
    }
    if let Some(length) = plan.length.as_ref() {
        renderer.emit_raw_stdout(&format!("  length: {}", length));
        printed = true;
    }
    if let Some(ends_at) = plan.ends_at.as_ref() {
        renderer.emit_raw_stdout(&format!("  ends_at: {}", ends_at));
        printed = true;
    }
    if let Some(starts_at) = plan.starts_at.as_ref() {
        renderer.emit_raw_stdout(&format!("  starts_at: {}", starts_at));
        printed = true;
    }
    if let Some(capacity) = plan.capacity.as_ref() {
        if capacity.points.is_some() || capacity.hours.is_some() {
            renderer.emit_raw_stdout("  capacity:");
            printed = true;
            if let Some(points) = capacity.points {
                renderer.emit_raw_stdout(&format!("    points: {}", points));
            }
            if let Some(hours) = capacity.hours {
                renderer.emit_raw_stdout(&format!("    hours: {}", hours));
            }
        }
    }
    if let Some(overdue_after) = plan.overdue_after.as_ref() {
        renderer.emit_raw_stdout(&format!("  overdue_after: {}", overdue_after));
        printed = true;
    }
    if let Some(notes) = plan.notes.as_ref() {
        renderer.emit_raw_stdout("  notes:");
        for line in notes.lines() {
            renderer.emit_raw_stdout(&format!("    {}", line));
        }
        printed = true;
    }
    if !printed {
        renderer.emit_raw_stdout("  (empty)");
    }
}

fn emit_actual(renderer: &OutputRenderer, actual: &SprintActual) {
    if actual.started_at.is_none() && actual.closed_at.is_none() {
        return;
    }
    renderer.emit_raw_stdout("Actual:");
    if let Some(started_at) = actual.started_at.as_ref() {
        renderer.emit_raw_stdout(&format!("  started_at: {}", started_at));
    }
    if let Some(closed_at) = actual.closed_at.as_ref() {
        renderer.emit_raw_stdout(&format!("  closed_at: {}", closed_at));
    }
}

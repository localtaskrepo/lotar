use std::path::PathBuf;

use chrono::Utc;

use super::super::helpers::select_sprint_id_for_review;
use super::text::{
    emit_actual, emit_plan, render_review_text, render_stats_text, render_summary_text,
};
use crate::cli::args::sprint::{
    SprintReviewArgs, SprintShowArgs, SprintStatsArgs, SprintSummaryArgs,
};
use crate::config::types::ResolvedConfig;
use crate::output::{OutputFormat, OutputRenderer};
use crate::services::sprint_analytics::{SprintDetail, SprintSummary};
use crate::services::sprint_reports::{
    compute_sprint_review, compute_sprint_stats, compute_sprint_summary,
};
use crate::services::sprint_service::{SprintRecord, SprintService};
use crate::services::sprint_status;
use crate::storage::manager::Storage;

use crate::cli::handlers::sprint::operations::resolve_sprint_records_context;

pub(crate) fn handle_show(
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
            renderer.emit_json(&payload);
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

pub(crate) fn handle_review(
    review_args: SprintReviewArgs,
    tasks_root: PathBuf,
    renderer: &OutputRenderer,
) -> Result<(), String> {
    let context = resolve_sprint_records_context(tasks_root, "running a review")?;

    let target_id = match review_args.sprint_id {
        Some(id) => id,
        None => select_sprint_id_for_review(&context.records)
            .ok_or_else(|| "No sprints available for review.".to_string())?,
    };

    if review_args.sprint_id.is_none() && !matches!(renderer.format, OutputFormat::Json) {
        renderer.emit_info(&format!("Auto-selected sprint #{} for review.", target_id));
    }

    let record = SprintService::get(&context.storage, target_id).map_err(|err| err.to_string())?;
    render_sprint_review(
        &context.storage,
        &record,
        &context.resolved_config,
        renderer,
    );

    Ok(())
}

pub(crate) fn handle_stats(
    stats_args: SprintStatsArgs,
    tasks_root: PathBuf,
    renderer: &OutputRenderer,
) -> Result<(), String> {
    let context = resolve_sprint_records_context(tasks_root, "running stats")?;

    let target_id = match stats_args.sprint_id {
        Some(id) => id,
        None => select_sprint_id_for_review(&context.records)
            .ok_or_else(|| "No sprints available for stats.".to_string())?,
    };

    if stats_args.sprint_id.is_none() && !matches!(renderer.format, OutputFormat::Json) {
        renderer.emit_info(&format!("Auto-selected sprint #{} for stats.", target_id));
    }

    let record = SprintService::get(&context.storage, target_id).map_err(|err| err.to_string())?;
    render_sprint_stats(
        &context.storage,
        &record,
        &context.resolved_config,
        renderer,
    );

    Ok(())
}

pub(crate) fn handle_summary(
    summary_args: SprintSummaryArgs,
    tasks_root: PathBuf,
    renderer: &OutputRenderer,
) -> Result<(), String> {
    let context = resolve_sprint_records_context(tasks_root, "running summary")?;

    let target_id = match summary_args.sprint_id {
        Some(id) => id,
        None => select_sprint_id_for_review(&context.records)
            .ok_or_else(|| "No sprints available for summary.".to_string())?,
    };

    if summary_args.sprint_id.is_none() && !matches!(renderer.format, OutputFormat::Json) {
        renderer.emit_info(&format!("Auto-selected sprint #{} for summary.", target_id));
    }

    let record = SprintService::get(&context.storage, target_id).map_err(|err| err.to_string())?;
    let summary_context = compute_sprint_summary(
        &context.storage,
        &record,
        &context.resolved_config,
        Utc::now(),
    );

    match renderer.format {
        OutputFormat::Json => {
            renderer.emit_raw_stdout(
                &serde_json::to_string(&summary_context.payload).unwrap_or_default(),
            );
        }
        _ => render_summary_text(renderer, &summary_context),
    }

    Ok(())
}

pub(crate) fn render_sprint_review(
    storage: &Storage,
    record: &SprintRecord,
    resolved_config: &ResolvedConfig,
    renderer: &OutputRenderer,
) {
    let context = compute_sprint_review(storage, record, resolved_config, Utc::now());

    match renderer.format {
        OutputFormat::Json => {
            renderer.emit_json(&context.payload);
        }
        _ => render_review_text(renderer, &context),
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
            renderer.emit_json(&context.payload);
        }
        _ => render_stats_text(renderer, &context),
    }
}

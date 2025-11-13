use std::path::PathBuf;

use chrono::Utc;

use crate::cli::args::sprint::SprintBurndownArgs;
use crate::config::resolution::load_and_merge_configs;
use crate::output::{OutputFormat, OutputRenderer};
use crate::services::sprint_metrics::SprintBurndownMetric;
use crate::services::sprint_reports::{SprintBurndownContext, compute_sprint_burndown};
use crate::services::sprint_service::SprintService;
use crate::storage::manager::Storage;

use super::helpers::{format_float, select_sprint_id_for_review};

pub(crate) fn handle_burndown(
    burndown_args: SprintBurndownArgs,
    tasks_root: PathBuf,
    renderer: &OutputRenderer,
) -> Result<(), String> {
    let config_root = tasks_root.clone();
    let storage = Storage::try_open(tasks_root)
        .ok_or_else(|| "No sprints found. Create one before running burndown.".to_string())?;

    let resolved_config =
        load_and_merge_configs(Some(config_root.as_path())).map_err(|err| err.to_string())?;

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
            renderer.emit_json(&context.payload);
        }
        _ => render_burndown_text(renderer, &context, burndown_args.metric),
    }

    Ok(())
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

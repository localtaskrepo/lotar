use std::path::PathBuf;

use chrono::Utc;

use crate::cli::args::sprint::SprintVelocityArgs;
use crate::cli::handlers::sprint::shared::truncate;
use crate::config::resolution::load_and_merge_configs;
use crate::output::{OutputFormat, OutputRenderer};
use crate::services::sprint_service::SprintService;
use crate::services::sprint_velocity::{
    DEFAULT_VELOCITY_WINDOW, VelocityComputation, VelocityOptions, compute_velocity,
};
use crate::storage::manager::Storage;

use super::helpers::{format_percentage, format_velocity_value, metric_label};

pub(crate) fn handle_velocity(
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
        load_and_merge_configs(Some(config_root.as_path())).map_err(|err| err.to_string())?;

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

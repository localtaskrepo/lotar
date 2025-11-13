use std::path::PathBuf;

use chrono::{DateTime, Duration, Utc};

use crate::cli::args::sprint::SprintCalendarArgs;
use crate::cli::handlers::sprint::shared::truncate;
use crate::output::{OutputFormat, OutputRenderer};
use crate::services::sprint_reports::{SprintCalendarContext, compute_sprint_calendar};
use crate::services::sprint_service::SprintService;
use crate::storage::manager::Storage;

use super::helpers::format_duration;

pub(crate) fn handle_calendar(
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
            renderer.emit_json(&value);
        }
        _ => render_calendar_text(renderer, &context, calendar_args.include_complete),
    }

    Ok(())
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

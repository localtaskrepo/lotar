use std::path::PathBuf;

use chrono::Utc;
use serde::Serialize;

use crate::cli::args::sprint::SprintListArgs;
use crate::cli::handlers::sprint::shared::{
    SprintAssignmentIntegrityPayload, build_assignment_integrity, emit_cleanup_summary,
    emit_missing_report, truncate,
};
use crate::output::{OutputFormat, OutputRenderer};
use crate::services::sprint_analytics::SprintSummary;
use crate::services::sprint_integrity::{self, MissingSprintReport, SprintCleanupOutcome};
use crate::services::sprint_service::{SprintRecord, SprintService};
use crate::services::sprint_status;
use crate::storage::manager::Storage;

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

pub(crate) fn handle_list(
    list_args: SprintListArgs,
    tasks_root: PathBuf,
    renderer: &OutputRenderer,
) -> Result<(), String> {
    if let Some(limit) = list_args.limit
        && limit == 0
    {
        return Err("--limit must be greater than zero".to_string());
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
                renderer.emit_json(&payload);
            }
            _ => renderer.emit_success("No sprints found."),
        }
        return Ok(());
    }

    let mut truncated = false;
    if let Some(limit) = list_args.limit
        && records.len() > limit
    {
        records.truncate(limit);
        truncated = true;
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
            renderer.emit_json(&payload);
        }
        _ => {
            let rows: Vec<PreparedSummaryRow> = summaries
                .iter()
                .map(PreparedSummaryRow::from_summary)
                .collect();
            let widths = compute_column_widths(&rows);

            renderer.emit_raw_stdout(format_table_header(&widths));
            let separator = "-".repeat(widths.total());
            renderer.emit_raw_stdout(separator);
            for row in &rows {
                renderer.emit_raw_stdout(format_summary_row(row, &widths));
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
    use crate::services::sprint_status::SprintLifecycleState::*;

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

    match crate::utils::time::parse_human_datetime_to_utc(raw) {
        Ok(dt) => {
            use chrono::Timelike;

            if dt.hour() == 0 && dt.minute() == 0 && dt.second() == 0 {
                dt.format("%b %-d").to_string()
            } else {
                dt.format("%b %-d %H:%M").to_string()
            }
        }
        Err(_) => raw.to_string(),
    }
}

use crate::output::OutputRenderer;
use crate::services::sprint_reports::{
    SprintReviewContext, SprintStatsContext, SprintSummaryContext,
};
use crate::storage::sprint::{SprintActual, SprintPlan};

use super::super::helpers::{format_duration, format_float, format_percentage};

pub(super) fn render_review_text(renderer: &OutputRenderer, context: &SprintReviewContext) {
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

pub(super) fn render_summary_text(renderer: &OutputRenderer, context: &SprintSummaryContext) {
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

pub(super) fn render_stats_text(renderer: &OutputRenderer, context: &SprintStatsContext) {
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

pub(super) fn emit_plan(renderer: &OutputRenderer, plan: &SprintPlan) {
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

pub(super) fn emit_actual(renderer: &OutputRenderer, actual: &SprintActual) {
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

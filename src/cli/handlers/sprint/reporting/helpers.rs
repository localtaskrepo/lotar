use chrono::{Duration, Utc};

use crate::services::sprint_metrics::SprintBurndownMetric;
use crate::services::sprint_service::SprintRecord;
use crate::services::sprint_status::{self, SprintLifecycleState};

pub(crate) fn format_percentage(value: Option<f64>) -> String {
    match value {
        Some(val) if val.is_finite() => format!("{:.1}%", val * 100.0),
        _ => "n/a".to_string(),
    }
}

pub(crate) fn format_float(value: f64) -> String {
    if !value.is_finite() {
        return "n/a".to_string();
    }
    if (value - value.round()).abs() < 0.05 {
        format!("{:.0}", value)
    } else {
        format!("{:.1}", value)
    }
}

pub(crate) fn format_velocity_value(metric: SprintBurndownMetric, value: f64) -> String {
    match metric {
        SprintBurndownMetric::Tasks => format!("{:.0}", value.round()),
        SprintBurndownMetric::Points | SprintBurndownMetric::Hours => format_float(value),
    }
}

pub(crate) fn metric_label(metric: SprintBurndownMetric) -> &'static str {
    match metric {
        SprintBurndownMetric::Tasks => "tasks",
        SprintBurndownMetric::Points => "points",
        SprintBurndownMetric::Hours => "hours",
    }
}

pub(crate) fn format_duration(duration: Duration) -> String {
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
    if negative { format!("-{text}") } else { text }
}

pub(crate) fn select_sprint_id_for_review(records: &[SprintRecord]) -> Option<u32> {
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

use chrono::{DateTime, Duration, Utc};

use crate::services::sprint_service::SprintRecord;
use crate::services::sprint_status::{SprintLifecycleState, SprintLifecycleStatus};
use crate::storage::task::Task as StoredTask;
use crate::utils::time;

#[derive(Debug, Clone, Copy, Default)]
pub struct SprintDurations {
    pub planned: Option<Duration>,
    pub actual: Option<Duration>,
    pub elapsed: Option<Duration>,
    pub remaining: Option<Duration>,
    pub overdue: Option<Duration>,
}

pub fn compute_sprint_durations(
    record: &SprintRecord,
    lifecycle: &SprintLifecycleStatus,
    now: DateTime<Utc>,
) -> SprintDurations {
    let planned_duration = match (lifecycle.planned_start, lifecycle.planned_end) {
        (Some(start), Some(end)) if end > start => Some(end - start),
        _ => record
            .sprint
            .plan
            .as_ref()
            .and_then(|plan| plan.length.as_ref())
            .and_then(|value| time::parse_duration_like(value)),
    };

    let actual_duration = match (lifecycle.actual_start, lifecycle.actual_end) {
        (Some(start), Some(end)) if end > start => Some(end - start),
        _ => None,
    };

    let elapsed_duration = lifecycle.actual_start.and_then(|start| {
        let pivot = lifecycle.actual_end.unwrap_or(now);
        if pivot > start {
            Some(pivot - start)
        } else {
            None
        }
    });

    let (remaining_duration, overdue_duration) = if lifecycle.actual_end.is_none() {
        if let Some(end) = lifecycle.computed_end {
            if end > now {
                (Some(end - now), None)
            } else if now > end {
                (None, Some(now - end))
            } else {
                (None, None)
            }
        } else {
            (None, None)
        }
    } else {
        (None, None)
    };

    SprintDurations {
        planned: planned_duration,
        actual: actual_duration,
        elapsed: elapsed_duration,
        remaining: remaining_duration,
        overdue: overdue_duration,
    }
}

pub fn format_duration(duration: Duration) -> String {
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

pub fn duration_to_days(duration: Duration) -> f64 {
    duration.num_seconds() as f64 / 86_400.0
}

pub fn resolve_calendar_start(
    record: &SprintRecord,
    lifecycle: &SprintLifecycleStatus,
) -> Option<DateTime<Utc>> {
    lifecycle
        .actual_start
        .or(lifecycle.planned_start)
        .or_else(|| {
            record
                .sprint
                .actual
                .as_ref()
                .and_then(|actual| actual.started_at.as_deref())
                .and_then(parse_optional_datetime)
        })
        .or_else(|| {
            record
                .sprint
                .plan
                .as_ref()
                .and_then(|plan| plan.starts_at.as_deref())
                .and_then(parse_optional_datetime)
        })
        .or_else(|| {
            record
                .sprint
                .created
                .as_deref()
                .and_then(parse_optional_datetime)
        })
}

pub fn resolve_calendar_end(
    record: &SprintRecord,
    lifecycle: &SprintLifecycleStatus,
) -> Option<DateTime<Utc>> {
    lifecycle
        .actual_end
        .or(lifecycle.computed_end)
        .or(lifecycle.planned_end)
        .or_else(|| {
            record
                .sprint
                .actual
                .as_ref()
                .and_then(|actual| actual.closed_at.as_deref())
                .and_then(parse_optional_datetime)
        })
        .or_else(|| {
            record
                .sprint
                .plan
                .as_ref()
                .and_then(|plan| plan.ends_at.as_deref())
                .and_then(parse_optional_datetime)
        })
        .or_else(|| {
            record
                .sprint
                .modified
                .as_deref()
                .and_then(parse_optional_datetime)
        })
}

pub fn format_calendar_relative(
    lifecycle: &SprintLifecycleStatus,
    start: Option<DateTime<Utc>>,
    end: Option<DateTime<Utc>>,
    now: DateTime<Utc>,
) -> String {
    match lifecycle.state {
        SprintLifecycleState::Pending => {
            if let Some(start_dt) = start {
                if start_dt > now {
                    format!("starts in {}", format_duration(start_dt - now))
                } else {
                    format!("start overdue by {}", format_duration(now - start_dt))
                }
            } else {
                "start date tbd".to_string()
            }
        }
        SprintLifecycleState::Active => {
            if let Some(end_dt) = end {
                if end_dt > now {
                    format!("ends in {}", format_duration(end_dt - now))
                } else {
                    format!("end overdue by {}", format_duration(now - end_dt))
                }
            } else {
                "in progress".to_string()
            }
        }
        SprintLifecycleState::Overdue => {
            if let Some(end_dt) = end {
                format!("overdue by {}", format_duration(now - end_dt))
            } else {
                "overdue (end tbd)".to_string()
            }
        }
        SprintLifecycleState::Complete => {
            if let Some(end_dt) = lifecycle.actual_end.or(end) {
                if end_dt <= now {
                    format!("ended {} ago", format_duration(now - end_dt))
                } else {
                    "completed".to_string()
                }
            } else {
                "completed".to_string()
            }
        }
    }
}

pub fn format_calendar_window(
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

pub fn parse_optional_datetime(raw: &str) -> Option<DateTime<Utc>> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return None;
    }
    time::parse_human_datetime_to_utc(trimmed).ok()
}

pub fn find_done_timestamp(
    task: &StoredTask,
    done_statuses: &std::collections::HashSet<String>,
) -> Option<DateTime<Utc>> {
    let mut earliest: Option<DateTime<Utc>> = None;

    for entry in &task.history {
        let timestamp = match time::parse_human_datetime_to_utc(entry.at.trim()) {
            Ok(dt) => dt,
            Err(_) => continue,
        };

        for change in &entry.changes {
            if !change.field.eq_ignore_ascii_case("status") {
                continue;
            }
            if let Some(new_status) = change.new.as_ref() {
                let lowered = new_status.trim().to_ascii_lowercase();
                if done_statuses.contains(&lowered)
                    && earliest.map(|current| timestamp < current).unwrap_or(true)
                {
                    earliest = Some(timestamp);
                }
            }
        }
    }

    if earliest.is_some() {
        return earliest;
    }

    let current_status = task.status.as_str().to_ascii_lowercase();
    if done_statuses.contains(&current_status) {
        if let Some(modified) = parse_optional_datetime(&task.modified) {
            return Some(modified);
        }
        if let Some(created) = parse_optional_datetime(&task.created) {
            return Some(created);
        }
    }

    None
}

pub fn resolve_burndown_window(
    record: &SprintRecord,
    lifecycle: &SprintLifecycleStatus,
    tasks: &[(String, StoredTask)],
    done_statuses: &std::collections::HashSet<String>,
) -> Result<(DateTime<Utc>, DateTime<Utc>), String> {
    let task_created_min = tasks
        .iter()
        .filter_map(|(_, task)| parse_optional_datetime(&task.created))
        .min();

    let start = lifecycle
        .actual_start
        .or(lifecycle.planned_start)
        .or(task_created_min)
        .ok_or_else(|| {
            format!(
                "Sprint #{} lacks plan/actual start timestamps and no tasks provide created times.",
                record.id
            )
        })?;

    let task_created_max = tasks
        .iter()
        .filter_map(|(_, task)| parse_optional_datetime(&task.created))
        .max();
    let task_modified_max = tasks
        .iter()
        .filter_map(|(_, task)| parse_optional_datetime(&task.modified))
        .max();
    let done_latest = tasks
        .iter()
        .filter_map(|(_, task)| find_done_timestamp(task, done_statuses))
        .max();

    let mut end_candidates = Vec::new();
    if let Some(dt) = lifecycle.actual_end {
        end_candidates.push(dt);
    }
    if let Some(dt) = lifecycle.computed_end {
        end_candidates.push(dt);
    }
    if let Some(dt) = lifecycle.planned_end {
        end_candidates.push(dt);
    }
    if let Some(dt) = done_latest {
        end_candidates.push(dt);
    }
    if let Some(dt) = task_modified_max {
        end_candidates.push(dt);
    }
    if let Some(dt) = task_created_max {
        end_candidates.push(dt);
    }

    let mut end = end_candidates
        .into_iter()
        .max()
        .ok_or_else(|| {
            format!(
                "Sprint #{} lacks plan/actual end timestamps and none of its tasks provide modified times.",
                record.id
            )
        })?;

    if end <= start {
        end = start + Duration::days(1);
    }

    Ok((start, end))
}

pub fn format_calendar_date_basic(value: DateTime<Utc>) -> String {
    value.format("%Y-%m-%d").to_string()
}

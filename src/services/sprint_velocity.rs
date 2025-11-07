use chrono::{DateTime, Duration, Utc};
use serde::Serialize;

use crate::config::types::ResolvedConfig;
use crate::services::sprint_analytics::{
    SprintReviewLifecyclePayload, SprintStatusWarningPayload, SprintSummary,
    to_status_warning_payloads,
};
use crate::services::sprint_metrics::{
    SprintBurndownMetric, compute_velocity_totals, determine_done_statuses_from_config,
    metric_label, ratio,
};
use crate::services::sprint_service::{SprintRecord, SprintService};
use crate::services::sprint_status::{self, SprintLifecycleState};
use crate::services::sprint_timing::{
    duration_to_days, format_calendar_relative, format_calendar_window, resolve_calendar_end,
    resolve_calendar_start,
};
use crate::storage::manager::Storage;

#[cfg(feature = "schema")]
use schemars::JsonSchema;

pub const DEFAULT_VELOCITY_WINDOW: usize = 6;

#[derive(Debug, Clone)]
pub struct VelocityOptions {
    pub limit: usize,
    pub include_active: bool,
    pub metric: SprintBurndownMetric,
}

#[derive(Debug, Clone)]
pub struct VelocityEntry {
    pub sprint_id: u32,
    pub summary: SprintSummary,
    pub lifecycle: SprintReviewLifecyclePayload,
    pub start: Option<DateTime<Utc>>,
    pub end: Option<DateTime<Utc>>,
    pub actual_start: Option<DateTime<Utc>>,
    pub actual_end: Option<DateTime<Utc>>,
    pub duration: Option<Duration>,
    pub relative: String,
    pub committed: f64,
    pub completed: f64,
    pub capacity: Option<f64>,
    pub completion_ratio: Option<f64>,
    pub warnings: Vec<SprintStatusWarningPayload>,
    pub sort_key: i64,
}

#[derive(Debug, Clone)]
pub struct VelocityComputation {
    pub metric: SprintBurndownMetric,
    pub entries: Vec<VelocityEntry>,
    pub total_matching: usize,
    pub truncated: bool,
    pub skipped_incomplete: bool,
    pub average_velocity: Option<f64>,
    pub average_completion_ratio: Option<f64>,
}

#[derive(Debug, Serialize, Clone)]
#[cfg_attr(feature = "schema", derive(JsonSchema))]
pub struct SprintVelocityEntryPayload {
    pub summary: SprintSummary,
    pub lifecycle: SprintReviewLifecyclePayload,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub start: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub end: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub actual_start: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub actual_end: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub duration_days: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub window: Option<String>,
    pub committed: f64,
    pub completed: f64,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub completion_ratio: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub capacity: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub capacity_commitment_ratio: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub capacity_consumed_ratio: Option<f64>,
    pub relative: String,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub status_warnings: Vec<SprintStatusWarningPayload>,
}

#[derive(Debug, Serialize, Clone)]
#[cfg_attr(feature = "schema", derive(JsonSchema))]
pub struct SprintVelocityResponse {
    pub status: &'static str,
    pub metric: &'static str,
    pub count: usize,
    pub truncated: bool,
    #[serde(skip_serializing_if = "is_false", default)]
    pub include_active: bool,
    #[serde(skip_serializing_if = "is_false", default)]
    pub skipped_incomplete: bool,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub average_velocity: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub average_completion_ratio: Option<f64>,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub entries: Vec<SprintVelocityEntryPayload>,
}

fn is_false(value: &bool) -> bool {
    !*value
}

impl VelocityComputation {
    pub fn to_payload(&self, include_active: bool) -> SprintVelocityResponse {
        SprintVelocityResponse {
            status: "ok",
            metric: metric_label(self.metric),
            count: self.entries.len(),
            truncated: self.truncated,
            include_active,
            skipped_incomplete: self.skipped_incomplete,
            average_velocity: self.average_velocity,
            average_completion_ratio: self.average_completion_ratio,
            entries: self
                .entries
                .iter()
                .map(|entry| SprintVelocityEntryPayload {
                    summary: entry.summary.clone(),
                    lifecycle: entry.lifecycle.clone(),
                    start: entry.start.as_ref().map(|dt| dt.to_rfc3339()),
                    end: entry.end.as_ref().map(|dt| dt.to_rfc3339()),
                    actual_start: entry.actual_start.as_ref().map(|dt| dt.to_rfc3339()),
                    actual_end: entry.actual_end.as_ref().map(|dt| dt.to_rfc3339()),
                    duration_days: entry
                        .duration
                        .as_ref()
                        .map(|duration| duration_to_days(*duration)),
                    window: Some(format_calendar_window(
                        entry.start,
                        entry.end,
                        entry.duration,
                    )),
                    committed: entry.committed,
                    completed: entry.completed,
                    completion_ratio: entry.completion_ratio,
                    capacity: entry.capacity,
                    capacity_commitment_ratio: entry
                        .capacity
                        .and_then(|cap| ratio(entry.committed, cap)),
                    capacity_consumed_ratio: entry
                        .capacity
                        .and_then(|cap| ratio(entry.completed, cap)),
                    relative: entry.relative.clone(),
                    status_warnings: entry.warnings.clone(),
                })
                .collect(),
        }
    }
}

pub fn compute_velocity(
    storage: &Storage,
    records: Vec<SprintRecord>,
    config: &ResolvedConfig,
    options: VelocityOptions,
    now: DateTime<Utc>,
) -> VelocityComputation {
    if records.is_empty() {
        return VelocityComputation {
            metric: options.metric,
            entries: Vec::new(),
            total_matching: 0,
            truncated: false,
            skipped_incomplete: false,
            average_velocity: None,
            average_completion_ratio: None,
        };
    }

    let done_statuses = determine_done_statuses_from_config(config);
    let mut entries = Vec::new();
    let mut skipped_incomplete = false;

    for record in records {
        let lifecycle = sprint_status::derive_status(&record.sprint, now);
        let include = match lifecycle.state {
            SprintLifecycleState::Complete => true,
            SprintLifecycleState::Active | SprintLifecycleState::Overdue => {
                if options.include_active {
                    true
                } else {
                    skipped_incomplete = true;
                    false
                }
            }
            SprintLifecycleState::Pending => {
                skipped_incomplete = true;
                false
            }
        };

        if !include {
            continue;
        }

        let summary = SprintSummary::from_record(&record, &lifecycle);
        let lifecycle_payload = SprintReviewLifecyclePayload::from_status(&lifecycle);
        let start = resolve_calendar_start(&record, &lifecycle);
        let end = resolve_calendar_end(&record, &lifecycle);
        let duration = match (start, end) {
            (Some(begin), Some(finish)) if finish > begin => Some(finish - begin),
            _ => None,
        };
        let relative = format_calendar_relative(&lifecycle, start, end, now);

        let tasks = SprintService::load_tasks_for_record(storage, &record);
        let totals = compute_velocity_totals(&tasks, &done_statuses);

        let (committed, completed, capacity) = match options.metric {
            SprintBurndownMetric::Tasks => {
                (totals.total_tasks as f64, totals.done_tasks as f64, None)
            }
            SprintBurndownMetric::Points => {
                let capacity = record
                    .sprint
                    .plan
                    .as_ref()
                    .and_then(|plan| plan.capacity.as_ref())
                    .and_then(|cap| cap.points)
                    .map(|value| value as f64);
                (totals.total_points, totals.done_points, capacity)
            }
            SprintBurndownMetric::Hours => {
                let capacity = record
                    .sprint
                    .plan
                    .as_ref()
                    .and_then(|plan| plan.capacity.as_ref())
                    .and_then(|cap| cap.hours)
                    .map(|value| value as f64);
                (totals.total_hours, totals.done_hours, capacity)
            }
        };

        let completion_ratio = ratio(completed, committed);
        let warnings = to_status_warning_payloads(&lifecycle.warnings);
        let actual_start = lifecycle.actual_start;
        let actual_end = lifecycle.actual_end;
        let sort_source = actual_end
            .as_ref()
            .or(end.as_ref())
            .or(actual_start.as_ref())
            .cloned()
            .unwrap_or(now);

        entries.push(VelocityEntry {
            sprint_id: record.id,
            summary,
            lifecycle: lifecycle_payload,
            start,
            end,
            actual_start,
            actual_end,
            duration,
            relative,
            committed,
            completed,
            capacity,
            completion_ratio,
            warnings,
            sort_key: sort_source.timestamp(),
        });
    }

    if entries.is_empty() {
        return VelocityComputation {
            metric: options.metric,
            entries,
            total_matching: 0,
            truncated: false,
            skipped_incomplete,
            average_velocity: None,
            average_completion_ratio: None,
        };
    }

    entries.sort_by(|a, b| {
        b.sort_key
            .cmp(&a.sort_key)
            .then(b.sprint_id.cmp(&a.sprint_id))
    });

    let total_matching = entries.len();
    let mut truncated = false;
    if options.limit < entries.len() {
        entries.truncate(options.limit);
        truncated = true;
    }

    let count = entries.len();
    let average_velocity = if count > 0 {
        Some(entries.iter().map(|entry| entry.completed).sum::<f64>() / count as f64)
    } else {
        None
    };
    let (ratio_sum, ratio_count) = entries.iter().fold((0.0, 0usize), |acc, entry| {
        let (mut sum, mut count) = acc;
        if let Some(r) = entry.completion_ratio {
            sum += r;
            count += 1;
        }
        (sum, count)
    });
    let average_completion_ratio = if ratio_count > 0 {
        Some(ratio_sum / ratio_count as f64)
    } else {
        None
    };

    VelocityComputation {
        metric: options.metric,
        entries,
        total_matching,
        truncated,
        skipped_incomplete,
        average_velocity,
        average_completion_ratio,
    }
}

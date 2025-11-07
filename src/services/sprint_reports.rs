use std::collections::{BTreeMap, HashSet};

use chrono::{DateTime, Duration, TimeZone, Utc};
use serde::Serialize;

#[cfg(feature = "schema")]
use schemars::JsonSchema;

use crate::config::types::ResolvedConfig;
use crate::services::sprint_analytics::{
    SprintDetail, SprintReviewLifecyclePayload, SprintStatusWarningPayload, SprintSummary,
    to_status_warning_payloads,
};
use crate::services::sprint_metrics::{
    determine_blocked_statuses_from_config, determine_done_statuses_from_config, ratio, ratio_usize,
};
use crate::services::sprint_service::{SprintRecord, SprintService};
use crate::services::sprint_status::{self, SprintLifecycleState, SprintLifecycleStatus};
use crate::services::sprint_timing::{
    SprintDurations, compute_sprint_durations, duration_to_days, find_done_timestamp,
    format_calendar_relative, format_calendar_window, resolve_burndown_window,
    resolve_calendar_end, resolve_calendar_start,
};
use crate::storage::manager::Storage;
use crate::storage::task::Task as StoredTask;
use crate::utils::effort::{self, EffortKind};

fn is_false(value: &bool) -> bool {
    !*value
}

#[derive(Debug, Serialize, Clone)]
#[cfg_attr(feature = "schema", derive(JsonSchema))]
pub struct SprintReviewTask {
    pub id: String,
    pub title: String,
    pub status: String,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub assignee: Option<String>,
}

#[derive(Debug, Serialize, Clone)]
#[cfg_attr(feature = "schema", derive(JsonSchema))]
pub struct SprintReviewStatusMetric {
    pub status: String,
    pub count: usize,
    #[serde(default)]
    pub done: bool,
}

#[derive(Debug, Clone)]
pub struct SprintTaskMetrics {
    pub total_tasks: usize,
    pub done_tasks: usize,
    pub status_breakdown: Vec<SprintReviewStatusMetric>,
    pub remaining_tasks: Vec<SprintReviewTask>,
    pub blocked_tasks: Vec<SprintReviewTask>,
    pub total_hours: f64,
    pub done_hours: f64,
    pub total_points: f64,
    pub done_points: f64,
}

impl SprintTaskMetrics {
    pub fn remaining_tasks_count(&self) -> usize {
        self.total_tasks.saturating_sub(self.done_tasks)
    }

    pub fn remaining_hours(&self) -> f64 {
        (self.total_hours - self.done_hours).max(0.0)
    }

    pub fn remaining_points(&self) -> f64 {
        (self.total_points - self.done_points).max(0.0)
    }
}

#[derive(Debug, Serialize, Clone)]
#[cfg_attr(feature = "schema", derive(JsonSchema))]
pub struct SprintReviewMetricsPayload {
    pub total_tasks: usize,
    pub done_tasks: usize,
    pub remaining_tasks: usize,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub status_breakdown: Vec<SprintReviewStatusMetric>,
}

#[derive(Debug, Serialize, Clone)]
#[cfg_attr(feature = "schema", derive(JsonSchema))]
pub struct SprintReviewResponse {
    pub status: &'static str,
    pub sprint: SprintDetail,
    pub lifecycle: SprintReviewLifecyclePayload,
    pub metrics: SprintReviewMetricsPayload,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub remaining_tasks: Vec<SprintReviewTask>,
}

#[derive(Debug, Serialize, Clone)]
#[cfg_attr(feature = "schema", derive(JsonSchema))]
pub struct SprintStatsCountsPayload {
    pub committed: usize,
    pub done: usize,
    pub remaining: usize,
    pub completion_ratio: f64,
}

#[derive(Debug, Serialize, Clone)]
#[cfg_attr(feature = "schema", derive(JsonSchema))]
pub struct SprintStatsEffortPayload {
    pub committed: f64,
    pub done: f64,
    pub remaining: f64,
    pub completion_ratio: f64,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub capacity: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub capacity_commitment_ratio: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub capacity_consumed_ratio: Option<f64>,
}

#[derive(Debug, Serialize, Clone)]
#[cfg_attr(feature = "schema", derive(JsonSchema))]
pub struct SprintStatsMetricsPayload {
    pub tasks: SprintStatsCountsPayload,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub hours: Option<SprintStatsEffortPayload>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub points: Option<SprintStatsEffortPayload>,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub status_breakdown: Vec<SprintReviewStatusMetric>,
}

#[derive(Debug, Serialize, Clone)]
#[cfg_attr(feature = "schema", derive(JsonSchema))]
pub struct SprintStatsTimelinePayload {
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub planned_start: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub actual_start: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub planned_end: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub computed_end: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub actual_end: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub planned_duration_days: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub actual_duration_days: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub elapsed_days: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub remaining_days: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub overdue_days: Option<f64>,
}

#[derive(Debug, Serialize, Clone)]
#[cfg_attr(feature = "schema", derive(JsonSchema))]
pub struct SprintStatsResponse {
    pub status: &'static str,
    pub sprint: SprintDetail,
    pub lifecycle: SprintReviewLifecyclePayload,
    pub metrics: SprintStatsMetricsPayload,
    pub timeline: SprintStatsTimelinePayload,
}

#[derive(Debug, Serialize, Clone)]
#[cfg_attr(feature = "schema", derive(JsonSchema))]
pub struct SprintSummaryReportMetrics {
    pub tasks: SprintStatsCountsPayload,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub hours: Option<SprintStatsEffortPayload>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub points: Option<SprintStatsEffortPayload>,
    #[serde(skip_serializing_if = "is_zero", default)]
    pub blocked: usize,
}

#[derive(Debug, Serialize, Clone)]
#[cfg_attr(feature = "schema", derive(JsonSchema))]
pub struct SprintSummaryReportResponse {
    pub status: &'static str,
    pub sprint: SprintDetail,
    pub lifecycle: SprintReviewLifecyclePayload,
    pub metrics: SprintSummaryReportMetrics,
    pub timeline: SprintStatsTimelinePayload,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub blocked_tasks: Vec<SprintReviewTask>,
}

fn is_zero(value: &usize) -> bool {
    *value == 0
}

#[derive(Debug, Serialize, Clone)]
#[cfg_attr(feature = "schema", derive(JsonSchema))]
pub struct SprintBurndownTotalsPayload {
    pub tasks: usize,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub points: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub hours: Option<f64>,
}

#[derive(Debug, Serialize, Clone)]
#[cfg_attr(feature = "schema", derive(JsonSchema))]
pub struct SprintBurndownPointPayload {
    pub date: String,
    pub remaining_tasks: usize,
    pub ideal_tasks: f64,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub remaining_points: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub ideal_points: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub remaining_hours: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub ideal_hours: Option<f64>,
}

#[derive(Debug, Serialize, Clone)]
#[cfg_attr(feature = "schema", derive(JsonSchema))]
pub struct SprintBurndownResponse {
    pub status: &'static str,
    pub sprint: SprintDetail,
    pub lifecycle: SprintReviewLifecyclePayload,
    pub totals: SprintBurndownTotalsPayload,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub series: Vec<SprintBurndownPointPayload>,
}

#[derive(Debug, Serialize, Clone)]
#[cfg_attr(feature = "schema", derive(JsonSchema))]
pub struct SprintCalendarEntryPayload {
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
    pub time_until_start_days: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub time_until_end_days: Option<f64>,
    pub window: String,
    #[serde(skip_serializing_if = "is_false", default)]
    pub has_warnings: bool,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub status_warnings: Vec<SprintStatusWarningPayload>,
    pub relative: String,
}

#[derive(Debug, Serialize, Clone)]
#[cfg_attr(feature = "schema", derive(JsonSchema))]
pub struct SprintCalendarResponse {
    pub status: &'static str,
    pub count: usize,
    pub truncated: bool,
    pub skipped_complete: bool,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub sprints: Vec<SprintCalendarEntryPayload>,
}

#[derive(Debug, Clone)]
pub struct SprintCalendarEntry {
    pub id: u32,
    pub summary: SprintSummary,
    pub lifecycle: SprintReviewLifecyclePayload,
    pub start: Option<DateTime<Utc>>,
    pub end: Option<DateTime<Utc>>,
    pub actual_start: Option<DateTime<Utc>>,
    pub actual_end: Option<DateTime<Utc>>,
    pub duration: Option<Duration>,
    pub relative: String,
    pub sort_key: i64,
    pub warnings: Vec<SprintStatusWarningPayload>,
}

#[derive(Debug, Clone)]
pub struct SprintReviewContext {
    pub payload: SprintReviewResponse,
    pub metrics: SprintTaskMetrics,
    pub summary: SprintSummary,
    pub lifecycle: SprintLifecycleStatus,
}

#[derive(Debug, Clone)]
pub struct SprintStatsContext {
    pub payload: SprintStatsResponse,
    pub metrics: SprintTaskMetrics,
    pub durations: SprintDurations,
    pub summary: SprintSummary,
    pub lifecycle: SprintLifecycleStatus,
}

#[derive(Debug, Clone)]
pub struct SprintSummaryContext {
    pub payload: SprintSummaryReportResponse,
    pub metrics: SprintTaskMetrics,
    pub durations: SprintDurations,
    pub summary: SprintSummary,
    pub lifecycle: SprintLifecycleStatus,
}

#[derive(Debug, Clone)]
pub struct SprintBurndownContext {
    pub payload: SprintBurndownResponse,
    pub computation: SprintBurndownComputation,
    pub summary: SprintSummary,
    pub lifecycle: SprintLifecycleStatus,
}

#[derive(Debug, Clone)]
pub struct SprintCalendarContext {
    pub payload: SprintCalendarResponse,
    pub entries: Vec<SprintCalendarEntry>,
}

#[derive(Debug, Clone)]
pub struct SprintBurndownComputation {
    pub totals: SprintBurndownTotals,
    pub series: Vec<SprintBurndownPoint>,
    pub day_span: usize,
}

#[derive(Debug, Clone)]
pub struct SprintBurndownTotals {
    pub tasks: usize,
    pub points: Option<f64>,
    pub hours: Option<f64>,
}

#[derive(Debug, Clone)]
pub struct SprintBurndownPoint {
    pub date: DateTime<Utc>,
    pub remaining_tasks: usize,
    pub ideal_tasks: f64,
    pub remaining_points: Option<f64>,
    pub ideal_points: Option<f64>,
    pub remaining_hours: Option<f64>,
    pub ideal_hours: Option<f64>,
}

struct SprintBurndownItem {
    done_at: Option<DateTime<Utc>>,
    points: f64,
    hours: f64,
}

struct StatusAggregate {
    label: String,
    count: usize,
    done: bool,
}

pub fn compute_sprint_review(
    storage: &Storage,
    record: &SprintRecord,
    config: &ResolvedConfig,
    now: DateTime<Utc>,
) -> SprintReviewContext {
    let lifecycle = sprint_status::derive_status(&record.sprint, now);
    let summary = SprintSummary::from_record(record, &lifecycle);
    let detail = SprintDetail::from_record(record, &summary, &lifecycle);

    let tasks = SprintService::load_tasks_for_record(storage, record);

    let done_statuses = determine_done_statuses_from_config(config);
    let blocked_statuses = determine_blocked_statuses_from_config(config);
    let metrics = summarize_sprint_tasks(&tasks, &done_statuses, &blocked_statuses);

    let payload = SprintReviewResponse {
        status: "ok",
        sprint: detail,
        lifecycle: SprintReviewLifecyclePayload::from_status(&lifecycle),
        metrics: SprintReviewMetricsPayload {
            total_tasks: metrics.total_tasks,
            done_tasks: metrics.done_tasks,
            remaining_tasks: metrics.remaining_tasks_count(),
            status_breakdown: metrics.status_breakdown.clone(),
        },
        remaining_tasks: metrics.remaining_tasks.clone(),
    };

    SprintReviewContext {
        payload,
        metrics,
        summary,
        lifecycle,
    }
}

pub fn compute_sprint_stats(
    storage: &Storage,
    record: &SprintRecord,
    config: &ResolvedConfig,
    now: DateTime<Utc>,
) -> SprintStatsContext {
    let lifecycle = sprint_status::derive_status(&record.sprint, now);
    let summary = SprintSummary::from_record(record, &lifecycle);
    let detail = SprintDetail::from_record(record, &summary, &lifecycle);

    let tasks = SprintService::load_tasks_for_record(storage, record);

    let done_statuses = determine_done_statuses_from_config(config);
    let blocked_statuses = determine_blocked_statuses_from_config(config);
    let metrics = summarize_sprint_tasks(&tasks, &done_statuses, &blocked_statuses);

    let capacity_points = record
        .sprint
        .plan
        .as_ref()
        .and_then(|plan| plan.capacity.as_ref())
        .and_then(|capacity| capacity.points)
        .map(|value| value as f64);
    let capacity_hours = record
        .sprint
        .plan
        .as_ref()
        .and_then(|plan| plan.capacity.as_ref())
        .and_then(|capacity| capacity.hours)
        .map(|value| value as f64);

    let durations = compute_sprint_durations(record, &lifecycle, now);

    let planned_duration_days = durations.planned.map(duration_to_days);
    let actual_duration_days = durations.actual.map(duration_to_days);
    let elapsed_days = durations.elapsed.map(duration_to_days);
    let remaining_days = durations.remaining.map(duration_to_days);
    let overdue_days = durations.overdue.map(duration_to_days);

    let tasks_payload = SprintStatsCountsPayload {
        committed: metrics.total_tasks,
        done: metrics.done_tasks,
        remaining: metrics.remaining_tasks_count(),
        completion_ratio: ratio_usize(metrics.done_tasks, metrics.total_tasks).unwrap_or(0.0),
    };

    let hours_payload = if metrics.total_hours > 0.0
        || metrics.done_hours > 0.0
        || metrics.remaining_hours() > 0.0
        || capacity_hours.is_some()
    {
        Some(SprintStatsEffortPayload {
            committed: metrics.total_hours,
            done: metrics.done_hours,
            remaining: metrics.remaining_hours(),
            completion_ratio: ratio(metrics.done_hours, metrics.total_hours).unwrap_or(0.0),
            capacity: capacity_hours,
            capacity_commitment_ratio: capacity_hours
                .and_then(|cap| ratio(metrics.total_hours, cap)),
            capacity_consumed_ratio: capacity_hours.and_then(|cap| ratio(metrics.done_hours, cap)),
        })
    } else {
        None
    };

    let points_payload = if metrics.total_points > 0.0
        || metrics.done_points > 0.0
        || metrics.remaining_points() > 0.0
        || capacity_points.is_some()
    {
        Some(SprintStatsEffortPayload {
            committed: metrics.total_points,
            done: metrics.done_points,
            remaining: metrics.remaining_points(),
            completion_ratio: ratio(metrics.done_points, metrics.total_points).unwrap_or(0.0),
            capacity: capacity_points,
            capacity_commitment_ratio: capacity_points
                .and_then(|cap| ratio(metrics.total_points, cap)),
            capacity_consumed_ratio: capacity_points
                .and_then(|cap| ratio(metrics.done_points, cap)),
        })
    } else {
        None
    };

    let metrics_payload = SprintStatsMetricsPayload {
        tasks: tasks_payload,
        hours: hours_payload,
        points: points_payload,
        status_breakdown: metrics.status_breakdown.clone(),
    };

    let timeline_payload = SprintStatsTimelinePayload {
        planned_start: lifecycle.planned_start.map(|dt| dt.to_rfc3339()),
        actual_start: lifecycle.actual_start.map(|dt| dt.to_rfc3339()),
        planned_end: lifecycle.planned_end.map(|dt| dt.to_rfc3339()),
        computed_end: lifecycle.computed_end.map(|dt| dt.to_rfc3339()),
        actual_end: lifecycle.actual_end.map(|dt| dt.to_rfc3339()),
        planned_duration_days,
        actual_duration_days,
        elapsed_days,
        remaining_days,
        overdue_days,
    };

    let payload = SprintStatsResponse {
        status: "ok",
        sprint: detail,
        lifecycle: SprintReviewLifecyclePayload::from_status(&lifecycle),
        metrics: metrics_payload,
        timeline: timeline_payload,
    };

    SprintStatsContext {
        payload,
        metrics,
        durations,
        summary,
        lifecycle,
    }
}

pub fn compute_sprint_summary(
    storage: &Storage,
    record: &SprintRecord,
    config: &ResolvedConfig,
    now: DateTime<Utc>,
) -> SprintSummaryContext {
    let lifecycle = sprint_status::derive_status(&record.sprint, now);
    let summary = SprintSummary::from_record(record, &lifecycle);
    let detail = SprintDetail::from_record(record, &summary, &lifecycle);

    let tasks = SprintService::load_tasks_for_record(storage, record);

    let done_statuses = determine_done_statuses_from_config(config);
    let blocked_statuses = determine_blocked_statuses_from_config(config);
    let metrics = summarize_sprint_tasks(&tasks, &done_statuses, &blocked_statuses);

    let capacity_points = record
        .sprint
        .plan
        .as_ref()
        .and_then(|plan| plan.capacity.as_ref())
        .and_then(|capacity| capacity.points)
        .map(|value| value as f64);
    let capacity_hours = record
        .sprint
        .plan
        .as_ref()
        .and_then(|plan| plan.capacity.as_ref())
        .and_then(|capacity| capacity.hours)
        .map(|value| value as f64);

    let durations = compute_sprint_durations(record, &lifecycle, now);

    let planned_duration_days = durations.planned.map(duration_to_days);
    let actual_duration_days = durations.actual.map(duration_to_days);
    let elapsed_days = durations.elapsed.map(duration_to_days);
    let remaining_days = durations.remaining.map(duration_to_days);
    let overdue_days = durations.overdue.map(duration_to_days);

    let tasks_payload = SprintStatsCountsPayload {
        committed: metrics.total_tasks,
        done: metrics.done_tasks,
        remaining: metrics.remaining_tasks_count(),
        completion_ratio: ratio_usize(metrics.done_tasks, metrics.total_tasks).unwrap_or(0.0),
    };

    let hours_payload = if metrics.total_hours > 0.0
        || metrics.done_hours > 0.0
        || metrics.remaining_hours() > 0.0
        || capacity_hours.is_some()
    {
        Some(SprintStatsEffortPayload {
            committed: metrics.total_hours,
            done: metrics.done_hours,
            remaining: metrics.remaining_hours(),
            completion_ratio: ratio(metrics.done_hours, metrics.total_hours).unwrap_or(0.0),
            capacity: capacity_hours,
            capacity_commitment_ratio: capacity_hours
                .and_then(|cap| ratio(metrics.total_hours, cap)),
            capacity_consumed_ratio: capacity_hours.and_then(|cap| ratio(metrics.done_hours, cap)),
        })
    } else {
        None
    };

    let points_payload = if metrics.total_points > 0.0
        || metrics.done_points > 0.0
        || metrics.remaining_points() > 0.0
        || capacity_points.is_some()
    {
        Some(SprintStatsEffortPayload {
            committed: metrics.total_points,
            done: metrics.done_points,
            remaining: metrics.remaining_points(),
            completion_ratio: ratio(metrics.done_points, metrics.total_points).unwrap_or(0.0),
            capacity: capacity_points,
            capacity_commitment_ratio: capacity_points
                .and_then(|cap| ratio(metrics.total_points, cap)),
            capacity_consumed_ratio: capacity_points
                .and_then(|cap| ratio(metrics.done_points, cap)),
        })
    } else {
        None
    };

    let metrics_payload = SprintSummaryReportMetrics {
        tasks: tasks_payload,
        hours: hours_payload,
        points: points_payload,
        blocked: metrics.blocked_tasks.len(),
    };

    let timeline_payload = SprintStatsTimelinePayload {
        planned_start: lifecycle.planned_start.map(|dt| dt.to_rfc3339()),
        actual_start: lifecycle.actual_start.map(|dt| dt.to_rfc3339()),
        planned_end: lifecycle.planned_end.map(|dt| dt.to_rfc3339()),
        computed_end: lifecycle.computed_end.map(|dt| dt.to_rfc3339()),
        actual_end: lifecycle.actual_end.map(|dt| dt.to_rfc3339()),
        planned_duration_days,
        actual_duration_days,
        elapsed_days,
        remaining_days,
        overdue_days,
    };

    let payload = SprintSummaryReportResponse {
        status: "ok",
        sprint: detail,
        lifecycle: SprintReviewLifecyclePayload::from_status(&lifecycle),
        metrics: metrics_payload,
        timeline: timeline_payload,
        blocked_tasks: metrics.blocked_tasks.clone(),
    };

    SprintSummaryContext {
        payload,
        metrics,
        durations,
        summary,
        lifecycle,
    }
}

pub fn compute_sprint_burndown(
    storage: &Storage,
    record: &SprintRecord,
    config: &ResolvedConfig,
    now: DateTime<Utc>,
) -> Result<SprintBurndownContext, String> {
    let lifecycle = sprint_status::derive_status(&record.sprint, now);
    let summary = SprintSummary::from_record(record, &lifecycle);
    let detail = SprintDetail::from_record(record, &summary, &lifecycle);

    let tasks = SprintService::load_tasks_for_record(storage, record);

    let done_statuses = determine_done_statuses_from_config(config);
    let computation = generate_burndown_series(record, &lifecycle, &tasks, &done_statuses)?;

    let payload = SprintBurndownResponse {
        status: "ok",
        sprint: detail,
        lifecycle: SprintReviewLifecyclePayload::from_status(&lifecycle),
        totals: SprintBurndownTotalsPayload {
            tasks: computation.totals.tasks,
            points: computation.totals.points,
            hours: computation.totals.hours,
        },
        series: computation
            .series
            .iter()
            .map(|point| SprintBurndownPointPayload {
                date: point.date.to_rfc3339(),
                remaining_tasks: point.remaining_tasks,
                ideal_tasks: point.ideal_tasks,
                remaining_points: point.remaining_points,
                ideal_points: point.ideal_points,
                remaining_hours: point.remaining_hours,
                ideal_hours: point.ideal_hours,
            })
            .collect(),
    };

    Ok(SprintBurndownContext {
        payload,
        computation,
        summary,
        lifecycle,
    })
}

pub fn compute_sprint_calendar(
    records: &[SprintRecord],
    include_complete: bool,
    limit: Option<usize>,
    now: DateTime<Utc>,
) -> SprintCalendarContext {
    let mut entries = Vec::new();
    let mut skipped_complete = false;

    for record in records {
        let lifecycle = sprint_status::derive_status(&record.sprint, now);
        if !include_complete && matches!(lifecycle.state, SprintLifecycleState::Complete) {
            skipped_complete = true;
            continue;
        }

        let summary = SprintSummary::from_record(record, &lifecycle);
        let lifecycle_payload = SprintReviewLifecyclePayload::from_status(&lifecycle);
        let start = resolve_calendar_start(record, &lifecycle);
        let end = resolve_calendar_end(record, &lifecycle);
        let duration = match (start, end) {
            (Some(begin), Some(finish)) if finish > begin => Some(finish - begin),
            _ => None,
        };
        let relative = format_calendar_relative(&lifecycle, start, end, now);
        let sort_source = start.or(end);
        let sort_key = sort_source
            .map(|dt| dt.timestamp())
            .unwrap_or_else(|| i64::MAX - record.id as i64);
        let warning_payloads = to_status_warning_payloads(&lifecycle.warnings);

        entries.push(SprintCalendarEntry {
            id: record.id,
            summary,
            lifecycle: lifecycle_payload,
            start,
            end,
            actual_start: lifecycle.actual_start,
            actual_end: lifecycle.actual_end,
            duration,
            relative,
            sort_key,
            warnings: warning_payloads,
        });
    }

    entries.sort_by(|a, b| {
        a.sort_key
            .cmp(&b.sort_key)
            .then(a.summary.id.cmp(&b.summary.id))
    });

    let mut truncated = false;
    if let Some(limit) = limit {
        if entries.len() > limit {
            entries.truncate(limit);
            truncated = true;
        }
    }

    let payload = SprintCalendarResponse {
        status: "ok",
        count: entries.len(),
        truncated,
        skipped_complete,
        sprints: entries
            .iter()
            .map(|entry| SprintCalendarEntryPayload {
                summary: entry.summary.clone(),
                lifecycle: entry.lifecycle.clone(),
                start: entry.start.map(|dt| dt.to_rfc3339()),
                end: entry.end.map(|dt| dt.to_rfc3339()),
                actual_start: entry.actual_start.map(|dt| dt.to_rfc3339()),
                actual_end: entry.actual_end.map(|dt| dt.to_rfc3339()),
                duration_days: entry.duration.map(duration_to_days),
                time_until_start_days: entry
                    .start
                    .map(|dt| duration_to_days(dt.signed_duration_since(now))),
                time_until_end_days: entry
                    .end
                    .map(|dt| duration_to_days(dt.signed_duration_since(now))),
                window: format_calendar_window(entry.start, entry.end, entry.duration),
                has_warnings: entry.summary.has_warnings,
                status_warnings: entry.warnings.clone(),
                relative: entry.relative.clone(),
            })
            .collect(),
    };

    SprintCalendarContext { payload, entries }
}

pub fn summarize_sprint_tasks(
    tasks: &[(String, StoredTask)],
    done_statuses: &HashSet<String>,
    blocked_statuses: &HashSet<String>,
) -> SprintTaskMetrics {
    let mut status_counts: BTreeMap<String, StatusAggregate> = BTreeMap::new();
    let mut remaining_tasks = Vec::new();
    let mut blocked_tasks = Vec::new();
    let mut done_tasks = 0usize;
    let mut total_hours = 0.0f64;
    let mut done_hours = 0.0f64;
    let mut total_points = 0.0f64;
    let mut done_points = 0.0f64;

    for (id, task) in tasks.iter() {
        let label = if task.status.is_empty() {
            "Unspecified".to_string()
        } else {
            task.status.as_str().to_string()
        };
        let key = label.to_ascii_lowercase();
        let is_done = done_statuses.contains(&key);
        let is_blocked =
            blocked_statuses.contains(&key) || label.to_ascii_lowercase().contains("block");
        let entry = status_counts
            .entry(key.clone())
            .or_insert_with(|| StatusAggregate {
                label: label.clone(),
                count: 0,
                done: is_done,
            });
        entry.count += 1;
        if entry.done {
            done_tasks += 1;
        } else {
            remaining_tasks.push(SprintReviewTask {
                id: id.clone(),
                title: task.title.clone(),
                status: label.clone(),
                assignee: task.assignee.clone(),
            });
            if is_blocked {
                blocked_tasks.push(SprintReviewTask {
                    id: id.clone(),
                    title: task.title.clone(),
                    status: label.clone(),
                    assignee: task.assignee.clone(),
                });
            }
        }

        if let Some(effort_str) = task.effort.as_ref() {
            if let Ok(parsed) = effort::parse_effort(effort_str) {
                match parsed.kind {
                    EffortKind::TimeHours(hours) => {
                        total_hours += hours;
                        if is_done {
                            done_hours += hours;
                        }
                    }
                    EffortKind::Points(points) => {
                        total_points += points;
                        if is_done {
                            done_points += points;
                        }
                    }
                }
            }
        }
    }

    remaining_tasks.sort_by(|a, b| {
        a.status
            .to_ascii_lowercase()
            .cmp(&b.status.to_ascii_lowercase())
            .then(a.id.cmp(&b.id))
    });

    blocked_tasks.sort_by(|a, b| a.id.cmp(&b.id));

    let status_breakdown: Vec<SprintReviewStatusMetric> = status_counts
        .into_values()
        .map(|aggregate| SprintReviewStatusMetric {
            status: aggregate.label,
            count: aggregate.count,
            done: aggregate.done,
        })
        .collect();

    SprintTaskMetrics {
        total_tasks: tasks.len(),
        done_tasks,
        status_breakdown,
        remaining_tasks,
        blocked_tasks,
        total_hours,
        done_hours,
        total_points,
        done_points,
    }
}

pub fn generate_burndown_series(
    record: &SprintRecord,
    lifecycle: &SprintLifecycleStatus,
    tasks: &[(String, StoredTask)],
    done_statuses: &HashSet<String>,
) -> Result<SprintBurndownComputation, String> {
    let (start, end) = resolve_burndown_window(record, lifecycle, tasks, done_statuses)?;

    let start_day = start.date_naive();
    let end_day = end.date_naive();
    let day_span = end_day.signed_duration_since(start_day).num_days().max(0) as usize;
    let series_len = day_span + 1;

    let mut items = Vec::with_capacity(tasks.len());
    let mut total_points = 0.0f64;
    let mut total_hours = 0.0f64;

    for (_, task) in tasks {
        let done_at = find_done_timestamp(task, done_statuses);
        let (points, hours) = parse_effort_values(task);
        total_points += points;
        total_hours += hours;
        items.push(SprintBurndownItem {
            done_at,
            points,
            hours,
        });
    }

    let total_tasks = items.len();
    let points_available = if total_points > 0.000_1 {
        Some(total_points)
    } else {
        None
    };
    let hours_available = if total_hours > 0.000_1 {
        Some(total_hours)
    } else {
        None
    };

    let start_anchor = start_day
        .and_hms_opt(0, 0, 0)
        .expect("valid start midnight");
    let mut series = Vec::with_capacity(series_len.max(1));

    for i in 0..=day_span {
        let day_start = Utc.from_utc_datetime(&(start_anchor + Duration::days(i as i64)));
        let day_end = day_start + Duration::days(1);

        let mut completed_tasks = 0usize;
        let mut completed_points = 0.0f64;
        let mut completed_hours = 0.0f64;
        for item in &items {
            if let Some(done_at) = item.done_at {
                if done_at < day_end {
                    completed_tasks += 1;
                    completed_points += item.points;
                    completed_hours += item.hours;
                }
            }
        }

        let remaining_tasks = total_tasks.saturating_sub(completed_tasks);
        let mut remaining_points =
            points_available.map(|total| (total - completed_points).max(0.0));
        let mut remaining_hours = hours_available.map(|total| (total - completed_hours).max(0.0));

        if let Some(value) = remaining_points.as_mut() {
            if value.abs() < 0.000_1 {
                *value = 0.0;
            }
        }
        if let Some(value) = remaining_hours.as_mut() {
            if value.abs() < 0.000_1 {
                *value = 0.0;
            }
        }

        let fraction = if day_span == 0 {
            if i == 0 { 0.0 } else { 1.0 }
        } else {
            i as f64 / day_span as f64
        };

        let ideal_tasks = (total_tasks as f64) * (1.0 - fraction);
        let ideal_points = points_available.map(|total| total * (1.0 - fraction));
        let ideal_hours = hours_available.map(|total| total * (1.0 - fraction));

        series.push(SprintBurndownPoint {
            date: day_start,
            remaining_tasks,
            ideal_tasks,
            remaining_points,
            ideal_points,
            remaining_hours,
            ideal_hours,
        });
    }

    let totals = SprintBurndownTotals {
        tasks: total_tasks,
        points: points_available,
        hours: hours_available,
    };

    Ok(SprintBurndownComputation {
        totals,
        series,
        day_span,
    })
}

fn parse_effort_values(task: &StoredTask) -> (f64, f64) {
    if let Some(effort_str) = task.effort.as_ref() {
        if let Ok(parsed) = effort::parse_effort(effort_str) {
            return match parsed.kind {
                EffortKind::Points(points) => (points, 0.0),
                EffortKind::TimeHours(hours) => (0.0, hours),
            };
        }
    }
    (0.0, 0.0)
}

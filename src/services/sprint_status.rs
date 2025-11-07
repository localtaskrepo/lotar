use chrono::{DateTime, Duration, Utc};

use crate::storage::sprint::Sprint;
use crate::utils::time;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SprintLifecycleState {
    Pending,
    Active,
    Overdue,
    Complete,
}

impl SprintLifecycleState {
    pub fn as_str(&self) -> &'static str {
        match self {
            SprintLifecycleState::Pending => "pending",
            SprintLifecycleState::Active => "active",
            SprintLifecycleState::Overdue => "overdue",
            SprintLifecycleState::Complete => "complete",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SprintStatusWarning {
    UnparseableTimestamp { field: &'static str, value: String },
    UnparseableLength { field: &'static str, value: String },
}

impl SprintStatusWarning {
    pub fn code(&self) -> &'static str {
        match self {
            SprintStatusWarning::UnparseableTimestamp { .. } => "unparseable_timestamp",
            SprintStatusWarning::UnparseableLength { .. } => "unparseable_length",
        }
    }

    pub fn message(&self) -> String {
        match self {
            SprintStatusWarning::UnparseableTimestamp { field, value } => {
                format!("{} has an invalid timestamp ('{}').", field, value)
            }
            SprintStatusWarning::UnparseableLength { field, value } => {
                format!("{} has an invalid duration ('{}').", field, value)
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct SprintLifecycleStatus {
    pub state: SprintLifecycleState,
    pub planned_start: Option<DateTime<Utc>>,
    pub planned_end: Option<DateTime<Utc>>,
    pub actual_start: Option<DateTime<Utc>>,
    pub actual_end: Option<DateTime<Utc>>,
    pub computed_end: Option<DateTime<Utc>>,
    pub warnings: Vec<SprintStatusWarning>,
}

impl SprintLifecycleStatus {
    pub fn label(&self) -> &'static str {
        self.state.as_str()
    }
}

pub fn derive_status(sprint: &Sprint, now: DateTime<Utc>) -> SprintLifecycleStatus {
    let mut warnings = Vec::new();
    let plan = sprint.plan.as_ref();
    let actual = sprint.actual.as_ref();

    let planned_start = parse_timestamp(
        "plan.starts_at",
        plan.and_then(|p| p.starts_at.as_ref()),
        &mut warnings,
    );
    let planned_end = parse_timestamp(
        "plan.ends_at",
        plan.and_then(|p| p.ends_at.as_ref()),
        &mut warnings,
    );
    let actual_start = parse_timestamp(
        "actual.started_at",
        actual.and_then(|a| a.started_at.as_ref()),
        &mut warnings,
    );
    let actual_end = parse_timestamp(
        "actual.closed_at",
        actual.and_then(|a| a.closed_at.as_ref()),
        &mut warnings,
    );
    let length = parse_length(
        "plan.length",
        plan.and_then(|p| p.length.as_ref()),
        &mut warnings,
    );

    let computed_end = if let Some(end) = planned_end {
        Some(end)
    } else if let (Some(actual_start), Some(length)) = (actual_start.as_ref(), length.as_ref()) {
        Some(*actual_start + *length)
    } else if let (Some(planned_start), Some(length)) = (planned_start.as_ref(), length.as_ref()) {
        Some(*planned_start + *length)
    } else {
        None
    };

    let state = if actual_end.is_some() {
        SprintLifecycleState::Complete
    } else if let Some(end_at) = computed_end.as_ref() {
        if now > *end_at {
            SprintLifecycleState::Overdue
        } else if actual_start.is_some() {
            SprintLifecycleState::Active
        } else {
            SprintLifecycleState::Pending
        }
    } else if actual_start.is_some() {
        SprintLifecycleState::Active
    } else {
        SprintLifecycleState::Pending
    };

    SprintLifecycleStatus {
        state,
        planned_start,
        planned_end,
        actual_start,
        actual_end,
        computed_end,
        warnings,
    }
}

fn parse_timestamp(
    field: &'static str,
    value: Option<&String>,
    warnings: &mut Vec<SprintStatusWarning>,
) -> Option<DateTime<Utc>> {
    value.and_then(|raw| {
        let trimmed = raw.trim();
        if trimmed.is_empty() {
            return None;
        }
        match time::parse_human_datetime_to_utc(trimmed) {
            Ok(dt) => Some(dt),
            Err(_) => {
                warnings.push(SprintStatusWarning::UnparseableTimestamp {
                    field,
                    value: raw.clone(),
                });
                None
            }
        }
    })
}

fn parse_length(
    field: &'static str,
    value: Option<&String>,
    warnings: &mut Vec<SprintStatusWarning>,
) -> Option<Duration> {
    value.and_then(|raw| {
        let trimmed = raw.trim();
        if trimmed.is_empty() {
            return None;
        }
        match time::parse_duration_like(trimmed) {
            Some(duration) => Some(duration),
            None => {
                warnings.push(SprintStatusWarning::UnparseableLength {
                    field,
                    value: raw.clone(),
                });
                None
            }
        }
    })
}

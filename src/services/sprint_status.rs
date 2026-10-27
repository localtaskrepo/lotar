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

    /// True while a sprint is running (Active) or past its end without
    /// completion (Overdue) — the "in flight" predicate used by default-sprint
    /// resolution, velocity windows, and lifecycle helpers.
    pub fn is_active_or_overdue(&self) -> bool {
        matches!(
            self,
            SprintLifecycleState::Active | SprintLifecycleState::Overdue
        )
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
    /// When the sprint turns overdue: `computed_end` plus `plan.overdue_after`.
    pub overdue_at: Option<DateTime<Utc>>,
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
    // Grace period after the end before the sprint counts as overdue
    // (DEV-154). Unreadable legacy values warn and give no grace.
    let grace = plan
        .and_then(|p| p.overdue_after.as_ref())
        .and_then(|raw| {
            let trimmed = raw.trim();
            if trimmed.is_empty() {
                return None;
            }
            match time::parse_sprint_overdue_after(trimmed) {
                Ok(duration) => Some(duration),
                Err(_) => {
                    warnings.push(SprintStatusWarning::UnparseableLength {
                        field: "plan.overdue_after",
                        value: raw.clone(),
                    });
                    None
                }
            }
        })
        .unwrap_or_else(Duration::zero);

    let computed_end = if let Some(end) = planned_end {
        Some(end)
    } else if let (Some(actual_start), Some(length)) = (actual_start.as_ref(), length.as_ref()) {
        // Checked: a legacy out-of-range length must not panic status reads.
        actual_start.checked_add_signed(*length)
    } else if let (Some(planned_start), Some(length)) = (planned_start.as_ref(), length.as_ref()) {
        planned_start.checked_add_signed(*length)
    } else {
        None
    };

    let overdue_at = computed_end.map(|end| end.checked_add_signed(grace).unwrap_or(end));

    let state = if actual_end.is_some() {
        SprintLifecycleState::Complete
    } else if let Some(overdue_at) = overdue_at {
        if now > overdue_at {
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
        overdue_at,
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

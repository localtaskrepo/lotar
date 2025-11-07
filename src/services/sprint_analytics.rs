use crate::services::sprint_service::SprintRecord;
use crate::services::sprint_status::{SprintLifecycleStatus, SprintStatusWarning};
use crate::storage::sprint::Sprint;
use serde::Serialize;

#[cfg(feature = "schema")]
use schemars::JsonSchema;

#[derive(Debug, Clone, Serialize)]
#[cfg_attr(feature = "schema", derive(JsonSchema))]
pub struct SprintStatusWarningPayload {
    pub code: &'static str,
    pub message: String,
}

pub fn to_status_warning_payloads(
    warnings: &[SprintStatusWarning],
) -> Vec<SprintStatusWarningPayload> {
    warnings
        .iter()
        .map(|warning| SprintStatusWarningPayload {
            code: warning.code(),
            message: warning.message(),
        })
        .collect()
}

#[derive(Debug, Clone, Serialize)]
#[cfg_attr(feature = "schema", derive(JsonSchema))]
pub struct SprintReviewLifecyclePayload {
    pub status: String,
    pub state: String,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub planned_start: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub planned_end: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub actual_start: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub actual_end: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub computed_end: Option<String>,
}

impl SprintReviewLifecyclePayload {
    pub fn from_status(status: &SprintLifecycleStatus) -> Self {
        Self {
            status: status.label().to_string(),
            state: status.state.as_str().to_string(),
            planned_start: status.planned_start.as_ref().map(|dt| dt.to_rfc3339()),
            planned_end: status.planned_end.as_ref().map(|dt| dt.to_rfc3339()),
            actual_start: status.actual_start.as_ref().map(|dt| dt.to_rfc3339()),
            actual_end: status.actual_end.as_ref().map(|dt| dt.to_rfc3339()),
            computed_end: status.computed_end.as_ref().map(|dt| dt.to_rfc3339()),
        }
    }
}

#[derive(Debug, Clone, Serialize)]
#[cfg_attr(feature = "schema", derive(JsonSchema))]
pub struct SprintSummary {
    pub id: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    pub status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub goal: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub starts_at: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ends_at: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub computed_end: Option<String>,
    #[serde(skip_serializing_if = "is_false", default)]
    pub has_warnings: bool,
}

fn is_false(value: &bool) -> bool {
    !*value
}

impl SprintSummary {
    pub fn from_record(record: &SprintRecord, lifecycle: &SprintLifecycleStatus) -> Self {
        let plan = record.sprint.plan.as_ref();
        let actual = record.sprint.actual.as_ref();

        let label = plan.and_then(|p| p.label.clone());
        let goal = plan.and_then(|p| p.goal.clone());
        let starts_at = actual
            .and_then(|a| a.started_at.clone())
            .or_else(|| plan.and_then(|p| p.starts_at.clone()));
        let ends_at = plan
            .and_then(|p| p.ends_at.clone())
            .or_else(|| lifecycle.computed_end.as_ref().map(|dt| dt.to_rfc3339()));
        let computed_end = lifecycle.computed_end.as_ref().map(|dt| dt.to_rfc3339());
        let has_warnings = !lifecycle.warnings.is_empty();

        Self {
            id: record.id,
            label,
            status: lifecycle.label().to_string(),
            goal,
            starts_at,
            ends_at,
            computed_end,
            has_warnings,
        }
    }
}

#[derive(Debug, Serialize, Clone)]
#[cfg_attr(feature = "schema", derive(JsonSchema))]
pub struct SprintDetail {
    pub id: u32,
    pub status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub goal: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub starts_at: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ends_at: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub computed_end: Option<String>,
    #[serde(skip_serializing_if = "is_false", default)]
    pub has_warnings: bool,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub status_warnings: Vec<SprintStatusWarningPayload>,
    pub sprint: Sprint,
}

impl SprintDetail {
    pub fn from_record(
        record: &SprintRecord,
        summary: &SprintSummary,
        lifecycle: &SprintLifecycleStatus,
    ) -> Self {
        let computed_end = lifecycle.computed_end.as_ref().map(|dt| dt.to_rfc3339());
        let status_warnings = to_status_warning_payloads(&lifecycle.warnings);

        Self {
            id: record.id,
            status: summary.status.clone(),
            label: summary.label.clone(),
            goal: summary.goal.clone(),
            starts_at: summary.starts_at.clone(),
            ends_at: summary.ends_at.clone(),
            computed_end,
            has_warnings: summary.has_warnings,
            status_warnings,
            sprint: record.sprint.clone(),
        }
    }
}

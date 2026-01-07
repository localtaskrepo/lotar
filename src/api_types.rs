use serde::de::Deserializer;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap};

#[cfg(feature = "schema")]
use schemars::JsonSchema;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(JsonSchema))]
pub struct TaskDTO {
    pub id: String,
    pub title: String,
    pub status: crate::types::TaskStatus,
    pub priority: crate::types::Priority,
    pub task_type: crate::types::TaskType,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub reporter: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub assignee: Option<String>,
    pub created: String,
    pub modified: String,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub due_date: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub effort: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub subtitle: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub description: Option<String>,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub tags: Vec<String>,
    #[serde(
        skip_serializing_if = "crate::types::TaskRelationships::is_empty",
        default
    )]
    pub relationships: crate::types::TaskRelationships,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub comments: Vec<crate::types::TaskComment>,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub references: Vec<crate::types::ReferenceEntry>,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub sprints: Vec<u32>,
    #[serde(
        skip_serializing_if = "crate::api_types::btreemap_u32_is_empty",
        default
    )]
    pub sprint_order: BTreeMap<u32, u32>,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub history: Vec<crate::types::TaskChangeLogEntry>,
    #[serde(skip_serializing_if = "crate::api_types::map_is_empty", default)]
    pub custom_fields: crate::types::CustomFields,
}

// Helper to allow skipping empty maps for custom_fields
pub fn map_is_empty(map: &crate::types::CustomFields) -> bool {
    // CustomFields is a type alias for HashMap<String, Value>
    let as_hash: &HashMap<String, crate::types::CustomFieldValue> = map;
    as_hash.is_empty()
}

pub fn btreemap_u32_is_empty(map: &BTreeMap<u32, u32>) -> bool {
    map.is_empty()
}

fn deserialize_double_option<'de, D, T>(deserializer: D) -> Result<Option<Option<T>>, D::Error>
where
    D: Deserializer<'de>,
    T: Deserialize<'de>,
{
    let inner = Option::<T>::deserialize(deserializer)?;
    Ok(Some(inner))
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[cfg_attr(feature = "schema", derive(JsonSchema))]
pub struct TaskCreate {
    pub title: String,
    pub project: Option<String>,
    pub priority: Option<crate::types::Priority>,
    pub task_type: Option<crate::types::TaskType>,
    pub reporter: Option<String>,
    pub assignee: Option<String>,
    pub due_date: Option<String>,
    pub effort: Option<String>,
    pub description: Option<String>,
    pub tags: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub relationships: Option<crate::types::TaskRelationships>,
    pub custom_fields: Option<crate::types::CustomFields>,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub sprints: Vec<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[cfg_attr(feature = "schema", derive(JsonSchema))]
pub struct TaskUpdate {
    pub title: Option<String>,
    pub status: Option<crate::types::TaskStatus>,
    pub priority: Option<crate::types::Priority>,
    pub task_type: Option<crate::types::TaskType>,
    pub reporter: Option<String>,
    pub assignee: Option<String>,
    pub due_date: Option<String>,
    pub effort: Option<String>,
    pub description: Option<String>,
    pub tags: Option<Vec<String>>, // replace whole list
    pub relationships: Option<crate::types::TaskRelationships>,
    pub custom_fields: Option<crate::types::CustomFields>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub sprints: Option<Vec<u32>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(JsonSchema))]
pub struct AttachmentUploadRequest {
    pub id: String,
    pub filename: String,
    pub content_base64: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(JsonSchema))]
pub struct AttachmentUploadResponse {
    pub stored_path: String,
    pub attached: bool,
    pub task: TaskDTO,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(JsonSchema))]
pub struct AttachmentRemoveRequest {
    pub id: String,
    pub stored_path: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(JsonSchema))]
pub struct AttachmentRemoveResponse {
    pub task: TaskDTO,
    pub deleted: bool,
    pub still_referenced: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(JsonSchema))]
pub struct LinkReferenceAddRequest {
    pub id: String,
    pub url: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(JsonSchema))]
pub struct LinkReferenceAddResponse {
    pub task: TaskDTO,
    pub added: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(JsonSchema))]
pub struct LinkReferenceRemoveRequest {
    pub id: String,
    pub url: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(JsonSchema))]
pub struct LinkReferenceRemoveResponse {
    pub task: TaskDTO,
    pub removed: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(JsonSchema))]
pub struct CodeReferenceAddRequest {
    pub id: String,
    pub code: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(JsonSchema))]
pub struct CodeReferenceAddResponse {
    pub task: TaskDTO,
    pub added: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(JsonSchema))]
pub struct CodeReferenceRemoveRequest {
    pub id: String,
    pub code: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(JsonSchema))]
pub struct CodeReferenceRemoveResponse {
    pub task: TaskDTO,
    pub removed: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[cfg_attr(feature = "schema", derive(JsonSchema))]
pub struct TaskListFilter {
    pub status: Vec<crate::types::TaskStatus>,
    pub priority: Vec<crate::types::Priority>,
    pub task_type: Vec<crate::types::TaskType>,
    pub project: Option<String>,
    pub tags: Vec<String>,
    pub text_query: Option<String>,
    pub sprints: Vec<u32>,
    #[serde(default)]
    pub custom_fields: BTreeMap<String, Vec<String>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[cfg_attr(feature = "schema", derive(JsonSchema))]
pub struct TaskSelection {
    #[serde(default)]
    pub filter: TaskListFilter,
    #[serde(default)]
    pub r#where: Vec<(String, String)>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(JsonSchema))]
#[serde(untagged)]
pub enum SprintSelector {
    Id(u32),
    Keyword(String),
}

impl SprintSelector {
    pub fn as_reference(&self) -> String {
        match self {
            SprintSelector::Id(id) => id.to_string(),
            SprintSelector::Keyword(value) => value.clone(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[cfg_attr(feature = "schema", derive(JsonSchema))]
pub struct SprintAssignmentRequest {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sprint: Option<SprintSelector>,
    #[serde(default)]
    pub tasks: Vec<String>,
    #[serde(default)]
    pub allow_closed: bool,
    #[serde(default)]
    pub cleanup_missing: bool,
    #[serde(default)]
    pub force_single: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub selection: Option<TaskSelection>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[cfg_attr(feature = "schema", derive(JsonSchema))]
pub struct SprintCreateRequest {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub goal: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub plan_length: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ends_at: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub starts_at: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub capacity_points: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub capacity_hours: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub overdue_after: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub notes: Option<String>,
    #[serde(default)]
    pub skip_defaults: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(JsonSchema))]
pub struct SprintCreateResponse {
    pub status: String,
    pub sprint: SprintListItem,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub warnings: Vec<String>,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub applied_defaults: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(JsonSchema))]
pub struct SprintUpdateRequest {
    pub sprint: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub goal: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub plan_length: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ends_at: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub starts_at: Option<String>,
    #[serde(
        skip_serializing_if = "Option::is_none",
        default,
        deserialize_with = "crate::api_types::deserialize_double_option"
    )]
    pub capacity_points: Option<Option<u32>>,
    #[serde(
        skip_serializing_if = "Option::is_none",
        default,
        deserialize_with = "crate::api_types::deserialize_double_option"
    )]
    pub capacity_hours: Option<Option<u32>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub overdue_after: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub notes: Option<String>,
    #[serde(
        skip_serializing_if = "Option::is_none",
        default,
        deserialize_with = "crate::api_types::deserialize_double_option"
    )]
    pub actual_started_at: Option<Option<String>>,
    #[serde(
        skip_serializing_if = "Option::is_none",
        default,
        deserialize_with = "crate::api_types::deserialize_double_option"
    )]
    pub actual_closed_at: Option<Option<String>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(JsonSchema))]
pub struct SprintUpdateResponse {
    pub status: String,
    pub sprint: SprintListItem,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[cfg_attr(feature = "schema", derive(JsonSchema))]
pub struct SprintReassignment {
    pub task_id: String,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub previous: Vec<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(JsonSchema))]
pub struct SprintAssignmentResponse {
    pub status: String,
    pub action: String,
    pub sprint_id: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sprint_label: Option<String>,
    pub modified: Vec<String>,
    pub unchanged: Vec<String>,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub replaced: Vec<SprintReassignment>,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub messages: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub integrity: Option<SprintIntegrityDiagnostics>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[cfg_attr(feature = "schema", derive(JsonSchema))]
pub struct SprintDeleteRequest {
    pub sprint: u32,
    #[serde(default)]
    pub cleanup_missing: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(JsonSchema))]
pub struct SprintDeleteResponse {
    pub status: String,
    pub deleted: bool,
    pub sprint_id: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sprint_label: Option<String>,
    pub removed_references: usize,
    pub updated_tasks: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub integrity: Option<SprintIntegrityDiagnostics>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[cfg_attr(feature = "schema", derive(JsonSchema))]
pub struct SprintIntegrityDiagnostics {
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub missing_sprints: Vec<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tasks_with_missing: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub auto_cleanup: Option<SprintCleanupSummary>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[cfg_attr(feature = "schema", derive(JsonSchema))]
pub struct SprintCleanupSummary {
    pub removed_references: usize,
    pub updated_tasks: usize,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub removed_by_sprint: Vec<SprintCleanupMetric>,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub remaining_missing: Vec<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(JsonSchema))]
pub struct SprintCleanupMetric {
    pub sprint_id: u32,
    pub count: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(JsonSchema))]
pub struct SprintBacklogItem {
    pub id: String,
    pub title: String,
    pub status: String,
    pub priority: String,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub assignee: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub due_date: Option<String>,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub tags: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(JsonSchema))]
pub struct SprintBacklogResponse {
    pub status: String,
    pub count: usize,
    pub truncated: bool,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub tasks: Vec<SprintBacklogItem>,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub missing_sprints: Vec<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub integrity: Option<SprintIntegrityDiagnostics>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(JsonSchema))]
pub struct SprintListItem {
    pub id: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    pub display_name: String,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub created: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub modified: Option<String>,
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
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub goal: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub plan_length: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub overdue_after: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub notes: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub capacity_points: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub capacity_hours: Option<u32>,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(JsonSchema))]
pub struct SprintListResponse {
    pub status: String,
    pub count: usize,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub sprints: Vec<SprintListItem>,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub missing_sprints: Vec<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub integrity: Option<SprintIntegrityDiagnostics>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(JsonSchema))]
pub struct ProjectDTO {
    pub name: String,
    pub prefix: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[cfg_attr(feature = "schema", derive(JsonSchema))]
pub struct ProjectStatsDTO {
    pub name: String,
    pub open_count: u64,
    pub done_count: u64,
    pub recent_modified: Option<String>,
    pub tags_top: Vec<String>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn sprint_update_request_handles_absent_closed_at() {
        let payload: SprintUpdateRequest = serde_json::from_value(json!({
            "sprint": 1
        }))
        .unwrap();
        assert!(payload.actual_closed_at.is_none());
    }

    #[test]
    fn sprint_update_request_handles_null_closed_at() {
        let payload: SprintUpdateRequest = serde_json::from_value(json!({
            "sprint": 1,
            "actual_closed_at": null
        }))
        .unwrap();
        assert!(matches!(payload.actual_closed_at, Some(None)));
    }

    #[test]
    fn sprint_update_request_handles_value_closed_at() {
        let iso = "2025-01-01T00:00:00Z";
        let payload: SprintUpdateRequest = serde_json::from_value(json!({
            "sprint": 1,
            "actual_closed_at": iso
        }))
        .unwrap();
        assert!(matches!(payload.actual_closed_at, Some(Some(ref value)) if value == iso));
    }

    #[test]
    fn sprint_update_request_handles_null_capacity_points() {
        let payload: SprintUpdateRequest = serde_json::from_value(json!({
            "sprint": 1,
            "capacity_points": null
        }))
        .unwrap();
        assert!(matches!(payload.capacity_points, Some(None)));
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(JsonSchema))]
pub struct ProjectCreateRequest {
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub prefix: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub values: Option<BTreeMap<String, String>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(JsonSchema))]
pub struct ReferenceSnippetLineDTO {
    pub number: usize,
    pub text: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(JsonSchema))]
pub struct ReferenceSnippetDTO {
    pub path: String,
    pub start_line: usize,
    pub end_line: usize,
    pub highlight_start: usize,
    pub highlight_end: usize,
    pub lines: Vec<ReferenceSnippetLineDTO>,
    pub has_more_before: bool,
    pub has_more_after: bool,
    pub total_lines: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(JsonSchema))]
pub struct ApiErrorPayload {
    pub code: String,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub details: Option<serde_json::Value>,
}

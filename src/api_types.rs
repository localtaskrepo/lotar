use serde::{Deserialize, Serialize};
use std::collections::HashMap;

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
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub category: Option<String>,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub tags: Vec<String>,
    #[serde(
        skip_serializing_if = "crate::types::TaskRelationships::is_empty",
        default
    )]
    pub relationships: crate::types::TaskRelationships,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub comments: Vec<crate::types::TaskComment>,
    #[serde(skip_serializing_if = "crate::api_types::map_is_empty", default)]
    pub custom_fields: crate::types::CustomFields,
}

// Helper to allow skipping empty maps for custom_fields
pub fn map_is_empty(map: &crate::types::CustomFields) -> bool {
    // CustomFields is a type alias for HashMap<String, Value>
    let as_hash: &HashMap<String, crate::types::CustomFieldValue> = map;
    as_hash.is_empty()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(JsonSchema))]
pub struct TaskCreate {
    pub title: String,
    pub project: Option<String>,
    pub priority: Option<crate::types::Priority>,
    pub task_type: Option<crate::types::TaskType>,
    pub assignee: Option<String>,
    pub due_date: Option<String>,
    pub effort: Option<String>,
    pub description: Option<String>,
    pub category: Option<String>,
    pub tags: Vec<String>,
    pub custom_fields: Option<crate::types::CustomFields>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[cfg_attr(feature = "schema", derive(JsonSchema))]
pub struct TaskUpdate {
    pub title: Option<String>,
    pub status: Option<crate::types::TaskStatus>,
    pub priority: Option<crate::types::Priority>,
    pub task_type: Option<crate::types::TaskType>,
    pub assignee: Option<String>,
    pub due_date: Option<String>,
    pub effort: Option<String>,
    pub description: Option<String>,
    pub category: Option<String>,
    pub tags: Option<Vec<String>>, // replace whole list
    pub custom_fields: Option<crate::types::CustomFields>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[cfg_attr(feature = "schema", derive(JsonSchema))]
pub struct TaskListFilter {
    pub status: Vec<crate::types::TaskStatus>,
    pub priority: Vec<crate::types::Priority>,
    pub task_type: Vec<crate::types::TaskType>,
    pub project: Option<String>,
    pub category: Option<String>,
    pub tags: Vec<String>,
    pub text_query: Option<String>,
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

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(JsonSchema))]
pub struct ApiErrorPayload {
    pub code: String,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub details: Option<serde_json::Value>,
}

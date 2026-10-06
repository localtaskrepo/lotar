use crate::types::{Priority, TaskStatus, TaskType};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// Lifecycle visibility for task queries (DEV-92).
///
/// `Active` (the default) hides soft-deleted tombstones, `Deleted` selects
/// only tombstones, and `All` returns every stored task file. Serialized
/// lowercase `active`|`deleted`|`all`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(rename_all = "lowercase")]
pub enum DeletionFilter {
    #[default]
    Active,
    Deleted,
    All,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct TaskFilter {
    pub status: Vec<TaskStatus>,
    pub priority: Vec<Priority>,
    pub task_type: Vec<TaskType>,
    pub project: Option<String>,
    pub tags: Vec<String>,
    pub text_query: Option<String>,
    pub sprints: Vec<u32>,
    #[serde(default)]
    pub custom_fields: BTreeMap<String, Vec<String>>,
    #[serde(default)]
    pub deletion: DeletionFilter,
}

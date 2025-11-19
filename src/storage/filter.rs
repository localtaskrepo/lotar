use crate::types::{Priority, TaskStatus, TaskType};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

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
}

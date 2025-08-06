use serde::{Serialize, Deserialize};
use crate::types::{TaskStatus, Priority};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskFilter {
    pub status: Vec<TaskStatus>,
    pub priority: Vec<Priority>,
    pub task_type: Vec<crate::types::TaskType>,
    pub project: Option<String>,
    pub category: Option<String>,
    pub tags: Vec<String>,
    pub text_query: Option<String>,
}

impl Default for TaskFilter {
    fn default() -> Self {
        Self {
            status: vec![],
            priority: vec![],
            task_type: vec![],
            project: None,
            category: None,
            tags: vec![],
            text_query: None,
        }
    }
}

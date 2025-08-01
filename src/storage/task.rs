use std::collections::HashMap;
use std::path::PathBuf;
use serde::{Serialize, Deserialize};
use std::fmt;
use crate::types::{TaskStatus, TaskType, Priority, TaskRelationships, TaskComment, CustomFields};

#[derive(Clone, Serialize, Deserialize)]
pub struct Task {
    // Built-in standard fields (special handling in UI)
    // Note: ID is no longer stored in file - it's derived from folder+filename
    pub title: String,
    #[serde(skip_serializing_if = "TaskStatus::is_default", default)]
    pub status: TaskStatus,
    #[serde(skip_serializing_if = "Priority::is_default", default)]
    pub priority: Priority,
    #[serde(skip_serializing_if = "TaskType::is_default", default)]
    pub task_type: TaskType,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub assignee: Option<String>,
    pub created: String,
    pub modified: String,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub due_date: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub effort: Option<String>, // e.g., "5d", "2w", "3h"

    // Built-in structured fields (special UI components)
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub acceptance_criteria: Vec<String>,
    #[serde(skip_serializing_if = "TaskRelationships::is_empty", default)]
    pub relationships: TaskRelationships,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub comments: Vec<TaskComment>,

    // Legacy fields (keeping for backward compatibility)
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub subtitle: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub category: Option<String>,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub tags: Vec<String>,

    // Team-specific custom fields (generic UI treatment based on type)
    #[serde(skip_serializing_if = "std::collections::HashMap::is_empty", default)]
    pub custom_fields: CustomFields,
}

impl Task {
    pub fn new(_root_path: PathBuf, title: String, priority: Priority) -> Self {
        let now = chrono::Utc::now().to_rfc3339();

        Self {
            title,
            status: TaskStatus::default(),
            priority,
            task_type: TaskType::default(),
            assignee: None,
            created: now.clone(),
            modified: now,
            due_date: None,
            effort: None,
            acceptance_criteria: vec![],
            relationships: TaskRelationships::default(),
            comments: vec![],
            subtitle: None,
            description: None,
            category: None,
            tags: vec![],
            custom_fields: HashMap::new(),
        }
    }

    pub fn update_status(&mut self, new_status: TaskStatus) -> Result<(), String> {
        // TODO: Add transition validation once we implement transitions.yaml
        self.status = new_status;
        self.modified = chrono::Utc::now().to_rfc3339();
        Ok(())
    }

    pub fn update_modified(&mut self) {
        self.modified = chrono::Utc::now().to_rfc3339();
    }
}

impl fmt::Display for Task {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "title: {}\nstatus: {}\nsubtitle: {:?}\ndescription: {:?}\npriority: {}\ncategory: {:?}\ncreated: {}\nmodified: {}\ndue_date: {:?}\ntags: {:?}",
               self.title, self.status, self.subtitle, self.description, self.priority, self.category, self.created, self.modified, self.due_date, self.tags)
    }
}

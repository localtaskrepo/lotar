use crate::types::{
    CustomFields, Priority, ReferenceEntry, TaskComment, TaskRelationships, TaskStatus, TaskType,
};
use serde::{Deserialize, Serialize};
use std::fmt;
use std::path::PathBuf;

#[derive(Clone, Default, Serialize, Deserialize)]
pub struct Task {
    // Built-in standard fields (special handling in UI)
    // Note: ID is no longer stored in file - it's derived from folder+filename
    pub title: String,
    #[serde(skip_serializing_if = "TaskStatus::is_empty", default)]
    pub status: TaskStatus,
    #[serde(skip_serializing_if = "Priority::is_empty", default)]
    pub priority: Priority,
    #[serde(
        rename = "type",
        alias = "task_type",
        skip_serializing_if = "TaskType::is_empty",
        default
    )]
    pub task_type: TaskType,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub reporter: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub assignee: Option<String>,
    pub created: String,
    #[serde(skip_serializing_if = "String::is_empty", default)]
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
    // General references attached to the task (code locations, links, etc.)
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub references: Vec<ReferenceEntry>,

    // Sprint memberships (numeric identifiers, derived from sprint filenames)
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub sprints: Vec<u32>,

    // Legacy fields (keeping for backward compatibility)
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub subtitle: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub description: Option<String>,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub tags: Vec<String>,

    // Team-specific custom fields (generic UI treatment based on type)
    #[serde(skip_serializing_if = "std::collections::HashMap::is_empty", default)]
    pub custom_fields: CustomFields,

    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub history: Vec<crate::types::TaskChangeLogEntry>,
}

impl Task {
    pub fn new(_root_path: PathBuf, title: String, priority: Priority) -> Self {
        Self {
            title,
            priority,
            created: chrono::Utc::now().to_rfc3339(),
            ..Self::default()
        }
    }
}

impl fmt::Display for Task {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(
            f,
            "title: {}\nstatus: {}\nsubtitle: {:?}\ndescription: {:?}\npriority: {}\ncreated: {}\nmodified: {}\ndue_date: {:?}\ntags: {:?}",
            self.title,
            self.status,
            self.subtitle,
            self.description,
            self.priority,
            self.created,
            self.modified,
            self.due_date,
            self.tags
        )
    }
}

// ── Tolerant parsing ────────────────────────────────────────────────────────
//
// Shared helpers for reading task YAML that may not fully satisfy the strict
// `Task` deserialization (e.g. mixed-case enum values written by other tools).
// Strict parse first; on failure, read fields from a generic YAML value and
// normalize enums case-insensitively.

fn tolerant_status(s: &str) -> Option<TaskStatus> {
    let norm = s.trim().to_ascii_lowercase().replace(['_', '-'], "");
    match norm.as_str() {
        "todo" => Some(TaskStatus::from("Todo")),
        "inprogress" => Some(TaskStatus::from("InProgress")),
        "verify" => Some(TaskStatus::from("Verify")),
        "blocked" => Some(TaskStatus::from("Blocked")),
        "done" => Some(TaskStatus::from("Done")),
        _ => s.parse().ok(),
    }
}

fn tolerant_priority(s: &str) -> Option<Priority> {
    match s.trim().to_ascii_lowercase().as_str() {
        "low" => Some(Priority::from("Low")),
        "medium" => Some(Priority::from("Medium")),
        "high" => Some(Priority::from("High")),
        "critical" => Some(Priority::from("Critical")),
        _ => s.parse().ok(),
    }
}

fn tolerant_task_type(s: &str) -> Option<TaskType> {
    match s.trim().to_ascii_lowercase().as_str() {
        "feature" => Some(TaskType::from("Feature")),
        "bug" => Some(TaskType::from("Bug")),
        "epic" => Some(TaskType::from("Epic")),
        "spike" => Some(TaskType::from("Spike")),
        "chore" => Some(TaskType::from("Chore")),
        _ => s.parse().ok(),
    }
}

/// Tolerantly extract just the `status` field from task YAML content.
pub fn parse_status_from_yaml(content: &str) -> Option<TaskStatus> {
    if let Ok(task) = serde_yaml_ng::from_str::<Task>(content) {
        return Some(task.status);
    }
    let val: serde_yaml_ng::Value = serde_yaml_ng::from_str(content).ok()?;
    let s = val.get("status")?.as_str()?;
    tolerant_status(s)
}

/// Tolerantly parse task YAML into a `Task`.
///
/// Strict deserialization first; on failure a generic YAML value is read
/// field-by-field with case-insensitive enum normalization. Structured
/// collections (comments, relationships, …) are left empty in the fallback
/// path — callers use this for read-only aggregation/reporting.
pub fn parse_task_yaml_tolerant(content: &str) -> Option<Task> {
    if let Ok(task) = serde_yaml_ng::from_str::<Task>(content) {
        return Some(task);
    }

    let v: serde_yaml_ng::Value = serde_yaml_ng::from_str(content).ok()?;
    let get_str =
        |k: &str| -> Option<String> { v.get(k).and_then(|x| x.as_str()).map(|s| s.to_string()) };
    let get_vec_str = |k: &str| -> Vec<String> {
        v.get(k)
            .and_then(|x| x.as_sequence())
            .map(|seq| {
                seq.iter()
                    .filter_map(|e| e.as_str().map(|s| s.to_string()))
                    .collect()
            })
            .unwrap_or_default()
    };

    let title = get_str("title").unwrap_or_default();
    let status = get_str("status")
        .and_then(|s| tolerant_status(&s))
        .unwrap_or_default();
    let priority = get_str("priority")
        .and_then(|s| tolerant_priority(&s))
        .unwrap_or_default();
    let task_type = get_str("task_type")
        .or_else(|| get_str("type"))
        .and_then(|s| tolerant_task_type(&s))
        .unwrap_or_default();
    let reporter = get_str("reporter");
    let assignee = get_str("assignee");
    let created = get_str("created").unwrap_or_else(|| "1970-01-01T00:00:00Z".to_string());
    let modified = get_str("modified").unwrap_or_default();
    let due_date = get_str("due_date");
    let effort = get_str("effort");
    let subtitle = get_str("subtitle");
    let description = get_str("description");
    let tags = get_vec_str("tags");
    let acceptance_criteria = get_vec_str("acceptance_criteria");

    let custom_fields = v
        .get("custom_fields")
        .and_then(|x| x.as_mapping())
        .and_then(|m| {
            serde_yaml_ng::from_value::<CustomFields>(serde_yaml_ng::Value::Mapping(m.clone())).ok()
        })
        .unwrap_or_default();

    Some(Task {
        title,
        status,
        priority,
        task_type,
        reporter,
        assignee,
        created,
        modified,
        due_date,
        effort,
        acceptance_criteria,
        relationships: TaskRelationships::default(),
        comments: vec![],
        references: vec![],
        sprints: vec![],
        subtitle,
        description,
        tags,
        custom_fields,
        history: vec![],
    })
}

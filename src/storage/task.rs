use crate::types::{
    CustomFields, Priority, ReferenceEntry, TaskComment, TaskRelationships, TaskStatus, TaskType,
};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
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

    /// User-owned YAML extensions, intentionally absent from API DTOs.
    #[serde(flatten, default, serialize_with = "serialize_extra_fields")]
    pub extra_fields: BTreeMap<String, serde_yaml_ng::Value>,
}

fn is_builtin_key(key: &str) -> bool {
    matches!(
        key,
        "title"
            | "status"
            | "priority"
            | "type"
            | "task_type"
            | "reporter"
            | "assignee"
            | "created"
            | "modified"
            | "due_date"
            | "effort"
            | "acceptance_criteria"
            | "relationships"
            | "comments"
            | "references"
            | "sprints"
            | "subtitle"
            | "description"
            | "tags"
            | "custom_fields"
            | "history"
    )
}

fn serialize_extra_fields<S: serde::Serializer>(
    fields: &BTreeMap<String, serde_yaml_ng::Value>,
    serializer: S,
) -> Result<S::Ok, S::Error> {
    use serde::ser::SerializeMap;
    let mut map = serializer.serialize_map(None)?;
    for (key, value) in fields {
        // Programmatic extensions must never shadow a typed field or its alias,
        // even when the typed field is omitted because it is empty.
        if !is_builtin_key(key) {
            map.serialize_entry(key, value)?;
        }
    }
    map.end()
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
/// with case-insensitive enum normalization and legacy missing-field defaults.
/// All other values must deserialize intact; malformed structured data is not
/// silently replaced with empty collections because callers may edit the task.
pub fn parse_task_yaml_tolerant(content: &str) -> Option<Task> {
    if let Ok(task) = serde_yaml_ng::from_str::<Task>(content) {
        return Some(task);
    }

    let mut v: serde_yaml_ng::Value = serde_yaml_ng::from_str(content).ok()?;
    let mapping = v.as_mapping_mut()?;
    for (key, default) in [("title", ""), ("created", "1970-01-01T00:00:00Z")] {
        mapping
            .entry(serde_yaml_ng::Value::String(key.into()))
            .or_insert_with(|| serde_yaml_ng::Value::String(default.into()));
    }
    for key in ["status", "priority", "type", "task_type"] {
        let Some(value) = mapping.get_mut(serde_yaml_ng::Value::String(key.into())) else {
            continue;
        };
        let Some(text) = value.as_str() else {
            continue;
        };
        let normalized = match key {
            "status" => tolerant_status(text).map(|v| v.to_string()),
            "priority" => tolerant_priority(text).map(|v| v.to_string()),
            _ => tolerant_task_type(text).map(|v| v.to_string()),
        };
        if let Some(normalized) = normalized {
            *value = serde_yaml_ng::Value::String(normalized);
        }
    }
    serde_yaml_ng::from_value(v).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    const EXTENSIONS: &str = "x_text: '001'\nx_number: 42\nx_bool: true\nx_null: null\nx_list: [one, 2, false]\nx_nested: {inner: {values: [null, 3.5, text]}}\n";

    #[test]
    fn yaml_extensions_roundtrip_semantically_with_typed_edits() {
        let input = format!("title: Before\ncreated: '2026-09-06T00:00:00Z'\n{EXTENSIONS}");
        let mut task: Task = serde_yaml_ng::from_str(&input).unwrap();
        assert_eq!(task.extra_fields.len(), 6);
        task.title = "After".into();
        let output: serde_yaml_ng::Value =
            serde_yaml_ng::from_str(&serde_yaml_ng::to_string(&task).unwrap()).unwrap();
        let expected: serde_yaml_ng::Value = serde_yaml_ng::from_str(EXTENSIONS).unwrap();
        for (key, value) in expected.as_mapping().unwrap() {
            assert_eq!(output.get(key), Some(value));
        }
        assert_eq!(output["title"].as_str(), Some("After"));
    }

    #[test]
    fn tolerant_parser_preserves_extensions_and_structured_fields() {
        // Missing created forces the legacy fallback instead of strict parsing.
        let input = format!(
            "title: Legacy\nstatus: in_progress\ntask_type: bug\ncomments: [{{date: '2026-09-06', text: Keep}}]\nreferences: [{{link: 'https://example.com'}}]\nrelationships: {{depends_on: [DEV-2]}}\nsprints: [3]\nacceptance_criteria: [Keep]\ncustom_fields: {{team: alpha}}\n{EXTENSIONS}"
        );
        assert!(serde_yaml_ng::from_str::<Task>(&input).is_err());
        let task = parse_task_yaml_tolerant(&input).unwrap();
        assert_eq!(task.extra_fields.len(), 6);
        assert_eq!(task.comments.len(), 1);
        assert_eq!(task.references.len(), 1);
        assert_eq!(task.sprints, vec![3]);
        assert_eq!(task.acceptance_criteria, vec!["Keep"]);
        let output: Task =
            serde_yaml_ng::from_str(&serde_yaml_ng::to_string(&task).unwrap()).unwrap();
        assert_eq!(output.extra_fields, task.extra_fields);
        assert_eq!(output.comments.len(), 1);
        assert!(!output.relationships.is_empty());
        assert_eq!(output.custom_fields.len(), 1);
    }

    #[test]
    fn extensions_cannot_shadow_builtin_fields_or_aliases() {
        let mut task = Task {
            title: "Authoritative".into(),
            ..Task::default()
        };
        for key in [
            "title",
            "status",
            "type",
            "task_type",
            "comments",
            "custom_fields",
        ] {
            task.extra_fields
                .insert(key.into(), serde_yaml_ng::Value::String("shadow".into()));
        }
        let output = serde_yaml_ng::to_string(&task).unwrap();
        let parsed: Task = serde_yaml_ng::from_str(&output).unwrap();
        assert_eq!(parsed.title, "Authoritative");
        assert!(parsed.status.is_empty());
        assert!(parsed.extra_fields.is_empty());
        assert!(!output.contains("shadow"));
    }

    #[test]
    fn malformed_structured_values_are_not_silently_dropped() {
        assert!(
            parse_task_yaml_tolerant("title: Legacy\ncomments: not-a-list\nx_keep: true\n")
                .is_none()
        );
        assert!(
            parse_task_yaml_tolerant("title: Legacy\ntags: [valid, {invalid: tag}]\n").is_none()
        );
        assert!(parse_task_yaml_tolerant("[]").is_none());
        let legacy = parse_task_yaml_tolerant("title: Legacy\ntask_type: bug\n").unwrap();
        assert!(legacy.extra_fields.is_empty());
        assert!(legacy.comments.is_empty());
        assert_eq!(legacy.created, "1970-01-01T00:00:00Z");
    }
}

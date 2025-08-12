use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fmt;
use std::str::FromStr;

#[cfg(feature = "schema")]
use schemars::JsonSchema;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Default)]
#[cfg_attr(feature = "schema", derive(JsonSchema))]
pub enum TaskStatus {
    #[default]
    Todo,
    InProgress,
    Verify,
    Blocked,
    Done,
}

impl TaskStatus {
    pub fn is_default(&self) -> bool {
        matches!(self, TaskStatus::Todo)
    }
}

impl fmt::Display for TaskStatus {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            TaskStatus::Todo => write!(f, "TODO"),
            TaskStatus::InProgress => write!(f, "IN_PROGRESS"),
            TaskStatus::Verify => write!(f, "VERIFY"),
            TaskStatus::Blocked => write!(f, "BLOCKED"),
            TaskStatus::Done => write!(f, "DONE"),
        }
    }
}

impl std::str::FromStr for TaskStatus {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_uppercase().as_str() {
            "TODO" => Ok(TaskStatus::Todo),
            "IN_PROGRESS" => Ok(TaskStatus::InProgress),
            "VERIFY" => Ok(TaskStatus::Verify),
            "BLOCKED" => Ok(TaskStatus::Blocked),
            "DONE" => Ok(TaskStatus::Done),
            _ => Err(format!("Invalid task status: {}", s)),
        }
    }
}

impl TaskStatus {
    /// Parse status string with project configuration context
    /// This method respects the project's configured issue_states
    /// Supports mixed config with specific values + wildcard (e.g., ["Todo", "InProgress", "*"])
    pub fn parse_with_config(
        s: &str,
        config: &crate::config::types::ResolvedConfig,
    ) -> Result<Self, String> {
        let input_upper = s.to_uppercase();

        // Get valid statuses from config (excluding wildcard for matching)
        let valid_statuses: Vec<String> = config
            .issue_states
            .values
            .iter()
            .map(|status| status.to_string())
            .filter(|status| status != "*") // Exclude wildcard from matching list
            .collect();

        let has_wildcard = config
            .issue_states
            .values
            .iter()
            .any(|status| status.to_string() == "*");

        // First, try to find case-insensitive match in configured values
        for valid_status in &valid_statuses {
            if valid_status.to_uppercase() == input_upper {
                // Found a match, parse using the canonical form
                return Self::from_str(valid_status);
            }
        }

        // If no match found but wildcard is present, try to parse with hardcoded enum
        if has_wildcard {
            return Self::from_str(s);
        }

        // No match found and no wildcard - reject
        Err(format!(
            "Invalid status '{}'. Valid statuses for this project: {}{}",
            s,
            valid_statuses.join(", "),
            if has_wildcard {
                " (or any other value)"
            } else {
                ""
            }
        ))
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Default)]
#[cfg_attr(feature = "schema", derive(JsonSchema))]
pub enum TaskType {
    #[default]
    Feature,
    Bug,
    Epic,
    Spike,
    Chore,
}

impl TaskType {
    pub fn is_default(&self) -> bool {
        matches!(self, TaskType::Feature)
    }
}

impl fmt::Display for TaskType {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            TaskType::Feature => write!(f, "feature"),
            TaskType::Bug => write!(f, "bug"),
            TaskType::Epic => write!(f, "epic"),
            TaskType::Spike => write!(f, "spike"),
            TaskType::Chore => write!(f, "chore"),
        }
    }
}

impl std::str::FromStr for TaskType {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "feature" => Ok(TaskType::Feature),
            "bug" => Ok(TaskType::Bug),
            "epic" => Ok(TaskType::Epic),
            "spike" => Ok(TaskType::Spike),
            "chore" => Ok(TaskType::Chore),
            _ => Err(format!("Invalid task type: {}", s)),
        }
    }
}

impl TaskType {
    /// Parse task type string with project configuration context
    /// This method respects the project's configured issue_types
    /// Supports mixed config with specific values + wildcard (e.g., ["Feature", "Bug", "*"])
    pub fn parse_with_config(
        s: &str,
        config: &crate::config::types::ResolvedConfig,
    ) -> Result<Self, String> {
        let input_lower = s.to_lowercase();

        // Get valid types from config (excluding wildcard for matching)
        let valid_types: Vec<String> = config
            .issue_types
            .values
            .iter()
            .map(|task_type| task_type.to_string())
            .filter(|task_type| task_type != "*") // Exclude wildcard from matching list
            .collect();

        let has_wildcard = config
            .issue_types
            .values
            .iter()
            .any(|task_type| task_type.to_string() == "*");

        // First, try to find case-insensitive match in configured values
        for valid_type in &valid_types {
            if valid_type.to_lowercase() == input_lower {
                // Found a match, parse using the canonical form
                return Self::from_str(valid_type);
            }
        }

        // If no match found but wildcard is present, try to parse with hardcoded enum
        if has_wildcard {
            return Self::from_str(s);
        }

        // No match found and no wildcard - reject
        Err(format!(
            "Invalid task type '{}'. Valid types for this project: {}{}",
            s,
            valid_types.join(", "),
            if has_wildcard {
                " (or any other value)"
            } else {
                ""
            }
        ))
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
#[cfg_attr(feature = "schema", derive(JsonSchema))]
pub struct TaskRelationships {
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub depends_on: Vec<String>,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub blocks: Vec<String>,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub related: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub parent: Option<String>,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub children: Vec<String>,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub fixes: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub duplicate_of: Option<String>,
}

impl TaskRelationships {
    pub fn is_empty(&self) -> bool {
        self.depends_on.is_empty()
            && self.blocks.is_empty()
            && self.related.is_empty()
            && self.parent.is_none()
            && self.children.is_empty()
            && self.fixes.is_empty()
            && self.duplicate_of.is_none()
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(JsonSchema))]
pub struct TaskComment {
    pub author: String,
    pub date: String,
    pub text: String,
}

// Type alias for custom fields - can hold any YAML-serializable value
// For schema generation, use serde_json::Value which has a JsonSchema implementation
#[cfg(not(feature = "schema"))]
pub type CustomFields = HashMap<String, serde_yaml::Value>;
#[cfg(feature = "schema")]
pub type CustomFields = HashMap<String, serde_json::Value>;

// Value type alias and helpers for constructing custom field values in a feature-aware way
#[cfg(not(feature = "schema"))]
pub type CustomFieldValue = serde_yaml::Value;
#[cfg(feature = "schema")]
pub type CustomFieldValue = serde_json::Value;

#[cfg(not(feature = "schema"))]
pub fn custom_value_string<S: Into<String>>(s: S) -> serde_yaml::Value {
    serde_yaml::Value::String(s.into())
}

#[cfg(feature = "schema")]
pub fn custom_value_string<S: Into<String>>(s: S) -> serde_json::Value {
    serde_json::Value::String(s.into())
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Copy, Default)]
#[cfg_attr(feature = "schema", derive(JsonSchema))]
pub enum Priority {
    Low,
    #[default]
    Medium,
    High,
    Critical,
}

impl Priority {
    pub fn is_default(&self) -> bool {
        matches!(self, Priority::Medium)
    }
}

impl fmt::Display for Priority {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Priority::Low => write!(f, "LOW"),
            Priority::Medium => write!(f, "MEDIUM"),
            Priority::High => write!(f, "HIGH"),
            Priority::Critical => write!(f, "CRITICAL"),
        }
    }
}

impl std::str::FromStr for Priority {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "critical" => Ok(Priority::Critical),
            "high" => Ok(Priority::High),
            "medium" => Ok(Priority::Medium),
            "low" => Ok(Priority::Low),
            _ => Err(format!("Invalid priority: {}", s)),
        }
    }
}

impl Priority {
    /// Parse priority string with project configuration context
    /// This method respects the project's configured issue_priorities
    /// Supports mixed config with specific values + wildcard (e.g., ["High", "Medium", "*"])
    pub fn parse_with_config(
        s: &str,
        config: &crate::config::types::ResolvedConfig,
    ) -> Result<Self, String> {
        let input_lower = s.to_lowercase();

        // Get valid priorities from config (excluding wildcard for matching)
        let valid_priorities: Vec<String> = config
            .issue_priorities
            .values
            .iter()
            .map(|priority| priority.to_string())
            .filter(|priority| priority != "*") // Exclude wildcard from matching list
            .collect();

        let has_wildcard = config
            .issue_priorities
            .values
            .iter()
            .any(|priority| priority.to_string() == "*");

        // First, try to find case-insensitive match in configured values
        for valid_priority in &valid_priorities {
            if valid_priority.to_lowercase() == input_lower {
                // Found a match, parse using the canonical form
                return Self::from_str(valid_priority);
            }
        }

        // If no match found but wildcard is present, try to parse with hardcoded enum
        if has_wildcard {
            return Self::from_str(s);
        }

        // No match found and no wildcard - reject
        Err(format!(
            "Invalid priority '{}'. Valid priorities for this project: {}{}",
            s,
            valid_priorities.join(", "),
            if has_wildcard {
                " (or any other value)"
            } else {
                ""
            }
        ))
    }
}

// inline tests moved to tests/types_priority_unit_test.rs

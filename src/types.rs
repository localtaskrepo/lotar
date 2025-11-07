use serde::{Deserialize, Serialize};
use std::cmp::Ordering;
use std::collections::HashMap;
use std::fmt;
use std::str::FromStr;

#[cfg(feature = "schema")]
use schemars::JsonSchema;

fn normalize_token(input: &str) -> String {
    input.trim().to_string()
}

fn canonical_key(input: &str) -> String {
    input
        .trim()
        .to_ascii_lowercase()
        .replace([' ', '_', '-'], "")
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(transparent)]
#[cfg_attr(feature = "schema", derive(JsonSchema))]
pub struct TaskStatus(String);

impl TaskStatus {
    pub fn new<S: Into<String>>(value: S) -> Self {
        Self(normalize_token(&value.into()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    pub fn is_empty(&self) -> bool {
        self.0.trim().is_empty()
    }

    pub fn eq_ignore_case<S: AsRef<str>>(&self, other: S) -> bool {
        self.0.eq_ignore_ascii_case(other.as_ref())
    }

    pub fn parse_with_config(
        raw: &str,
        config: &crate::config::types::ResolvedConfig,
    ) -> Result<Self, String> {
        let candidate = normalize_token(raw);
        if candidate.is_empty() {
            return Err("Status cannot be empty.".to_string());
        }
        let allowed = &config.issue_states.values;
        if allowed.is_empty() {
            return Ok(TaskStatus::new(candidate));
        }
        let candidate_key = canonical_key(&candidate);
        if let Some(existing) = allowed
            .iter()
            .find(|status| canonical_key(status.as_str()) == candidate_key)
        {
            return Ok(existing.clone());
        }
        let labels = allowed
            .iter()
            .map(|status| status.as_str())
            .collect::<Vec<_>>()
            .join(", ");
        Err(format!(
            "Status '{}' is not enabled for this project. Valid statuses: {}",
            candidate, labels
        ))
    }

    fn sort_key(&self) -> (usize, String) {
        const ORDER: &[&str] = &[
            "todo",
            "inprogress",
            "verify",
            "blocked",
            "done",
            "canceled",
            "cancelled",
        ];
        let key = canonical_key(self.as_str());
        let idx = ORDER
            .iter()
            .position(|item| *item == key)
            .unwrap_or(ORDER.len());
        (idx, key)
    }
}

impl fmt::Display for TaskStatus {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl From<String> for TaskStatus {
    fn from(value: String) -> Self {
        TaskStatus::new(value)
    }
}

impl From<&str> for TaskStatus {
    fn from(value: &str) -> Self {
        TaskStatus::new(value)
    }
}

impl FromStr for TaskStatus {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let normalized = normalize_token(s);
        if normalized.is_empty() {
            Err("Status cannot be empty.".to_string())
        } else {
            Ok(TaskStatus(normalized))
        }
    }
}

impl Ord for TaskStatus {
    fn cmp(&self, other: &Self) -> Ordering {
        self.sort_key().cmp(&other.sort_key())
    }
}

impl PartialOrd for TaskStatus {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(transparent)]
#[cfg_attr(feature = "schema", derive(JsonSchema))]
pub struct TaskType(String);

impl TaskType {
    pub fn new<S: Into<String>>(value: S) -> Self {
        Self(normalize_token(&value.into()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    pub fn is_empty(&self) -> bool {
        self.0.trim().is_empty()
    }

    pub fn eq_ignore_case<S: AsRef<str>>(&self, other: S) -> bool {
        self.0.eq_ignore_ascii_case(other.as_ref())
    }

    pub fn parse_with_config(
        raw: &str,
        config: &crate::config::types::ResolvedConfig,
    ) -> Result<Self, String> {
        let candidate = normalize_token(raw);
        if candidate.is_empty() {
            return Err("Type cannot be empty.".to_string());
        }
        let allowed = &config.issue_types.values;
        if allowed.is_empty() {
            return Ok(TaskType::new(candidate));
        }
        let candidate_key = canonical_key(&candidate);
        if let Some(existing) = allowed
            .iter()
            .find(|ty| canonical_key(ty.as_str()) == candidate_key)
        {
            return Ok(existing.clone());
        }
        let labels = allowed
            .iter()
            .map(|ty| ty.as_str())
            .collect::<Vec<_>>()
            .join(", ");
        Err(format!(
            "Type '{}' is not enabled for this project. Valid types: {}",
            candidate, labels
        ))
    }

    pub fn ensure_leading_uppercase(&mut self) {
        if self.0.is_empty() {
            return;
        }
        let mut chars = self.0.chars();
        if let Some(first) = chars.next() {
            let mut normalized = first.to_uppercase().collect::<String>();
            normalized.push_str(chars.as_str());
            self.0 = normalized;
        }
    }
}

impl fmt::Display for TaskType {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl From<String> for TaskType {
    fn from(value: String) -> Self {
        TaskType::new(value)
    }
}

impl From<&str> for TaskType {
    fn from(value: &str) -> Self {
        TaskType::new(value)
    }
}

impl FromStr for TaskType {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let normalized = normalize_token(s);
        if normalized.is_empty() {
            Err("Type cannot be empty.".to_string())
        } else {
            Ok(TaskType(normalized))
        }
    }
}

impl Ord for TaskType {
    fn cmp(&self, other: &Self) -> Ordering {
        self.0
            .to_ascii_lowercase()
            .cmp(&other.0.to_ascii_lowercase())
    }
}

impl PartialOrd for TaskType {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(transparent)]
#[cfg_attr(feature = "schema", derive(JsonSchema))]
pub struct Priority(String);

impl Priority {
    pub fn new<S: Into<String>>(value: S) -> Self {
        Self(normalize_token(&value.into()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    pub fn eq_ignore_case<S: AsRef<str>>(&self, other: S) -> bool {
        self.0.eq_ignore_ascii_case(other.as_ref())
    }

    pub fn is_empty(&self) -> bool {
        self.0.trim().is_empty()
    }

    pub fn parse_with_config(
        raw: &str,
        config: &crate::config::types::ResolvedConfig,
    ) -> Result<Self, String> {
        let candidate = normalize_token(raw);
        if candidate.is_empty() {
            return Err("Priority cannot be empty.".to_string());
        }
        let allowed = &config.issue_priorities.values;
        if allowed.is_empty() {
            return Ok(Priority::new(candidate));
        }
        let candidate_key = canonical_key(&candidate);
        if let Some(existing) = allowed
            .iter()
            .find(|priority| canonical_key(priority.as_str()) == candidate_key)
        {
            return Ok(existing.clone());
        }
        if allowed.iter().any(|priority| priority.as_str() == "*") {
            return Ok(Priority::new(candidate));
        }
        let labels = allowed
            .iter()
            .map(|priority| priority.as_str())
            .collect::<Vec<_>>()
            .join(", ");
        Err(format!(
            "Priority '{}' is not enabled for this project. Valid priorities: {}",
            candidate, labels
        ))
    }

    fn sort_key(&self) -> (usize, String) {
        const ORDER: &[&str] = &[
            "lowest", "low", "medium", "normal", "high", "critical", "blocker", "urgent",
        ];
        let key = canonical_key(self.as_str());
        let idx = ORDER
            .iter()
            .position(|item| *item == key)
            .unwrap_or(ORDER.len());
        (idx, key)
    }
}

impl fmt::Display for Priority {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl From<String> for Priority {
    fn from(value: String) -> Self {
        Priority::new(value)
    }
}

impl From<&str> for Priority {
    fn from(value: &str) -> Self {
        Priority::new(value)
    }
}

impl FromStr for Priority {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let normalized = normalize_token(s);
        if normalized.is_empty() {
            Err("Priority cannot be empty.".to_string())
        } else {
            Ok(Priority(normalized))
        }
    }
}

impl Ord for Priority {
    fn cmp(&self, other: &Self) -> Ordering {
        self.sort_key().cmp(&other.sort_key())
    }
}

impl PartialOrd for Priority {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

// inline tests moved to tests/types_priority_unit_test.rs

#[derive(Clone, Debug, Serialize, Deserialize, Default, PartialEq)]
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
    pub date: String,
    pub text: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, Default, PartialEq)]
#[cfg_attr(feature = "schema", derive(JsonSchema))]
pub struct TaskChange {
    pub field: String,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub old: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub new: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, Default, PartialEq)]
#[cfg_attr(feature = "schema", derive(JsonSchema))]
pub struct TaskChangeLogEntry {
    pub at: String,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub actor: Option<String>,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub changes: Vec<TaskChange>,
}

// Typed external references attached to a task/ticket.
// Minimal schema for now: support code references and generic links.
#[derive(Clone, Debug, Serialize, Deserialize, Default, PartialEq)]
#[cfg_attr(feature = "schema", derive(JsonSchema))]
pub struct ReferenceEntry {
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub code: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub link: Option<String>,
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

/// Convert a custom field value to a comparable display string for sorting/filtering.
#[cfg(feature = "schema")]
pub fn custom_value_to_string(v: &CustomFieldValue) -> String {
    match v {
        serde_json::Value::Null => String::new(),
        serde_json::Value::Bool(b) => b.to_string(),
        serde_json::Value::Number(n) => n.to_string(),
        serde_json::Value::String(s) => s.clone(),
        serde_json::Value::Array(_) => "[array]".to_string(),
        serde_json::Value::Object(_) => "{object}".to_string(),
    }
}

/// Convert a custom field value to a comparable display string for sorting/filtering.
#[cfg(not(feature = "schema"))]
pub fn custom_value_to_string(v: &CustomFieldValue) -> String {
    match v {
        serde_yaml::Value::Null => String::new(),
        serde_yaml::Value::Bool(b) => b.to_string(),
        serde_yaml::Value::Number(n) => n.to_string(),
        serde_yaml::Value::String(s) => s.clone(),
        serde_yaml::Value::Sequence(_) => "[array]".to_string(),
        serde_yaml::Value::Mapping(_) => "{object}".to_string(),
        _ => "other".to_string(),
    }
}

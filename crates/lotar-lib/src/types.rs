use serde::{Deserialize, Serialize};
use std::cmp::Ordering;
use std::fmt;
use std::str::FromStr;

#[cfg(feature = "schema")]
use schemars::JsonSchema;

fn normalize_token(input: &str) -> String {
    input.trim().to_string()
}

pub(crate) fn canonical_key(input: &str) -> String {
    input.trim().to_ascii_lowercase().replace([' ', '_', '-'], "")
}

/// Case-, space-, underscore-, and hyphen-insensitive equality for enum-like
/// tokens (statuses, types, priorities). Shared by task parsing and config
/// candidate validation so membership checks agree on canonical spellings.
pub(crate) fn enum_token_eq(a: &str, b: &str) -> bool {
    canonical_key(a) == canonical_key(b)
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

    pub fn parse_with_config(raw: &str, config: &crate::config::types::ResolvedConfig) -> Result<Self, String> {
        let candidate = normalize_token(raw);
        if candidate.is_empty() {
            return Err("Status cannot be empty.".to_string());
        }
        let allowed = &config.issue_states.values;
        if allowed.is_empty() {
            return Ok(TaskStatus::new(candidate));
        }
        let candidate_key = canonical_key(&candidate);
        if let Some(existing) = allowed.iter().find(|status| canonical_key(status.as_str()) == candidate_key) {
            return Ok(existing.clone());
        }
        let labels = allowed.iter().map(|status| status.as_str()).collect::<Vec<_>>().join(", ");
        Err(format!("Status '{}' is not enabled for this project. Valid statuses: {}", candidate, labels))
    }

    fn sort_key(&self) -> (usize, String) {
        const ORDER: &[&str] = &["todo", "inprogress", "verify", "needsreview", "blocked", "done", "canceled", "cancelled"];
        let key = canonical_key(self.as_str());
        let idx = ORDER.iter().position(|item| *item == key).unwrap_or(ORDER.len());
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
        if normalized.is_empty() { Err("Status cannot be empty.".to_string()) } else { Ok(TaskStatus(normalized)) }
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

    pub fn parse_with_config(raw: &str, config: &crate::config::types::ResolvedConfig) -> Result<Self, String> {
        let candidate = normalize_token(raw);
        if candidate.is_empty() {
            return Err("Type cannot be empty.".to_string());
        }
        let allowed = &config.issue_types.values;
        if allowed.is_empty() {
            return Ok(TaskType::new(candidate));
        }
        let candidate_key = canonical_key(&candidate);
        if let Some(existing) = allowed.iter().find(|ty| canonical_key(ty.as_str()) == candidate_key) {
            return Ok(existing.clone());
        }
        let labels = allowed.iter().map(|ty| ty.as_str()).collect::<Vec<_>>().join(", ");
        Err(format!("Type '{}' is not enabled for this project. Valid types: {}", candidate, labels))
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
        if normalized.is_empty() { Err("Type cannot be empty.".to_string()) } else { Ok(TaskType(normalized)) }
    }
}

impl Ord for TaskType {
    fn cmp(&self, other: &Self) -> Ordering {
        self.0.to_ascii_lowercase().cmp(&other.0.to_ascii_lowercase())
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

    pub fn parse_with_config(raw: &str, config: &crate::config::types::ResolvedConfig) -> Result<Self, String> {
        let candidate = normalize_token(raw);
        if candidate.is_empty() {
            return Err("Priority cannot be empty.".to_string());
        }
        let allowed = &config.issue_priorities.values;
        if allowed.is_empty() {
            return Ok(Priority::new(candidate));
        }
        let candidate_key = canonical_key(&candidate);
        if let Some(existing) = allowed.iter().find(|priority| canonical_key(priority.as_str()) == candidate_key) {
            return Ok(existing.clone());
        }
        if allowed.iter().any(|priority| priority.as_str() == "*") {
            return Ok(Priority::new(candidate));
        }
        let labels = allowed.iter().map(|priority| priority.as_str()).collect::<Vec<_>>().join(", ");
        Err(format!("Priority '{}' is not enabled for this project. Valid priorities: {}", candidate, labels))
    }

    fn sort_key(&self) -> (usize, String) {
        const ORDER: &[&str] = &["lowest", "low", "medium", "normal", "high", "critical", "blocker", "urgent"];
        let key = canonical_key(self.as_str());
        let idx = ORDER.iter().position(|item| *item == key).unwrap_or(ORDER.len());
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
        if normalized.is_empty() { Err("Priority cannot be empty.".to_string()) } else { Ok(Priority(normalized)) }
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
    /// Keys this version does not know, kept so a newer tool's or a
    /// hand-written field survives edits (DEV-268).
    #[serde(flatten, default)]
    #[cfg_attr(feature = "schema", schemars(skip))]
    pub extra: std::collections::BTreeMap<String, serde_yaml_ng::Value>,
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
            && self.extra.is_empty()
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[cfg_attr(feature = "schema", derive(JsonSchema))]
pub struct TaskComment {
    pub date: String,
    /// Who wrote the comment: the same identity the history records as
    /// actor (DEV-327). Comments from older versions have none.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub author: Option<String>,
    pub text: String,
    /// The comment asks a question with options (DEV-488). `text` holds
    /// the question and its options in prose for readers without this.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub question: Option<CommentQuestion>,
    /// The comment answers a question (DEV-488); `text` holds the answer in
    /// prose.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub answer: Option<CommentAnswer>,
    /// The comment reports a review's verdict and findings (DEV-489);
    /// `text` holds the summary and the findings in prose.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub review: Option<CommentReview>,
    /// Keys this version does not know, kept so a newer tool's or a
    /// hand-written field survives edits (DEV-268).
    #[serde(flatten, default)]
    #[cfg_attr(feature = "schema", schemars(skip))]
    pub extra: std::collections::BTreeMap<String, serde_yaml_ng::Value>,
}

impl TaskComment {
    pub fn new(date: String, author: Option<String>, text: String) -> Self {
        Self { date, author, text, question: None, answer: None, review: None, extra: Default::default() }
    }
}

/// A question the reader answers by choosing an option (DEV-488).
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[cfg_attr(feature = "schema", derive(JsonSchema))]
pub struct CommentQuestion {
    pub options: Vec<QuestionOption>,
    /// The option the asker recommends.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub recommended: Option<String>,
    /// Several options may be chosen together.
    #[serde(skip_serializing_if = "std::ops::Not::not", default)]
    pub multiple: bool,
    /// A free-text answer instead of an option is allowed.
    #[serde(skip_serializing_if = "is_true", default = "default_true")]
    pub other: bool,
    #[serde(flatten, default)]
    #[cfg_attr(feature = "schema", schemars(skip))]
    pub extra: std::collections::BTreeMap<String, serde_yaml_ng::Value>,
}

fn default_true() -> bool {
    true
}

fn is_true(value: &bool) -> bool {
    *value
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[cfg_attr(feature = "schema", derive(JsonSchema))]
pub struct QuestionOption {
    pub key: String,
    pub label: String,
    /// Fields that choosing this option sets on the ticket, as in an
    /// automation `set:`.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub set: Option<OptionSet>,
    #[serde(flatten, default)]
    #[cfg_attr(feature = "schema", schemars(skip))]
    pub extra: std::collections::BTreeMap<String, serde_yaml_ng::Value>,
}

/// The ticket fields an answer may set: a subset of automation `set:`.
/// Keys this version does not know land in `extra`; asking refuses them.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Default)]
#[cfg_attr(feature = "schema", derive(JsonSchema))]
pub struct OptionSet {
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub status: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub assignee: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub reporter: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub priority: Option<String>,
    #[serde(rename = "type", skip_serializing_if = "Option::is_none", default)]
    pub task_type: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub due_date: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub effort: Option<String>,
    /// Replaces the ticket's tags, as `set: tags` does.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub tags: Option<Vec<String>>,
    #[serde(flatten, default)]
    #[cfg_attr(feature = "schema", schemars(skip))]
    pub extra: std::collections::BTreeMap<String, serde_yaml_ng::Value>,
}

impl OptionSet {
    /// The names of the fields this set changes, in a fixed order.
    pub fn fields(&self) -> Vec<&'static str> {
        [
            ("status", self.status.is_some()),
            ("assignee", self.assignee.is_some()),
            ("reporter", self.reporter.is_some()),
            ("priority", self.priority.is_some()),
            ("type", self.task_type.is_some()),
            ("due_date", self.due_date.is_some()),
            ("effort", self.effort.is_some()),
            ("tags", self.tags.is_some()),
        ]
        .into_iter()
        .filter_map(|(name, set)| set.then_some(name))
        .collect()
    }
}

/// An answer to a question comment (DEV-488).
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[cfg_attr(feature = "schema", derive(JsonSchema))]
pub struct CommentAnswer {
    /// The `date` of the question comment (comments have no ID).
    pub to: String,
    /// The chosen option keys; empty for a free-text ("other") answer.
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub choice: Vec<String>,
    /// The answerer's own words, if any.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub note: Option<String>,
    #[serde(flatten, default)]
    #[cfg_attr(feature = "schema", schemars(skip))]
    pub extra: std::collections::BTreeMap<String, serde_yaml_ng::Value>,
}

/// A review's verdicts: `pass` lets the work go on, `block` sends it back.
pub const REVIEW_VERDICTS: [&str; 2] = ["pass", "block"];

/// A review's result (DEV-489).
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[cfg_attr(feature = "schema", derive(JsonSchema))]
pub struct CommentReview {
    /// One of [`REVIEW_VERDICTS`]. Kept as text so a verdict a newer
    /// version adds still loads.
    pub verdict: String,
    /// The commit the review looked at.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub commit: Option<String>,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub findings: Vec<ReviewFinding>,
    #[serde(flatten, default)]
    #[cfg_attr(feature = "schema", schemars(skip))]
    pub extra: std::collections::BTreeMap<String, serde_yaml_ng::Value>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[cfg_attr(feature = "schema", derive(JsonSchema))]
pub struct ReviewFinding {
    pub summary: String,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub file: Option<String>,
    /// The line in `file`, starting at 1.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub line: Option<u32>,
    /// The finding must be fixed before the work goes on.
    #[serde(skip_serializing_if = "std::ops::Not::not", default)]
    pub blocking: bool,
    #[serde(flatten, default)]
    #[cfg_attr(feature = "schema", schemars(skip))]
    pub extra: std::collections::BTreeMap<String, serde_yaml_ng::Value>,
}

/// The newest review comment, if any.
pub fn latest_review(comments: &[TaskComment]) -> Option<&TaskComment> {
    comments.iter().rev().find(|comment| comment.review.is_some())
}

/// Indexes of the question comments no answer points to, oldest first.
pub fn open_questions(comments: &[TaskComment]) -> Vec<usize> {
    comments
        .iter()
        .enumerate()
        .filter(|(_, comment)| comment.question.is_some())
        .filter(|(_, question)| !comments.iter().any(|comment| comment.answer.as_ref().is_some_and(|answer| answer.to == question.date)))
        .map(|(index, _)| index)
        .collect()
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
    /// Keys this version does not know, kept so a newer tool's or a
    /// hand-written field survives edits (DEV-268).
    #[serde(flatten, default)]
    #[cfg_attr(feature = "schema", schemars(skip))]
    pub extra: std::collections::BTreeMap<String, serde_yaml_ng::Value>,
}

// Typed external references attached to a task/ticket.
// `code` and `file` are repository-relative paths (`code` carries a
// `#N[-M]` line anchor); `attachment` is a managed attachments-store
// blob name; `link`/`jira`/`github` are external values. The two
// file-ish kinds are distinct on purpose (DEV-61): managed blobs are
// never stored under `file` and repository files never under
// `attachment`, so kind confusion cannot cross storage roots.
#[derive(Clone, Debug, Serialize, Deserialize, Default, PartialEq)]
#[cfg_attr(feature = "schema", derive(JsonSchema))]
pub struct ReferenceEntry {
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub code: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub link: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub file: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub attachment: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub jira: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub github: Option<String>,
    /// Keys this version does not know, kept so a newer tool's or a
    /// hand-written field survives edits (DEV-268).
    #[serde(flatten, default)]
    #[cfg_attr(feature = "schema", schemars(skip))]
    pub extra: std::collections::BTreeMap<String, serde_yaml_ng::Value>,
}

// Type alias for custom fields - can hold any YAML-serializable value
// For schema generation, use serde_json::Value which has a JsonSchema implementation.
// Sorted, so task files list fields in the same order on every write (DEV-266).
#[cfg(not(feature = "schema"))]
pub type CustomFields = std::collections::BTreeMap<String, serde_yaml_ng::Value>;
#[cfg(feature = "schema")]
pub type CustomFields = std::collections::BTreeMap<String, serde_json::Value>;

// Value type alias and helpers for constructing custom field values in a feature-aware way
#[cfg(not(feature = "schema"))]
pub type CustomFieldValue = serde_yaml_ng::Value;
#[cfg(feature = "schema")]
pub type CustomFieldValue = serde_json::Value;

#[cfg(not(feature = "schema"))]
pub fn custom_value_string<S: Into<String>>(s: S) -> serde_yaml_ng::Value {
    serde_yaml_ng::Value::String(s.into())
}

#[cfg(feature = "schema")]
pub fn custom_value_string<S: Into<String>>(s: S) -> serde_json::Value {
    serde_json::Value::String(s.into())
}

/// Convert a JSON custom field value into the configured `CustomFieldValue`
/// representation so REST/MCP payloads round-trip identically.
#[cfg(feature = "schema")]
pub fn custom_value_from_json(value: &serde_json::Value) -> CustomFieldValue {
    value.clone()
}

#[cfg(not(feature = "schema"))]
pub fn custom_value_from_json(value: &serde_json::Value) -> CustomFieldValue {
    serde_yaml_ng::to_value(value).unwrap_or(serde_yaml_ng::Value::Null)
}

/// The strings a custom field value holds: one for a scalar, one per item
/// for a list (DEV-391), so a list never reads as `[array]`.
#[cfg(feature = "schema")]
pub fn custom_value_items(v: &CustomFieldValue) -> Vec<String> {
    match v {
        serde_json::Value::Array(items) => items.iter().flat_map(custom_value_items).collect(),
        other => vec![custom_value_to_string(other)],
    }
}

#[cfg(not(feature = "schema"))]
pub fn custom_value_items(v: &CustomFieldValue) -> Vec<String> {
    match v {
        serde_yaml_ng::Value::Sequence(items) => items.iter().flat_map(custom_value_items).collect(),
        other => vec![custom_value_to_string(other)],
    }
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
        serde_yaml_ng::Value::Null => String::new(),
        serde_yaml_ng::Value::Bool(b) => b.to_string(),
        serde_yaml_ng::Value::Number(n) => n.to_string(),
        serde_yaml_ng::Value::String(s) => s.clone(),
        serde_yaml_ng::Value::Sequence(_) => "[array]".to_string(),
        serde_yaml_ng::Value::Mapping(_) => "{object}".to_string(),
        _ => "other".to_string(),
    }
}

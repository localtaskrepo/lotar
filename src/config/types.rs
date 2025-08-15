use crate::types::{Priority, TaskStatus, TaskType};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectTemplate {
    pub name: String,
    pub description: String,
    pub config: ProjectConfig,
}

#[derive(Debug, Clone)]
pub struct ConfigurableField<T> {
    pub values: Vec<T>,
}

impl<T> Serialize for ConfigurableField<T>
where
    T: Serialize,
{
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        self.values.serialize(serializer)
    }
}

impl<'de, T> Deserialize<'de> for ConfigurableField<T>
where
    T: Deserialize<'de>,
{
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let values = Vec::<T>::deserialize(deserializer)?;
        Ok(ConfigurableField { values })
    }
}

impl<T> ConfigurableField<T> where
    T: Clone + PartialEq + std::fmt::Debug + for<'de> serde::Deserialize<'de>
{
}

// Specialized implementation for String fields that support wildcard
// This will serialize as a direct array without the "values" wrapper
#[derive(Debug, Clone)]
pub struct StringConfigField {
    pub values: Vec<String>,
}

impl Serialize for StringConfigField {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        self.values.serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for StringConfigField {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let values = Vec::<String>::deserialize(deserializer)?;
        Ok(StringConfigField { values })
    }
}

impl StringConfigField {
    pub fn new_wildcard() -> Self {
        Self {
            values: vec!["*".to_string()],
        }
    }

    #[allow(dead_code)]
    pub fn new_strict(values: Vec<String>) -> Self {
        Self { values }
    }

    pub fn has_wildcard(&self) -> bool {
        self.values.contains(&"*".to_string())
    }

    #[allow(dead_code)]
    pub fn get_suggestions(&self) -> Vec<String> {
        self.values.iter().filter(|v| *v != "*").cloned().collect()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectConfig {
    pub project_name: String,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub issue_states: Option<ConfigurableField<TaskStatus>>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub issue_types: Option<ConfigurableField<TaskType>>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub issue_priorities: Option<ConfigurableField<Priority>>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub categories: Option<StringConfigField>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub tags: Option<StringConfigField>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub default_assignee: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub default_reporter: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub default_category: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub default_tags: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub default_priority: Option<Priority>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub default_status: Option<TaskStatus>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub custom_fields: Option<StringConfigField>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub auto_set_reporter: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub auto_assign_on_status: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub scan_signal_words: Option<Vec<String>>, // case-insensitive signal words for scanner
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub scan_ticket_patterns: Option<Vec<String>>, // regex patterns to detect ticket keys
}

impl ProjectConfig {
    pub fn new(project_name: String) -> Self {
        Self {
            project_name,
            issue_states: None,
            issue_types: None,
            issue_priorities: None,
            categories: None,
            tags: None,
            default_assignee: None,
            default_reporter: None,
            default_category: None,
            default_tags: None,
            default_priority: None,
            default_status: None,
            custom_fields: None,
            auto_set_reporter: None,
            auto_assign_on_status: None,
            scan_signal_words: None,
            scan_ticket_patterns: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GlobalConfig {
    #[serde(default = "default_port")]
    pub server_port: u16,
    #[serde(default = "default_prefix_name", rename = "default_project")]
    pub default_prefix: String,

    // Default configurations for all projects
    #[serde(default = "default_issue_states")]
    pub issue_states: ConfigurableField<TaskStatus>,
    #[serde(default = "default_issue_types")]
    pub issue_types: ConfigurableField<TaskType>,
    #[serde(default = "default_issue_priorities")]
    pub issue_priorities: ConfigurableField<Priority>,
    #[serde(default = "default_categories")]
    pub categories: StringConfigField,
    #[serde(default = "default_tags")]
    pub tags: StringConfigField,

    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub default_assignee: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub default_reporter: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub default_category: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub default_tags: Vec<String>,
    #[serde(default = "default_true")]
    pub auto_set_reporter: bool,
    #[serde(default = "default_true")]
    pub auto_assign_on_status: bool,
    #[serde(default = "default_true")]
    pub auto_codeowners_assign: bool,
    #[serde(default = "default_priority")]
    pub default_priority: Priority,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub default_status: Option<TaskStatus>,
    #[serde(default = "default_custom_fields")]
    pub custom_fields: StringConfigField,
    #[serde(default = "default_scan_signal_words")]
    pub scan_signal_words: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub scan_ticket_patterns: Option<Vec<String>>,

    // Automation toggles
    #[serde(default = "default_true")]
    pub auto_identity: bool,
    #[serde(default = "default_true")]
    pub auto_identity_git: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct ResolvedConfig {
    pub server_port: u16,
    #[serde(rename = "default_project")]
    pub default_prefix: String,
    pub issue_states: ConfigurableField<TaskStatus>,
    pub issue_types: ConfigurableField<TaskType>,
    pub issue_priorities: ConfigurableField<Priority>,
    pub categories: StringConfigField,
    pub tags: StringConfigField,
    pub default_assignee: Option<String>,
    pub default_reporter: Option<String>,
    pub default_category: Option<String>,
    pub default_tags: Vec<String>,
    pub auto_set_reporter: bool,
    pub auto_assign_on_status: bool,
    pub auto_codeowners_assign: bool,
    pub default_priority: Priority,
    pub default_status: Option<TaskStatus>,
    pub custom_fields: StringConfigField,
    pub scan_signal_words: Vec<String>,

    // Automation toggles (effective)
    pub auto_identity: bool,
    pub auto_identity_git: bool,
}

#[derive(Debug)]
pub enum ConfigError {
    IoError(String),
    ParseError(String),
    FileNotFound(String),
}

impl std::fmt::Display for ConfigError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ConfigError::IoError(msg) => write!(f, "IO Error: {}", msg),
            ConfigError::ParseError(msg) => write!(f, "Parse Error: {}", msg),
            ConfigError::FileNotFound(msg) => write!(f, "Config file not found: {}", msg),
        }
    }
}

impl std::error::Error for ConfigError {}

// Default value functions
fn default_port() -> u16 {
    8080
}
fn default_prefix_name() -> String {
    // Don't default to "auto" - this should only be set during initial config creation
    // with actual auto-detection. Empty string means no default project is set.
    "".to_string()
}
fn default_priority() -> Priority {
    Priority::Medium
}

fn default_issue_states() -> ConfigurableField<TaskStatus> {
    ConfigurableField {
        values: vec![TaskStatus::Todo, TaskStatus::InProgress, TaskStatus::Done],
    }
}

fn default_issue_types() -> ConfigurableField<TaskType> {
    ConfigurableField {
        values: vec![TaskType::Feature, TaskType::Bug, TaskType::Chore],
    }
}

fn default_issue_priorities() -> ConfigurableField<Priority> {
    ConfigurableField {
        values: vec![Priority::Low, Priority::Medium, Priority::High],
    }
}

fn default_categories() -> StringConfigField {
    StringConfigField::new_wildcard()
}

fn default_tags() -> StringConfigField {
    StringConfigField::new_wildcard()
}

fn default_custom_fields() -> StringConfigField {
    StringConfigField::new_wildcard()
}

fn default_scan_signal_words() -> Vec<String> {
    vec![
        "TODO".to_string(),
        "FIXME".to_string(),
        "HACK".to_string(),
        "BUG".to_string(),
        "NOTE".to_string(),
    ]
}

impl Default for GlobalConfig {
    fn default() -> Self {
        Self {
            server_port: default_port(),
            default_prefix: default_prefix_name(),
            issue_states: default_issue_states(),
            issue_types: default_issue_types(),
            issue_priorities: default_issue_priorities(),
            categories: default_categories(),
            tags: default_tags(),
            default_assignee: None,
            default_reporter: None,
            default_category: None,
            default_tags: Vec::new(),
            auto_set_reporter: true,
            auto_assign_on_status: true,
            auto_codeowners_assign: true,
            default_priority: default_priority(),
            default_status: None,
            custom_fields: default_custom_fields(),
            scan_signal_words: default_scan_signal_words(),
            scan_ticket_patterns: None,
            auto_identity: true,
            auto_identity_git: true,
        }
    }
}

fn default_true() -> bool {
    true
}

// Helper accessors for optional fields used by normalization without exposing internals
pub fn maybe_scan_ticket_patterns(cfg: &GlobalConfig) -> Option<&Vec<String>> {
    cfg.scan_ticket_patterns.as_ref()
}

pub fn maybe_project_scan_ticket_patterns(cfg: &ProjectConfig) -> Option<&Vec<String>> {
    cfg.scan_ticket_patterns.as_ref()
}

// Constants that are actually used would go here if needed

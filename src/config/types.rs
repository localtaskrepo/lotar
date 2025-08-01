use crate::types::{Priority, TaskStatus, TaskType};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectTemplate {
    pub name: String,
    pub description: String,
    pub config: ProjectConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConfigurableField<T> {
    pub values: Vec<T>,
}

impl<T> ConfigurableField<T> where
    T: Clone + PartialEq + std::fmt::Debug + for<'de> serde::Deserialize<'de>
{
}

// Specialized implementation for String fields that support wildcard
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StringConfigField {
    pub values: Vec<String>,
}

impl StringConfigField {
    pub fn new_wildcard() -> Self {
        Self {
            values: vec!["*".to_string()],
        }
    }

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
    pub default_priority: Option<Priority>,
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
            default_priority: None,
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
    #[serde(default = "default_priority")]
    pub default_priority: Priority,
}

#[derive(Debug, Clone)]
pub struct ResolvedConfig {
    pub server_port: u16,
    pub default_prefix: String,
    pub issue_states: ConfigurableField<TaskStatus>,
    pub issue_types: ConfigurableField<TaskType>,
    pub issue_priorities: ConfigurableField<Priority>,
    pub categories: StringConfigField,
    pub tags: StringConfigField,
    pub default_assignee: Option<String>,
    pub default_priority: Priority,
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
            default_priority: default_priority(),
        }
    }
}

// Constants that are actually used would go here if needed

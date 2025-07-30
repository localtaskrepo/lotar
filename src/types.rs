use serde::{Serialize, Deserialize};
use std::fmt;
use std::collections::HashMap;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub enum TaskStatus {
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

impl Default for TaskStatus {
    fn default() -> Self {
        TaskStatus::Todo
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

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub enum TaskType {
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

impl Default for TaskType {
    fn default() -> Self {
        TaskType::Feature
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

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
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
        self.depends_on.is_empty() &&
        self.blocks.is_empty() &&
        self.related.is_empty() &&
        self.parent.is_none() &&
        self.children.is_empty() &&
        self.fixes.is_empty() &&
        self.duplicate_of.is_none()
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TaskComment {
    pub author: String,
    pub date: String,
    pub text: String,
}

// Type alias for custom fields - can hold any YAML-serializable value
pub type CustomFields = HashMap<String, serde_yaml::Value>;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Copy)]
pub enum Priority {
    Low,
    Medium,
    High,
    Critical,
}

impl Priority {
    pub fn is_default(&self) -> bool {
        matches!(self, Priority::Medium)
    }
}

impl Default for Priority {
    fn default() -> Self {
        Priority::Medium
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
        match s.to_uppercase().as_str() {
            "LOW" => Ok(Priority::Low),
            "MEDIUM" => Ok(Priority::Medium),
            "HIGH" => Ok(Priority::High),
            "CRITICAL" => Ok(Priority::Critical),
            _ => Err(format!("Invalid priority: {}", s)),
        }
    }
}

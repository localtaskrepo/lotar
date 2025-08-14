use clap::{Args, Subcommand, ValueEnum};
use serde::Deserialize;
use serde::de::{Deserializer, Error as DeError};
use serde_json::Value as JsonValue;

use super::common::parse_key_value;

/// Available fields for sorting tasks
#[derive(Clone, Debug, ValueEnum, Deserialize)]
pub enum SortField {
    Priority,
    DueDate,
    Created,
    Modified,
    Status,
}

// Custom deserializer for fields: support {k:v}, ["k=v"], [["k","v"]]
fn deserialize_kv_pairs<'de, D>(deserializer: D) -> Result<Vec<(String, String)>, D::Error>
where
    D: Deserializer<'de>,
{
    let v: JsonValue = JsonValue::deserialize(deserializer)?;
    match v {
        JsonValue::Object(map) => Ok(map
            .into_iter()
            .map(|(k, v)| (k, v.as_str().unwrap_or(&v.to_string()).to_string()))
            .collect()),
        JsonValue::Array(arr) => {
            let mut out = Vec::with_capacity(arr.len());
            for item in arr {
                match item {
                    JsonValue::String(s) => {
                        if let Some((k, val)) = s.split_once('=') {
                            out.push((k.trim().to_string(), val.trim().to_string()));
                        } else {
                            return Err(DeError::custom(format!("Invalid key=value entry: {}", s)));
                        }
                    }
                    JsonValue::Array(two) if two.len() == 2 => {
                        let k = two[0]
                            .as_str()
                            .ok_or_else(|| DeError::custom("Expected [key, value] with strings"))?;
                        let val = two[1]
                            .as_str()
                            .ok_or_else(|| DeError::custom("Expected [key, value] with strings"))?;
                        out.push((k.to_string(), val.to_string()));
                    }
                    JsonValue::Object(o) => {
                        if o.len() != 1 {
                            return Err(DeError::custom(
                                "Object must contain exactly one key for pair",
                            ));
                        }
                        let (k, v) = o.into_iter().next().unwrap();
                        out.push((k, v.as_str().unwrap_or(&v.to_string()).to_string()));
                    }
                    other => {
                        return Err(DeError::custom(format!(
                            "Unsupported fields entry type: {}",
                            other
                        )));
                    }
                }
            }
            Ok(out)
        }
        JsonValue::Null => Ok(Vec::new()),
        other => Err(DeError::custom(format!(
            "Expected object or array for fields, got {}",
            other
        ))),
    }
}

#[derive(Args, Deserialize, Debug)]
pub struct AddArgs {
    /// Task title
    pub title: String,

    /// Task type
    #[arg(long = "type", short = 't')]
    #[serde(alias = "type")]
    pub task_type: Option<String>,

    /// Priority level
    #[arg(long, short = 'P')]
    pub priority: Option<String>,

    /// Assignee (email or @username)
    #[arg(long, short = 'a', alias = "assign")]
    pub assignee: Option<String>,

    /// Effort estimate (e.g., 2d, 5h, 1w)
    #[arg(long, short = 'E')]
    pub effort: Option<String>,

    /// Due date (YYYY-MM-DD or relative like 'tomorrow')
    #[arg(long, short = 'd')]
    #[serde(alias = "due_date")]
    pub due: Option<String>,

    /// Task description
    #[arg(long, short = 'D', alias = "desc")]
    pub description: Option<String>,

    /// Category
    #[arg(long, short = 'c', alias = "cat")]
    pub category: Option<String>,

    /// Tags (can be used multiple times)
    #[arg(long = "tag", short = 'i')]
    #[serde(default)]
    pub tags: Vec<String>,

    /// Arbitrary properties (format: key=value)
    #[arg(long = "field", short = 'F', value_parser = parse_key_value)]
    #[serde(default, deserialize_with = "deserialize_kv_pairs")]
    pub fields: Vec<(String, String)>,

    /// Mark as bug (shorthand for --type=bug)
    #[arg(long)]
    #[serde(default)]
    pub bug: bool,

    /// Mark as epic (shorthand for --type=epic)
    #[arg(long)]
    #[serde(default)]
    pub epic: bool,

    /// Mark as critical priority  
    #[arg(long)]
    #[serde(default)]
    pub critical: bool,

    /// Mark as high priority  
    #[arg(long)]
    #[serde(default)]
    pub high: bool,

    /// Preview without writing
    #[arg(long, short = 'n')]
    #[serde(default)]
    pub dry_run: bool,

    /// Explain defaults and choices
    #[arg(long, short = 'e')]
    #[serde(default)]
    pub explain: bool,
}

#[derive(Subcommand)]
pub enum TaskAction {
    /// Add a new task
    Add(TaskAddArgs),
    /// List tasks (with optional filters)
    List(TaskSearchArgs),
    /// Edit an existing task
    Edit(TaskEditArgs),
    /// Change task status
    Status(TaskStatusArgs),
    /// Change task priority
    Priority {
        /// Task ID (with or without project prefix)
        id: String,
        /// New priority (must be valid for project). If omitted, shows current priority.
        priority: Option<String>,
    },
    /// Change task assignee
    Assignee {
        /// Task ID (with or without project prefix)
        id: String,
        /// New assignee. If omitted, shows current assignee.
        assignee: Option<String>,
    },
    /// Change task due date
    DueDate {
        /// Task ID (with or without project prefix)
        id: String,
        /// New due date (YYYY-MM-DD or relative like 'tomorrow'). If omitted, shows current due date.
        due_date: Option<String>,
    },
    /// Delete a task
    Delete(TaskDeleteArgs),
}

#[derive(Args, Deserialize, Debug)]
pub struct TaskAddArgs {
    /// Task title
    pub title: String,

    /// Task type
    #[arg(long = "type", short = 't')]
    #[serde(alias = "type")]
    pub task_type: Option<String>,

    /// Priority level
    #[arg(long, short = 'P')]
    pub priority: Option<String>,

    /// Assignee (email or @username)
    #[arg(long, short = 'a', alias = "assign")]
    pub assignee: Option<String>,

    /// Effort estimate (e.g., 2d, 5h, 1w)
    #[arg(long, short = 'E')]
    pub effort: Option<String>,

    /// Due date (YYYY-MM-DD or relative like 'tomorrow')
    #[arg(long, short = 'd')]
    #[serde(alias = "due_date")]
    pub due: Option<String>,

    /// Task description
    #[arg(long, short = 'D', alias = "desc")]
    pub description: Option<String>,

    /// Category
    #[arg(long, short = 'c', alias = "cat")]
    pub category: Option<String>,

    /// Tags (can be used multiple times)
    #[arg(long = "tag", short = 'i')]
    #[serde(default)]
    pub tags: Vec<String>,

    /// Custom fields
    #[arg(long = "field", short = 'F', value_parser = parse_key_value)]
    #[serde(default, deserialize_with = "deserialize_kv_pairs")]
    pub fields: Vec<(String, String)>,
}

#[derive(Args, Deserialize, Debug)]
pub struct TaskEditArgs {
    /// Task ID to edit
    pub id: String,

    /// New title
    #[arg(long, short = 'T')]
    pub title: Option<String>,

    /// New type
    #[arg(long = "type", short = 't')]
    #[serde(alias = "type")]
    pub task_type: Option<String>,

    /// New priority
    #[arg(long, short = 'P')]
    pub priority: Option<String>,

    /// New assignee
    #[arg(long, short = 'a')]
    pub assignee: Option<String>,

    /// New effort estimate
    #[arg(long, short = 'E')]
    pub effort: Option<String>,

    /// New due date
    #[arg(long, short = 'd')]
    #[serde(alias = "due_date")]
    pub due: Option<String>,

    /// New description
    #[arg(long, short = 'D')]
    pub description: Option<String>,

    /// New category
    #[arg(long, short = 'c')]
    pub category: Option<String>,

    /// Add tags (can be used multiple times)
    #[arg(long = "tag", short = 'i')]
    #[serde(default, alias = "tags")]
    pub tags: Vec<String>,

    /// Set custom fields
    #[arg(long = "field", short = 'F', value_parser = parse_key_value)]
    #[serde(default, deserialize_with = "deserialize_kv_pairs")]
    pub fields: Vec<(String, String)>,

    /// Preview changes without saving
    #[arg(long, short = 'n')]
    #[serde(default)]
    pub dry_run: bool,
}

#[derive(Args, Deserialize, Debug)]
pub struct TaskStatusArgs {
    /// Task ID
    pub id: String,

    /// New status
    pub status: String,
}

#[derive(Args, Deserialize, Debug)]
pub struct TaskSearchArgs {
    /// Search query (optional - if not provided, lists all tasks matching filters)
    pub query: Option<String>,

    /// Filter by assignee (@me for current user)
    #[arg(long, short = 'a')]
    pub assignee: Option<String>,

    /// Show only my tasks
    #[arg(long, short = 'm')]
    #[serde(default)]
    pub mine: bool,

    /// Filter by status (can be used multiple times)
    #[arg(long, short = 's')]
    #[serde(default)]
    pub status: Vec<String>,

    /// Filter by priority (can be used multiple times)
    #[arg(long, short = 'P')]
    #[serde(default)]
    pub priority: Vec<String>,

    /// Filter by type (can be used multiple times)
    #[arg(long = "type", short = 't')]
    #[serde(default, alias = "type")]
    pub task_type: Vec<String>,

    /// Filter by tag (can be used multiple times)
    #[arg(long, short = 'i')]
    #[serde(default, alias = "tags")]
    pub tag: Vec<String>,

    /// Filter by category
    #[arg(long, short = 'c')]
    pub category: Option<String>,

    /// Show only high priority tasks
    #[arg(long, short = 'H')]
    #[serde(default)]
    pub high: bool,

    /// Show only critical priority tasks
    #[arg(long, short = 'C')]
    #[serde(default)]
    pub critical: bool,

    /// Sort tasks by field (priority, due-date, created, modified, status)
    #[arg(long, short = 'S')]
    pub sort_by: Option<SortField>,

    /// Reverse sort order
    #[arg(long, short = 'R')]
    #[serde(default)]
    pub reverse: bool,

    /// Limit results
    #[arg(long, short = 'L', default_value = "20")]
    pub limit: usize,
}

#[derive(Args, Deserialize, Debug)]
pub struct TaskDeleteArgs {
    /// Task ID to delete
    pub id: String,

    /// Confirm deletion without prompt
    #[arg(long)]
    #[serde(default)]
    pub force: bool,

    /// Preview deletion without removing the file
    #[arg(long)]
    #[serde(default)]
    pub dry_run: bool,
}

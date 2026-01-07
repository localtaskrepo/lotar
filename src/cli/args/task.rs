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

#[derive(Copy, Clone, Debug, Eq, PartialEq, ValueEnum, Deserialize)]
pub enum RelationshipKind {
    DependsOn,
    Blocks,
    Related,
    Parent,
    Children,
    Fixes,
    DuplicateOf,
}

impl RelationshipKind {
    pub fn as_kebab(self) -> &'static str {
        match self {
            RelationshipKind::DependsOn => "depends-on",
            RelationshipKind::Blocks => "blocks",
            RelationshipKind::Related => "related",
            RelationshipKind::Parent => "parent",
            RelationshipKind::Children => "children",
            RelationshipKind::Fixes => "fixes",
            RelationshipKind::DuplicateOf => "duplicate-of",
        }
    }

    pub fn as_snake(self) -> &'static str {
        match self {
            RelationshipKind::DependsOn => "depends_on",
            RelationshipKind::Blocks => "blocks",
            RelationshipKind::Related => "related",
            RelationshipKind::Parent => "parent",
            RelationshipKind::Children => "children",
            RelationshipKind::Fixes => "fixes",
            RelationshipKind::DuplicateOf => "duplicate_of",
        }
    }
}

// ...existing code...

// Custom deserializer for fields: support {k:v}, ["k=v"], [["k","v"]]
// ...existing code...

// Custom deserializer for fields: support {k:v}, ["k=v"], [["k","v"]]
// ...existing code...
// ...existing code...

// ...existing code...
// Custom deserializer for fields: support {k:v}, ["k=v"], [["k","v"]]

#[allow(dead_code)]
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
                        if let Some((k, v)) = o.into_iter().next() {
                            out.push((k, v.as_str().unwrap_or(&v.to_string()).to_string()));
                        } else {
                            return Err(DeError::custom(
                                "Object must contain exactly one key for pair",
                            ));
                        }
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

    /// Reporter (email or @username)
    #[arg(long, short = 'R')]
    pub reporter: Option<String>,

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
    /// View or update effort for a task
    Effort(TaskEffortArgs),

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
    /// Show relationships for a task
    Relationships(TaskRelationshipsArgs),
    /// Delete a task
    Delete(TaskDeleteArgs),

    /// Show git history for a task file (read-only)
    History {
        /// Task ID (with or without project prefix)
        id: String,
        /// Limit number of commits
        #[arg(long, short = 'L', default_value = "20")]
        limit: usize,
    },

    /// Show per-field change history derived from git snapshots
    #[command(alias = "history-field")]
    HistoryByField {
        /// Field to trace (status|priority|assignee|tags)
        #[arg(value_enum)]
        field: HistoryField,
        /// Task ID (with or without project prefix)
        id: String,
        /// Limit number of changes (default 20)
        #[arg(long, short = 'L', default_value = "20")]
        limit: usize,
    },

    /// Show raw diff patch for the latest or given commit touching the task
    Diff {
        /// Task ID (with or without project prefix)
        id: String,
        /// Specific commit SHA to show (default: latest for this file)
        #[arg(long)]
        commit: Option<String>,
        /// Show structured field-level diff instead of raw patch
        #[arg(long, default_value_t = false)]
        fields: bool,
    },

    /// Show the file snapshot at a specific commit
    At {
        /// Task ID (with or without project prefix)
        id: String,
        /// Commit SHA (or ref) to load
        commit: String,
    },

    /// Add or list comments for a task
    Comment {
        /// Task ID (with or without project prefix)
        id: String,
        /// Comment text (optional if using -m or -F)
        text: Option<String>,
        /// Comment message (useful for shell-safe multi-word input)
        #[arg(short = 'm', long = "message")]
        message: Option<String>,
        /// Read comment text from file
        #[arg(short = 'F', long = "file")]
        file: Option<String>,
        /// Preview the comment without saving
        #[arg(long, short = 'n')]
        dry_run: bool,
        /// Explain how comments are persisted
        #[arg(long, short = 'e')]
        explain: bool,
    },

    /// Add or remove references for a task
    #[command(alias = "references")]
    Reference(TaskReferenceArgs),
}

#[derive(Args, Deserialize, Debug)]
pub struct TaskReferenceArgs {
    #[command(subcommand)]
    pub action: TaskReferenceAction,
}

#[derive(Subcommand, Deserialize, Debug)]
pub enum TaskReferenceAction {
    /// Attach a reference to a task
    Add(TaskReferenceAddArgs),
    /// Detach a reference from a task
    Remove(TaskReferenceRemoveArgs),
}

#[derive(Args, Deserialize, Debug)]
pub struct TaskReferenceAddArgs {
    #[command(subcommand)]
    pub kind: TaskReferenceKindAdd,
}

#[derive(Subcommand, Deserialize, Debug)]
pub enum TaskReferenceKindAdd {
    /// Add a link reference (URL)
    Link { id: String, url: String },
    /// Add a file reference (repo-relative path)
    File { id: String, path: String },
    /// Add a code reference (e.g. src/lib.rs#10-12)
    Code { id: String, code: String },
}

#[derive(Args, Deserialize, Debug)]
pub struct TaskReferenceRemoveArgs {
    #[command(subcommand)]
    pub kind: TaskReferenceKindRemove,
}

#[derive(Subcommand, Deserialize, Debug)]
pub enum TaskReferenceKindRemove {
    /// Remove a link reference (URL)
    Link { id: String, url: String },
    /// Remove a file reference (repo-relative path)
    File { id: String, path: String },
    /// Remove a code reference (e.g. src/lib.rs#10-12)
    Code { id: String, code: String },
}

#[derive(Args, Deserialize, Debug)]
pub struct TaskRelationshipsArgs {
    /// Task ID (with or without project prefix)
    pub id: String,

    /// Filter by relationship kind (repeat to include multiple)
    #[arg(long = "kind", value_enum)]
    #[serde(default)]
    pub kinds: Vec<RelationshipKind>,
}

#[derive(Args, Debug)]
pub struct TaskEffortArgs {
    /// Task ID (with or without project prefix)
    pub id: String,
    /// New effort value (e.g., 2d, 5h, 1w). If omitted, shows current effort.
    pub effort: Option<String>,
    /// Clear effort value
    #[arg(long)]
    pub clear: bool,
    /// Preview changes without saving
    #[arg(long)]
    pub dry_run: bool,
    /// Explain normalization and parsing
    #[arg(long)]
    pub explain: bool,
}

#[derive(Clone, Debug, ValueEnum)]
pub enum HistoryField {
    Status,
    Priority,
    Assignee,
    Tags,
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

    /// Reporter (email or @username)
    #[arg(long, short = 'R')]
    pub reporter: Option<String>,

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

    /// New reporter
    #[arg(long, short = 'R')]
    pub reporter: Option<String>,

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

    /// Show only high priority tasks
    #[arg(long, short = 'H')]
    #[serde(default)]
    pub high: bool,

    /// Show only critical priority tasks
    #[arg(long, short = 'C')]
    #[serde(default)]
    pub critical: bool,

    /// Sort tasks by key (priority|due-date|created|modified|status|assignee|type|project|id)
    #[arg(long, short = 'S')]
    pub sort_by: Option<String>,

    /// Reverse sort order
    #[arg(long, short = 'R')]
    #[serde(default)]
    pub reverse: bool,

    /// Limit results
    #[arg(long, short = 'L', default_value = "20")]
    pub limit: usize,

    /// Show only overdue tasks (due date strictly before now)
    #[arg(long)]
    #[serde(default)]
    pub overdue: bool,

    /// Show only tasks due within N days (default 7 if not provided)
    #[arg(long = "due-soon", num_args = 0..=1, value_name = "DAYS")]
    pub due_soon: Option<Option<usize>>, // --due-soon or --due-soon=N

    /// Unified filters: key=value, repeatable (keys: assignee|status|priority|type|tag|project|field:<name>)
    #[arg(long = "where", value_parser = crate::cli::args::common::parse_key_value, num_args=0.., value_delimiter=None)]
    #[serde(default)]
    pub r#where: Vec<(String, String)>,

    /// Minimum effort (accepts effort format e.g., 2h, 1d, or points number)
    #[arg(long = "effort-min")]
    pub effort_min: Option<String>,

    /// Maximum effort (accepts effort format e.g., 2h, 1d, or points number)
    #[arg(long = "effort-max")]
    pub effort_max: Option<String>,
}

#[derive(Args, Deserialize, Debug)]
pub struct TaskDeleteArgs {
    /// Task ID to delete
    pub id: String,

    /// Confirm deletion without prompt
    #[arg(long, short = 'y', alias = "yes")]
    #[serde(default)]
    pub force: bool,

    /// Preview deletion without removing the file
    #[arg(long)]
    #[serde(default)]
    pub dry_run: bool,
}

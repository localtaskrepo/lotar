use clap::{Args, Subcommand, ValueEnum};

/// Available fields for sorting tasks
#[derive(Clone, Debug, ValueEnum)]
pub enum SortField {
    Priority,
    DueDate,
    Created,
    Modified,
    Status,
}

// Helper function for parsing key=value pairs
fn parse_key_value(s: &str) -> Result<(String, String), String> {
    if let Some((key, value)) = s.split_once('=') {
        Ok((key.trim().to_string(), value.trim().to_string()))
    } else {
        Err(format!(
            "Invalid key=value format: '{}'. Expected format: key=value",
            s
        ))
    }
}

#[derive(Args)]
pub struct AddArgs {
    /// Task title
    pub title: String,

    /// Task type
    #[arg(long = "type")]
    pub task_type: Option<String>,

    /// Priority level
    #[arg(long)]
    pub priority: Option<String>,

    /// Assignee (email or @username)
    #[arg(long, alias = "assign")]
    pub assignee: Option<String>,

    /// Effort estimate (e.g., 2d, 5h, 1w)
    #[arg(long)]
    pub effort: Option<String>,

    /// Due date (YYYY-MM-DD or relative like 'tomorrow')
    #[arg(long)]
    pub due: Option<String>,

    /// Task description
    #[arg(long, alias = "desc")]
    pub description: Option<String>,

    /// Category
    #[arg(long, alias = "cat")]
    pub category: Option<String>,

    /// Tags (can be used multiple times)
    #[arg(long = "tag")]
    pub tags: Vec<String>,

    /// Arbitrary properties (format: key=value)
    #[arg(long = "field", value_parser = parse_key_value)]
    pub fields: Vec<(String, String)>,

    /// Mark as bug (shorthand for --type=bug)
    #[arg(long)]
    pub bug: bool,

    /// Mark as epic (shorthand for --type=epic)
    #[arg(long)]
    pub epic: bool,

    /// Mark as critical priority  
    #[arg(long)]
    pub critical: bool,

    /// Mark as high priority  
    #[arg(long)]
    pub high: bool,
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

#[derive(Args)]
pub struct TaskAddArgs {
    /// Task title
    pub title: String,

    /// Task type
    #[arg(long = "type")]
    pub task_type: Option<String>,

    /// Priority level
    #[arg(long)]
    pub priority: Option<String>,

    /// Assignee (email or @username)
    #[arg(long, alias = "assign")]
    pub assignee: Option<String>,

    /// Effort estimate (e.g., 2d, 5h, 1w)
    #[arg(long)]
    pub effort: Option<String>,

    /// Due date (YYYY-MM-DD or relative like 'tomorrow')
    #[arg(long)]
    pub due: Option<String>,

    /// Task description
    #[arg(long, alias = "desc")]
    pub description: Option<String>,

    /// Category
    #[arg(long, alias = "cat")]
    pub category: Option<String>,

    /// Tags (can be used multiple times)
    #[arg(long = "tag")]
    pub tags: Vec<String>,

    /// Custom fields
    #[arg(long = "field", value_parser = parse_key_value)]
    pub fields: Vec<(String, String)>,
}

#[derive(Args)]
pub struct TaskEditArgs {
    /// Task ID to edit
    pub id: String,

    /// New title
    #[arg(long)]
    pub title: Option<String>,

    /// New type
    #[arg(long = "type")]
    pub task_type: Option<String>,

    /// New priority
    #[arg(long)]
    pub priority: Option<String>,

    /// New assignee
    #[arg(long)]
    pub assignee: Option<String>,

    /// New effort estimate
    #[arg(long)]
    pub effort: Option<String>,

    /// New due date
    #[arg(long)]
    pub due: Option<String>,

    /// New description
    #[arg(long)]
    pub description: Option<String>,

    /// New category
    #[arg(long)]
    pub category: Option<String>,

    /// Add tags (can be used multiple times)
    #[arg(long = "tag")]
    pub tags: Vec<String>,

    /// Set custom fields
    #[arg(long = "field", value_parser = parse_key_value)]
    pub fields: Vec<(String, String)>,
}

#[derive(Args)]
pub struct TaskStatusArgs {
    /// Task ID
    pub id: String,

    /// New status
    pub status: String,
}

#[derive(Args)]
pub struct TaskSearchArgs {
    /// Search query (optional - if not provided, lists all tasks matching filters)
    pub query: Option<String>,

    /// Filter by assignee (@me for current user)
    #[arg(long)]
    pub assignee: Option<String>,

    /// Show only my tasks
    #[arg(long)]
    pub mine: bool,

    /// Filter by status (can be used multiple times)
    #[arg(long)]
    pub status: Vec<String>,

    /// Filter by priority (can be used multiple times)
    #[arg(long)]
    pub priority: Vec<String>,

    /// Filter by type (can be used multiple times)
    #[arg(long = "type")]
    pub task_type: Vec<String>,

    /// Filter by tag (can be used multiple times)
    #[arg(long)]
    pub tag: Vec<String>,

    /// Filter by category
    #[arg(long)]
    pub category: Option<String>,

    /// Show only high priority tasks
    #[arg(long)]
    pub high: bool,

    /// Show only critical priority tasks
    #[arg(long)]
    pub critical: bool,

    /// Sort tasks by field (priority, due-date, created, modified, status)
    #[arg(long)]
    pub sort_by: Option<SortField>,

    /// Reverse sort order
    #[arg(long)]
    pub reverse: bool,

    /// Limit results
    #[arg(long, default_value = "20")]
    pub limit: usize,
}

#[derive(Args)]
pub struct TaskDeleteArgs {
    /// Task ID to delete
    pub id: String,

    /// Confirm deletion without prompt
    #[arg(long)]
    pub force: bool,
}

use crate::output::OutputFormat;
use crate::types::{Priority, TaskType};
use clap::{Args, Parser, Subcommand, ValueEnum};

/// Available fields for sorting tasks
#[derive(Clone, Debug, ValueEnum)]
pub enum SortField {
    Priority,
    DueDate,
    Created,
    Modified,
    Status,
}

#[derive(Parser)]
#[command(name = "lotar")]
#[command(about = "Local Task Repository - Git-integrated task management")]
#[command(version, author)]
pub struct Cli {
    /// Global project context (overrides auto-detection)
    #[arg(short = 'p', long, global = true)]
    pub project: Option<String>,

    /// Tasks directory path (overrides default)
    #[arg(long, global = true)]
    pub tasks_dir: Option<String>,

    /// Output format
    #[arg(long, global = true, value_enum, default_value_t = OutputFormat::Text)]
    pub format: OutputFormat,

    /// Verbose output
    #[arg(short, long, global = true)]
    pub verbose: bool,

    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Subcommand)]
pub enum Commands {
    /// Quick add task with smart defaults
    Add(AddArgs),

    /// Quick list tasks  
    List(TaskSearchArgs),

    /// Change task status (validates against project config)
    Status {
        /// Task ID (with or without project prefix)
        id: String,
        /// New status (must be valid for project). If omitted, shows current status.
        status: Option<String>,
    },

    /// Change task priority (validates against project config)
    Priority {
        /// Task ID (with or without project prefix)
        id: String,
        /// New priority (must be valid for project). If omitted, shows current priority.
        priority: Option<String>,
    },

    /// Priority shortcut - same as priority command
    #[command(alias = "p")]
    PriorityShort {
        /// Task ID (with or without project prefix)  
        id: String,
        /// New priority (must be valid for project). If omitted, shows current priority.
        priority: Option<String>,
    },

    /// Change task due date
    DueDate {
        /// Task ID (with or without project prefix)
        id: String,
        /// New due date (YYYY-MM-DD or relative like 'tomorrow'). If omitted, shows current due date.
        due_date: Option<String>,
    },

    /// Set arbitrary task property
    Set {
        /// Task ID
        id: String,
        /// Property name
        property: String,
        /// Property value
        value: String,
    },

    /// Full task management (existing functionality)
    #[command(alias = "tasks")]
    Task {
        #[command(subcommand)]
        action: TaskAction,
    },

    /// Configuration management (existing functionality)
    Config {
        #[command(subcommand)]
        action: ConfigAction,
    },

    /// Scan source files for TODO comments (existing)
    Scan(ScanArgs),

    /// Start web server (existing)
    Serve(ServeArgs),

    /// Index management (existing)
    Index(IndexArgs),
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

// CLI-compatible enums that map to our internal types
#[derive(Clone, Debug, ValueEnum)]
pub enum CliTaskType {
    Feature,
    Bug,
    Epic,
    Spike,
    Chore,
}

impl From<CliTaskType> for TaskType {
    fn from(cli_type: CliTaskType) -> Self {
        match cli_type {
            CliTaskType::Feature => TaskType::Feature,
            CliTaskType::Bug => TaskType::Bug,
            CliTaskType::Epic => TaskType::Epic,
            CliTaskType::Spike => TaskType::Spike,
            CliTaskType::Chore => TaskType::Chore,
        }
    }
}

#[derive(Clone, Debug, ValueEnum)]
pub enum CliPriority {
    Low,
    Medium,
    High,
    Critical,
}

impl From<CliPriority> for Priority {
    fn from(cli_priority: CliPriority) -> Self {
        match cli_priority {
            CliPriority::Low => Priority::Low,
            CliPriority::Medium => Priority::Medium,
            CliPriority::High => Priority::High,
            CliPriority::Critical => Priority::Critical,
        }
    }
}

// Existing command structures (placeholder - will use existing implementations)
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
    #[arg(short = 'n', long, default_value = "20")]
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

#[derive(Subcommand)]
pub enum ConfigAction {
    /// Show current configuration
    Show(ConfigShowArgs),
    /// Set a configuration value
    Set(ConfigSetArgs),
    /// Initialize project configuration
    Init(ConfigInitArgs),
    /// List available templates
    Templates,
}

#[derive(Args)]
pub struct ConfigShowArgs {
    /// Show project-specific configuration
    #[arg(long)]
    pub project: Option<String>,
}

#[derive(Args)]
pub struct ConfigSetArgs {
    /// Configuration field name
    pub field: String,

    /// Configuration value
    pub value: String,

    /// Perform a dry-run without making changes
    #[arg(long)]
    pub dry_run: bool,

    /// Skip validation warnings
    #[arg(long)]
    pub force: bool,

    /// Apply to global configuration instead of project
    #[arg(long)]
    pub global: bool,
}

#[derive(Args)]
pub struct ConfigInitArgs {
    /// Template to use
    #[arg(long, default_value = "default")]
    pub template: String,

    /// Project prefix (e.g., 'PROJ' for PROJ-1, PROJ-2, etc.)
    #[arg(long)]
    pub prefix: Option<String>,

    /// Project name
    #[arg(long)]
    pub project: Option<String>,

    /// Copy settings from another project
    #[arg(long)]
    pub copy_from: Option<String>,

    /// Initialize global configuration instead of project
    #[arg(long)]
    pub global: bool,

    /// Perform a dry-run without making changes
    #[arg(long)]
    pub dry_run: bool,

    /// Force initialization even if config exists
    #[arg(long)]
    pub force: bool,
}

#[derive(Args)]
pub struct ScanArgs {
    /// Path to scan (defaults to current directory)
    pub path: Option<String>,

    /// Include specific file extensions
    #[arg(long)]
    pub include: Vec<String>,

    /// Exclude specific file extensions
    #[arg(long)]
    pub exclude: Vec<String>,

    /// Show detailed output
    #[arg(long)]
    pub detailed: bool,
}

#[derive(Args)]
pub struct ServeArgs {
    /// Port to serve on
    #[arg(default_value = "8080")]
    pub port: Option<u16>,

    /// Host to bind to
    #[arg(long, default_value = "localhost")]
    pub host: String,

    /// Open browser automatically
    #[arg(long)]
    pub open: bool,
}

#[derive(Args)]
pub struct IndexArgs {
    #[command(subcommand)]
    pub action: IndexAction,
}

#[derive(Subcommand)]
pub enum IndexAction {
    /// Rebuild the search index
    Rebuild,
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

pub mod handlers;
pub mod project;
pub mod validation;

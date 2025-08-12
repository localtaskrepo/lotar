use crate::output::{LogLevel, OutputFormat};
use crate::types::{Priority, TaskType};
use clap::{Parser, Subcommand, ValueEnum};

// CLI argument modules consolidated under cli/args
pub mod args;
pub use args::{
    AddArgs, ConfigAction, ConfigInitArgs, ConfigSetArgs, ConfigShowArgs, ConfigValidateArgs,
    IndexAction, IndexArgs, ScanArgs, ServeArgs, SortField, TaskAction, TaskAddArgs,
    TaskDeleteArgs, TaskEditArgs, TaskSearchArgs, TaskStatusArgs, parse_key_value,
};

#[derive(Parser)]
#[command(name = "lotar")]
#[command(about = "Local Task Repository - Git-integrated task management")]
#[command(version, author)]
pub struct Cli {
    /// Global project context (overrides auto-detection)
    #[arg(long, global = true)]
    pub project: Option<String>,

    /// Tasks directory path (overrides default)
    #[arg(long, global = true)]
    pub tasks_dir: Option<String>,

    /// Output format
    #[arg(long, global = true, value_enum, default_value_t = OutputFormat::Text)]
    pub format: OutputFormat,

    /// Log level (controls diagnostic verbosity)
    #[arg(long, global = true, value_enum, env = "LOTAR_LOG_LEVEL", default_value_t = LogLevel::Warn)]
    pub log_level: LogLevel,

    /// Backward-compat verbose flag (maps to Info level)
    #[arg(long, global = true, hide = true)]
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

    /// Start MCP JSON-RPC server over stdio
    Mcp,
}

// AddArgs moved to args_task

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

// Task-related arg structs and enums are re-exported from args_task

// ScanArgs, ServeArgs, IndexArgs and IndexAction moved to args_* modules

// parse_key_value moved to args_common

pub mod handlers;
pub mod project;
pub mod validation;

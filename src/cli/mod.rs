use crate::output::{LogLevel, OutputFormat};
use clap::{Parser, Subcommand};

// CLI argument modules consolidated under cli/args
pub mod args;
pub use args::{
    AddArgs, CompletionShell, CompletionsAction, CompletionsArgs, ConfigAction, ConfigInitArgs,
    ConfigNormalizeArgs, ConfigSetArgs, ConfigShowArgs, ConfigValidateArgs, GitAction,
    GitHooksAction, GitHooksInstallArgs, IndexAction, IndexArgs, ScanArgs, ServeArgs, SortField,
    SprintAction, SprintArgs, SprintCreateArgs, SprintListArgs, SprintShowArgs, StatsArgs,
    TaskAction, TaskAddArgs, TaskDeleteArgs, TaskEditArgs, TaskSearchArgs, TaskStatusArgs,
    parse_key_value,
};
pub mod preprocess;

#[derive(Parser)]
#[command(name = "lotar")]
#[command(about = "Local Task Repository - Git-integrated task management")]
#[command(version, author)]
pub struct Cli {
    /// Global project context (overrides auto-detection)
    #[arg(long, short = 'p', global = true)]
    pub project: Option<String>,

    /// Tasks directory path (overrides default)
    #[arg(long, global = true)]
    pub tasks_dir: Option<String>,

    /// Output format
    #[arg(long, short = 'f', global = true, value_parser = crate::output::parse_output_format, default_value = "text")]
    pub format: OutputFormat,

    /// Log level (controls diagnostic verbosity)
    #[arg(long, short = 'l', global = true, value_enum, default_value_t = LogLevel::Warn)]
    pub log_level: LogLevel,

    /// Backward-compat verbose flag (maps to Info level)
    #[arg(long, short = 'v', global = true, hide = true)]
    pub verbose: bool,

    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Subcommand)]
pub enum Commands {
    /// Quick add task with smart defaults
    Add(AddArgs),

    /// Quick list tasks  
    #[command(aliases = ["ls"])]
    List(TaskSearchArgs),

    /// Change task status (validates against project config)
    Status {
        /// Task ID (with or without project prefix)
        id: String,
        /// New status (must be valid for project). If omitted, shows current status.
        status: Option<String>,
        /// Preview the change without saving
        #[arg(long, short = 'n')]
        dry_run: bool,
        /// Explain what values are chosen and why
        #[arg(long, short = 'e')]
        explain: bool,
    },

    /// Change task priority (validates against project config)
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

    /// Change or view task effort estimate
    Effort {
        /// Task ID (with or without project prefix)
        id: String,
        /// New effort value (e.g., 2d, 5h, 1w, or points as a number). If omitted, shows current effort.
        effort: Option<String>,
        /// Clear effort value
        #[arg(long)]
        clear: bool,
        /// Preview the change without saving
        #[arg(long, short = 'n')]
        dry_run: bool,
        /// Explain normalization and parsing
        #[arg(long, short = 'e')]
        explain: bool,
    },

    /// Add a comment to a task (shortcut)
    #[command(alias = "c")]
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
    },

    /// Full task management (existing functionality)
    #[command(alias = "tasks")]
    Task {
        #[command(subcommand)]
        action: Option<TaskAction>,
    },

    /// Configuration management (existing functionality)
    Config {
        #[command(subcommand)]
        action: Option<ConfigAction>,
    },

    /// Scan source files for TODO comments (existing)
    Scan(ScanArgs),

    /// Start web server (existing)
    Serve(ServeArgs),

    /// Statistics and analytics (read-only)
    Stats(StatsArgs),

    /// Sprint storage management (create/list/show commands)
    Sprint(SprintArgs),

    /// Show task changes (default: vs HEAD working tree; optionally vs a ref)
    Changelog {
        /// Compare since this git ref (e.g., HEAD~1, a tag, or a commit); if omitted, compares working tree vs HEAD
        since: Option<String>,
        /// Span all projects under .tasks instead of the current/effective project
        #[arg(long)]
        global: bool,
    },

    /// Start MCP JSON-RPC server over stdio
    Mcp,

    /// Show the resolved current user identity used for reporter/assignee
    Whoami {
        /// Explain resolution chain and sources
        #[arg(long)]
        explain: bool,
    },

    /// Git integration helpers (hooks, status checks)
    Git {
        #[command(subcommand)]
        action: Option<GitAction>,
    },

    /// Shell completions support
    Completions(CompletionsArgs),
}

// AddArgs moved to args_task

// Task-related arg structs and enums are re-exported from args_task

// ScanArgs, ServeArgs, IndexArgs and IndexAction moved to args_* modules

// parse_key_value moved to args_common

pub mod handlers;
pub mod project;
pub mod validation;

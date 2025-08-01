use clap::{Parser, Subcommand, Args, ValueEnum};
use crate::types::{Priority, TaskType};
use crate::output::OutputFormat;

#[derive(Parser)]
#[command(name = "lotar")]
#[command(about = "Local Task Repository - Git-integrated task management")]
#[command(version, author)]
pub struct Cli {
    /// Global project context (overrides auto-detection)
    #[arg(short = 'p', long, global = true)]
    pub project: Option<String>,
    
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
    List(ListArgs),
    
    /// Change task status (validates against project config)
    Status {
        /// Task ID (with or without project prefix)
        id: String,
        /// New status (must be valid for project)
        status: String,
    },
    
    /// Status shortcut - same as status command
    #[command(alias = "s")]
    StatusShort {
        /// Task ID (with or without project prefix)  
        id: String,
        /// New status (must be valid for project)
        status: String,
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
    #[arg(long, value_enum)]
    pub task_type: Option<CliTaskType>,
    
    /// Priority level
    #[arg(long, value_enum)]  
    pub priority: Option<CliPriority>,
    
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

#[derive(Args)]
pub struct ListArgs {
    /// Filter by assignee (@me for current user)
    #[arg(long)]
    pub assignee: Option<String>,
    
    /// Show only my tasks
    #[arg(long)]
    pub mine: bool,
    
    /// Filter by status
    #[arg(long)]
    pub status: Option<String>,
    
    /// Filter by priority
    #[arg(long, value_enum)]
    pub priority: Option<CliPriority>,
    
    /// Filter by type
    #[arg(long, value_enum)]
    pub task_type: Option<CliTaskType>,
    
    /// Filter by category
    #[arg(long)]
    pub category: Option<String>,
    
    /// Filter by tag
    #[arg(long)]
    pub tag: Option<String>,
    
    /// Show only high priority tasks
    #[arg(long)]
    pub high: bool,
    
    /// Show only critical priority tasks
    #[arg(long)]
    pub critical: bool,
    
    /// Limit number of results
    #[arg(short = 'n', long, default_value = "20")]
    pub limit: usize,
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
    Add,
    List, 
    Edit,
    Status,
    Search,
    Delete,
}

#[derive(Subcommand)]
pub enum ConfigAction {
    Get,
    Set,
    List,
}

#[derive(Args)]
pub struct ScanArgs {
    // Use existing scan arguments
}

#[derive(Args)]
pub struct ServeArgs {
    // Use existing serve arguments
}

#[derive(Args)]
pub struct IndexArgs {
    // Use existing index arguments
}

// Helper function for parsing key=value pairs
fn parse_key_value(s: &str) -> Result<(String, String), String> {
    if let Some((key, value)) = s.split_once('=') {
        Ok((key.trim().to_string(), value.trim().to_string()))
    } else {
        Err(format!("Invalid key=value format: '{}'. Expected format: key=value", s))
    }
}

pub mod handlers;
pub mod validation;
pub mod project;

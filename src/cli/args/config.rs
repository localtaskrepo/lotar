use clap::{Args, Subcommand};

#[derive(Subcommand)]
pub enum ConfigAction {
    /// Show current configuration
    Show(ConfigShowArgs),
    /// Set a configuration value
    Set(ConfigSetArgs),
    /// Initialize project configuration
    Init(Box<ConfigInitArgs>),
    /// List available templates
    Templates,
    /// Validate configuration files
    Validate(ConfigValidateArgs),
    /// Normalize config files to the canonical nested YAML form
    Normalize(ConfigNormalizeArgs),
}

#[derive(Args)]
pub struct ConfigShowArgs {
    /// Show project-specific configuration
    #[arg(long)]
    pub project: Option<String>,

    /// Explain where each value comes from (env, home, global, project, default)
    #[arg(long)]
    pub explain: bool,

    /// Print the complete effective configuration in canonical YAML (or JSON when requested)
    #[arg(long)]
    pub full: bool,
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
    /// Workflow preset: default | agile | kanban
    #[arg(long)]
    pub workflow: Option<String>,

    /// Alias for --workflow (also accepts legacy scaffold names like agent-pipeline)
    #[arg(long)]
    pub template: Option<String>,

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

    /// Skip interactive prompts and accept all defaults (non-interactive)
    #[arg(long, short = 'y', alias = "non-interactive")]
    pub yes: bool,

    /// Add scaffolds: comma-separated list (automation, agents, agents:pipeline, agents:reviewed, sync:jira, sync:github)
    #[arg(long, value_delimiter = ',')]
    pub with: Vec<String>,

    /// Default assignee for new tasks (e.g., @me or alice)
    #[arg(long)]
    pub default_assignee: Option<String>,

    /// Default reporter for new tasks
    #[arg(long)]
    pub default_reporter: Option<String>,

    /// Default priority for new tasks (Low|Medium|High|Critical)
    #[arg(long)]
    pub default_priority: Option<String>,

    /// Default status for new tasks (e.g., Todo)
    #[arg(long)]
    pub default_status: Option<String>,

    /// Comma-separated task states (overrides workflow defaults)
    #[arg(long, value_delimiter = ',')]
    pub states: Vec<String>,

    /// Comma-separated task types
    #[arg(long, value_delimiter = ',')]
    pub types: Vec<String>,

    /// Comma-separated task priorities
    #[arg(long, value_delimiter = ',')]
    pub priorities: Vec<String>,

    /// Comma-separated tag allowlist (use "*" for wildcard)
    #[arg(long, value_delimiter = ',')]
    pub tags: Vec<String>,
}

#[derive(Args)]
pub struct ConfigValidateArgs {
    /// Validate specific project configuration
    #[arg(long)]
    pub project: Option<String>,

    /// Validate global configuration instead of project
    #[arg(long)]
    pub global: bool,

    /// Attempt to automatically fix simple issues
    #[arg(long)]
    pub fix: bool,

    /// Only show errors, not warnings
    #[arg(long)]
    pub errors_only: bool,
}

#[derive(Args)]
pub struct ConfigNormalizeArgs {
    /// Normalize global configuration only
    #[arg(long)]
    pub global: bool,

    /// Normalize a specific project configuration (by prefix)
    #[arg(long)]
    pub project: Option<String>,

    /// Actually write the normalized file(s) to disk (otherwise dry-run)
    #[arg(long)]
    pub write: bool,
}

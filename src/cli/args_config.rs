use clap::{Args, Subcommand};

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
    /// Validate configuration files
    Validate(ConfigValidateArgs),
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

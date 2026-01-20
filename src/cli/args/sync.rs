use clap::{Args, Subcommand};
use serde::Deserialize;

#[derive(Args, Deserialize, Debug)]
pub struct SyncCommandArgs {
    #[command(subcommand)]
    pub action: SyncCommandAction,
}

#[derive(Subcommand, Deserialize, Debug)]
pub enum SyncCommandAction {
    /// Validate remote credentials and filters
    Check(SyncCheckArgs),
}

#[derive(Args, Deserialize, Debug)]
pub struct SyncCheckArgs {
    /// Sync remote name (e.g., jira-home, github-company)
    pub remote: String,

    /// Override auth profile name (home config)
    #[arg(long = "auth-profile")]
    pub auth_profile: Option<String>,
}

#[derive(Args, Deserialize, Debug)]
pub struct SyncArgs {
    /// Sync remote name (e.g., jira-home, github-company)
    pub remote: String,

    /// Sync a single task id (e.g., ABC-123)
    #[arg(long = "task")]
    pub task_id: Option<String>,

    /// Override auth profile name (home config)
    #[arg(long = "auth-profile")]
    pub auth_profile: Option<String>,

    /// Preview planned changes without applying
    #[arg(long, short = 'n')]
    pub dry_run: bool,
}

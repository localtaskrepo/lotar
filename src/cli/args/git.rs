use clap::{Args, Subcommand};

/// Git-related CLI actions.
#[derive(Debug, Clone, Subcommand)]
pub enum GitAction {
    /// Manage git hook integration bundled with the repository.
    Hooks {
        #[command(subcommand)]
        action: GitHooksAction,
    },
}

/// Subcommands available under `lotar git hooks`.
#[derive(Debug, Clone, Subcommand)]
pub enum GitHooksAction {
    /// Configure git hooks to point at the repository's `.githooks` directory.
    Install(GitHooksInstallArgs),
}

/// Options for installing git hooks.
#[derive(Debug, Clone, Args, Default)]
pub struct GitHooksInstallArgs {
    /// Overwrite an existing `core.hooksPath` value.
    #[arg(long)]
    pub force: bool,

    /// Preview the changes without modifying git configuration or file permissions.
    #[arg(long)]
    pub dry_run: bool,
}

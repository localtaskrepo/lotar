use clap::{Args, Subcommand};

use super::sprint::TaskSelectionArgs;

#[derive(Args, Debug)]
pub struct ReleaseArgs {
    #[command(subcommand)]
    pub action: Option<ReleaseAction>,
}

#[derive(Subcommand, Debug)]
pub enum ReleaseAction {
    /// Create a release for a version
    Create(ReleaseCreateArgs),
    /// List releases in version order
    List,
    /// Show a release and its tickets
    Show(ReleaseRefArgs),
    /// Change a release's version, label, notes or dates
    Update(ReleaseUpdateArgs),
    /// Delete a release and take it off its tickets
    Delete(ReleaseDeleteArgs),
    /// Add tickets to a release (they keep their other releases)
    Add(ReleaseAssignArgs),
    /// Move tickets to a release, replacing their other releases
    Move(ReleaseAssignArgs),
    /// Remove tickets from a release
    Remove(ReleaseAssignArgs),
    /// List open tickets that are in no release
    Backlog(ReleaseBacklogArgs),
    /// Mark a release as released
    Release(ReleaseMarkArgs),
    /// Mark a released release as planned again
    Reopen(ReleaseRefArgs),
}

#[derive(Args, Debug, Default)]
pub struct ReleaseCreateArgs {
    /// Version the release ships (e.g. 0.15.0 or v0.15.0)
    pub version: String,
    /// Friendly label (e.g. "Spring")
    #[arg(long)]
    pub label: Option<String>,
    /// Release notes
    #[arg(long)]
    pub notes: Option<String>,
    /// Planned release day (YYYY-MM-DD)
    #[arg(long = "planned", alias = "planned-date", value_name = "DATE")]
    pub planned_date: Option<String>,
    /// When it was released, for a release that is already out (RFC 3339 or YYYY-MM-DD)
    #[arg(long = "released-at", value_name = "WHEN")]
    pub released_at: Option<String>,
}

#[derive(Args, Debug)]
pub struct ReleaseRefArgs {
    /// Release ID (3 or #3), version or label
    pub release: String,
}

#[derive(Args, Debug)]
pub struct ReleaseUpdateArgs {
    /// Release ID (3 or #3), version or label
    pub release: String,
    /// New version
    #[arg(long)]
    pub version: Option<String>,
    /// Label; an empty value clears it
    #[arg(long)]
    pub label: Option<String>,
    /// Notes; an empty value clears them
    #[arg(long)]
    pub notes: Option<String>,
    /// Planned day (YYYY-MM-DD); an empty value clears it
    #[arg(long = "planned", alias = "planned-date", value_name = "DATE")]
    pub planned_date: Option<String>,
    /// When it was released; an empty value marks it planned again
    #[arg(long = "released-at", value_name = "WHEN")]
    pub released_at: Option<String>,
}

#[derive(Args, Debug)]
pub struct ReleaseDeleteArgs {
    /// Release ID (3 or #3), version or label
    pub release: String,
    /// Confirm deletion; required with JSON output or non-interactive input
    #[arg(long)]
    pub force: bool,
}

#[derive(Args, Debug)]
pub struct ReleaseAssignArgs {
    /// Release ID (3 or #3), version or label, then the task IDs; the task
    /// IDs may be left out when `--select*` flags pick the tasks
    #[arg(value_name = "RELEASE", required = true)]
    pub items: Vec<String>,

    #[command(flatten)]
    pub select: TaskSelectionArgs,
}

#[derive(Args, Debug)]
pub struct ReleaseMarkArgs {
    /// Release ID (3 or #3), version or label
    pub release: String,
    /// When it was released (defaults to now)
    #[arg(long = "at", value_name = "WHEN")]
    pub at: Option<String>,
}

#[derive(Args, Debug, Default)]
pub struct ReleaseBacklogArgs {
    /// Filters and search text in the web search syntax, as for `lotar list`
    #[arg(num_args = 0..)]
    pub query: Vec<String>,
    /// Excluded filters from the query, routed by the argument preprocessor.
    #[arg(long = "query-term", hide = true, allow_hyphen_values = true)]
    pub query_terms: Vec<String>,
    /// Restrict results to a project prefix (e.g. TEST)
    #[arg(long)]
    pub project: Option<String>,
    /// Maximum number of tickets to show
    #[arg(long, default_value_t = 20)]
    pub limit: usize,
}

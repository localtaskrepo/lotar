use crate::services::sprint_metrics::SprintBurndownMetric;
use clap::{Args, Subcommand};

#[derive(Args, Debug)]
pub struct SprintArgs {
    #[command(subcommand)]
    pub action: SprintAction,
}

#[derive(Subcommand, Debug)]
pub enum SprintAction {
    /// Create a new sprint from CLI-provided metadata
    Create(SprintCreateArgs),
    /// Update an existing sprint's metadata.
    ///
    /// Use `lotar sprint update --sprint <id> --actual-closed-at <iso8601 timestamp> --actual-started-at <iso8601 timestamp>`
    /// to override lifecycle timestamps for a locked sprint directly from the CLI.
    Update(SprintUpdateArgs),
    /// Mark a sprint as started
    Start(SprintStartArgs),
    /// Mark a sprint as closed
    Close(SprintCloseArgs),
    /// Review sprint outcomes and remaining work
    Review(SprintReviewArgs),
    /// Summarize sprint throughput, capacity, and timeline metrics
    Stats(SprintStatsArgs),
    /// Print a concise health summary for a sprint
    Summary(SprintSummaryArgs),
    /// Generate burndown data for sprint progress tracking
    Burndown(SprintBurndownArgs),
    /// Show an upcoming and in-flight sprint schedule
    Calendar(SprintCalendarArgs),
    /// Summarize completed sprint velocity over time
    Velocity(SprintVelocityArgs),
    /// Remove dangling sprint references from tasks
    CleanupRefs(SprintCleanupRefsArgs),
    /// Normalize sprint files to canonical formatting
    Normalize(SprintNormalizeArgs),
    /// Attach tasks to a sprint
    Add(SprintAddArgs),
    /// Move tasks to a sprint, replacing existing sprint memberships
    Move(SprintMoveArgs),
    /// Detach tasks from a sprint
    Remove(SprintRemoveArgs),
    /// Delete a sprint file and optionally clean up dangling references
    Delete(SprintDeleteArgs),
    /// List tasks that are not assigned to any sprint
    Backlog(SprintBacklogArgs),
    /// List stored sprints with optional limit
    List(SprintListArgs),
    /// Show detailed information about a sprint
    Show(SprintShowArgs),
}

#[derive(Args, Debug, Default)]
pub struct SprintCreateArgs {
    /// Friendly label for the sprint (e.g. "Sprint 42")
    #[arg(long)]
    pub label: Option<String>,
    /// High-level goal statement for the sprint
    #[arg(long)]
    pub goal: Option<String>,
    /// Planned relative length (e.g. 2w, 10d)
    #[arg(long = "length")]
    pub plan_length: Option<String>,
    /// Planned end timestamp (ISO8601)
    #[arg(long)]
    pub ends_at: Option<String>,
    /// Planned start timestamp (ISO8601)
    #[arg(long)]
    pub starts_at: Option<String>,
    /// Planned velocity capacity in points
    #[arg(long)]
    pub capacity_points: Option<u32>,
    /// Planned capacity in hours
    #[arg(long)]
    pub capacity_hours: Option<u32>,
    /// Grace period before overdue warnings (e.g. 12h, 1d)
    #[arg(long)]
    pub overdue_after: Option<String>,
    /// Freeform notes captured in the plan
    #[arg(long)]
    pub notes: Option<String>,
    /// Skip applying global sprint defaults
    #[arg(long)]
    pub no_defaults: bool,
}

#[derive(Args, Debug)]
pub struct SprintUpdateArgs {
    /// Numeric sprint identifier (e.g. 1)
    #[arg(
        value_name = "SPRINT_ID",
        required_unless_present = "sprint",
        conflicts_with = "sprint"
    )]
    pub sprint_id: Option<u32>,
    /// Numeric sprint identifier (e.g. 1)
    #[arg(
        long = "sprint",
        value_name = "SPRINT_ID",
        required_unless_present = "sprint_id",
        conflicts_with = "sprint_id"
    )]
    pub sprint: Option<u32>,
    /// Friendly label for the sprint (e.g. "Sprint 42")
    #[arg(long)]
    pub label: Option<String>,
    /// High-level goal statement for the sprint
    #[arg(long)]
    pub goal: Option<String>,
    /// Planned relative length (e.g. 2w, 10d)
    #[arg(long = "length")]
    pub plan_length: Option<String>,
    /// Planned end timestamp (ISO8601)
    #[arg(long)]
    pub ends_at: Option<String>,
    /// Planned start timestamp (ISO8601)
    #[arg(long)]
    pub starts_at: Option<String>,
    /// Planned velocity capacity in points
    #[arg(long)]
    pub capacity_points: Option<u32>,
    /// Planned capacity in hours
    #[arg(long)]
    pub capacity_hours: Option<u32>,
    /// Grace period before overdue warnings (e.g. 12h, 1d)
    #[arg(long)]
    pub overdue_after: Option<String>,
    /// Freeform notes captured in the plan
    #[arg(long)]
    pub notes: Option<String>,
    /// Observed start timestamp (ISO8601)
    #[arg(long = "actual-started-at")]
    pub actual_started_at: Option<String>,
    /// Observed close timestamp (ISO8601)
    #[arg(long = "actual-closed-at")]
    pub actual_closed_at: Option<String>,
}

impl SprintUpdateArgs {
    pub fn has_mutations(&self) -> bool {
        self.label.is_some()
            || self.goal.is_some()
            || self.plan_length.is_some()
            || self.ends_at.is_some()
            || self.starts_at.is_some()
            || self.capacity_points.is_some()
            || self.capacity_hours.is_some()
            || self.overdue_after.is_some()
            || self.notes.is_some()
            || self.actual_started_at.is_some()
            || self.actual_closed_at.is_some()
    }

    pub fn resolved_sprint_id(&self) -> u32 {
        self.sprint
            .or(self.sprint_id)
            .expect("clap ensures sprint id is provided")
    }
}

#[derive(Args, Debug, Default)]
pub struct SprintStartArgs {
    /// Numeric sprint identifier (e.g. 1). When omitted, the CLI selects the next pending sprint.
    pub sprint_id: Option<u32>,
    /// Timestamp to record as the actual start instant (defaults to now)
    #[arg(long = "at")]
    pub at: Option<String>,
    /// Override existing started timestamp if already present
    #[arg(long)]
    pub force: bool,
    /// Suppress lifecycle warnings for this invocation
    #[arg(long = "no-warn")]
    pub no_warn: bool,
}

#[derive(Args, Debug, Default)]
pub struct SprintCloseArgs {
    /// Numeric sprint identifier (e.g. 1). When omitted, the CLI selects the most recent active sprint.
    pub sprint_id: Option<u32>,
    /// Timestamp to record as the actual close instant (defaults to now)
    #[arg(long = "at")]
    pub at: Option<String>,
    /// Override existing closed timestamp or close an unstarted sprint
    #[arg(long)]
    pub force: bool,
    /// Suppress lifecycle warnings for this invocation
    #[arg(long = "no-warn")]
    pub no_warn: bool,
    /// Run a review immediately after closing
    #[arg(long)]
    pub review: bool,
}

#[derive(Args, Debug, Default)]
pub struct SprintListArgs {
    /// Maximum number of sprints to display (oldest first)
    #[arg(long)]
    pub limit: Option<usize>,
    /// Automatically remove dangling sprint references before listing
    #[arg(long = "cleanup-missing")]
    pub cleanup_missing: bool,
}

#[derive(Args, Debug)]
pub struct SprintShowArgs {
    /// Numeric sprint identifier (e.g. 1)
    pub sprint_id: u32,
}

#[derive(Args, Debug, Default)]
pub struct SprintReviewArgs {
    /// Numeric sprint identifier (e.g. 1). When omitted, the CLI selects the most relevant sprint.
    pub sprint_id: Option<u32>,
}

#[derive(Args, Debug, Default)]
pub struct SprintStatsArgs {
    /// Numeric sprint identifier (e.g. 1). When omitted, the CLI selects the most relevant sprint.
    pub sprint_id: Option<u32>,
}

#[derive(Args, Debug, Default)]
pub struct SprintSummaryArgs {
    /// Numeric sprint identifier (e.g. 1). When omitted, the CLI selects the most relevant sprint.
    pub sprint_id: Option<u32>,
}

#[derive(Args, Debug, Default)]
pub struct SprintBurndownArgs {
    /// Numeric sprint identifier (e.g. 1). When omitted, the CLI selects the most relevant sprint.
    pub sprint_id: Option<u32>,
    /// Focus the CLI table on tasks, points, or hours
    #[arg(long, value_enum, default_value_t)]
    pub metric: SprintBurndownMetric,
}

#[derive(Args, Debug, Default)]
pub struct SprintCalendarArgs {
    /// Maximum number of sprints to display in the calendar view
    #[arg(long)]
    pub limit: Option<usize>,
    /// Include completed sprints alongside pending/active entries
    #[arg(long = "include-complete")]
    pub include_complete: bool,
}

#[derive(Args, Debug, Default)]
pub struct SprintVelocityArgs {
    /// Maximum number of sprints to evaluate (defaults to 6)
    #[arg(long)]
    pub limit: Option<usize>,
    /// Include active or overdue sprints in the velocity calculation
    #[arg(long = "include-active")]
    pub include_active: bool,
    /// Measure velocity using tasks, points, or hours (defaults to points)
    #[arg(long, value_enum, default_value_t = SprintBurndownMetric::Points)]
    pub metric: SprintBurndownMetric,
}

#[derive(Args, Debug, Default)]
pub struct SprintCleanupRefsArgs {
    /// Numeric sprint identifier (e.g. 1). When omitted, all missing sprints are cleaned up.
    pub sprint_id: Option<u32>,
}

#[derive(Args, Debug, Default)]
pub struct SprintNormalizeArgs {
    /// Numeric sprint identifier (e.g. 1). When omitted, all sprints are normalized.
    pub sprint_id: Option<u32>,
    /// Fail if normalization changes would be applied (default)
    #[arg(long)]
    pub check: bool,
    /// Write canonical formatting back to disk
    #[arg(long)]
    pub write: bool,
}

#[derive(Args, Debug, Default)]
pub struct SprintAddArgs {
    /// Explicit sprint reference (numeric id, next, previous). When omitted, the command defaults to the active sprint.
    #[arg(long = "sprint")]
    pub sprint: Option<String>,
    /// Permit adding tasks to a sprint that is already closed.
    #[arg(long)]
    pub allow_closed: bool,
    /// Replace existing sprint membership instead of erroring when a task already belongs to another sprint.
    #[arg(long)]
    pub force: bool,
    /// Automatically remove dangling sprint references before assigning
    #[arg(long = "cleanup-missing")]
    pub cleanup_missing: bool,
    /// Sprint reference (optional) followed by one or more task identifiers.
    #[arg(value_name = "TARGET", required = true)]
    pub items: Vec<String>,
}

#[derive(Args, Debug, Default)]
pub struct SprintMoveArgs {
    /// Explicit sprint reference (numeric id, next, previous). When omitted, the command defaults to the active sprint.
    #[arg(long = "sprint")]
    pub sprint: Option<String>,
    /// Permit moving tasks into a sprint that is already closed.
    #[arg(long)]
    pub allow_closed: bool,
    /// Automatically remove dangling sprint references before moving
    #[arg(long = "cleanup-missing")]
    pub cleanup_missing: bool,
    /// Sprint reference (optional) followed by one or more task identifiers.
    #[arg(value_name = "TARGET", required = true)]
    pub items: Vec<String>,
}

#[derive(Args, Debug, Default)]
pub struct SprintRemoveArgs {
    /// Explicit sprint reference (numeric id, next, previous). When omitted, the command defaults to the active sprint.
    #[arg(long = "sprint")]
    pub sprint: Option<String>,
    /// Automatically remove dangling sprint references before removing memberships
    #[arg(long = "cleanup-missing")]
    pub cleanup_missing: bool,
    /// Sprint reference (optional) followed by one or more task identifiers.
    #[arg(value_name = "TARGET", required = true)]
    pub items: Vec<String>,
}

#[derive(Args, Debug, Default)]
pub struct SprintDeleteArgs {
    /// Numeric sprint identifier (e.g. 1)
    #[arg(
        value_name = "SPRINT_ID",
        required_unless_present = "sprint",
        conflicts_with = "sprint"
    )]
    pub sprint_id: Option<u32>,
    /// Numeric sprint identifier (e.g. 1)
    #[arg(
        long = "sprint",
        value_name = "SPRINT_ID",
        required_unless_present = "sprint_id",
        conflicts_with = "sprint_id"
    )]
    pub sprint: Option<u32>,
    /// Skip the interactive confirmation prompt
    #[arg(long)]
    pub force: bool,
    /// Automatically remove dangling sprint references after deleting the sprint
    #[arg(long = "cleanup-missing")]
    pub cleanup_missing: bool,
}

impl SprintDeleteArgs {
    pub fn resolved_sprint_id(&self) -> u32 {
        self.sprint
            .or(self.sprint_id)
            .expect("clap ensures sprint id is provided")
    }
}

#[derive(Args, Debug, Default)]
pub struct SprintBacklogArgs {
    /// Restrict results to a specific project prefix (e.g. TEST)
    #[arg(long)]
    pub project: Option<String>,
    /// Filter by one or more task tags
    #[arg(long = "tag")]
    pub tag: Vec<String>,
    /// Filter by assignee (supports @me)
    #[arg(long)]
    pub assignee: Option<String>,
    /// Filter by one or more statuses
    #[arg(long = "status")]
    pub status: Vec<String>,
    /// Maximum number of backlog tasks to show
    #[arg(long, default_value_t = 20)]
    pub limit: usize,
    /// Automatically remove dangling sprint references before loading the backlog
    #[arg(long = "cleanup-missing")]
    pub cleanup_missing: bool,
}

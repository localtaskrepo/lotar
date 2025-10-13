use clap::{Args, Subcommand};

#[derive(Args, Debug)]
pub struct StatsArgs {
    #[command(subcommand)]
    pub action: StatsAction,
}

#[derive(Subcommand, Debug)]
pub enum StatsAction {
    /// List tickets changed within a window (git-only)
    Changed {
        /// Since (e.g., 14d, 2025-01-01, "2025-01-01T10:00Z")
        #[arg(long)]
        since: Option<String>,
        /// Until (defaults to now)
        #[arg(long)]
        until: Option<String>,
        /// Filter commits by author substring (case-insensitive)
        #[arg(long)]
        author: Option<String>,
        /// Limit number of tickets in output (default 20)
        #[arg(long, default_value = "20")]
        limit: usize,
        /// Span all projects (default: current project only)
        #[arg(long)]
        global: bool,
    },

    /// Show tickets with highest churn (commit count) in a window
    Churn {
        /// Since (e.g., 14d, 2025-01-01, "2025-01-01T10:00Z")
        #[arg(long)]
        since: Option<String>,
        /// Until (defaults to now)
        #[arg(long)]
        until: Option<String>,
        /// Filter commits by author substring (case-insensitive)
        #[arg(long)]
        author: Option<String>,
        /// Limit number of tickets in output (default 20)
        #[arg(long, default_value = "20")]
        limit: usize,
        /// Span all projects (default: current project only)
        #[arg(long)]
        global: bool,
    },

    /// List top authors (by commits touching tasks) in a window
    Authors {
        /// Since (e.g., 14d, 2025-01-01, "2025-01-01T10:00Z")
        #[arg(long)]
        since: Option<String>,
        /// Until (defaults to now)
        #[arg(long)]
        until: Option<String>,
        /// Limit number of authors in output (default 20)
        #[arg(long, default_value = "20")]
        limit: usize,
        /// Span all projects (default: current project only)
        #[arg(long)]
        global: bool,
    },

    /// Activity grouped by author|day|week|project
    Activity {
        /// Since (e.g., 14d)
        #[arg(long)]
        since: Option<String>,
        /// Until (defaults to now)
        #[arg(long)]
        until: Option<String>,
        /// Grouping key: author|day|week|project
        #[arg(long, value_parser = ["author", "day", "week", "project"], default_value = "day")]
        group_by: String,
        /// Limit number of groups (default 20)
        #[arg(long, default_value = "20")]
        limit: usize,
        /// Span all projects (default: current project only)
        #[arg(long)]
        global: bool,
    },

    /// Tickets with last change older than a threshold (stale)
    Stale {
        /// Threshold age (e.g., 21d, 8w)
        #[arg(long, default_value = "21d")]
        threshold: String,
        /// Limit number of tickets (default 20)
        #[arg(long, default_value = "20")]
        limit: usize,
        /// Span all projects (default: current project only)
        #[arg(long)]
        global: bool,
    },

    /// Top tags across tasks (current state snapshot)
    Tags {
        /// Limit number of tags (default 20)
        #[arg(long, default_value = "20", alias = "top")]
        limit: usize,
        /// Span all projects (default: current project only)
        #[arg(long)]
        global: bool,
    },

    /// Top categories across tasks (current state snapshot)
    Categories {
        /// Limit number of categories (default 20)
        #[arg(long, default_value = "20", alias = "top")]
        limit: usize,
        /// Span all projects (default: current project only)
        #[arg(long)]
        global: bool,
    },

    /// Distribution of tasks by a field (snapshot)
    Distribution {
        /// Field to group by
        #[arg(long, value_enum)]
        field: StatsDistributionField,
        /// Limit number of groups (default 20)
        #[arg(long, default_value = "20")]
        limit: usize,
        /// Span all projects (default: current project only)
        #[arg(long)]
        global: bool,
    },

    /// Due date buckets summary (snapshot)
    Due {
        /// Buckets to include (comma-separated). Default: overdue,today,week,month,later
        #[arg(long)]
        buckets: Option<String>,
        /// Show only overdue items with optional threshold (e.g., 0d, 7d)
        #[arg(long)]
        overdue: bool,
        /// Minimum overdue age; used only with --overdue (default: 0d)
        #[arg(long, default_value = "0d")]
        threshold: String,
        /// Span all projects (default: current project only)
        #[arg(long)]
        global: bool,
    },

    /// Age of tasks since creation, grouped by day|week|month (snapshot)
    Age {
        /// Distribution unit: day|week|month
        #[arg(long, value_enum, default_value_t = StatsAgeDistribution::Day)]
        distribution: StatsAgeDistribution,
        /// Limit number of buckets (default 20)
        #[arg(long, default_value = "20")]
        limit: usize,
        /// Span all projects (default: current project only)
        #[arg(long)]
        global: bool,
    },

    /// Aggregate effort estimates across tasks (snapshot)
    Effort {
        /// Grouping key (built-ins: assignee|type|project|status|priority|reporter|category|tag|field:<name>)
        #[arg(long = "by", default_value = "assignee")]
        by: String,
        /// Filters: --where key=value (repeatable). Keys same as --by; for custom fields use field:<name>.
        #[arg(long = "where", value_parser = crate::cli::args::common::parse_key_value, num_args=0.., value_delimiter=None)]
        r#where: Vec<(String, String)>,
        /// Output unit: hours|days|weeks|points|auto
        #[arg(long = "unit", value_enum, default_value_t = StatsEffortUnit::Hours)]
        unit: StatsEffortUnit,
        /// Limit number of groups (default 20)
        #[arg(long, default_value = "20")]
        limit: usize,
        /// Span all projects (default: current project only)
        #[arg(long)]
        global: bool,
        /// Since (e.g., 14d, 2025-01-01, "2025-01-01T10:00Z")
        #[arg(long)]
        since: Option<String>,
        /// Until (defaults to now)
        #[arg(long)]
        until: Option<String>,
        /// Transitions: only include tasks that changed into this status within the window
        #[arg(long = "transitions")]
        transitions: Option<String>,
    },

    /// Comments statistics (snapshot)
    CommentsTop {
        /// Limit number of tasks (default 20)
        #[arg(long, default_value = "20")]
        limit: usize,
        /// Span all projects (default: current project only)
        #[arg(long)]
        global: bool,
    },

    /// Comments grouped by author (snapshot)
    CommentsByAuthor {
        /// Limit number of authors (default 20)
        #[arg(long, default_value = "20")]
        limit: usize,
        /// Span all projects (default: current project only)
        #[arg(long)]
        global: bool,
    },

    /// Custom fields: list top keys present across tasks (snapshot)
    CustomKeys {
        /// Limit number of keys (default 20)
        #[arg(long, default_value = "20")]
        limit: usize,
        /// Span all projects (default: current project only)
        #[arg(long)]
        global: bool,
    },

    /// Custom fields: distribution of values for a specific field (snapshot)
    CustomField {
        /// Field name
        #[arg(long = "field")]
        name: String,
        /// Limit number of values (default 20)
        #[arg(long, default_value = "20")]
        limit: usize,
        /// Span all projects (default: current project only)
        #[arg(long)]
        global: bool,
    },

    /// Compute time spent in each status for tasks within a window (git-based)
    TimeInStatus {
        /// Since (e.g., 14d, 2025-01-01, "2025-01-01T10:00Z")
        #[arg(long)]
        since: Option<String>,
        /// Until (defaults to now)
        #[arg(long)]
        until: Option<String>,
        /// Limit number of tasks in output (default 20)
        #[arg(long, default_value = "20")]
        limit: usize,
        /// Span all projects (default: current project only)
        #[arg(long)]
        global: bool,
    },

    /// Per-ticket statistics
    Status {
        /// Task ID (full like PROJ-123 or numeric like 123 with project context)
        #[arg()]
        id: String,
        /// Compute time-in-status for this task within a window
        #[arg(long, default_value_t = false)]
        time_in_status: bool,
        /// Since (e.g., 14d, 2025-01-01, "2025-01-01T10:00Z")
        #[arg(long)]
        since: Option<String>,
        /// Until (defaults to now)
        #[arg(long)]
        until: Option<String>,
    },
}

#[derive(clap::ValueEnum, Clone, Debug)]
pub enum StatsDistributionField {
    Status,
    Priority,
    Type,
    Assignee,
    Reporter,
    Project,
    Tag,
    Category,
}

#[derive(clap::ValueEnum, Clone, Debug)]
pub enum StatsAgeDistribution {
    Day,
    Week,
    Month,
}

// Note (LOTA-1): StatsEffortGroupBy removed in favor of free-form string keys via unified field resolver

#[derive(clap::ValueEnum, Clone, Debug)]
pub enum StatsEffortUnit {
    Hours,
    Days,
    Weeks,
    Points,
    Auto,
}

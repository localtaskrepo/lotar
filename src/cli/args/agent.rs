use clap::{Args, Subcommand};

#[derive(Args)]
pub struct AgentArgs {
    #[command(subcommand)]
    pub action: AgentAction,
}

#[derive(Subcommand)]
pub enum AgentAction {
    /// Run an agent job for a ticket
    Run(AgentRunArgs),
    /// Show job status
    Status { id: String },
    /// Show job log entries
    Logs { id: String },
    /// Cancel a job
    Cancel { id: String },
    /// Check for in-progress agent work (useful for git hooks)
    Check(AgentCheckArgs),
    /// List running agent jobs (requires wrapper processes)
    #[command(name = "list-running", alias = "ls")]
    ListRunning,
    /// List all job logs from disk
    #[command(name = "list-jobs")]
    ListJobs(AgentListJobsArgs),
    /// Inspect and manage the file-backed agent queue
    Queue(AgentQueueArgs),
    /// Manage agent worktrees
    Worktree(WorktreeArgs),
    /// Internal worker for agent queueing
    #[command(name = "worker", hide = true)]
    Worker(AgentWorkerArgs),
}

#[derive(Args)]
pub struct WorktreeArgs {
    #[command(subcommand)]
    pub action: WorktreeAction,
}

#[derive(Subcommand)]
pub enum WorktreeAction {
    /// List agent worktrees
    List,
    /// Remove stale agent worktrees and optionally their branches
    Cleanup(WorktreeCleanupArgs),
}

#[derive(Args)]
pub struct WorktreeCleanupArgs {
    /// Also delete the associated git branches
    #[arg(long)]
    pub delete_branches: bool,
    /// Remove all agent worktrees (not just those for completed tickets)
    #[arg(long)]
    pub all: bool,
    /// Preview what would be removed without actually removing anything
    #[arg(long)]
    pub dry_run: bool,
}

#[derive(Args)]
pub struct AgentRunArgs {
    /// Ticket id (e.g. TEST-1)
    pub ticket: String,
    /// Prompt for the agent
    pub prompt: String,
    /// Runner to use (copilot, claude, codex, gemini)
    #[arg(long)]
    pub runner: Option<String>,
    /// Named agent profile to use
    #[arg(long)]
    pub agent: Option<String>,
    /// Wait for the job to finish
    #[arg(long)]
    pub wait: bool,
    /// Stream job events while waiting
    #[arg(long)]
    pub follow: bool,
    /// Timeout (seconds) when waiting for completion
    #[arg(long, value_parser = clap::value_parser!(u64))]
    pub timeout_seconds: Option<u64>,
}

#[derive(Args)]
pub struct AgentCheckArgs {
    /// Status values to flag (defaults to configured agent start status)
    #[arg(long = "status")]
    pub statuses: Vec<String>,
    /// Filter to a specific assignee (e.g. @claude-review)
    #[arg(long)]
    pub assignee: Option<String>,
}

#[derive(Args)]
pub struct AgentListJobsArgs {
    /// Limit number of jobs to list
    #[arg(long, short = 'n', default_value = "20")]
    pub limit: usize,
}

#[derive(Args)]
pub struct AgentQueueArgs {
    #[command(subcommand)]
    pub action: Option<AgentQueueAction>,
}

#[derive(Subcommand)]
pub enum AgentQueueAction {
    /// Remove all pending entries from the queue
    Flush,
    /// Remove a specific pending entry by ticket id
    Remove {
        /// The ticket identifier to remove (e.g. PROJ-1)
        ticket: String,
    },
}

#[derive(Args)]
pub struct AgentWorkerArgs {}

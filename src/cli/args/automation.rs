use clap::{Args, Subcommand};

#[derive(Args)]
pub struct AutomationArgs {
    #[command(subcommand)]
    pub action: AutomationAction,
}

#[derive(Subcommand)]
pub enum AutomationAction {
    /// Simulate automation rules for a ticket and event (dry-run, no side effects)
    #[command(alias = "dry-run")]
    Simulate(AutomationSimulateArgs),
    /// Run rules' manual actions (on.manual) on the tasks they match now, e.g. from cron
    Run(AutomationRunArgs),
}

#[derive(Args)]
pub struct AutomationRunArgs {
    /// Names of the rules to run (default: every rule with a manual action)
    pub rules: Vec<String>,
    /// Only consider tasks matching this query (the `lotar list` search syntax)
    #[arg(long, value_name = "QUERY", allow_hyphen_values = true)]
    pub select: Option<String>,
    /// Show what would change without changing anything
    #[arg(long, short = 'n')]
    pub dry_run: bool,
}

#[derive(Args)]
pub struct AutomationSimulateArgs {
    /// Ticket ID to simulate against (e.g. PROJ-1)
    #[arg(long)]
    pub ticket: String,
    /// Event to simulate: created, updated, assigned, job_started, job_completed, job_failed, job_cancelled
    #[arg(long, default_value = "updated")]
    pub event: String,
}

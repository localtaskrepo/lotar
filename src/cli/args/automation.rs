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

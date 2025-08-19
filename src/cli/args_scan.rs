use clap::Args;

#[derive(Args)]
pub struct ScanArgs {
    /// Paths to scan (defaults to project root when none are provided)
    pub paths: Vec<String>,

    /// Include specific file extensions
    #[arg(long)]
    pub include: Vec<String>,

    /// Exclude specific file extensions
    #[arg(long)]
    pub exclude: Vec<String>,

    /// Show detailed output
    #[arg(long)]
    pub detailed: bool,
}

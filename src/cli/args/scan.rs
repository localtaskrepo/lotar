use clap::Args;

#[derive(Args)]
pub struct ScanArgs {
    /// Path to scan (defaults to current directory)
    pub path: Option<String>,

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

use clap::Args;

#[derive(Args)]
pub struct ScanArgs {
    /// One or more paths to scan; if omitted, defaults to current project or '.'
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

    /// Show N lines of context around each match (used with --detailed)
    #[arg(long, default_value_t = 0)]
    pub context: usize,

    /// Preview without writing (no file modifications)
    #[arg(long)]
    pub dry_run: bool,

    /// Override attribute stripping policy (true/false). If omitted, uses config resolution.
    #[arg(long)]
    pub strip_attributes: Option<bool>,

    /// Re-anchor mode: when adding code references for existing keys, prune anchors across files
    /// keeping only the newest location. Default is false (conservative).
    #[arg(long)]
    pub reanchor: bool,

    /// Scan only git-modified/renamed files (based on `git status --porcelain`).
    /// Falls back to full scan when not in a git repository.
    #[arg(long)]
    pub modified_only: bool,
}

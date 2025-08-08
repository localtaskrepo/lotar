use clap::{Args, Subcommand};

#[derive(Args)]
pub struct IndexArgs {
    #[command(subcommand)]
    pub action: IndexAction,
}

#[derive(Subcommand)]
pub enum IndexAction {
    /// Rebuild the search index
    Rebuild,
}

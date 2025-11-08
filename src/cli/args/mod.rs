pub mod common;
pub mod completions;
pub mod config;
pub mod git;
pub mod index;
pub mod scan;
pub mod serve;
pub mod sprint;
pub mod stats;
pub mod task;

// Re-exports for ergonomic imports from crate::cli::args
pub use common::parse_key_value;
pub use completions::{CompletionShell, CompletionsAction, CompletionsArgs};
pub use config::{
    ConfigAction, ConfigInitArgs, ConfigNormalizeArgs, ConfigSetArgs, ConfigShowArgs,
    ConfigValidateArgs,
};
pub use git::{GitAction, GitHooksAction, GitHooksInstallArgs};
pub use index::{IndexAction, IndexArgs};
pub use scan::ScanArgs;
pub use serve::ServeArgs;
pub use sprint::{
    SprintAction, SprintArgs, SprintCloseArgs, SprintCreateArgs, SprintListArgs, SprintShowArgs,
    SprintStartArgs, SprintUpdateArgs,
};
pub use stats::{StatsAction, StatsArgs};
pub use task::{
    AddArgs, RelationshipKind, SortField, TaskAction, TaskAddArgs, TaskDeleteArgs, TaskEditArgs,
    TaskRelationshipsArgs, TaskSearchArgs, TaskStatusArgs,
};

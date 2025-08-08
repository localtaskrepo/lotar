pub mod common;
pub mod config;
pub mod index;
pub mod scan;
pub mod serve;
pub mod task;

// Re-exports for ergonomic imports from crate::cli::args
pub use common::parse_key_value;
pub use config::{ConfigAction, ConfigInitArgs, ConfigSetArgs, ConfigShowArgs, ConfigValidateArgs};
pub use index::{IndexAction, IndexArgs};
pub use scan::ScanArgs;
pub use serve::ServeArgs;
pub use task::{
    AddArgs, SortField, TaskAction, TaskAddArgs, TaskDeleteArgs, TaskEditArgs, TaskSearchArgs,
    TaskStatusArgs,
};

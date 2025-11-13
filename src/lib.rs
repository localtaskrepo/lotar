// Note: clippy allows for `uninlined_format_args` and `collapsible_if` were
// previously added to suppress new lints when upgrading the stable toolchain.
// We'll remove them temporarily in CI so we can evaluate whether the warnings
// still occur and address them individually. If necessary we can re-add a more
// targeted allow later.

pub mod api_events;
pub mod api_server;
pub mod api_types;
pub mod cli;
pub mod config;
pub mod errors;
pub mod help;
pub mod mcp;
pub mod output;
pub mod project;
pub mod routes;
pub mod scanner;
pub mod services;
pub mod storage;
pub mod types;
pub mod utils;
pub mod web_server;
pub mod workspace;

pub use errors::{LoTaRError, LoTaRResult};
pub use storage::{TaskFilter, manager::Storage, task::Task};
pub use types::TaskStatus;
pub use workspace::TasksDirectoryResolver;

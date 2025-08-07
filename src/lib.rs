// Allow uninlined format args since it's mostly a style preference
#![allow(clippy::uninlined_format_args)]

pub mod api_server;
pub mod cli; // Add the CLI module
pub mod config; // Add the new config module
pub mod errors; // Add the new errors module
pub mod help; // Add help system module
pub mod output; // Add output formatting module
pub mod project;
pub mod routes;
pub mod scanner;
pub mod storage; // Updated to use storage module
pub mod types;
pub mod utils;
pub mod web_server; // Add utilities module
pub mod workspace; // Add workspace resolution module

pub use errors::{LoTaRError, LoTaRResult};
pub use storage::{TaskFilter, manager::Storage, task::Task};
pub use types::TaskStatus; // Export error types
pub use workspace::TasksDirectoryResolver;

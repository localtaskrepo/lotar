pub mod api_server;
pub mod cli; // Add the CLI module
pub mod config; // Add the new config module
pub mod errors; // Add the new errors module
pub mod help; // Add help system module
pub mod index;
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
pub use index::{TaskFilter, TaskIndex};
pub use project::{get_project_name, get_project_path, get_effective_project_name};
pub use scanner::Scanner;
pub use storage::{Storage, Task};
pub use types::TaskStatus; // Export error types
pub use workspace::TasksDirectoryResolver;

pub mod api_server;
pub mod config; // Add the new config module
pub mod errors; // Add the new errors module
pub mod index;
pub mod project;
pub mod routes;
pub mod scanner;
pub mod storage; // Updated to use storage module
pub mod tasks;
pub mod types;
pub mod utils;
pub mod web_server; // Add utilities module

pub use errors::{LoTaRError, LoTaRResult};
pub use index::{TaskFilter, TaskIndex};
pub use project::{get_project_name, get_project_path};
pub use scanner::Scanner;
pub use storage::{Storage, Task};
pub use tasks::task_command;
pub use types::TaskStatus; // Export error types

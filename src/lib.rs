pub mod api_server;
pub mod config;  // Add the new config module
pub mod project;
pub mod routes;
pub mod web_server;
pub mod tasks;
pub mod store;
pub mod scanner;
pub mod index;
pub mod types;
pub mod errors;  // Add the new errors module

pub use store::{Task, Storage};
pub use types::TaskStatus;
pub use scanner::Scanner;
pub use tasks::task_command;
pub use project::{get_project_name, get_project_path};
pub use index::{TaskIndex, TaskFilter};
pub use errors::{LoTaRError, LoTaRResult};  // Export error types

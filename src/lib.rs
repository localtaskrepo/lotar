#![allow(clippy::uninlined_format_args)]

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

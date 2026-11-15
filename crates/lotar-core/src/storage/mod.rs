// Storage module for task persistence and retrieval
//
// This module is organized into focused submodules:
// - task: Task entity definition and methods
// - operations: Core CRUD operations (add, get, edit, delete)
// - search: Search and filtering functionality
// - manager: High-level storage coordination and project management
// - filter: Task filtering utilities
// - release: Release entity definition (DEV-393)
// - sprint: Sprint entity definition and helpers
// - write_log: Which tasks a thread wrote while a log is open (DEV-410)
pub mod filter;
pub mod identity;
pub mod layout;
pub mod locator;
pub mod manager;
pub mod moves;
pub mod operations;
pub mod release;
pub mod safety;
pub mod search;
pub mod sprint;
pub mod task;
pub mod transaction;
pub mod write_log;

// Re-export commonly used types and operations for convenience
pub use filter::{DeletionFilter, TaskFilter};
pub use identity::{TaskId, TaskLocation, compare_task_ids};

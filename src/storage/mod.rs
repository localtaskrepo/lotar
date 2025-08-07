// Storage module for task persistence and retrieval
//
// This module is organized into focused submodules:
// - task: Task entity definition and methods
// - operations: Core CRUD operations (add, get, edit, delete)
// - search: Search and filtering functionality
// - manager: High-level storage coordination and project management
// - filter: Task filtering utilities

pub mod filter;
pub mod manager;
pub mod operations;
pub mod search;
pub mod task;

// Re-export commonly used types and operations for convenience
pub use filter::TaskFilter;

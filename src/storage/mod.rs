// Storage module for task persistence and retrieval
//
// This module is organized into focused submodules:
// - task: Task entity definition and methods
// - operations: Core CRUD operations (add, get, edit, delete)
// - search: Search and filtering functionality
// - manager: High-level storage coordination and project management

pub mod manager;
pub mod operations;
pub mod search;
pub mod task;

// Re-export the main types for convenience
pub use manager::Storage;
pub use task::Task;

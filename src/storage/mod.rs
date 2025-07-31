// Storage module for task persistence and retrieval
//
// This module is organized into focused submodules:
// - task: Task entity definition and methods
// - operations: Core CRUD operations (add, get, edit, delete)
// - search: Search and filtering functionality
// - manager: High-level storage coordination and project management

pub mod task;
pub mod operations;
pub mod search;
pub mod manager;

// Re-export the main types for convenience
pub use task::Task;
pub use manager::Storage;

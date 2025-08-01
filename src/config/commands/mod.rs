// Configuration commands module
//
// This module handles all configuration-related CLI commands and is organized into:
// - router: Main command dispatch and routing
// - handlers: Command-specific handling logic (show, set, init, templates)
// - operations: Field-specific setter operations for configuration values
// - persistence: Configuration file reading, writing, and template management
// - utils: Parsing, display, argument utilities, and help text

pub mod router;
pub mod handlers;
pub mod operations;
pub mod persistence;
pub mod utils;

// Re-export the main entry point for convenience
pub use router::config_command;

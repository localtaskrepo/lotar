// Commands module for CLI command handling
//
// This module organizes all command-line interface commands into focused submodules:
// - config: Configuration management commands (show, set, init, templates)

pub mod config;

// Re-export main command entry points
pub use config::config_command;

//! Configuration management system for lotar
//! 
//! This module handles all configuration-related functionality including:
//! - Global and project-specific configuration loading
//! - Template management for project initialization
//! - Configuration validation and merging
//! - CLI commands for configuration management
//! 
//! The module is organized into:
//! - `types`: Core configuration types and structures
//! - `templates`: Template loading and management
//! - `manager`: Main configuration management logic
//! - `commands`: CLI command handlers for configuration operations

pub mod types;
pub mod templates;
pub mod manager;
pub mod commands;

// Re-export main types for convenience
pub use types::*;
pub use manager::ConfigManager;
pub use commands::config_command;

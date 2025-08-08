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
//! - `manager`: Main configuration management coordination
//! - `operations`: CRUD operations for configuration data
//! - `persistence`: File I/O operations for configuration files
//! - `resolution`: Configuration merging and resolution logic
//! - `validation`: Configuration validation system

pub mod manager;
pub mod operations;
pub mod persistence;
pub mod resolution;
pub mod types;
pub mod validation;

// Re-export main types for public API
pub use manager::ConfigManager;
pub use types::*;

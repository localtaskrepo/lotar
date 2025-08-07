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
//! - `validation`: Configuration validation system

pub mod commands;
pub mod manager;
pub mod templates;
pub mod types;
pub mod validation;

pub use manager::ConfigManager;

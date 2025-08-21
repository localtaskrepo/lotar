pub mod codeowners;
pub mod config;
pub mod effort;
pub mod fields;
pub mod filesystem;
pub mod fuzzy_match;
pub mod git;
pub mod identity;
pub mod identity_detectors;
pub mod paths;
pub mod project;
pub mod time;
pub mod workspace_labels;

// Back-compat re-exports used across the codebase
pub use project::{
    generate_project_prefix, generate_unique_project_prefix, resolve_project_input,
    validate_explicit_prefix,
};

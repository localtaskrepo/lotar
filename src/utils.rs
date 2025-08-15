// Thin utils module root that organizes domain utilities and re-exports

// Domain submodules
pub mod codeowners;
pub mod config;
pub mod filesystem;
pub mod identity;
pub mod identity_detectors;
// (git helpers are exposed at crate::utils_git)
pub mod paths;
pub mod project;
pub mod workspace_labels;

// Backwards-compatible re-exports for widely used project helpers
pub use project::{
    generate_project_prefix, generate_unique_project_prefix, resolve_project_input,
    validate_explicit_prefix,
};

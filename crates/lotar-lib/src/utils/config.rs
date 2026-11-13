use std::fs;
use std::path::Path;

/// Read the project name using the canonical normalization-aware parser.
/// Returns None if the file doesn't exist or the field is not found.
pub fn read_project_name_from_config(config_path: &Path) -> Option<String> {
    if !config_path.exists() {
        return None;
    }

    let content = fs::read_to_string(config_path).ok()?;
    let config = crate::config::normalization::parse_project_from_yaml_str("", &content).ok()?;
    Some(config.project_name).filter(|name| !name.trim().is_empty())
}

// Placeholder for config-related utilities.
// We'll move config parsing/merging helpers here as we continue Task 2.2.

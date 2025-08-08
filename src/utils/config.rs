use std::fs;
use std::path::Path;

/// Read the project_name from a YAML config file (very lightweight parser).
/// Returns None if the file doesn't exist or the field is not found.
pub fn read_project_name_from_config(config_path: &Path) -> Option<String> {
    if !config_path.exists() {
        return None;
    }

    if let Ok(config_content) = fs::read_to_string(config_path) {
        for line in config_content.lines() {
            if let Some(name) = line.strip_prefix("project_name: ") {
                return Some(name.trim().trim_matches('"').to_string());
            }
        }
    }

    None
}

// Placeholder for config-related utilities.
// We'll move config parsing/merging helpers here as we continue Task 2.2.

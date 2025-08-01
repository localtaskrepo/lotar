use std::fs;
use std::path::PathBuf;
use crate::workspace::TasksDirectoryResolver;

/// Get the effective project name by checking global config first, then falling back to auto-detection
pub fn get_effective_project_name(resolver: &TasksDirectoryResolver) -> String {
    // Try to read from global config first
    let global_config_path = resolver.path.join("config.yml");
    if global_config_path.exists() {
        if let Ok(content) = std::fs::read_to_string(&global_config_path) {
            if let Ok(config) = serde_yaml::from_str::<crate::config::types::GlobalConfig>(&content) {
                // If default_prefix is set (not empty), use it
                if !config.default_prefix.is_empty() {
                    return config.default_prefix;
                }
            }
        }
    }

    // Fall back to auto-detection (but generate prefix from detected name)
    if let Some(project_name) = get_project_name() {
        crate::utils::generate_project_prefix(&project_name)
    } else {
        "DEFAULT".to_string()
    }
}

pub fn get_project_name() -> Option<String> {
    detect_project_name()
}

pub fn detect_project_name() -> Option<String> {
    // 1. Check environment variable first
    if let Ok(project) = std::env::var("LOTAR_PROJECT") {
        if !project.is_empty() {
            return Some(project);
        }
    }

    // 2. Try to detect from project files
    if let Some(name) = detect_from_project_files() {
        return Some(name);
    }

    // 3. Use current folder name
    if let Some(name) = get_current_folder_name() {
        return Some(name);
    }

    // 5. Final fallback
    Some("default".to_string())
}

fn detect_from_project_files() -> Option<String> {
    let current_dir = std::env::current_dir().ok()?;

    // Check Cargo.toml (most relevant for this Rust project)
    if let Some(name) = read_cargo_toml_name(&current_dir) {
        return Some(name);
    }

    // Could add other TOML-based project files if needed
    // pyproject.toml, etc. but keeping it simple for now

    None
}

fn read_cargo_toml_name(dir: &PathBuf) -> Option<String> {
    let cargo_toml_path = dir.join("Cargo.toml");
    if !cargo_toml_path.exists() {
        return None;
    }

    let content = fs::read_to_string(&cargo_toml_path).ok()?;

    // Simple regex-based parsing for the name field
    let re = regex::Regex::new(r#"(?m)^name\s*=\s*"([^"]+)""#).ok()?;
    let captures = re.captures(&content)?;

    captures.get(1)
        .map(|m| m.as_str().to_string())
        .filter(|s| !s.is_empty())
}

fn get_current_folder_name() -> Option<String> {
    std::env::current_dir()
        .ok()?
        .file_name()?
        .to_str()
        .map(|s| s.to_string())
        .filter(|s| !s.is_empty() && s != "/" && s != ".")
}


pub fn get_project_path() -> Option<PathBuf> {
    Some(std::env::current_dir().unwrap())
}
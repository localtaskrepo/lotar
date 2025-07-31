use std::fs;
use std::path::PathBuf;

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

    // 4. Check global config for default project
    // Commenting out config manager access to avoid circular dependency
    /*
    if let Ok(config_manager) = ConfigManager::new() {
        let global_config = config_manager.get_global_config();
        if global_config.default_project != "auto" {
            return Some(global_config.default_project.clone());
        }
    }
    */

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
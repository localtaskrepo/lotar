use crate::config::types::*;
use std::env;
use std::fs;
use std::path::Path;

/// Ensure global config exists, creating default if necessary
pub fn ensure_global_config_exists(tasks_dir: Option<&Path>) -> Result<(), ConfigError> {
    let config_path = match tasks_dir {
        Some(dir) => crate::utils::paths::global_config_path(dir),
        None => crate::utils::paths::global_config_path(&crate::utils::paths::tasks_root_from(
            Path::new("."),
        )),
    };

    if !config_path.exists() {
        create_default_global_config(tasks_dir)?;
    }

    Ok(())
}

/// Load global configuration from tasks_dir/config.yml
pub fn load_global_config(tasks_dir: Option<&Path>) -> Result<GlobalConfig, ConfigError> {
    let path = match tasks_dir {
        Some(dir) => crate::utils::paths::global_config_path(dir),
        None => crate::utils::paths::global_config_path(&crate::utils::paths::tasks_root_from(
            Path::new("."),
        )),
    };
    load_config_file(&path)
}

/// Load home configuration from ~/.lotar
pub fn load_home_config() -> Result<GlobalConfig, ConfigError> {
    let home_dir =
        dirs::home_dir().ok_or(ConfigError::IoError("Home directory not found".to_string()))?;
    let path = home_dir.join(".lotar");
    load_config_file(&path)
}

/// Load home configuration with optional path override
pub fn load_home_config_with_override(
    home_config_path: Option<&Path>,
) -> Result<GlobalConfig, ConfigError> {
    let path = match home_config_path {
        Some(override_path) => override_path.to_path_buf(),
        None => {
            let home_dir = dirs::home_dir()
                .ok_or(ConfigError::IoError("Home directory not found".to_string()))?;
            home_dir.join(".lotar")
        }
    };
    load_config_file(&path)
}

/// Load project configuration from .tasks/{project}/config.yml
pub fn load_project_config(project_name: &str) -> Result<ProjectConfig, ConfigError> {
    load_project_config_from_dir(project_name, Path::new(".tasks"))
}

/// Load project configuration from specified directory
pub fn load_project_config_from_dir(
    project_name: &str,
    tasks_dir: &Path,
) -> Result<ProjectConfig, ConfigError> {
    let path = crate::utils::paths::project_config_path(tasks_dir, project_name);
    if !path.exists() {
        return Ok(ProjectConfig::new(project_name.to_string()));
    }

    let content = fs::read_to_string(&path)
        .map_err(|e| ConfigError::IoError(format!("Failed to read project config: {}", e)))?;

    serde_yaml::from_str(&content)
        .map_err(|e| ConfigError::ParseError(format!("Failed to parse project config: {}", e)))
}

/// Load configuration from a specific file path
fn load_config_file(path: &Path) -> Result<GlobalConfig, ConfigError> {
    if !path.exists() {
        return Err(ConfigError::FileNotFound(
            path.to_string_lossy().to_string(),
        ));
    }

    let content = fs::read_to_string(path)
        .map_err(|e| ConfigError::IoError(format!("Failed to read config: {}", e)))?;

    serde_yaml::from_str(&content)
        .map_err(|e| ConfigError::ParseError(format!("Failed to parse config: {}", e)))
}

/// Apply environment variable overrides to configuration
pub fn apply_env_overrides(config: &mut GlobalConfig) {
    if let Ok(port) = env::var("LOTAR_PORT") {
        if let Ok(port_num) = port.parse::<u16>() {
            config.server_port = port_num;
        }
    }

    if let Ok(project) = env::var("LOTAR_PROJECT") {
        // Convert project name to prefix for storage
        config.default_prefix = crate::utils::generate_project_prefix(&project);
    }

    if let Ok(assignee) = env::var("LOTAR_DEFAULT_ASSIGNEE") {
        config.default_assignee = Some(assignee);
    }
}

/// Create default global configuration file
fn create_default_global_config(tasks_dir: Option<&Path>) -> Result<(), ConfigError> {
    let config_path = match tasks_dir {
        Some(dir) => crate::utils::paths::global_config_path(dir),
        None => crate::utils::paths::global_config_path(&crate::utils::paths::tasks_root_from(
            Path::new("."),
        )),
    };

    // Create tasks directory if it doesn't exist
    if let Some(parent) = config_path.parent() {
        if !parent.exists() {
            fs::create_dir_all(parent).map_err(|e| {
                ConfigError::IoError(format!("Failed to create tasks directory: {}", e))
            })?;
        }
    }

    // Create default config with auto-detected prefix
    let mut default_config = GlobalConfig::default();

    // Auto-detect the default prefix from the tasks directory structure
    // This only happens during initial global config creation
    if let Some(tasks_dir_path) = tasks_dir {
        if let Some(detected_prefix) = auto_detect_prefix(tasks_dir_path) {
            default_config.default_prefix = detected_prefix;
        }
        // If no existing projects found, leave default_prefix empty
        // It will be set when the first project is created
    }

    let config_yaml = serde_yaml::to_string(&default_config).map_err(|e| {
        ConfigError::ParseError(format!("Failed to serialize default config: {}", e))
    })?;

    fs::write(&config_path, config_yaml).map_err(|e| {
        ConfigError::IoError(format!("Failed to write default global config: {}", e))
    })?;

    println!(
        "Created default global configuration at: {}",
        config_path.display()
    );
    Ok(())
}

/// Auto-detect the default prefix from existing project directories
/// This scans for existing project directories and their configurations
pub fn auto_detect_prefix(tasks_dir: &Path) -> Option<String> {
    let mut project_prefixes = Vec::new();

    // Look for project directories that exist and have config files
    for (prefix, path) in crate::utils::filesystem::list_visible_subdirs(tasks_dir) {
        let config_path = path.join("config.yml");
        if config_path.exists() {
            project_prefixes.push(prefix);
        }
    }

    if !project_prefixes.is_empty() {
        // If we have existing projects, sort them and return the first one alphabetically
        // This provides deterministic behavior
        project_prefixes.sort();
        return Some(project_prefixes[0].clone());
    }

    // No existing project directories found
    // Return None so default_prefix remains empty until first project is created
    None
}

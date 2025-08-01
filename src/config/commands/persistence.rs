use crate::config::{ConfigError, ConfigManager, GlobalConfig, ProjectConfig};
use crate::project;
use std::fs;
use std::path::PathBuf;

/// Create a project configuration from a template
pub fn create_config_from_template(
    tasks_dir: &PathBuf,
    template: &str,
    project_name: &str,
) -> Result<String, ConfigError> {
    // First, ensure global config exists by creating a ConfigManager
    // This will trigger auto-generation of global config if it doesn't exist
    let _config_manager = ConfigManager::new_with_tasks_dir_ensure_config(tasks_dir)?;

    let config = match ConfigManager::load_template(template) {
        Ok(template_data) => ConfigManager::apply_template_to_project(&template_data, project_name),
        Err(e) => {
            eprintln!("Error: Template '{}' not found: {}", template, e);
            eprintln!("Available templates can be seen with: lotar config templates");
            std::process::exit(1);
        }
    };

    // Generate the 4-letter prefix for the project folder
    let project_prefix = crate::utils::generate_project_prefix(project_name);

    // Create project directory using the prefix
    let project_dir = tasks_dir.join(&project_prefix);
    if !project_dir.exists() {
        fs::create_dir_all(&project_dir).map_err(|e| {
            ConfigError::IoError(format!("Failed to create project directory: {}", e))
        })?;
    }

    // Write config file
    let config_path = project_dir.join("config.yml");
    let config_yaml = serde_yaml::to_string(&config)
        .map_err(|e| ConfigError::ParseError(format!("Failed to serialize config: {}", e)))?;

    fs::write(&config_path, config_yaml)
        .map_err(|e| ConfigError::IoError(format!("Failed to write config file: {}", e)))?;

    Ok(project_prefix)
}

/// Auto-initialize a project with default configuration if it doesn't exist
/// This is called automatically when adding tasks to ensure the project is properly set up
pub fn auto_initialize_project_if_needed(
    tasks_dir: &PathBuf,
    project_name: &str,
) -> Result<String, ConfigError> {
    // Generate the 4-letter prefix for the project folder
    let project_prefix = crate::utils::generate_project_prefix(project_name);
    let project_dir = tasks_dir.join(&project_prefix);
    let config_path = project_dir.join("config.yml");

    // Check if project config already exists
    if config_path.exists() {
        return Ok(project_prefix); // Project already initialized
    }

    // For auto-initialization, create a minimal config that only contains the project name
    // and inherits everything else from global config
    create_minimal_project_config(tasks_dir, project_name)
}

/// Create a minimal project config that only contains the project name
/// All other settings will be inherited from global config
fn create_minimal_project_config(
    tasks_dir: &PathBuf,
    project_name: &str,
) -> Result<String, ConfigError> {
    // First, ensure global config exists by creating a ConfigManager
    // This will trigger auto-generation of global config if it doesn't exist
    let mut config_manager = ConfigManager::new_manager_with_tasks_dir_ensure_config(tasks_dir)?;

    // Create a minimal project config with only the project name
    let config = crate::config::types::ProjectConfig::new(project_name.to_string());

    // Generate the 4-letter prefix for the project folder
    let project_prefix = crate::utils::generate_project_prefix(project_name);

    // Create project directory using the prefix
    let project_dir = tasks_dir.join(&project_prefix);
    if !project_dir.exists() {
        std::fs::create_dir_all(&project_dir).map_err(|e| {
            ConfigError::IoError(format!("Failed to create project directory: {}", e))
        })?;
    }

    // Write minimal config file - only contains project_name, everything else is None/defaults
    let config_path = project_dir.join("config.yml");
    let config_yaml = serde_yaml::to_string(&config)
        .map_err(|e| ConfigError::ParseError(format!("Failed to serialize config: {}", e)))?;

    std::fs::write(&config_path, config_yaml)
        .map_err(|e| ConfigError::IoError(format!("Failed to write config file: {}", e)))?;

    // Update global config's default_prefix if it's currently empty
    // This sets the first created project as the default
    config_manager.set_default_prefix_if_empty(&project_prefix, tasks_dir)?;

    Ok(project_prefix)
}

/// Save a project configuration field using a closure to modify the config
pub fn save_project_config_field<F>(
    tasks_dir: &PathBuf,
    project_name: Option<&str>,
    update_fn: F,
) -> Result<(), ConfigError>
where
    F: FnOnce(&mut ProjectConfig),
{
    let project_name_owned = project_name
        .map(|s| s.to_string())
        .unwrap_or_else(|| project::detect_project_name().unwrap_or_else(|| "default".to_string()));

    // Generate the 4-letter prefix for the project folder
    let project_prefix = crate::utils::generate_project_prefix(&project_name_owned);

    // Load existing project config or create new one
    let config_path = tasks_dir.join(&project_prefix).join("config.yml");
    let mut config = if config_path.exists() {
        let content = fs::read_to_string(&config_path)
            .map_err(|e| ConfigError::IoError(format!("Failed to read config: {}", e)))?;
        serde_yaml::from_str(&content)
            .map_err(|e| ConfigError::ParseError(format!("Failed to parse config: {}", e)))?
    } else {
        // Create project directory if it doesn't exist
        let project_dir = config_path.parent().unwrap();
        if !project_dir.exists() {
            fs::create_dir_all(project_dir).map_err(|e| {
                ConfigError::IoError(format!("Failed to create project directory: {}", e))
            })?;
        }
        ProjectConfig::new(project_name_owned.clone())
    };

    // Apply the update
    update_fn(&mut config);

    // Optimize the config before saving - remove values that match global defaults or are None
    let optimized_config = optimize_project_config(config)?;

    // Save the optimized config
    let config_yaml = serde_yaml::to_string(&optimized_config)
        .map_err(|e| ConfigError::ParseError(format!("Failed to serialize config: {}", e)))?;

    fs::write(&config_path, config_yaml)
        .map_err(|e| ConfigError::IoError(format!("Failed to write config file: {}", e)))?;

    Ok(())
}

/// Save a global configuration field using a closure to modify the config
pub fn save_global_config_field<F>(tasks_dir: &PathBuf, update_fn: F) -> Result<(), ConfigError>
where
    F: FnOnce(&mut GlobalConfig),
{
    // Load existing global config or create new one
    let config_path = tasks_dir.join("config.yml");
    let mut config = if config_path.exists() {
        let content = fs::read_to_string(&config_path)
            .map_err(|e| ConfigError::IoError(format!("Failed to read config: {}", e)))?;
        serde_yaml::from_str(&content)
            .map_err(|e| ConfigError::ParseError(format!("Failed to parse config: {}", e)))?
    } else {
        // Create tasks directory if it doesn't exist
        let parent_dir = config_path.parent().unwrap();
        if !parent_dir.exists() {
            fs::create_dir_all(parent_dir).map_err(|e| {
                ConfigError::IoError(format!("Failed to create tasks directory: {}", e))
            })?;
        }
        GlobalConfig::default()
    };

    // Apply the update
    update_fn(&mut config);

    // Save the updated config
    let config_yaml = serde_yaml::to_string(&config)
        .map_err(|e| ConfigError::ParseError(format!("Failed to serialize config: {}", e)))?;

    fs::write(&config_path, config_yaml)
        .map_err(|e| ConfigError::IoError(format!("Failed to write config file: {}", e)))?;

    Ok(())
}

/// Optimize project config by removing values that match global defaults
fn optimize_project_config(mut config: ProjectConfig) -> Result<ProjectConfig, ConfigError> {
    // Load global config to compare against defaults
    let global_config = GlobalConfig::default();

    // Remove fields that match global defaults
    if let Some(ref states) = config.issue_states {
        if states.values == global_config.issue_states.values {
            config.issue_states = None;
        }
    }

    if let Some(ref types) = config.issue_types {
        if types.values == global_config.issue_types.values {
            config.issue_types = None;
        }
    }

    if let Some(ref priorities) = config.issue_priorities {
        if priorities.values == global_config.issue_priorities.values {
            config.issue_priorities = None;
        }
    }

    if let Some(ref categories) = config.categories {
        if categories.values == global_config.categories.values {
            config.categories = None;
        }
    }

    if let Some(ref tags) = config.tags {
        if tags.values == global_config.tags.values {
            config.tags = None;
        }
    }

    if let Some(ref priority) = config.default_priority {
        if *priority == global_config.default_priority {
            config.default_priority = None;
        }
    }

    if let Some(assignee) = &config.default_assignee {
        if assignee.is_empty() {
            config.default_assignee = None;
        }
    }

    Ok(config)
}

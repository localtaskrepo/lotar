use std::fs;
use std::path::PathBuf;
use crate::config::{ConfigManager, GlobalConfig, ProjectConfig, ConfigError};
use crate::project;

/// Create a project configuration from a template
pub fn create_config_from_template(template: &str, project_name: &str) -> Result<String, ConfigError> {
    // First, ensure global config exists by creating a ConfigManager
    // This will trigger auto-generation of global config if it doesn't exist
    let _config_manager = ConfigManager::new()?;

    let config = match ConfigManager::load_template(template) {
        Ok(template_data) => {
            ConfigManager::apply_template_to_project(&template_data, project_name)
        }
        Err(e) => {
            eprintln!("Error: Template '{}' not found: {}", template, e);
            eprintln!("Available templates can be seen with: lotar config templates");
            std::process::exit(1);
        }
    };

    // Generate the 4-letter prefix for the project folder
    let project_prefix = crate::utils::generate_project_prefix(project_name);

    // Create project directory using the prefix
    let project_dir = PathBuf::from(".tasks").join(&project_prefix);
    if !project_dir.exists() {
        fs::create_dir_all(&project_dir)
            .map_err(|e| ConfigError::IoError(format!("Failed to create project directory: {}", e)))?;
    }

    // Write config file
    let config_path = project_dir.join("config.yml");
    let config_yaml = serde_yaml::to_string(&config)
        .map_err(|e| ConfigError::ParseError(format!("Failed to serialize config: {}", e)))?;

    fs::write(&config_path, config_yaml)
        .map_err(|e| ConfigError::IoError(format!("Failed to write config file: {}", e)))?;

    Ok(project_prefix)
}

/// Save a project configuration field using a closure to modify the config
pub fn save_project_config_field<F>(project_name: Option<&str>, update_fn: F) -> Result<(), ConfigError>
where
    F: FnOnce(&mut ProjectConfig),
{
    let project_name_owned = project_name.map(|s| s.to_string()).unwrap_or_else(|| {
        project::detect_project_name().unwrap_or_else(|| "default".to_string())
    });

    // Generate the 4-letter prefix for the project folder
    let project_prefix = crate::utils::generate_project_prefix(&project_name_owned);

    // Load existing project config or create new one
    let config_path = PathBuf::from(".tasks").join(&project_prefix).join("config.yml");
    let mut config = if config_path.exists() {
        let content = fs::read_to_string(&config_path)
            .map_err(|e| ConfigError::IoError(format!("Failed to read config: {}", e)))?;
        serde_yaml::from_str(&content)
            .map_err(|e| ConfigError::ParseError(format!("Failed to parse config: {}", e)))?
    } else {
        // Create project directory if it doesn't exist
        let project_dir = config_path.parent().unwrap();
        if !project_dir.exists() {
            fs::create_dir_all(project_dir)
                .map_err(|e| ConfigError::IoError(format!("Failed to create project directory: {}", e)))?;
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
pub fn save_global_config_field<F>(update_fn: F) -> Result<(), ConfigError>
where
    F: FnOnce(&mut GlobalConfig),
{
    // Load existing global config or create new one
    let config_path = PathBuf::from(".tasks/config.yml");
    let mut config = if config_path.exists() {
        let content = fs::read_to_string(&config_path)
            .map_err(|e| ConfigError::IoError(format!("Failed to read config: {}", e)))?;
        serde_yaml::from_str(&content)
            .map_err(|e| ConfigError::ParseError(format!("Failed to parse config: {}", e)))?
    } else {
        // Create .tasks directory if it doesn't exist
        let tasks_dir = config_path.parent().unwrap();
        if !tasks_dir.exists() {
            fs::create_dir_all(tasks_dir)
                .map_err(|e| ConfigError::IoError(format!("Failed to create .tasks directory: {}", e)))?;
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

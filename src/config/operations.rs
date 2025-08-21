use crate::config::types::*;
use crate::types::{Priority, TaskStatus, TaskType};
use crate::utils::project::generate_project_prefix;
use std::fs;
use std::path::Path;

/// Save global configuration to tasks_dir/config.yml
pub fn save_global_config(tasks_dir: &Path, config: &GlobalConfig) -> Result<(), ConfigError> {
    let config_path = crate::utils::paths::global_config_path(tasks_dir);

    // Ensure the tasks directory exists
    if let Some(parent) = config_path.parent() {
        fs::create_dir_all(parent).map_err(|e| {
            ConfigError::IoError(format!("Failed to create tasks directory: {}", e))
        })?;
    }

    // Serialize in canonical nested format
    let config_yaml = crate::config::normalization::to_canonical_global_yaml(config);

    fs::write(&config_path, config_yaml)
        .map_err(|e| ConfigError::IoError(format!("Failed to write global config: {}", e)))?;

    // Invalidate cache for this tasks_dir
    crate::config::resolution::invalidate_config_cache_for(Some(tasks_dir));
    crate::utils::identity::invalidate_identity_cache(Some(tasks_dir));

    Ok(())
}

/// Save updated project configuration to tasks_dir/{project}/config.yml
pub fn save_project_config(
    tasks_dir: &Path,
    project_prefix: &str,
    config: &ProjectConfig,
) -> Result<(), ConfigError> {
    let project_dir = crate::utils::paths::project_dir(tasks_dir, project_prefix);
    let config_path = crate::utils::paths::project_config_path(tasks_dir, project_prefix);

    // Ensure the project directory exists
    fs::create_dir_all(&project_dir)
        .map_err(|e| ConfigError::IoError(format!("Failed to create project directory: {}", e)))?;

    // Serialize in canonical nested format
    let config_yaml = crate::config::normalization::to_canonical_project_yaml(config);

    fs::write(&config_path, config_yaml)
        .map_err(|e| ConfigError::IoError(format!("Failed to write project config: {}", e)))?;

    // Invalidate cache for this tasks_dir
    crate::config::resolution::invalidate_config_cache_for(Some(tasks_dir));
    crate::utils::identity::invalidate_identity_cache(Some(tasks_dir));

    Ok(())
}

/// Update a specific field in global or project configuration
pub fn update_config_field(
    tasks_dir: &Path,
    field: &str,
    value: &str,
    project_prefix: Option<&str>,
) -> Result<(), ConfigError> {
    if let Some(project) = project_prefix {
        // Update project config
        let mut project_config =
            crate::config::persistence::load_project_config_from_dir(project, tasks_dir)
                .unwrap_or_else(|_| ProjectConfig::new(project.to_string()));

        apply_field_to_project_config(&mut project_config, field, value)?;
        save_project_config(tasks_dir, project, &project_config)?;
    } else {
        // Update global config
        let mut global_config =
            crate::config::persistence::load_global_config(Some(tasks_dir)).unwrap_or_default();
        apply_field_to_global_config(&mut global_config, field, value)?;
        save_global_config(tasks_dir, &global_config)?;
    }

    Ok(())
}

/// Apply a field update to GlobalConfig
fn apply_field_to_global_config(
    config: &mut GlobalConfig,
    field: &str,
    value: &str,
) -> Result<(), ConfigError> {
    match field {
        "server_port" => {
            config.server_port = value
                .parse::<u16>()
                .map_err(|_| ConfigError::ParseError(format!("Invalid port number: {}", value)))?;
        }
        "default_prefix" | "default_project" => {
            // Normalize to storage prefix, accepting either full project name or prefix
            config.default_prefix = generate_project_prefix(value);
        }
        "default_assignee" => {
            config.default_assignee = if value.is_empty() {
                None
            } else {
                Some(value.to_string())
            };
        }
        "default_priority" => {
            config.default_priority = value.parse::<Priority>().map_err(|_| {
                ConfigError::ParseError(format!(
                    "Invalid priority: {}. Valid values: Low, Medium, High, Critical",
                    value
                ))
            })?;
        }
        "default_status" => {
            let status = value.parse::<TaskStatus>().map_err(|_| {
                ConfigError::ParseError(format!(
                    "Invalid status: {}. Valid values: Todo, InProgress, Done, Cancelled",
                    value
                ))
            })?;
            config.default_status = Some(status);
        }
        "categories" | "tags" | "custom_fields" => {
            let values: Vec<String> = value.split(',').map(|s| s.trim().to_string()).collect();
            if field == "custom_fields" {
                for v in &values {
                    if let Some(canonical) = crate::utils::fields::is_reserved_field(v) {
                        return Err(ConfigError::ParseError(format!(
                            "Custom field '{}' collides with built-in field '{}'. Choose a different name.",
                            v, canonical
                        )));
                    }
                }
            }
            let field_config = StringConfigField { values };

            match field {
                "categories" => config.categories = field_config,
                "tags" => config.tags = field_config,
                "custom_fields" => config.custom_fields = field_config,
                _ => unreachable!(),
            }
        }
        "default_category" => {
            config.default_category = if value.is_empty() {
                None
            } else {
                Some(value.to_string())
            };
        }
        "default_tags" => {
            let tags: Vec<String> = value.split(',').map(|s| s.trim().to_string()).collect();
            config.default_tags = tags;
        }
        _ => {
            return Err(ConfigError::ParseError(format!(
                "Unknown global config field: {}",
                field
            )));
        }
    }
    Ok(())
}

/// Apply a field update to ProjectConfig
fn apply_field_to_project_config(
    config: &mut ProjectConfig,
    field: &str,
    value: &str,
) -> Result<(), ConfigError> {
    match field {
        "project_name" => {
            config.project_name = value.to_string();
        }
        "default_assignee" => {
            config.default_assignee = if value.is_empty() {
                None
            } else {
                Some(value.to_string())
            };
        }
        "default_priority" => {
            let priority = value.parse::<Priority>().map_err(|_| {
                ConfigError::ParseError(format!(
                    "Invalid priority: {}. Valid values: Low, Medium, High, Critical",
                    value
                ))
            })?;
            config.default_priority = Some(priority);
        }
        "default_status" => {
            let status = value.parse::<TaskStatus>().map_err(|_| {
                ConfigError::ParseError(format!(
                    "Invalid status: {}. Valid values: Todo, InProgress, Done, Cancelled",
                    value
                ))
            })?;
            config.default_status = Some(status);
        }
        "default_category" => {
            config.default_category = if value.is_empty() {
                None
            } else {
                Some(value.to_string())
            };
        }
        "default_tags" => {
            let values: Vec<String> = value.split(',').map(|s| s.trim().to_string()).collect();
            config.default_tags = Some(values);
        }
        "issue_states" => {
            let states: Result<Vec<TaskStatus>, _> =
                value.split(',').map(|s| s.trim().parse()).collect();
            let states = states.map_err(|_| {
                ConfigError::ParseError(format!("Invalid task state in: {}", value))
            })?;
            config.issue_states = Some(ConfigurableField { values: states });
        }
        "issue_types" => {
            let types: Result<Vec<TaskType>, _> =
                value.split(',').map(|s| s.trim().parse()).collect();
            let types = types
                .map_err(|_| ConfigError::ParseError(format!("Invalid task type in: {}", value)))?;
            config.issue_types = Some(ConfigurableField { values: types });
        }
        "issue_priorities" => {
            let priorities: Result<Vec<Priority>, _> =
                value.split(',').map(|s| s.trim().parse()).collect();
            let priorities = priorities
                .map_err(|_| ConfigError::ParseError(format!("Invalid priority in: {}", value)))?;
            config.issue_priorities = Some(ConfigurableField { values: priorities });
        }
        "categories" | "tags" | "custom_fields" => {
            let values: Vec<String> = value.split(',').map(|s| s.trim().to_string()).collect();
            if field == "custom_fields" {
                for v in &values {
                    if let Some(canonical) = crate::utils::fields::is_reserved_field(v) {
                        return Err(ConfigError::ParseError(format!(
                            "Custom field '{}' collides with built-in field '{}'. Choose a different name.",
                            v, canonical
                        )));
                    }
                }
            }
            let field_config = StringConfigField { values };

            match field {
                "categories" => config.categories = Some(field_config),
                "tags" => config.tags = Some(field_config),
                "custom_fields" => config.custom_fields = Some(field_config),
                _ => unreachable!(),
            }
        }
        _ => {
            return Err(ConfigError::ParseError(format!(
                "Unknown project config field: {}",
                field
            )));
        }
    }
    Ok(())
}

/// Validate that a field name is valid for the given scope
pub fn validate_field_name(field: &str, is_global: bool) -> Result<(), ConfigError> {
    let valid_global_fields = vec![
        "server_port",
        "default_prefix",
        "default_project",
        "default_assignee",
        "default_category",
        "default_tags",
        "default_priority",
        "default_status",
        "categories",
        "tags",
        "custom_fields",
    ];
    let valid_project_fields = vec![
        "project_name",
        "default_assignee",
        "default_category",
        "default_tags",
        "default_priority",
        "default_status",
        "issue_states",
        "issue_types",
        "issue_priorities",
        "categories",
        "tags",
        "custom_fields",
    ];

    let valid_fields = if is_global {
        &valid_global_fields
    } else {
        &valid_project_fields
    };

    if !valid_fields.contains(&field) {
        let scope = if is_global { "global" } else { "project" };
        return Err(ConfigError::ParseError(format!(
            "Invalid {} config field: '{}'. Valid fields: {}",
            scope,
            field,
            valid_fields.join(", ")
        )));
    }

    Ok(())
}

/// Validate that a field value is valid for the given field
pub fn validate_field_value(field: &str, value: &str) -> Result<(), ConfigError> {
    match field {
        "server_port" => {
            let port = value
                .parse::<u16>()
                .map_err(|_| ConfigError::ParseError(format!("Invalid port number: {}", value)))?;
            if port < 1024 {
                return Err(ConfigError::ParseError(
                    "Port number must be 1024 or higher".to_string(),
                ));
            }
        }
        "default_priority" => {
            value.parse::<Priority>().map_err(|_| {
                ConfigError::ParseError(format!(
                    "Invalid priority: {}. Valid values: Low, Medium, High, Critical",
                    value
                ))
            })?;
        }
        "default_status" => {
            value.parse::<TaskStatus>().map_err(|_| {
                ConfigError::ParseError(format!(
                    "Invalid status: {}. Valid values: Todo, InProgress, Done, Cancelled",
                    value
                ))
            })?;
        }
        "default_prefix" | "default_project" => {
            if !value
                .chars()
                .all(|c| c.is_alphanumeric() || c == '-' || c == '_')
            {
                return Err(ConfigError::ParseError(
                    "Project prefix can only contain alphanumeric characters, hyphens, and underscores".to_string()
                ));
            }
            if value.len() > 20 {
                return Err(ConfigError::ParseError(
                    "Project prefix cannot be longer than 20 characters".to_string(),
                ));
            }
        }
        "default_assignee" | "project_name" => {
            // Basic validation - not empty and reasonable length
            if value.len() > 100 {
                return Err(ConfigError::ParseError(format!(
                    "{} cannot be longer than 100 characters",
                    field
                )));
            }
        }
        "default_category" => {
            if value.len() > 100 {
                return Err(ConfigError::ParseError(
                    "default_category cannot be longer than 100 characters".to_string(),
                ));
            }
        }
        "default_tags" | "categories" | "tags" | "custom_fields" => {
            // Basic validation: allow comma-separated values, each up to 50 chars
            for part in value.split(',') {
                let p = part.trim();
                if p.len() > 50 {
                    return Err(ConfigError::ParseError(format!(
                        "Value '{}' is too long (max 50 chars)",
                        p
                    )));
                }
            }
        }
        _ => {} // Other fields don't need specific validation
    }
    Ok(())
}

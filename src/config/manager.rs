use crate::config::types::*;
use crate::types::{Priority, TaskStatus, TaskType};
use std::env;
use std::fs;
use std::path::{Path, PathBuf};

pub struct ConfigManager {
    resolved_config: ResolvedConfig,
}

impl ConfigManager {
    #[allow(dead_code)]
    pub fn new() -> Result<Self, ConfigError> {
        let resolved_config = Self::load_and_merge_configs(None)?;
        Ok(Self { resolved_config })
    }

    /// Create a ConfigManager from an existing ResolvedConfig (for testing)
    #[cfg(test)]
    pub fn from_resolved_config(resolved_config: ResolvedConfig) -> Self {
        Self { resolved_config }
    }

    /// Get a reference to the resolved configuration
    pub fn get_resolved_config(&self) -> &ResolvedConfig {
        &self.resolved_config
    }

    #[allow(dead_code)]
    pub fn new_with_tasks_dir(tasks_dir: &Path) -> Result<ResolvedConfig, ConfigError> {
        Self::new_with_tasks_dir_and_home_override_internal(tasks_dir, None, false)
    }

    /// Create a ConfigManager instance that ensures global config exists (for write operations)
    pub fn new_manager_with_tasks_dir_ensure_config(tasks_dir: &Path) -> Result<Self, ConfigError> {
        let resolved_config = Self::new_with_tasks_dir_and_home_override_internal(tasks_dir, None, true)?;
        Ok(Self { resolved_config })
    }

    /// Create a ConfigManager instance for read-only operations (does not create config files)
    pub fn new_manager_with_tasks_dir_readonly(tasks_dir: &Path) -> Result<Self, ConfigError> {
        let resolved_config = Self::new_with_tasks_dir_and_home_override_internal(tasks_dir, None, false)?;
        Ok(Self { resolved_config })
    }

    /// Ensure default_prefix is set in global config, auto-detecting if necessary
    pub fn ensure_default_prefix(&mut self, tasks_dir: &Path) -> Result<String, ConfigError> {
        // Check if default_prefix is already set
        if !self.resolved_config.default_prefix.is_empty() {
            return Ok(self.resolved_config.default_prefix.clone());
        }

        // Default prefix is empty, need to auto-detect and update
        let detected_prefix = if let Some(detected) = Self::auto_detect_prefix(&tasks_dir.to_path_buf()) {
            detected
        } else {
            // No existing projects, generate from current directory
            if let Some(project_name) = crate::project::get_project_name() {
                crate::utils::generate_project_prefix(&project_name)
            } else {
                "DEFAULT".to_string()
            }
        };

        // Update the global config with the detected prefix
        let config_path = tasks_dir.join("config.yml");
        if config_path.exists() {
            // Load current config
            let content = fs::read_to_string(&config_path).map_err(|e| {
                ConfigError::IoError(format!("Failed to read global config: {}", e))
            })?;
            let mut global_config: GlobalConfig = serde_yaml::from_str(&content).map_err(|e| {
                ConfigError::ParseError(format!("Failed to parse global config: {}", e))
            })?;

            // Update the default_prefix
            global_config.default_prefix = detected_prefix.clone();

            // Save updated config
            let updated_yaml = serde_yaml::to_string(&global_config).map_err(|e| {
                ConfigError::ParseError(format!("Failed to serialize updated config: {}", e))
            })?;
            fs::write(&config_path, updated_yaml).map_err(|e| {
                ConfigError::IoError(format!("Failed to write updated global config: {}", e))
            })?;

            // Update our resolved config too
            self.resolved_config.default_prefix = detected_prefix.clone();
        }

        Ok(detected_prefix)
    }

    /// Save updated global configuration to tasks_dir/config.yml
    pub fn save_global_config(tasks_dir: &Path, config: &GlobalConfig) -> Result<(), ConfigError> {
        let config_path = tasks_dir.join("config.yml");
        
        // Ensure the tasks directory exists
        if let Some(parent) = config_path.parent() {
            fs::create_dir_all(parent).map_err(|e| {
                ConfigError::IoError(format!("Failed to create tasks directory: {}", e))
            })?;
        }

        let config_yaml = serde_yaml::to_string(config).map_err(|e| {
            ConfigError::ParseError(format!("Failed to serialize global config: {}", e))
        })?;

        fs::write(&config_path, config_yaml).map_err(|e| {
            ConfigError::IoError(format!("Failed to write global config: {}", e))
        })?;

        Ok(())
    }

    /// Save updated project configuration to tasks_dir/{project}/config.yml
    pub fn save_project_config(tasks_dir: &Path, project_prefix: &str, config: &ProjectConfig) -> Result<(), ConfigError> {
        let project_dir = tasks_dir.join(project_prefix);
        let config_path = project_dir.join("config.yml");
        
        // Ensure the project directory exists
        fs::create_dir_all(&project_dir).map_err(|e| {
            ConfigError::IoError(format!("Failed to create project directory: {}", e))
        })?;

        let config_yaml = serde_yaml::to_string(config).map_err(|e| {
            ConfigError::ParseError(format!("Failed to serialize project config: {}", e))
        })?;

        fs::write(&config_path, config_yaml).map_err(|e| {
            ConfigError::IoError(format!("Failed to write project config: {}", e))
        })?;

        Ok(())
    }

    /// Update a specific field in global or project configuration
    pub fn update_config_field(
        tasks_dir: &Path, 
        field: &str, 
        value: &str, 
        project_prefix: Option<&str>
    ) -> Result<(), ConfigError> {
        if let Some(project) = project_prefix {
            // Update project config
            let mut project_config = Self::load_project_config_from_dir(project, tasks_dir).unwrap_or_else(|_| {
                ProjectConfig::new(project.to_string())
            });
            
            Self::apply_field_to_project_config(&mut project_config, field, value)?;
            Self::save_project_config(tasks_dir, project, &project_config)?;
        } else {
            // Update global config
            let mut global_config = Self::load_global_config(Some(&tasks_dir.to_path_buf())).unwrap_or_default();
            Self::apply_field_to_global_config(&mut global_config, field, value)?;
            Self::save_global_config(tasks_dir, &global_config)?;
        }
        
        Ok(())
    }

    /// Apply a field update to GlobalConfig
    fn apply_field_to_global_config(config: &mut GlobalConfig, field: &str, value: &str) -> Result<(), ConfigError> {
        match field {
            "server_port" => {
                config.server_port = value.parse::<u16>().map_err(|_| {
                    ConfigError::ParseError(format!("Invalid port number: {}", value))
                })?;
            }
            "default_prefix" | "default_project" => {
                config.default_prefix = value.to_string();
            }
            "default_assignee" => {
                config.default_assignee = if value.is_empty() { None } else { Some(value.to_string()) };
            }
            "default_priority" => {
                config.default_priority = value.parse::<Priority>().map_err(|_| {
                    ConfigError::ParseError(format!("Invalid priority: {}. Valid values: Low, Medium, High, Critical", value))
                })?;
            }
            _ => {
                return Err(ConfigError::ParseError(format!("Unknown global config field: {}", field)));
            }
        }
        Ok(())
    }

    /// Apply a field update to ProjectConfig
    fn apply_field_to_project_config(config: &mut ProjectConfig, field: &str, value: &str) -> Result<(), ConfigError> {
        match field {
            "project_name" => {
                config.project_name = value.to_string();
            }
            "default_assignee" => {
                config.default_assignee = if value.is_empty() { None } else { Some(value.to_string()) };
            }
            "default_priority" => {
                let priority = value.parse::<Priority>().map_err(|_| {
                    ConfigError::ParseError(format!("Invalid priority: {}. Valid values: Low, Medium, High, Critical", value))
                })?;
                config.default_priority = Some(priority);
            }
            "issue_states" => {
                let states: Result<Vec<TaskStatus>, _> = value.split(',')
                    .map(|s| s.trim().parse())
                    .collect();
                let states = states.map_err(|_| {
                    ConfigError::ParseError(format!("Invalid task state in: {}", value))
                })?;
                config.issue_states = Some(ConfigurableField { values: states });
            }
            "issue_types" => {
                let types: Result<Vec<TaskType>, _> = value.split(',')
                    .map(|s| s.trim().parse())
                    .collect();
                let types = types.map_err(|_| {
                    ConfigError::ParseError(format!("Invalid task type in: {}", value))
                })?;
                config.issue_types = Some(ConfigurableField { values: types });
            }
            "issue_priorities" => {
                let priorities: Result<Vec<Priority>, _> = value.split(',')
                    .map(|s| s.trim().parse())
                    .collect();
                let priorities = priorities.map_err(|_| {
                    ConfigError::ParseError(format!("Invalid priority in: {}", value))
                })?;
                config.issue_priorities = Some(ConfigurableField { values: priorities });
            }
            "categories" | "tags" | "custom_fields" => {
                let values: Vec<String> = value.split(',')
                    .map(|s| s.trim().to_string())
                    .collect();
                let field_config = StringConfigField { values };
                
                match field {
                    "categories" => config.categories = Some(field_config),
                    "tags" => config.tags = Some(field_config),
                    "custom_fields" => config.custom_fields = Some(field_config),
                    _ => unreachable!(),
                }
            }
            _ => {
                return Err(ConfigError::ParseError(format!("Unknown project config field: {}", field)));
            }
        }
        Ok(())
    }

    /// Validate that a field name is valid for the given scope
    pub fn validate_field_name(field: &str, is_global: bool) -> Result<(), ConfigError> {
        let valid_global_fields = vec![
            "server_port", "default_prefix", "default_project", "default_assignee", "default_priority"
        ];
        let valid_project_fields = vec![
            "project_name", "default_assignee", "default_priority",
            "issue_states", "issue_types", "issue_priorities", "categories", "tags", "custom_fields"
        ];

        let valid_fields = if is_global { &valid_global_fields } else { &valid_project_fields };
        
        if !valid_fields.contains(&field) {
            let scope = if is_global { "global" } else { "project" };
            return Err(ConfigError::ParseError(format!(
                "Invalid {} config field: '{}'. Valid fields: {}", 
                scope, field, valid_fields.join(", ")
            )));
        }
        
        Ok(())
    }

    /// Validate that a field value is valid for the given field
    pub fn validate_field_value(field: &str, value: &str) -> Result<(), ConfigError> {
        match field {
            "server_port" => {
                let port = value.parse::<u16>().map_err(|_| {
                    ConfigError::ParseError(format!("Invalid port number: {}", value))
                })?;
                if port < 1024 {
                    return Err(ConfigError::ParseError(
                        "Port number must be 1024 or higher".to_string()
                    ));
                }
            }
            "default_priority" => {
                value.parse::<Priority>().map_err(|_| {
                    ConfigError::ParseError(format!("Invalid priority: {}. Valid values: Low, Medium, High, Critical", value))
                })?;
            }
            "default_prefix" | "default_project" => {
                if !value.chars().all(|c| c.is_alphanumeric() || c == '-' || c == '_') {
                    return Err(ConfigError::ParseError(
                        "Project prefix can only contain alphanumeric characters, hyphens, and underscores".to_string()
                    ));
                }
                if value.len() > 20 {
                    return Err(ConfigError::ParseError(
                        "Project prefix cannot be longer than 20 characters".to_string()
                    ));
                }
            }
            "default_assignee" | "project_name" => {
                // Basic validation - not empty and reasonable length
                if value.len() > 100 {
                    return Err(ConfigError::ParseError(
                        format!("{} cannot be longer than 100 characters", field)
                    ));
                }
            }
            _ => {} // Other fields don't need specific validation
        }
        Ok(())
    }

    fn new_with_tasks_dir_and_home_override_internal(
        tasks_dir: &Path,
        home_config_override: Option<&PathBuf>,
        ensure_config_exists: bool,
    ) -> Result<ResolvedConfig, ConfigError> {
        let mut config = GlobalConfig::default();

        // Convert Path to PathBuf for the existing API
        let tasks_dir_buf = tasks_dir.to_path_buf();

        // Only ensure global config exists if explicitly requested (for write operations)
        if ensure_config_exists {
            Self::ensure_global_config_exists(Some(&tasks_dir_buf))?;
        }

        // 4. Global config (.tasks/config.yml or custom dir) - lowest priority (after defaults)
        if let Ok(global_config) = Self::load_global_config(Some(&tasks_dir_buf)) {
            Self::merge_global_config(&mut config, global_config);
        }

        // 3. Project config (.tasks/{project}/config.yml) - will be handled per-project
        // For now, we'll use global as base

        // 2. Home config (~/.lotar) - higher priority
        if let Ok(home_config) = Self::load_home_config_with_override(home_config_override) {
            Self::merge_global_config(&mut config, home_config);
        }

        // 1. Environment variables (highest priority)
        Self::apply_env_overrides(&mut config);

        Ok(ResolvedConfig::from_global(config))
    }

    /// Ensure global config exists and create if missing
    fn ensure_global_config_exists(tasks_dir: Option<&PathBuf>) -> Result<(), ConfigError> {
        let config_path = match tasks_dir {
            Some(dir) => dir.join("config.yml"),
            None => PathBuf::from(".tasks/config.yml"),
        };

        if !config_path.exists() {
            Self::create_default_global_config(tasks_dir)?;
        }

        Ok(())
    }

    /// Load and merge all configurations with proper priority order
    #[allow(dead_code)]
    fn load_and_merge_configs(tasks_dir: Option<&PathBuf>) -> Result<ResolvedConfig, ConfigError> {
        // Start with built-in defaults
        let mut config = GlobalConfig::default();

        // Don't ensure global config exists for read-only operations
        // Only create it when actually needed for task operations

        // 4. Global config (.tasks/config.yml or custom dir) - lowest priority (after defaults)
        if let Ok(global_config) = Self::load_global_config(tasks_dir) {
            Self::merge_global_config(&mut config, global_config);
        }

        // 3. Project config (.tasks/{project}/config.yml) - will be handled per-project
        // For now, we'll use global as base

        // 2. Home config (~/.lotar) - higher priority
        if let Ok(home_config) = Self::load_home_config() {
            Self::merge_global_config(&mut config, home_config);
        }

        // 1. Environment variables (highest priority)
        Self::apply_env_overrides(&mut config);

        Ok(ResolvedConfig::from_global(config))
    }

    /// Improved merging that only overrides non-default values
    fn merge_global_config(base: &mut GlobalConfig, override_config: GlobalConfig) {
        // Only override fields that are different from defaults
        let defaults = GlobalConfig::default();

        if override_config.server_port != defaults.server_port {
            base.server_port = override_config.server_port;
        }
        if override_config.default_prefix != defaults.default_prefix {
            base.default_prefix = override_config.default_prefix;
        }

        // For configurable fields, we do full replacement if they differ
        if override_config.issue_states.values != defaults.issue_states.values {
            base.issue_states = override_config.issue_states;
        }
        if override_config.issue_types.values != defaults.issue_types.values {
            base.issue_types = override_config.issue_types;
        }
        if override_config.issue_priorities.values != defaults.issue_priorities.values {
            base.issue_priorities = override_config.issue_priorities;
        }
        if override_config.categories.values != defaults.categories.values {
            base.categories = override_config.categories;
        }
        if override_config.tags.values != defaults.tags.values {
            base.tags = override_config.tags;
        }

        // Optional fields
        if override_config.default_assignee.is_some() {
            base.default_assignee = override_config.default_assignee;
        }
        if override_config.default_priority != defaults.default_priority {
            base.default_priority = override_config.default_priority;
        }
    }

    fn load_global_config(tasks_dir: Option<&PathBuf>) -> Result<GlobalConfig, ConfigError> {
        let path = match tasks_dir {
            Some(dir) => dir.join("config.yml"),
            None => PathBuf::from(".tasks/config.yml"),
        };
        Self::load_config_file(&path)
    }

    #[allow(dead_code)]
    pub fn load_home_config() -> Result<GlobalConfig, ConfigError> {
        let home_dir =
            dirs::home_dir().ok_or(ConfigError::IoError("Home directory not found".to_string()))?;
        let path = home_dir.join(".lotar");
        Self::load_config_file(&path)
    }

    pub fn load_home_config_with_override(
        home_config_path: Option<&PathBuf>,
    ) -> Result<GlobalConfig, ConfigError> {
        let path = match home_config_path {
            Some(override_path) => override_path.clone(),
            None => {
                let home_dir = dirs::home_dir()
                    .ok_or(ConfigError::IoError("Home directory not found".to_string()))?;
                home_dir.join(".lotar")
            }
        };
        Self::load_config_file(&path)
    }

    fn load_project_config(project_name: &str) -> Result<ProjectConfig, ConfigError> {
        Self::load_project_config_from_dir(project_name, &PathBuf::from(".tasks"))
    }

    fn load_project_config_from_dir(project_name: &str, tasks_dir: &Path) -> Result<ProjectConfig, ConfigError> {
        let path = tasks_dir
            .join(project_name)
            .join("config.yml");
        if !path.exists() {
            return Ok(ProjectConfig::new(project_name.to_string()));
        }

        let content = fs::read_to_string(&path)
            .map_err(|e| ConfigError::IoError(format!("Failed to read project config: {}", e)))?;

        serde_yaml::from_str(&content)
            .map_err(|e| ConfigError::ParseError(format!("Failed to parse project config: {}", e)))
    }

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

    fn apply_env_overrides(config: &mut GlobalConfig) {
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

    fn create_default_global_config(tasks_dir: Option<&PathBuf>) -> Result<(), ConfigError> {
        let config_path = match tasks_dir {
            Some(dir) => dir.join("config.yml"),
            None => PathBuf::from(".tasks/config.yml"),
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
            if let Some(detected_prefix) = Self::auto_detect_prefix(tasks_dir_path) {
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
    fn auto_detect_prefix(tasks_dir: &PathBuf) -> Option<String> {
        let mut project_prefixes = Vec::new();
        
        // Look for project directories that exist and have config files
        if let Ok(entries) = fs::read_dir(tasks_dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_dir() && !path.file_name().unwrap_or_default().to_string_lossy().starts_with('.') {
                    let config_path = path.join("config.yml");
                    if config_path.exists() {
                        // Found a project directory with config - add its prefix
                        if let Some(prefix) = path.file_name()
                            .and_then(|name| name.to_str())
                            .map(|s| s.to_string()) {
                            project_prefixes.push(prefix);
                        }
                    }
                }
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

    pub fn get_project_config(&self, project_name: &str) -> Result<ResolvedConfig, ConfigError> {
        // Load project-specific config and merge with global
        let project_config = Self::load_project_config(project_name)?;
        let mut resolved = self.resolved_config.clone();

        // Apply project-specific overrides
        if let Some(states) = project_config.issue_states {
            resolved.issue_states = states;
        }
        if let Some(types) = project_config.issue_types {
            resolved.issue_types = types;
        }
        if let Some(priorities) = project_config.issue_priorities {
            resolved.issue_priorities = priorities;
        }
        if let Some(categories) = project_config.categories {
            resolved.categories = categories;
        }
        if let Some(tags) = project_config.tags {
            resolved.tags = tags;
        }
        if let Some(assignee) = project_config.default_assignee {
            resolved.default_assignee = Some(assignee);
        }
        if let Some(priority) = project_config.default_priority {
            resolved.default_priority = priority;
        }
        if let Some(custom_fields) = project_config.custom_fields {
            resolved.custom_fields = custom_fields;
        }

        Ok(resolved)
    }
}

impl ResolvedConfig {
    pub fn from_global(global: GlobalConfig) -> Self {
        Self {
            server_port: global.server_port,
            default_prefix: global.default_prefix,
            issue_states: global.issue_states,
            issue_types: global.issue_types,
            issue_priorities: global.issue_priorities,
            categories: global.categories,
            tags: global.tags,
            default_assignee: global.default_assignee,
            default_priority: global.default_priority,
            custom_fields: global.custom_fields,
        }
    }
}

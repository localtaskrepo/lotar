use crate::config::templates;
use crate::config::types::*;
use std::env;
use std::fs;
use std::path::{Path, PathBuf};

pub struct ConfigManager {
    resolved_config: ResolvedConfig,
}

impl ConfigManager {
    pub fn new() -> Result<Self, ConfigError> {
        let resolved_config = Self::load_and_merge_configs(None)?;
        Ok(Self { resolved_config })
    }

    /// Get a reference to the resolved configuration
    pub fn get_resolved_config(&self) -> &ResolvedConfig {
        &self.resolved_config
    }

    pub fn new_with_tasks_dir(tasks_dir: &Path) -> Result<ResolvedConfig, ConfigError> {
        Self::new_with_tasks_dir_and_home_override(tasks_dir, None)
    }

    pub fn new_with_tasks_dir_and_home_override(
        tasks_dir: &Path,
        home_config_override: Option<&PathBuf>,
    ) -> Result<ResolvedConfig, ConfigError> {
        let mut config = GlobalConfig::default();

        // Convert Path to PathBuf for the existing API
        let tasks_dir_buf = tasks_dir.to_path_buf();

        // Ensure global config exists
        Self::ensure_global_config_exists(Some(&tasks_dir_buf))?;

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
    fn load_and_merge_configs(tasks_dir: Option<&PathBuf>) -> Result<ResolvedConfig, ConfigError> {
        // Start with built-in defaults
        let mut config = GlobalConfig::default();

        // Ensure global config exists
        Self::ensure_global_config_exists(tasks_dir)?;

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
        if override_config.task_file_extension != defaults.task_file_extension {
            base.task_file_extension = override_config.task_file_extension;
        }
        if override_config.tasks_folder != defaults.tasks_folder {
            base.tasks_folder = override_config.tasks_folder;
        }
        if override_config.default_project != defaults.default_project {
            base.default_project = override_config.default_project;
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
        let path = PathBuf::from(".tasks")
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
            config.default_project = project;
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

        // Write default global config
        let default_config = GlobalConfig::default();
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

        Ok(resolved)
    }

    pub fn get_global_config(&self) -> &ResolvedConfig {
        &self.resolved_config
    }

    // Template management functions

    /// Load a template by name, with fallback to hardcoded templates
    pub fn load_template(template_name: &str) -> Result<ProjectTemplate, ConfigError> {
        let template_path =
            PathBuf::from("src/config/templates").join(format!("{}.yml", template_name));

        // Try to load from file first
        if template_path.exists() {
            let content = fs::read_to_string(&template_path).map_err(|e| {
                ConfigError::IoError(format!("Failed to read template file: {}", e))
            })?;

            let template: ProjectTemplate = serde_yaml::from_str(&content)
                .map_err(|e| ConfigError::ParseError(format!("Failed to parse template: {}", e)))?;

            return Ok(template);
        }

        // Fallback to hardcoded templates when files are not available (e.g., in tests)
        let template = match template_name {
            "default" => templates::create_default_template(),
            "simple" => templates::create_simple_template(),
            "agile" => templates::create_agile_template(),
            "kanban" => templates::create_kanban_template(),
            _ => {
                return Err(ConfigError::FileNotFound(format!(
                    "Template '{}' not found at {:?}",
                    template_name, template_path
                )));
            }
        };

        Ok(template)
    }

    /// List all available templates, both from files and hardcoded fallbacks
    pub fn list_available_templates() -> Result<Vec<String>, ConfigError> {
        let templates_dir = PathBuf::from("src/config/templates");

        if !templates_dir.exists() {
            return Ok(vec![
                "default".to_string(),
                "simple".to_string(),
                "agile".to_string(),
                "kanban".to_string(),
            ]);
        }

        let mut templates = Vec::new();
        let entries = fs::read_dir(&templates_dir).map_err(|e| {
            ConfigError::IoError(format!("Failed to read templates directory: {}", e))
        })?;

        for entry in entries {
            let entry = entry.map_err(|e| {
                ConfigError::IoError(format!("Failed to read directory entry: {}", e))
            })?;
            let path = entry.path();

            if path.extension().and_then(|s| s.to_str()) == Some("yml") {
                if let Some(stem) = path.file_stem().and_then(|s| s.to_str()) {
                    templates.push(stem.to_string());
                }
            }
        }

        if templates.is_empty() {
            templates = vec![
                "default".to_string(),
                "simple".to_string(),
                "agile".to_string(),
                "kanban".to_string(),
            ];
        }

        templates.sort();
        Ok(templates)
    }

    /// Apply a template to create a project config with the given project name
    pub fn apply_template_to_project(
        template: &ProjectTemplate,
        project_name: &str,
    ) -> ProjectConfig {
        let mut config = template.config.clone();
        config.project_name = project_name.to_string();
        config
    }
}

impl ResolvedConfig {
    pub fn from_global(global: GlobalConfig) -> Self {
        Self {
            server_port: global.server_port,
            task_file_extension: global.task_file_extension,
            default_project: global.default_project,
            issue_states: global.issue_states,
            issue_types: global.issue_types,
            issue_priorities: global.issue_priorities,
            categories: global.categories,
            tags: global.tags,
            default_assignee: global.default_assignee,
            default_priority: global.default_priority,
        }
    }
}

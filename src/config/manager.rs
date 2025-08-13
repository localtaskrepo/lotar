use crate::config::types::*;
use std::fs;
use std::path::{Path, PathBuf};

pub struct ConfigManager {
    resolved_config: ResolvedConfig,
    tasks_dir: PathBuf,
}

impl ConfigManager {
    #[allow(dead_code)]
    pub fn new() -> Result<Self, ConfigError> {
        let resolved_config = crate::config::resolution::load_and_merge_configs(None)?;
        let default_root = crate::utils::paths::tasks_root_from(Path::new("."));
        Ok(Self {
            resolved_config,
            tasks_dir: default_root,
        })
    }

    /// Create a ConfigManager from an existing ResolvedConfig (for testing)
    #[cfg(test)]
    pub fn from_resolved_config(resolved_config: ResolvedConfig) -> Self {
        let default_root = crate::utils::paths::tasks_root_from(Path::new("."));
        Self {
            resolved_config,
            tasks_dir: default_root,
        }
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
        let resolved_config =
            Self::new_with_tasks_dir_and_home_override_internal(tasks_dir, None, true)?;
        Ok(Self {
            resolved_config,
            tasks_dir: tasks_dir.to_path_buf(),
        })
    }

    /// Create a ConfigManager instance for read-only operations (does not create config files)
    pub fn new_manager_with_tasks_dir_readonly(tasks_dir: &Path) -> Result<Self, ConfigError> {
        let resolved_config =
            Self::new_with_tasks_dir_and_home_override_internal(tasks_dir, None, false)?;
        Ok(Self {
            resolved_config,
            tasks_dir: tasks_dir.to_path_buf(),
        })
    }

    /// Ensure default_prefix is set in global config, auto-detecting if necessary
    pub fn ensure_default_prefix(&mut self, tasks_dir: &Path) -> Result<String, ConfigError> {
        // Check if default_prefix is already set
        if !self.resolved_config.default_prefix.is_empty() {
            return Ok(self.resolved_config.default_prefix.clone());
        }

        // Default prefix is empty, need to auto-detect and update
        let detected_prefix =
            if let Some(detected) = crate::config::persistence::auto_detect_prefix(tasks_dir) {
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
        let config_path = crate::utils::paths::global_config_path(tasks_dir);
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

    /// Get project-specific configuration by merging with global config
    pub fn get_project_config(&self, project_name: &str) -> Result<ResolvedConfig, ConfigError> {
        crate::config::resolution::get_project_config(
            &self.resolved_config,
            project_name,
            &self.tasks_dir,
        )
    }

    /// Delegate to operations module
    pub fn save_global_config(tasks_dir: &Path, config: &GlobalConfig) -> Result<(), ConfigError> {
        crate::config::operations::save_global_config(tasks_dir, config)
    }

    /// Delegate to operations module
    pub fn save_project_config(
        tasks_dir: &Path,
        project_prefix: &str,
        config: &ProjectConfig,
    ) -> Result<(), ConfigError> {
        crate::config::operations::save_project_config(tasks_dir, project_prefix, config)
    }

    /// Delegate to operations module
    pub fn update_config_field(
        tasks_dir: &Path,
        field: &str,
        value: &str,
        project_prefix: Option<&str>,
    ) -> Result<(), ConfigError> {
        crate::config::operations::update_config_field(tasks_dir, field, value, project_prefix)
    }

    /// Delegate to operations module
    pub fn validate_field_name(field: &str, is_global: bool) -> Result<(), ConfigError> {
        crate::config::operations::validate_field_name(field, is_global)
    }

    /// Delegate to operations module
    pub fn validate_field_value(field: &str, value: &str) -> Result<(), ConfigError> {
        crate::config::operations::validate_field_value(field, value)
    }

    /// Delegate to persistence module
    pub fn load_home_config() -> Result<GlobalConfig, ConfigError> {
        crate::config::persistence::load_home_config()
    }

    /// Delegate to persistence module
    pub fn load_home_config_with_override(
        home_config_path: Option<&Path>,
    ) -> Result<GlobalConfig, ConfigError> {
        crate::config::persistence::load_home_config_with_override(home_config_path)
    }

    /// Internal constructor with full configuration options
    fn new_with_tasks_dir_and_home_override_internal(
        tasks_dir: &Path,
        home_config_override: Option<&Path>,
        ensure_config_exists: bool,
    ) -> Result<ResolvedConfig, ConfigError> {
        let mut config = GlobalConfig::default();

        // Convert Path to PathBuf for the existing API
        let tasks_dir_buf = tasks_dir.to_path_buf();

        // Only ensure global config exists if explicitly requested (for write operations)
        if ensure_config_exists {
            crate::config::persistence::ensure_global_config_exists(Some(tasks_dir_buf.as_path()))?;
        }

        // 4. Global config (.tasks/config.yml or custom dir) - lowest priority (after defaults)
        if let Ok(global_config) =
            crate::config::persistence::load_global_config(Some(tasks_dir_buf.as_path()))
        {
            crate::config::resolution::merge_global_config(&mut config, global_config);
        }

        // 3. Project config (.tasks/{project}/config.yml) - will be handled per-project
        // For now, we'll use global as base

        // 2. Home config (~/.lotar) - higher priority
        if let Ok(home_config) =
            crate::config::persistence::load_home_config_with_override(home_config_override)
        {
            crate::config::resolution::merge_global_config(&mut config, home_config);
        }

        // 1. Environment variables (highest priority)
        crate::config::persistence::apply_env_overrides(&mut config);

        Ok(ResolvedConfig::from_global(config))
    }
}

/// Public test support to construct a ConfigManager from a ResolvedConfig in integration tests
#[doc(hidden)]
pub mod test_support {
    use super::*;

    pub fn from_resolved_config(resolved_config: ResolvedConfig) -> ConfigManager {
        let default_root = crate::utils::paths::tasks_root_from(Path::new("."));
        ConfigManager {
            resolved_config,
            tasks_dir: default_root,
        }
    }
}

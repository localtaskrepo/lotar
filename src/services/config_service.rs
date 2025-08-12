use crate::config::manager::ConfigManager;
use crate::errors::{LoTaRError, LoTaRResult};
use crate::workspace::TasksDirectoryResolver;

pub struct ConfigService;

impl ConfigService {
    pub fn show(
        resolver: &TasksDirectoryResolver,
        project_prefix: Option<&str>,
    ) -> LoTaRResult<serde_json::Value> {
        let mgr = ConfigManager::new_manager_with_tasks_dir_readonly(&resolver.path)
            .map_err(|e| LoTaRError::ValidationError(format!("Failed to load config: {}", e)))?;

        if let Some(prefix) = project_prefix {
            let project_cfg = mgr.get_project_config(prefix).map_err(|e| {
                LoTaRError::ValidationError(format!(
                    "Failed to load project config for '{}': {}",
                    prefix, e
                ))
            })?;
            Ok(serde_json::to_value(project_cfg).unwrap())
        } else {
            Ok(serde_json::to_value(mgr.get_resolved_config()).unwrap())
        }
    }

    /// Set one or more fields with validation; returns true if updated
    pub fn set(
        resolver: &TasksDirectoryResolver,
        values: &std::collections::BTreeMap<String, String>,
        global: bool,
        project: Option<&str>,
    ) -> LoTaRResult<bool> {
        let mgr = ConfigManager::new_manager_with_tasks_dir_ensure_config(&resolver.path).map_err(
            |e| LoTaRError::ValidationError(format!("Failed to init config manager: {}", e)),
        )?;

        // Validate keys first
        for (k, v) in values {
            ConfigManager::validate_field_name(k, global).map_err(|e| {
                LoTaRError::ValidationError(format!("Invalid field '{}': {}", k, e))
            })?;
            ConfigManager::validate_field_value(k, v).map_err(|e| {
                LoTaRError::ValidationError(format!("Invalid value for '{}': {}", k, e))
            })?;
        }

        // Determine project prefix if needed
        let target_project = if global {
            None
        } else {
            Some(
                project
                    .map(|s| s.to_string())
                    .unwrap_or_else(|| mgr.get_resolved_config().default_prefix.clone()),
            )
        };

        // Apply updates
        for (k, v) in values {
            ConfigManager::update_config_field(&resolver.path, k, v, target_project.as_deref())
                .map_err(|e| {
                    LoTaRError::ValidationError(format!("Failed to update '{}': {}", k, e))
                })?;
        }

        Ok(true)
    }
}

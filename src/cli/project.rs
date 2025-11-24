use crate::config::manager::ConfigManager;
use crate::config::types::ResolvedConfig;
use crate::utils::project::resolve_project_input;
use crate::workspace::TasksDirectoryResolver;

/// Project detection and resolution logic
pub struct ProjectResolver {
    config_manager: ConfigManager,
    tasks_dir: std::path::PathBuf,
}

impl ProjectResolver {
    pub fn new(resolver: &TasksDirectoryResolver) -> Result<Self, String> {
        let config_manager = ConfigManager::new_manager_with_tasks_dir_readonly(&resolver.path)
            .map_err(|e| format!("Failed to load config: {}", e))?;

        Ok(Self {
            config_manager,
            tasks_dir: resolver.path.clone(),
        })
    }

    /// Resolve project from task ID, explicit project arg, or default
    pub fn resolve_project(
        &mut self,
        task_id: &str,
        explicit_project: Option<&str>,
    ) -> Result<String, String> {
        // Extract project from task ID if present
        let id_project = self.extract_project_from_task_id(task_id);

        match (explicit_project, id_project) {
            // Both explicit project and task ID prefix provided
            (Some(explicit), Some(ref id_prefix)) => {
                // Validate the explicit project name format
                self.validate_project_name(explicit)?;

                // Check if explicit project matches the task ID prefix
                // The explicit project could be either the prefix itself or the full project name
                // that resolves to the same prefix
                if explicit == id_prefix {
                    // Direct match - explicit project is the prefix
                    Ok(explicit.to_string())
                } else {
                    // Check if explicit project resolves to the same prefix as in task ID
                    // This allows using full project names that map to the prefix
                    let explicit_as_prefix = self.resolve_project_name_to_prefix(explicit);
                    if explicit_as_prefix == *id_prefix {
                        Ok(id_prefix.clone()) // Use the prefix from task ID
                    } else {
                        Err(format!(
                            "Project mismatch: task ID '{}' belongs to project '{}', but '{}' was specified",
                            task_id, id_prefix, explicit
                        ))
                    }
                }
            }
            // Only explicit project provided
            (Some(explicit), None) => {
                // Validate the explicit project name
                self.validate_project_name(explicit)?;
                // Resolve project name to its prefix
                let resolved_prefix = self.resolve_project_name_to_prefix(explicit);
                Ok(resolved_prefix)
            }
            // Only task ID prefix provided
            (None, Some(id_prefix)) => Ok(id_prefix),
            // Neither provided - use default
            (None, None) => {
                // Ensure default_project is set, auto-detecting if necessary
                let default_project = self
                    .config_manager
                    .ensure_default_project(&self.tasks_dir)
                    .map_err(|e| format!("Failed to determine default project: {}", e))?;
                Ok(default_project)
            }
        }
    }

    /// Resolve a project name (which could be a full name) to its prefix
    pub fn resolve_project_name_to_prefix(&self, project_name: &str) -> String {
        // Use the existing utility function that handles project name -> prefix mapping
        resolve_project_input(project_name, self.tasks_dir.as_path())
    }

    /// Validate project name format
    fn validate_project_name(&self, project_name: &str) -> Result<(), String> {
        if project_name.is_empty() {
            return Err("Project name cannot be empty".to_string());
        }

        // Project names should be alphanumeric (allowing underscores, hyphens, and spaces)
        // but not special characters like ! ? @ etc.
        if !project_name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-' || c == ' ')
        {
            return Err(format!(
                "Invalid project name '{}'. Project names can only contain letters, numbers, underscores, hyphens, and spaces",
                project_name
            ));
        }

        // Should not start or end with special characters
        if project_name.starts_with('-')
            || project_name.starts_with('_')
            || project_name.ends_with('-')
            || project_name.ends_with('_')
        {
            return Err(format!(
                "Invalid project name '{}'. Project names cannot start or end with hyphens or underscores",
                project_name
            ));
        }

        Ok(())
    }

    /// Extract project prefix from task ID (e.g., "AUTH-123" -> "AUTH")
    pub fn extract_project_from_task_id(&self, task_id: &str) -> Option<String> {
        // Look for pattern: PREFIX-NUMBER (e.g., AUTH-123, TI-456, MOBILE-789)
        if let Some(dash_pos) = task_id.find('-') {
            let prefix = &task_id[..dash_pos];
            // Verify it's all uppercase alphanumeric (letters or digits)
            if !prefix.is_empty()
                && prefix
                    .chars()
                    .all(|c| c.is_ascii_digit() || (c.is_ascii_uppercase()))
            {
                return Some(prefix.to_string());
            }
        }

        None
    }

    /// Get the resolved global configuration
    pub fn get_config(&self) -> &ResolvedConfig {
        self.config_manager.get_resolved_config()
    }

    /// Get project-specific configuration
    pub fn get_project_config(&self, project_name: &str) -> Result<ResolvedConfig, String> {
        self.config_manager
            .get_project_config(project_name)
            .map_err(|e| format!("Failed to get project config for '{}': {}", project_name, e))
    }

    /// Validate if a task ID format is valid
    pub fn validate_task_id_format(&self, task_id: &str) -> Result<(), String> {
        if task_id.is_empty() {
            return Err("Task ID cannot be empty".to_string());
        }

        // Check if it has a project prefix
        if self.extract_project_from_task_id(task_id).is_some() {
            // Full task ID with prefix (e.g., "AUTH-123")
            Ok(())
        } else {
            // Could be a numeric ID that uses default project (e.g., "123")
            if task_id.chars().all(|c| c.is_ascii_digit()) {
                Ok(())
            } else {
                Err(format!(
                    "Invalid task ID format: '{}'. Expected format: PROJECT-NUMBER (e.g., AUTH-123) or NUMBER (e.g., 123)",
                    task_id
                ))
            }
        }
    }

    /// Get the full task ID with project prefix
    pub fn get_full_task_id(
        &mut self,
        task_id: &str,
        explicit_project: Option<&str>,
    ) -> Result<String, String> {
        // If task ID already has a prefix, use as-is
        if self.extract_project_from_task_id(task_id).is_some() {
            return Ok(task_id.to_string());
        }

        // If it's just a number, add the project prefix
        if task_id.chars().all(|c| c.is_ascii_digit()) {
            let project = self.resolve_project(task_id, explicit_project)?;
            return Ok(format!("{}-{}", project, task_id));
        }

        // Invalid format
        Err(format!("Cannot determine full task ID for: '{}'", task_id))
    }
}

/// Public test support for constructing a ProjectResolver in integration tests
#[doc(hidden)]
pub mod test_support {
    use super::*;
    use crate::config::types::ResolvedConfig;

    /// Create a ProjectResolver from a provided ResolvedConfig and tasks_dir
    pub fn resolver_from_config(
        config: ResolvedConfig,
        tasks_dir: std::path::PathBuf,
    ) -> ProjectResolver {
        // Build a ConfigManager via public test support
        let config_manager = crate::config::manager::test_support::from_resolved_config(config);
        ProjectResolver {
            config_manager,
            tasks_dir,
        }
    }
}

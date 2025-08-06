use crate::config::manager::ConfigManager;
use crate::config::types::ResolvedConfig;
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
    pub fn resolve_project(&mut self, task_id: &str, explicit_project: Option<&str>) -> Result<String, String> {
        // Extract project from task ID if present
        let id_project = self.extract_project_from_task_id(task_id);
        
        match (explicit_project, id_project) {
            // Both explicit project and task ID prefix provided
            (Some(explicit), Some(ref id_prefix)) => {
                // Validate the explicit project name format
                if let Err(e) = self.validate_project_name(explicit) {
                    return Err(e);
                }
                
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
            },
            // Only explicit project provided
            (Some(explicit), None) => {
                // Validate the explicit project name
                if let Err(e) = self.validate_project_name(explicit) {
                    return Err(e);
                }
                // Resolve project name to its prefix
                let resolved_prefix = self.resolve_project_name_to_prefix(explicit);
                Ok(resolved_prefix)
            },
            // Only task ID prefix provided
            (None, Some(id_prefix)) => {
                Ok(id_prefix)
            },
            // Neither provided - use default
            (None, None) => {
                // Ensure default_prefix is set, auto-detecting if necessary
                let default_prefix = self.config_manager.ensure_default_prefix(&self.tasks_dir)
                    .map_err(|e| format!("Failed to determine default project: {}", e))?;
                Ok(default_prefix)
            }
        }
    }
    
    /// Resolve a project name (which could be a full name) to its prefix
    pub fn resolve_project_name_to_prefix(&self, project_name: &str) -> String {
        // Use the existing utility function that handles project name -> prefix mapping
        crate::utils::resolve_project_input(project_name, &self.tasks_dir)
    }
    
    /// Validate project name format
    fn validate_project_name(&self, project_name: &str) -> Result<(), String> {
        if project_name.is_empty() {
            return Err("Project name cannot be empty".to_string());
        }
        
        // Project names should be alphanumeric (allowing underscores, hyphens, and spaces)
        // but not special characters like ! ? @ etc.
        if !project_name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-' || c == ' ') {
            return Err(format!("Invalid project name '{}'. Project names can only contain letters, numbers, underscores, hyphens, and spaces", project_name));
        }
        
        // Should not start or end with special characters
        if project_name.starts_with('-') || project_name.starts_with('_') ||
           project_name.ends_with('-') || project_name.ends_with('_') {
            return Err(format!("Invalid project name '{}'. Project names cannot start or end with hyphens or underscores", project_name));
        }
        
        Ok(())
    }
    
    /// Extract project prefix from task ID (e.g., "AUTH-123" -> "AUTH")
    pub fn extract_project_from_task_id(&self, task_id: &str) -> Option<String> {
        // Look for pattern: LETTERS-NUMBERS (e.g., AUTH-123, TI-456, MOBILE-789)
        if let Some(dash_pos) = task_id.find('-') {
            let prefix = &task_id[..dash_pos];
            // Verify it's all uppercase letters (project prefixes should be consistent)
            if prefix.chars().all(|c| c.is_ascii_uppercase()) && !prefix.is_empty() {
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
        self.config_manager.get_project_config(project_name)
            .map_err(|e| format!("Failed to get project config for '{}': {}", project_name, e))
    }
    
    /// Validate if a task ID format is valid
    pub fn validate_task_id_format(&self, task_id: &str) -> Result<(), String> {
        if task_id.is_empty() {
            return Err("Task ID cannot be empty".to_string());
        }
        
        // Check if it has a project prefix
        if let Some(_) = self.extract_project_from_task_id(task_id) {
            // Full task ID with prefix (e.g., "AUTH-123")
            Ok(())
        } else {
            // Could be a numeric ID that uses default project (e.g., "123")
            if task_id.chars().all(|c| c.is_ascii_digit()) {
                Ok(())
            } else {
                Err(format!("Invalid task ID format: '{}'. Expected format: PROJECT-NUMBER (e.g., AUTH-123) or NUMBER (e.g., 123)", task_id))
            }
        }
    }
    
    /// Get the full task ID with project prefix
    pub fn get_full_task_id(&mut self, task_id: &str, explicit_project: Option<&str>) -> Result<String, String> {
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

#[cfg(test)]
mod tests {
    use super::*;
    
    fn create_test_resolver() -> ProjectResolver {
        use crate::config::types::*;
        use crate::types::{Priority, TaskStatus, TaskType};
        
        // Create a test config manager with a mock ResolvedConfig
        let config = ResolvedConfig {
            server_port: 8080,
            default_prefix: "TEST".to_string(),
            issue_states: ConfigurableField { values: vec![TaskStatus::Todo, TaskStatus::Done] },
            issue_types: ConfigurableField { values: vec![TaskType::Feature, TaskType::Bug] },
            issue_priorities: ConfigurableField { values: vec![Priority::Low, Priority::High] },
            categories: crate::config::types::StringConfigField::new_wildcard(),
            tags: crate::config::types::StringConfigField::new_wildcard(),
            default_assignee: None,
            default_priority: Priority::Medium,
            custom_fields: crate::config::types::StringConfigField::new_wildcard(),
        };
        
        // Create a ConfigManager with the test config
        let config_manager = ConfigManager::from_resolved_config(config);
        
        ProjectResolver {
            config_manager,
            tasks_dir: std::path::PathBuf::from("/tmp"),
        }
    }
    
    #[test]
    fn test_extract_project_from_task_id() {
        let resolver = create_test_resolver();
        
        assert_eq!(resolver.extract_project_from_task_id("AUTH-123"), Some("AUTH".to_string()));
        assert_eq!(resolver.extract_project_from_task_id("TI-456"), Some("TI".to_string()));
        assert_eq!(resolver.extract_project_from_task_id("MOBILE-789"), Some("MOBILE".to_string()));
        
        // Invalid formats
        assert_eq!(resolver.extract_project_from_task_id("123"), None);
        assert_eq!(resolver.extract_project_from_task_id("auth-123"), None); // lowercase
        assert_eq!(resolver.extract_project_from_task_id("AUTH123"), None);  // no dash
        assert_eq!(resolver.extract_project_from_task_id("-123"), None);     // empty prefix
    }
    
    #[test]
    fn test_resolve_project() {
        let mut resolver = create_test_resolver();
        
        // Case 1: Explicit project matches task ID prefix (direct match)
        assert_eq!(resolver.resolve_project("AUTH-123", Some("AUTH")).unwrap(), "AUTH");
        
        // Case 2: Explicit project matches task ID prefix (case insensitive)
        assert_eq!(resolver.resolve_project("AUTH-123", Some("auth")).unwrap(), "AUTH");
        
        // Case 3: Explicit project CONFLICTS with task ID prefix - should ERROR
        let result = resolver.resolve_project("AUTH-123", Some("FRONTEND"));
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Project mismatch"));
        
        // Case 4: Only explicit project provided (no task ID prefix)
        assert_eq!(resolver.resolve_project("123", Some("MOBILE")).unwrap(), "MOBI");
        
        // Case 5: Only task ID prefix provided (no explicit project)
        assert_eq!(resolver.resolve_project("AUTH-123", None).unwrap(), "AUTH");
        
        // Case 6: Neither provided - fall back to default
        assert_eq!(resolver.resolve_project("123", None).unwrap(), "TEST");
        assert_eq!(resolver.resolve_project("", None).unwrap(), "TEST");
        
        // Case 7: Invalid explicit project format should error
        let result = resolver.resolve_project("123", Some("INVALID!"));
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Invalid project name"));
    }
    
    #[test]
    fn test_resolve_project_original_behavior() {
        let mut resolver = create_test_resolver();
        
        // Test edge cases from original implementation
        assert_eq!(resolver.resolve_project("no-prefix", None).unwrap(), "TEST"); // No uppercase prefix gets default
    }
    
    #[test]
    fn test_get_full_task_id() {
        let mut resolver = create_test_resolver();
        
        // Already has prefix
        assert_eq!(resolver.get_full_task_id("AUTH-123", None).unwrap(), "AUTH-123");
        
        // Numeric ID, use default project
        assert_eq!(resolver.get_full_task_id("123", None).unwrap(), "TEST-123");
        
        // Numeric ID with explicit project
        assert_eq!(resolver.get_full_task_id("123", Some("MOBILE")).unwrap(), "MOBI-123");
    }
}

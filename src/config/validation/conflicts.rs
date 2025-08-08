use crate::config::manager::ConfigManager;
use crate::config::validation::errors::{ValidationError, ValidationResult};
use std::collections::HashSet;
use std::path::Path;

pub struct PrefixConflictDetector {
    existing_prefixes: HashSet<String>,
}

impl PrefixConflictDetector {
    pub fn new(tasks_dir: &Path) -> Result<Self, String> {
        let mut existing_prefixes = HashSet::new();

        // Scan existing project directories for prefixes
        if tasks_dir.exists() {
            for (dir_name, _path) in crate::utils::filesystem::list_visible_subdirs(tasks_dir) {
                // Try to load config to get actual prefix
                let config_path = crate::utils::paths::project_config_path(tasks_dir, &dir_name);
                if config_path.exists() {
                    if let Ok(config_manager) =
                        ConfigManager::new_manager_with_tasks_dir_readonly(tasks_dir)
                    {
                        if let Ok(project_config) = config_manager.get_project_config(&dir_name) {
                            existing_prefixes.insert(project_config.default_prefix.to_uppercase());
                            continue;
                        }
                    }
                }
                // Fallback to directory name as prefix
                existing_prefixes.insert(dir_name.to_uppercase());
            }
        }

        Ok(Self { existing_prefixes })
    }

    pub fn check_conflicts(&self, new_prefix: &str) -> ValidationResult {
        let mut result = ValidationResult::new();
        let new_prefix_upper = new_prefix.to_uppercase();

        // Exact match check
        if self.existing_prefixes.contains(&new_prefix_upper) {
            result.add_error(
                ValidationError::error(
                    Some("default_prefix".to_string()),
                    format!("Project prefix '{}' already exists", new_prefix),
                )
                .with_fix("Choose a different prefix or use --force to override".to_string()),
            );
            return result;
        }

        // Substring conflict check
        for existing in &self.existing_prefixes {
            if existing.contains(&new_prefix_upper) || new_prefix_upper.contains(existing) {
                result.add_error(
                    ValidationError::warning(
                        Some("default_prefix".to_string()),
                        format!(
                            "Project prefix '{}' conflicts with existing prefix '{}'",
                            new_prefix, existing
                        ),
                    )
                    .with_fix(
                        "Consider using a more distinct prefix to avoid confusion".to_string(),
                    ),
                );
            }
        }

        // Similar prefix warning (edit distance)
        for existing in &self.existing_prefixes {
            if self.are_similar(&new_prefix_upper, existing) {
                result.add_error(
                    ValidationError::warning(
                        Some("default_prefix".to_string()),
                        format!(
                            "Project prefix '{}' is very similar to existing prefix '{}'",
                            new_prefix, existing
                        ),
                    )
                    .with_fix("Consider using a more distinct prefix".to_string()),
                );
            }
        }

        result
    }

    #[allow(dead_code)]
    pub fn suggest_alternatives(&self, base_prefix: &str) -> Vec<String> {
        let mut suggestions = Vec::new();
        let base_upper = base_prefix.to_uppercase();

        // Try numbered variants
        for i in 1..=9 {
            let candidate = format!("{}{}", base_upper, i);
            if !self.existing_prefixes.contains(&candidate) {
                suggestions.push(candidate);
                if suggestions.len() >= 3 {
                    break;
                }
            }
        }

        // Try character variations
        if suggestions.len() < 3 {
            for suffix in ["X", "V2", "NEW"] {
                let candidate = format!("{}{}", base_upper, suffix);
                if !self.existing_prefixes.contains(&candidate) {
                    suggestions.push(candidate);
                    if suggestions.len() >= 3 {
                        break;
                    }
                }
            }
        }

        suggestions
    }

    fn are_similar(&self, a: &str, b: &str) -> bool {
        if a.len() != b.len() {
            return false;
        }

        let diff_count = a.chars().zip(b.chars()).filter(|(c1, c2)| c1 != c2).count();
        diff_count == 1 && a.len() >= 3 // Only flag as similar if they differ by exactly 1 char and are 3+ chars
    }
}

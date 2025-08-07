use crate::config::types::GlobalConfig;
use crate::index::TaskFilter;
use crate::storage::operations::StorageOperations;
use crate::storage::search::StorageSearch;
use crate::storage::task::Task;
use std::fs;
use std::path::PathBuf;

/// Main storage manager that orchestrates all storage operations
pub struct Storage {
    pub root_path: PathBuf,
}

impl Storage {
    pub fn new(root_path: PathBuf) -> Self {
        fs::create_dir_all(&root_path).unwrap();

        // Ensure global config exists
        Self::ensure_global_config_exists(&root_path, None);

        Self { root_path }
    }

    /// Create Storage with intelligent global config creation
    pub fn new_with_context(root_path: PathBuf, project_context: Option<&str>) -> Self {
        fs::create_dir_all(&root_path).unwrap();

        // Ensure global config exists with smart default_prefix detection
        Self::ensure_global_config_exists(&root_path, project_context);

        Self { root_path }
    }

    /// Ensure global config exists, creating it intelligently if missing
    fn ensure_global_config_exists(root_path: &PathBuf, project_context: Option<&str>) {
        let global_config_path = root_path.join("config.yml");

        if global_config_path.exists() {
            return; // Already exists, nothing to do
        }

        // Create global config with intelligent default_prefix
        let mut global_config = GlobalConfig::default();

        // Try to set a smart default_prefix
        if let Some(smart_prefix) = Self::determine_smart_default_prefix(root_path, project_context)
        {
            global_config.default_prefix = smart_prefix;
        }

        // Write the global config
        if let Ok(config_yaml) = serde_yaml::to_string(&global_config) {
            let _ = fs::write(&global_config_path, config_yaml);
        }
    }

    /// Determine the best default_prefix for global config
    fn determine_smart_default_prefix(
        root_path: &PathBuf,
        project_context: Option<&str>,
    ) -> Option<String> {
        // 1. Use explicit project context if provided
        if let Some(project_name) = project_context {
            if let Ok(prefix) =
                crate::utils::generate_unique_project_prefix(project_name, root_path)
            {
                return Some(prefix);
            }
        }

        // 2. Try auto-detection from current directory
        if let Some(auto_detected) = crate::project::detect_project_name() {
            if let Ok(prefix) =
                crate::utils::generate_unique_project_prefix(&auto_detected, root_path)
            {
                return Some(prefix);
            }
        }

        // 3. Check if any existing projects exist and use one as default
        if let Ok(entries) = fs::read_dir(root_path) {
            for entry in entries.flatten() {
                if entry.file_type().map(|t| t.is_dir()).unwrap_or(false) {
                    let dir_name = entry.file_name().to_string_lossy().to_string();
                    // Skip special directories
                    if dir_name != "." && dir_name != ".." && !dir_name.starts_with('.') {
                        return Some(dir_name);
                    }
                }
            }
        }

        // 4. Fall back to empty (no default project)
        None
    }

    /// Try to open existing storage without creating directories
    /// Returns None if the storage directory doesn't exist
    pub fn try_open(root_path: PathBuf) -> Option<Self> {
        if !root_path.exists() {
            return None;
        }

        Some(Self { root_path })
    }

    pub fn add(
        &mut self,
        task: &Task,
        project_prefix: &str,
        original_project_name: Option<&str>,
    ) -> String {
        match StorageOperations::add(&self.root_path, task, project_prefix, original_project_name) {
            Ok(formatted_id) => formatted_id,
            Err(_) => "ERROR".to_string(), // TODO: Better error handling
        }
    }

    pub fn get(&self, id: &str, project: String) -> Option<Task> {
        StorageOperations::get(&self.root_path, id, project)
    }

    pub fn edit(&mut self, id: &str, new_task: &Task) {
        let _ = StorageOperations::edit(&self.root_path, id, new_task);
    }

    pub fn delete(&mut self, id: &str, project: String) -> bool {
        StorageOperations::delete(&self.root_path, id, project).unwrap_or_default()
    }

    pub fn search(&self, filter: &TaskFilter) -> Vec<(String, Task)> {
        StorageSearch::search(&self.root_path, filter)
    }
}

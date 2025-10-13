use crate::config::types::GlobalConfig;
use crate::storage::TaskFilter;
use crate::storage::backend::{FsBackend, StorageBackend};
use crate::storage::search::StorageSearch;
use crate::storage::task::Task;
use crate::utils::project::generate_unique_project_prefix;
use std::fs;
use std::path::{Path, PathBuf};

/// Main storage manager that orchestrates all storage operations
pub struct Storage {
    pub root_path: PathBuf,
    backend: Box<dyn StorageBackend>,
}

impl Storage {
    /// Create storage with default filesystem backend
    pub fn new(root_path: PathBuf) -> Self {
        let backend: Box<dyn StorageBackend> = Box::new(FsBackend);
        Self::new_with_backend(root_path, backend)
    }

    /// Create storage with an explicit backend implementation
    pub fn new_with_backend(root_path: PathBuf, backend: Box<dyn StorageBackend>) -> Self {
        let _ = fs::create_dir_all(&root_path);

        // Ensure global config exists
        Self::ensure_global_config_exists(&root_path, None);
        Self { root_path, backend }
    }

    /// Create Storage with intelligent global config creation
    pub fn new_with_context(root_path: PathBuf, project_context: Option<&str>) -> Self {
        let backend: Box<dyn StorageBackend> = Box::new(FsBackend);
        let _ = fs::create_dir_all(&root_path);

        // Ensure global config exists with smart default_prefix detection
        Self::ensure_global_config_exists(&root_path, project_context);
        Self { root_path, backend }
    }

    /// Ensure global config exists, creating it intelligently if missing
    fn ensure_global_config_exists(root_path: &Path, project_context: Option<&str>) {
        let global_config_path = crate::utils::paths::global_config_path(root_path);

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

        // Write the global config in canonical nested format
        let config_yaml = crate::config::normalization::to_canonical_global_yaml(&global_config);
        let _ = fs::write(&global_config_path, config_yaml);
    }

    /// Determine the best default_prefix for global config
    fn determine_smart_default_prefix(
        root_path: &Path,
        project_context: Option<&str>,
    ) -> Option<String> {
        // 1. Use explicit project context if provided
        if let Some(project_name) = project_context {
            if let Ok(prefix) = generate_unique_project_prefix(project_name, root_path) {
                return Some(prefix);
            }
        }

        // 2. Try auto-detection from current directory
        if let Some(auto_detected) = crate::project::detect_project_name() {
            if let Ok(prefix) = generate_unique_project_prefix(&auto_detected, root_path) {
                return Some(prefix);
            }
        }

        // 3. Check if any existing projects exist and use one as default
        if let Some((dir_name, _path)) = crate::utils::filesystem::list_visible_subdirs(root_path)
            .into_iter()
            .next()
        {
            return Some(dir_name);
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

        Some(Self {
            root_path,
            backend: Box::new(FsBackend),
        })
    }

    pub fn add(
        &mut self,
        task: &Task,
        project_prefix: &str,
        original_project_name: Option<&str>,
    ) -> String {
        match self
            .backend
            .add(&self.root_path, task, project_prefix, original_project_name)
        {
            Ok(formatted_id) => formatted_id,
            Err(_) => "ERROR".to_string(), // TODO: Better error handling
        }
    }

    pub fn get(&self, id: &str, project: String) -> Option<Task> {
        self.backend.get(&self.root_path, id, &project)
    }

    pub fn find_task_by_numeric_id(&self, numeric_id: &str) -> Option<(String, Task)> {
        if !numeric_id.chars().all(|c| c.is_ascii_digit()) {
            return None;
        }

        let debug_scan = std::env::var("LOTAR_DEBUG_STATUS").is_ok();

        let mut candidate_roots = Vec::new();
        candidate_roots.push(self.root_path.clone());
        if let Ok(canonical) = std::fs::canonicalize(&self.root_path) {
            if canonical != self.root_path {
                candidate_roots.push(canonical);
            }
        }
        if let Some(parent) = self.root_path.parent() {
            for (name, dir_path) in crate::utils::filesystem::list_visible_subdirs(parent) {
                let child_tasks = dir_path.join(".tasks");
                if child_tasks.exists() && child_tasks.is_dir() {
                    if debug_scan {
                        eprintln!(
                            "[lotar][debug] considering sibling tasks root {} from {}",
                            child_tasks.display(),
                            name
                        );
                    }
                    candidate_roots.push(child_tasks);
                }
            }
        }

        candidate_roots.sort();
        candidate_roots.dedup();

        for root in candidate_roots {
            if debug_scan {
                eprintln!("[lotar][debug] scanning tasks root {}", root.display());
                match std::fs::read_dir(&root) {
                    Ok(entries) => {
                        for entry in entries.flatten() {
                            let path = entry.path();
                            eprintln!(
                                "[lotar][debug]   root entry: {} (dir={})",
                                path.display(),
                                path.is_dir()
                            );
                        }
                    }
                    Err(err) => {
                        eprintln!(
                            "[lotar][debug]   unable to read root {}: {}",
                            root.display(),
                            err
                        );
                    }
                }
            }

            for (prefix, dir_path) in crate::utils::filesystem::list_visible_subdirs(&root) {
                if debug_scan {
                    let candidate_file = dir_path.join(format!("{}.yml", numeric_id));
                    eprintln!(
                        "[lotar][debug]   probing numeric={} candidate_prefix={} dir={} exists={} file_exists={}",
                        numeric_id,
                        prefix,
                        dir_path.display(),
                        dir_path.exists(),
                        candidate_file.exists()
                    );
                }

                let full_id = format!("{}-{}", prefix, numeric_id);
                if let Some(task) = self.backend.get(&self.root_path, &full_id, &prefix) {
                    if debug_scan {
                        eprintln!(
                            "[lotar][debug]   matched numeric={} as {}",
                            numeric_id, full_id
                        );
                    }
                    return Some((full_id, task));
                }
            }
        }

        if debug_scan {
            eprintln!(
                "[lotar][debug] numeric={} not found under {}",
                numeric_id,
                self.root_path.display()
            );
        }

        None
    }

    pub fn edit(&mut self, id: &str, new_task: &Task) {
        let _ = self.backend.edit(&self.root_path, id, new_task);
    }

    pub fn delete(&mut self, id: &str, project: String) -> bool {
        self.backend
            .delete(&self.root_path, id, &project)
            .unwrap_or_default()
    }

    pub fn search(&self, filter: &TaskFilter) -> Vec<(String, Task)> {
        StorageSearch::search(&self.root_path, filter)
    }
}

use crate::errors::{LoTaRError, LoTaRResult};
use crate::storage::TaskFilter;
use crate::storage::locator::StorageLocator;
use crate::storage::operations::StorageOperations;
use crate::storage::search::StorageSearch;
use crate::storage::task::Task;
use std::fs;
use std::path::{Path, PathBuf};

/// Main storage manager that orchestrates all storage operations
pub struct Storage {
    pub root_path: PathBuf,
}

impl Storage {
    /// Create storage rooted at the given path
    pub fn new(root_path: &Path) -> Self {
        let root_path = root_path.to_path_buf();
        let _ = fs::create_dir_all(&root_path);

        // Ensure global config exists
        let _ = crate::config::bootstrap::ensure_global_config(&root_path, None);
        Self { root_path }
    }

    /// Create Storage with intelligent global config creation
    pub fn new_with_context(root_path: &Path, project_context: Option<&str>) -> Self {
        let root_path = root_path.to_path_buf();
        let _ = fs::create_dir_all(&root_path);

        // Ensure global config exists with smart default_project detection
        let _ = crate::config::bootstrap::ensure_global_config(&root_path, project_context);
        Self { root_path }
    }

    /// Try to open existing storage without creating directories
    /// Returns None if the storage directory doesn't exist
    pub fn try_open(root_path: &Path) -> Option<Self> {
        if !root_path.exists() {
            return None;
        }

        Some(Self {
            root_path: root_path.to_path_buf(),
        })
    }

    pub fn add(
        &mut self,
        task: &Task,
        project_prefix: &str,
        original_project_name: Option<&str>,
    ) -> LoTaRResult<String> {
        StorageOperations::add(&self.root_path, task, project_prefix, original_project_name)
            .map_err(map_storage_error)
    }

    pub fn get(&self, id: &str, project: &str) -> Option<Task> {
        StorageOperations::get(&self.root_path, id, project)
    }

    pub fn find_task_by_numeric_id(&self, numeric_id: &str) -> Option<(String, Task)> {
        if !numeric_id.chars().all(|c| c.is_ascii_digit()) {
            return None;
        }

        let debug_scan = std::env::var("LOTAR_DEBUG_STATUS").is_ok();

        let candidate_roots = StorageLocator::candidate_task_roots(&self.root_path);

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
                if let Some(task) = StorageOperations::get(&self.root_path, &full_id, &prefix) {
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

    pub fn edit(&mut self, id: &str, new_task: &Task) -> LoTaRResult<()> {
        StorageOperations::edit(&self.root_path, id, new_task).map_err(map_storage_error)
    }

    pub fn delete(&mut self, id: &str, project: &str) -> LoTaRResult<bool> {
        StorageOperations::delete(&self.root_path, id, project).map_err(map_storage_error)
    }

    pub fn search(&self, filter: &TaskFilter) -> Vec<(String, Task)> {
        StorageSearch::search(&self.root_path, filter)
    }
}

pub(crate) fn map_storage_error(err: Box<dyn std::error::Error>) -> LoTaRError {
    match err.downcast::<std::io::Error>() {
        Ok(io_err) => LoTaRError::IoError(*io_err),
        Err(err) => match err.downcast::<serde_yaml_ng::Error>() {
            Ok(yaml_err) => LoTaRError::SerializationError(yaml_err.to_string()),
            Err(other) => LoTaRError::ValidationError(other.to_string()),
        },
    }
}

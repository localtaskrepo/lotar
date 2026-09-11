use crate::errors::{LoTaRError, LoTaRResult};
use crate::storage::TaskFilter;
use crate::storage::identity::{self, TaskId, TaskLocation, TaskLookupError};
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

    /// Resolve a full task ID to its single storage location across the
    /// primary and sibling workspace roots, failing closed (with diagnostics)
    /// on malformed, unknown, or ambiguous identifiers.
    pub fn resolve_task_location(&self, id: &str) -> Result<TaskLocation, TaskLookupError> {
        identity::resolve(&self.root_path, id)
    }

    /// Resolve a bare numeric identifier to its single storage location,
    /// failing closed when more than one project/root stores that number.
    pub fn resolve_numeric_id(&self, numeric_id: &str) -> Result<(String, Task), TaskLookupError> {
        let location = identity::resolve_numeric(&self.root_path, numeric_id)?;
        if std::env::var("LOTAR_DEBUG_STATUS").is_ok() {
            eprintln!(
                "[lotar][debug]   matched numeric={} as {} under {}",
                numeric_id,
                location.full_id(),
                location.root.display()
            );
        }
        let task = StorageSearch::load_task_file(&location.file)
            .ok_or_else(|| TaskLookupError::NotFound(location.full_id()))?;
        Ok((location.full_id(), task))
    }

    /// Backward-compatible numeric lookup: `None` for not-found AND for
    /// ambiguous numbers (never an arbitrary pick). Callers that can surface
    /// a clear error should prefer [`Self::resolve_numeric_id`].
    pub fn find_task_by_numeric_id(&self, numeric_id: &str) -> Option<(String, Task)> {
        if !numeric_id.chars().all(|c| c.is_ascii_digit()) {
            return None;
        }
        self.resolve_numeric_id(numeric_id).ok()
    }

    /// Parsed canonical identity, when the string is well-formed.
    pub fn parse_task_id(&self, id: &str) -> Option<TaskId> {
        TaskId::parse(id).ok()
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

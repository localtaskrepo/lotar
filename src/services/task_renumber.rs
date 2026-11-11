//! Giving one copy of a duplicated task number a number of its own (DEV-270).
//!
//! A number stored twice in one project (two folders, or a folder and the
//! archive) is listed once and refuses changes. Renumbering moves the chosen
//! copy to the next free number in the same folder and records the old ID in
//! its history. No redirect is left: the old ID still belongs to the other
//! copy.

use std::path::{Path, PathBuf};

use crate::errors::{LoTaRError, LoTaRResult};
use crate::services::task_service::TaskService;
use crate::storage::TaskId;
use crate::storage::layout;
use crate::storage::manager::Storage;
use crate::storage::operations::StorageOperations;
use crate::types::{TaskChange, TaskChangeLogEntry};

/// What a renumber changed (or would change, for a dry run).
#[derive(Debug, Clone)]
pub struct RenumberOutcome {
    pub from: String,
    pub to: String,
    /// The renumbered copy's old and new file.
    pub old_file: PathBuf,
    pub new_file: PathBuf,
    /// The copies that keep the old ID.
    pub kept: Vec<PathBuf>,
}

impl TaskService {
    /// Renumber one copy of a duplicated task ID; `file` picks the copy and
    /// is required, since nothing tells the copies apart reliably.
    pub fn renumber_duplicate(storage: &mut Storage, id: &str, file: Option<&Path>, dry_run: bool) -> LoTaRResult<RenumberOutcome> {
        let parsed = TaskId::parse(id).map_err(|err| LoTaRError::InvalidTaskId(format!("{id}: {err}")))?;
        let from = parsed.canonical();
        let root = storage.root_path.clone();
        let project_dir = root.join(&parsed.project);
        crate::storage::safety::with_storage_lock(&project_dir, "task", || {
            let copies = layout::find(&project_dir, parsed.number);
            match copies.len() {
                0 => return Err(LoTaRError::TaskNotFound(from.clone())),
                1 => {
                    return Err(LoTaRError::ValidationError(format!(
                        "Task '{from}' is stored in one file; renumbering only resolves a number stored twice. To change the project, use `lotar task move`"
                    )));
                }
                _ => {}
            }
            let listing = || copies.iter().map(|copy| copy.path.display().to_string()).collect::<Vec<_>>().join(", ");
            let Some(wanted) = file else {
                return Err(LoTaRError::ValidationError(format!(
                    "Task '{from}' is stored in {} files: {}. Choose the copy to renumber with --file <path>",
                    copies.len(),
                    listing()
                )));
            };
            let wanted = wanted.canonicalize().unwrap_or_else(|_| wanted.to_path_buf());
            let Some(chosen) = copies.iter().find(|copy| copy.path.canonicalize().is_ok_and(|path| path == wanted)) else {
                return Err(LoTaRError::ValidationError(format!("'{}' is not one of the files of '{from}': {}", wanted.display(), listing())));
            };

            let content = std::fs::read_to_string(&chosen.path)?;
            let mut task = crate::storage::task::parse_task_yaml_tolerant(&content).ok_or_else(|| {
                LoTaRError::ValidationError(format!("Cannot renumber malformed task YAML in {}; repair the file first", chosen.path.display()))
            })?;
            if let Some(target) = task.moved_to.as_deref() {
                return Err(LoTaRError::ValidationError(format!(
                    "{} is the redirect stub of a task moved to '{target}'; renumber the other copy",
                    chosen.path.display()
                )));
            }

            let number = StorageOperations::get_current_id(&project_dir) + 1;
            let to = format!("{}-{number}", parsed.project);
            let new_file = chosen.path.with_file_name(format!("{number}.yml"));
            let kept = copies.iter().filter(|copy| copy.path != chosen.path).map(|copy| copy.path.clone()).collect();
            let outcome = RenumberOutcome { from: from.clone(), to: to.clone(), old_file: chosen.path.clone(), new_file: new_file.clone(), kept };
            if dry_run {
                return Ok(outcome);
            }

            task.history.push(TaskChangeLogEntry {
                extra: Default::default(),
                at: chrono::Utc::now().to_rfc3339(),
                actor: crate::utils::identity::resolve_current_user_for_project(Some(root.as_path()), Some(&parsed.project)),
                changes: vec![TaskChange { field: "renumbered".into(), old: Some(from.clone()), new: Some(to.clone()) }],
            });
            let serialized = serde_yaml_ng::to_string(&task).map_err(|err| LoTaRError::SerializationError(err.to_string()))?;
            crate::storage::safety::atomic_write_file(&new_file, &serialized)?;
            std::fs::remove_file(&chosen.path)?;
            Ok(outcome)
        })
    }
}

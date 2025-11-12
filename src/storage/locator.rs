use std::collections::HashSet;
use std::path::{Path, PathBuf};

/// Utilities for discovering task storage locations across nested workspaces.
pub struct StorageLocator;

impl StorageLocator {
    /// Return the set of candidate `.tasks` roots to inspect when operating on storage.
    ///
    /// The search includes:
    /// - the provided `root_path`
    /// - its canonical form (to handle symlinks)
    /// - any sibling directories that contain their own `.tasks` folder (monorepo layout)
    pub fn candidate_task_roots(root_path: &Path) -> Vec<PathBuf> {
        let mut roots: Vec<PathBuf> = Vec::new();
        let mut seen: HashSet<PathBuf> = HashSet::new();

        let mut push_unique = |path: &Path| {
            let canonical_or_original = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());

            if seen.insert(canonical_or_original.clone()) {
                roots.push(canonical_or_original);
            }
        };

        push_unique(root_path);

        if let Some(parent) = root_path.parent() {
            for (_, dir_path) in crate::utils::filesystem::list_visible_subdirs(parent) {
                let child_tasks = dir_path.join(".tasks");
                if child_tasks.exists() && child_tasks.is_dir() {
                    push_unique(&child_tasks);
                }
            }
        }

        roots.sort();
        roots
    }

    /// Identify project folders matching the provided identifier (typically a prefix).
    pub fn project_folders_for_name(root_path: &Path, project_name: &str) -> Vec<String> {
        let mut folders = Vec::new();
        let candidate = root_path.join(project_name);
        if candidate.exists() {
            folders.push(project_name.to_string());
        }
        folders
    }
}

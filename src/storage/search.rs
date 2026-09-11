use crate::storage::TaskFilter;
use crate::storage::locator::StorageLocator;
use crate::storage::task::Task;
#[cfg(feature = "parallel")]
use rayon::prelude::*;
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::SystemTime;

/// Cached parse of one task file, validated on every read by the file's
/// (mtime, len) fingerprint. Entries are never trusted across an external
/// modification: any edit changes the mtime (APFS has nanosecond granularity)
/// and virtually always the length too, so the next search re-parses the file.
/// This makes the cache behave exactly like an uncached scan while skipping
/// the YAML parse for unchanged files — including in serve mode, where tasks
/// are routinely edited on disk by other tools.
struct CachedTask {
    mtime: SystemTime,
    len: u64,
    task: Arc<Task>,
}

/// Upper bound on cached entries so pathological workspaces cannot grow the
/// resident set without limit. Way above any real project's task count.
const TASK_CACHE_MAX_ENTRIES: usize = 100_000;

fn task_cache() -> &'static Mutex<HashMap<PathBuf, CachedTask>> {
    static CACHE: OnceLock<Mutex<HashMap<PathBuf, CachedTask>>> = OnceLock::new();
    CACHE.get_or_init(|| Mutex::new(HashMap::new()))
}

/// Search and filtering functionality for task storage
pub struct StorageSearch;

impl StorageSearch {
    /// Load and parse a task file, using the (mtime, len)-validated parse cache.
    /// Corrupt files warn once and return None, mirroring uncached behavior.
    pub fn load_task_file(path: &Path) -> Option<Task> {
        let metadata = fs::metadata(path).ok()?;
        let mtime = metadata.modified().ok()?;
        let len = metadata.len();

        let cache_key = path.to_path_buf();
        if let Ok(cache) = task_cache().lock()
            && let Some(hit) = cache.get(&cache_key)
            && hit.mtime == mtime
            && hit.len == len
        {
            return Some((*hit.task).clone());
        }

        let content = match fs::read_to_string(path) {
            Ok(content) => content,
            Err(_) => return None,
        };
        let task = match serde_yaml_ng::from_str::<Task>(&content) {
            Ok(task) => task,
            Err(e) => {
                crate::storage::safety::warn_corrupt_once(path, &e.to_string());
                return None;
            }
        };
        if let Ok(mut cache) = task_cache().lock() {
            // Opportunistic size guard: drop the whole cache if it grew past
            // the bound rather than tracking LRU state. Rebuilding it is
            // cheap (one parse per file) and the case is pathological anyway.
            if cache.len() >= TASK_CACHE_MAX_ENTRIES {
                cache.clear();
            }
            cache.insert(
                cache_key,
                CachedTask {
                    mtime,
                    len,
                    task: Arc::new(task.clone()),
                },
            );
        }
        Some(task)
    }

    /// Search for tasks based on filter criteria
    pub fn search(root_path: &Path, filter: &TaskFilter) -> Vec<(String, Task)> {
        let mut results: Vec<(String, Task)> = Vec::new();

        // No longer use index for tag pre-filtering - do all filtering during file scan

        // If we have a specific project filter, search only that project
        if let Some(project) = &filter.project {
            for candidate_root in StorageLocator::candidate_task_roots(root_path) {
                let project_folders =
                    StorageLocator::project_folders_for_name(&candidate_root, project);
                for project_folder in project_folders {
                    let project_path = candidate_root.join(&project_folder);
                    let files = crate::utils::filesystem::list_files_with_ext(&project_path, "yml");

                    #[cfg(feature = "parallel")]
                    let mut partial: Vec<(String, Task)> = files
                        .par_iter()
                        .filter_map(|path| {
                            let numeric_id = crate::utils::filesystem::file_numeric_stem(path)?;
                            let task_id = format!("{}-{}", project_folder, numeric_id);
                            let task = Self::load_task_file(path)?;
                            if Self::task_matches_filter(&task_id, &task, filter) {
                                Some((task_id, task))
                            } else {
                                None
                            }
                        })
                        .collect();

                    #[cfg(not(feature = "parallel"))]
                    let mut partial: Vec<(String, Task)> = files
                        .iter()
                        .filter_map(|path| {
                            let numeric_id = crate::utils::filesystem::file_numeric_stem(path)?;
                            let task_id = format!("{}-{}", project_folder, numeric_id);
                            let task = Self::load_task_file(path)?;
                            if Self::task_matches_filter(&task_id, &task, filter) {
                                Some((task_id, task))
                            } else {
                                None
                            }
                        })
                        .collect();

                    results.append(&mut partial);
                }
            }
        } else {
            // Search across all projects of every candidate root (primary +
            // sibling workspaces), matching the project-filtered branch so
            // unfiltered queries discover the same nested-root tasks.
            let subdirs = StorageLocator::candidate_task_roots(root_path)
                .into_iter()
                .flat_map(|candidate_root| {
                    crate::utils::filesystem::list_visible_subdirs(&candidate_root)
                        .into_iter()
                        .map(move |(project_folder, dir_path)| {
                            (project_folder, dir_path, candidate_root.clone())
                        })
                });
            let all_files: Vec<(String, std::path::PathBuf)> = subdirs
                .into_iter()
                .flat_map(|(project_folder, dir_path, _root)| {
                    let files = crate::utils::filesystem::list_files_with_ext(&dir_path, "yml");
                    files.into_iter().map(move |p| (project_folder.clone(), p))
                })
                .collect();

            #[cfg(feature = "parallel")]
            {
                results = all_files
                    .par_iter()
                    .filter_map(|(project_folder, task_path)| {
                        let numeric_id = crate::utils::filesystem::file_numeric_stem(task_path)?;
                        let task_id = format!("{}-{}", project_folder, numeric_id);
                        let task = Self::load_task_file(task_path)?;
                        if Self::task_matches_filter(&task_id, &task, filter) {
                            Some((task_id, task))
                        } else {
                            None
                        }
                    })
                    .collect();
            }
            #[cfg(not(feature = "parallel"))]
            {
                results = all_files
                    .iter()
                    .filter_map(|(project_folder, task_path)| {
                        let numeric_id =
                            match crate::utils::filesystem::file_numeric_stem(task_path) {
                                Some(n) => n,
                                None => return None,
                            };
                        let task_id = format!("{}-{}", project_folder, numeric_id);
                        let task = Self::load_task_file(task_path)?;
                        if Self::task_matches_filter(&task_id, &task, filter) {
                            Some((task_id, task))
                        } else {
                            None
                        }
                    })
                    .collect();
            }
        }
        // Deterministic order
        results.sort_by(|a, b| a.0.cmp(&b.0));
        results
    }

    /// Helper method to check if a task matches all filter criteria
    pub fn task_matches_filter(task_id: &str, task: &Task, filter: &TaskFilter) -> bool {
        // Check status filter (OR logic - match any of the specified statuses)
        if !filter.status.is_empty() && !filter.status.contains(&task.status) {
            return false;
        }

        // Check priority filter (OR logic - match any of the specified priorities)
        if !filter.priority.is_empty() && !filter.priority.contains(&task.priority) {
            return false;
        }

        // Check task type filter (OR logic - match any of the specified types)
        if !filter.task_type.is_empty() && !filter.task_type.contains(&task.task_type) {
            return false;
        }

        // Check text query
        if !Self::matches_text_filter(task_id, task, &filter.text_query) {
            return false;
        }

        // Check tag filters (OR logic - match any of the specified tags)
        if !filter.tags.is_empty() {
            let task_has_matching_tag = filter.tags.iter().any(|filter_tag| {
                task.tags
                    .iter()
                    .any(|task_tag| crate::utils::fuzzy_match::fuzzy_contains(task_tag, filter_tag))
            });
            if !task_has_matching_tag {
                return false;
            }
        }

        if !filter.custom_fields.is_empty() {
            for (name, allowed) in &filter.custom_fields {
                let Some(values) =
                    crate::utils::custom_fields::extract_value_strings(&task.custom_fields, name)
                else {
                    return false;
                };
                if values.is_empty()
                    || !crate::utils::fuzzy_match::fuzzy_set_match(&values, allowed)
                {
                    return false;
                }
            }
        }

        true
    }

    /// Check if a task matches text filter criteria
    pub fn matches_text_filter(task_id: &str, task: &Task, text_query: &Option<String>) -> bool {
        if let Some(query) = text_query {
            let query_lower = query.to_lowercase();
            let id_lower = task_id.to_lowercase();
            id_lower.contains(&query_lower)
                || task.title.to_lowercase().contains(&query_lower)
                || task
                    .subtitle
                    .as_ref()
                    .is_some_and(|s| s.to_lowercase().contains(&query_lower))
                || task
                    .description
                    .as_ref()
                    .is_some_and(|s| s.to_lowercase().contains(&query_lower))
                || task
                    .tags
                    .iter()
                    .any(|tag| tag.to_lowercase().contains(&query_lower))
        } else {
            true
        }
    }
}

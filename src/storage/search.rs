use crate::storage::TaskFilter;
use crate::storage::task::Task;
#[cfg(feature = "parallel")]
use rayon::prelude::*;
use std::fs;
use std::path::Path;

/// Search and filtering functionality for task storage
pub struct StorageSearch;

impl StorageSearch {
    /// Search for tasks based on filter criteria
    pub fn search(root_path: &Path, filter: &TaskFilter) -> Vec<(String, Task)> {
        let mut results: Vec<(String, Task)> = Vec::new();

        // No longer use index for tag pre-filtering - do all filtering during file scan

        // If we have a specific project filter, search only that project
        if let Some(project) = &filter.project {
            // Try to find the project folder (could be the project name itself or a mapped prefix)
            let project_folders = Self::get_project_folders_for_name(root_path, project);

            for project_folder in project_folders {
                let project_path = root_path.join(&project_folder);
                let files = crate::utils::filesystem::list_files_with_ext(&project_path, "yml");

                #[cfg(feature = "parallel")]
                let mut partial: Vec<(String, Task)> = files
                    .par_iter()
                    .filter_map(|path| {
                        let numeric_id = crate::utils::filesystem::file_numeric_stem(path)?;
                        let task_id = format!("{}-{}", project_folder, numeric_id);
                        let content = fs::read_to_string(path).ok()?;
                        let task: Task = serde_yaml::from_str(&content).ok()?;
                        if Self::task_matches_filter(&task, filter) {
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
                        let content = fs::read_to_string(path).ok()?;
                        let task: Task = serde_yaml::from_str(&content).ok()?;
                        if Self::task_matches_filter(&task, filter) {
                            Some((task_id, task))
                        } else {
                            None
                        }
                    })
                    .collect();

                results.append(&mut partial);
            }
        } else {
            // Search across all projects
            let all_files: Vec<(String, std::path::PathBuf)> =
                crate::utils::filesystem::list_visible_subdirs(root_path)
                    .into_iter()
                    .flat_map(|(project_folder, dir_path)| {
                        crate::utils::filesystem::list_files_with_ext(&dir_path, "yml")
                            .into_iter()
                            .map(move |p| (project_folder.clone(), p))
                    })
                    .collect();

            #[cfg(feature = "parallel")]
            {
                results = all_files
                    .par_iter()
                    .filter_map(|(project_folder, task_path)| {
                        let numeric_id = crate::utils::filesystem::file_numeric_stem(task_path)?;
                        let task_id = format!("{}-{}", project_folder, numeric_id);
                        let content = fs::read_to_string(task_path).ok()?;
                        let task: Task = serde_yaml::from_str(&content).ok()?;
                        if Self::task_matches_filter(&task, filter) {
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
                        let numeric_id = crate::utils::filesystem::file_numeric_stem(task_path)?;
                        let task_id = format!("{}-{}", project_folder, numeric_id);
                        let content = fs::read_to_string(task_path).ok()?;
                        let task: Task = serde_yaml::from_str(&content).ok()?;
                        if Self::task_matches_filter(&task, filter) {
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

    /// Helper method to get potential project folders for a given project name
    pub fn get_project_folders_for_name(root_path: &Path, project_name: &str) -> Vec<String> {
        let mut folders = Vec::new();

        // Only add the project if the directory actually exists
        if root_path.join(project_name).exists() {
            folders.push(project_name.to_string());
        }

        folders
    }

    /// Helper method to check if a task matches all filter criteria
    pub fn task_matches_filter(task: &Task, filter: &TaskFilter) -> bool {
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

        // Check category filter
        if let Some(category) = &filter.category {
            if task.category.as_ref() != Some(category) {
                return false;
            }
        }

        // Check text query
        if !Self::matches_text_filter(task, &filter.text_query) {
            return false;
        }

        // Check tag filters (OR logic - match any of the specified tags)
        if !filter.tags.is_empty() {
            let task_has_matching_tag = filter
                .tags
                .iter()
                .any(|filter_tag| task.tags.iter().any(|task_tag| task_tag == filter_tag));
            if !task_has_matching_tag {
                return false;
            }
        }

        true
    }

    /// Check if a task matches text filter criteria
    pub fn matches_text_filter(task: &Task, text_query: &Option<String>) -> bool {
        if let Some(query) = text_query {
            let query_lower = query.to_lowercase();
            task.title.to_lowercase().contains(&query_lower)
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

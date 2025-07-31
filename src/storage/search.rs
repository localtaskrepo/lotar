use std::fs;
use std::path::Path;
use crate::storage::task::Task;
use crate::index::{TaskIndex, TaskFilter};

/// Search and filtering functionality for task storage
pub struct StorageSearch;

impl StorageSearch {
    /// Search for tasks based on filter criteria
    pub fn search(
        root_path: &Path,
        index: &TaskIndex,
        filter: &TaskFilter,
    ) -> Vec<(String, Task)> {
        let mut results = Vec::new();

        // If we have tag filters, use the index to get candidate task IDs
        let tag_candidates = if !filter.tags.is_empty() {
            index.find_by_filter(filter)
        } else {
            Vec::new()
        };

        // If we have a specific project filter, search only that project
        if let Some(project) = &filter.project {
            // Try to find the project folder (could be the project name itself or a mapped prefix)
            let project_folders = Self::get_project_folders_for_name(root_path, project);

            for project_folder in project_folders {
                let project_path = root_path.join(&project_folder);
                if let Ok(entries) = fs::read_dir(&project_path) {
                    for entry in entries.flatten() {
                        let path = entry.path();
                        if path.is_file() && path.extension().map_or(false, |ext| ext == "yml") {
                            // Extract task ID from filename
                            if let Some(stem) = path.file_stem().and_then(|s| s.to_str()) {
                                if let Ok(numeric_id) = stem.parse::<u64>() {
                                    let task_id = format!("{}-{}", project_folder, numeric_id);

                                    // If we have tag filters, check if this task matches
                                    if !filter.tags.is_empty() && !tag_candidates.contains(&task_id) {
                                        continue;
                                    }

                                    // Load and filter the task
                                    if let Ok(content) = fs::read_to_string(&path) {
                                        if let Ok(task) = serde_yaml::from_str::<Task>(&content) {
                                            if Self::task_matches_filter(&task, filter) {
                                                results.push((task_id, task));
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        } else {
            // Search across all projects
            if let Ok(entries) = fs::read_dir(root_path) {
                for entry in entries.flatten() {
                    let path = entry.path();
                    if path.is_dir() && !path.file_name().unwrap_or_default().to_string_lossy().starts_with('.') {
                        let project_folder = path.file_name().unwrap().to_string_lossy().to_string();

                        if let Ok(project_entries) = fs::read_dir(&path) {
                            for project_entry in project_entries.flatten() {
                                let task_path = project_entry.path();
                                if task_path.is_file() && task_path.extension().map_or(false, |ext| ext == "yml") {
                                    // Extract task ID from filename
                                    if let Some(stem) = task_path.file_stem().and_then(|s| s.to_str()) {
                                        if let Ok(numeric_id) = stem.parse::<u64>() {
                                            let task_id = format!("{}-{}", project_folder, numeric_id);

                                            // If we have tag filters, check if this task matches
                                            if !filter.tags.is_empty() && !tag_candidates.contains(&task_id) {
                                                continue;
                                            }

                                            // Load and filter the task
                                            if let Ok(content) = fs::read_to_string(&task_path) {
                                                if let Ok(task) = serde_yaml::from_str::<Task>(&content) {
                                                    if Self::task_matches_filter(&task, filter) {
                                                        results.push((task_id, task));
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }

        results
    }

    /// List all tasks for a specific project
    pub fn list_by_project(
        root_path: &Path,
        index: &TaskIndex,
        project: &str,
    ) -> Vec<(String, Task)> {
        let filter = TaskFilter {
            project: Some(project.to_string()),
            ..Default::default()
        };
        Self::search(root_path, index, &filter)
    }

    /// Helper method to get potential project folders for a given project name
    pub fn get_project_folders_for_name(root_path: &Path, project_name: &str) -> Vec<String> {
        let mut folders = Vec::new();

        // First, try the project name as-is
        if root_path.join(project_name).exists() {
            folders.push(project_name.to_string());
        }

        // Then try to find folders that might correspond to this project
        if let Ok(entries) = fs::read_dir(root_path) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_dir() {
                    let folder_name = path.file_name().unwrap().to_string_lossy().to_string();
                    if !folder_name.starts_with('.') && folder_name != project_name {
                        // Could check metadata or use heuristics here
                        folders.push(folder_name);
                    }
                }
            }
        }

        folders
    }

    /// Helper method to check if a task matches all filter criteria
    pub fn task_matches_filter(task: &Task, filter: &TaskFilter) -> bool {
        // Check status filter
        if let Some(status) = &filter.status {
            if task.status != *status {
                return false;
            }
        }

        // Check priority filter
        if let Some(priority) = filter.priority {
            if task.priority != priority {
                return false;
            }
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

        // Tag filtering is handled at the index level before this method is called

        true
    }

    /// Check if a task matches text filter criteria
    pub fn matches_text_filter(task: &Task, text_query: &Option<String>) -> bool {
        if let Some(query) = text_query {
            let query_lower = query.to_lowercase();
            task.title.to_lowercase().contains(&query_lower) ||
            task.subtitle.as_ref().map_or(false, |s| s.to_lowercase().contains(&query_lower)) ||
            task.description.as_ref().map_or(false, |s| s.to_lowercase().contains(&query_lower)) ||
            task.tags.iter().any(|tag| tag.to_lowercase().contains(&query_lower))
        } else {
            true
        }
    }
}

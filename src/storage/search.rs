use crate::storage::TaskFilter;
use crate::storage::task::Task;
use std::fs;
use std::path::Path;

/// Search and filtering functionality for task storage
pub struct StorageSearch;

impl StorageSearch {
    /// Search for tasks based on filter criteria
    pub fn search(root_path: &Path, filter: &TaskFilter) -> Vec<(String, Task)> {
        let mut results = Vec::new();

        // No longer use index for tag pre-filtering - do all filtering during file scan

        // If we have a specific project filter, search only that project
        if let Some(project) = &filter.project {
            // Try to find the project folder (could be the project name itself or a mapped prefix)
            let project_folders = Self::get_project_folders_for_name(root_path, project);

            for project_folder in project_folders {
                let project_path = root_path.join(&project_folder);
                if let Ok(entries) = fs::read_dir(&project_path) {
                    for entry in entries.flatten() {
                        let path = entry.path();
                        if path.is_file() && path.extension().is_some_and(|ext| ext == "yml") {
                            // Extract task ID from filename
                            if let Some(stem) = path.file_stem().and_then(|s| s.to_str()) {
                                if let Ok(numeric_id) = stem.parse::<u64>() {
                                    let task_id = format!("{}-{}", project_folder, numeric_id);

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
                    if path.is_dir()
                        && !path
                            .file_name()
                            .unwrap_or_default()
                            .to_string_lossy()
                            .starts_with('.')
                    {
                        let project_folder =
                            path.file_name().unwrap().to_string_lossy().to_string();

                        if let Ok(project_entries) = fs::read_dir(&path) {
                            for project_entry in project_entries.flatten() {
                                let task_path = project_entry.path();
                                if task_path.is_file()
                                    && task_path.extension().is_some_and(|ext| ext == "yml")
                                {
                                    // Extract task ID from filename
                                    if let Some(stem) =
                                        task_path.file_stem().and_then(|s| s.to_str())
                                    {
                                        if let Ok(numeric_id) = stem.parse::<u64>() {
                                            let task_id =
                                                format!("{}-{}", project_folder, numeric_id);

                                            // Load and filter the task
                                            if let Ok(content) = fs::read_to_string(&task_path) {
                                                if let Ok(task) =
                                                    serde_yaml::from_str::<Task>(&content)
                                                {
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

use std::fs;
use std::path::{Path, PathBuf};
use crate::storage::task::Task;
use crate::index::TaskIndex;
use crate::utils::generate_project_prefix;

/// Core CRUD operations for task storage
pub struct StorageOperations;

impl StorageOperations {
    /// Add a new task to storage
    pub fn add(
        root_path: &Path,
        index: &mut TaskIndex,
        task: &Task,
    ) -> Result<String, Box<dyn std::error::Error>> {
        // For new architecture, we need to determine the prefix first
        let desired_prefix = if task.project.trim().is_empty() {
            "DEFAULT".to_string()
        } else {
            // Generate a prefix for this project if it doesn't exist as a folder
            Self::get_or_create_project_prefix(root_path, &task.project)?
        };

        // The project folder name IS the prefix
        let project_folder = desired_prefix.clone();
        let project_path = root_path.join(&project_folder);

        // Ensure project directory exists
        fs::create_dir_all(&project_path)?;

        // Get the next numeric ID by finding the highest existing ID
        let next_numeric_id = Self::get_current_id(&project_path) + 1;

        // Create the formatted ID for external use
        let formatted_id = format!("{}-{}", project_folder, next_numeric_id);

        // Create a mutable copy of the task and update project
        let mut task_to_store = task.clone();
        task_to_store.project = project_folder.clone();

        // Get file path using the numeric ID
        let file_path = Self::get_file_path(&project_folder, next_numeric_id, root_path);
        let file_string = serde_yaml::to_string(&task_to_store)?;
        fs::create_dir_all(file_path.parent().unwrap())?;
        fs::write(&file_path, file_string)?;

        // Update index
        let relative_path = file_path.strip_prefix(root_path)
            .unwrap_or(&file_path)
            .to_string_lossy()
            .to_string();
        index.add_task_with_id(&formatted_id, &task_to_store, &relative_path);

        Ok(formatted_id)
    }

    /// Get a task by ID
    pub fn get(root_path: &Path, id: &str, project: String) -> Option<Task> {
        // Extract project folder from the task ID if provided
        if let Some(folder_from_id) = Self::get_project_for_task(id) {
            // SECURITY: Enforce project isolation - verify the project folder from ID matches the provided project
            let project_name = if project.trim().is_empty() {
                "default".to_string()
            } else {
                project.clone()
            };

            // If the project folder extracted from ID doesn't match the provided project, deny access
            if folder_from_id != project_name {
                return None;
            }

            // Use filesystem-based file path resolution
            let project_path = root_path.join(&folder_from_id);
            if let Some(file_path) = Self::get_file_path_for_id(&project_path, id) {
                if let Ok(file_string) = fs::read_to_string(&file_path) {
                    if let Ok(task) = serde_yaml::from_str::<Task>(&file_string) {
                        return Some(task);
                    }
                }
            }
        }

        // Fallback: try the provided project name (for backward compatibility)
        let project_name = if project.trim().is_empty() {
            "default".to_string()
        } else {
            project
        };

        let project_path = root_path.join(&project_name);
        if let Some(file_path) = Self::get_file_path_for_id(&project_path, id) {
            if let Ok(file_string) = fs::read_to_string(&file_path) {
                if let Ok(task) = serde_yaml::from_str::<Task>(&file_string) {
                    return Some(task);
                }
            }
        }

        None
    }

    /// Edit an existing task
    pub fn edit(
        root_path: &Path,
        index: &mut TaskIndex,
        id: &str,
        new_task: &Task,
    ) -> Result<(), Box<dyn std::error::Error>> {
        // Get old task for index update and determine the actual project folder
        let old_task = Self::get(root_path, id, new_task.project.clone());

        // Extract the project folder from the task ID
        let project_folder = match Self::get_project_for_task(id) {
            Some(folder) => folder,
            None => return Err("Invalid task ID format".into()),
        };

        let project_path = root_path.join(&project_folder);

        // Use filesystem-based file path resolution
        let file_path = match Self::get_file_path_for_id(&project_path, id) {
            Some(path) => path,
            None => return Err("Task file not found".into()),
        };

        // Update the task's project field to match the actual folder
        let mut task_to_save = new_task.clone();
        task_to_save.project = project_folder.clone();

        let file_string = serde_yaml::to_string(&task_to_save)?;
        fs::write(&file_path, file_string)?;

        // Update index using new method with explicit ID
        if let Some(old) = old_task {
            let relative_path = file_path.strip_prefix(root_path)
                .unwrap_or(&file_path)
                .to_string_lossy()
                .to_string();
            index.update_task_with_id(id, &old, &task_to_save, &relative_path);
        }

        Ok(())
    }

    /// Delete a task
    pub fn delete(
        root_path: &Path,
        index: &mut TaskIndex,
        id: &str,
        project: String,
    ) -> Result<bool, Box<dyn std::error::Error>> {
        // Get task for index update
        let task = Self::get(root_path, id, project.clone());

        let project_path = root_path.join(&project);

        // Use filesystem-based file path resolution
        let file_path = match Self::get_file_path_for_id(&project_path, id) {
            Some(path) => path,
            None => return Ok(false), // Task file not found
        };

        match fs::remove_file(file_path) {
            Ok(_) => {
                // Update index using new method with explicit ID
                if let Some(t) = task {
                    index.remove_task_with_id(id, &t);
                }
                Ok(true)
            }
            Err(_) => Ok(false)
        }
    }

    /// Get file path for a task
    pub fn get_file_path(project_folder: &str, numeric_id: u64, root_path: &Path) -> PathBuf {
        let mut file_path = root_path.to_path_buf();
        file_path.push(project_folder);
        file_path.push(format!("{}.yml", numeric_id));
        file_path
    }

    /// Get the file path for a task ID (relative to tasks root)
    pub fn get_file_path_for_id(project_path: &Path, task_id: &str) -> Option<PathBuf> {
        // Extract numeric part from task ID (e.g., "TP-001" -> "1")
        let parts: Vec<&str> = task_id.split('-').collect();
        if parts.len() >= 2 {
            if let Ok(numeric_id) = parts[1].parse::<u64>() {
                let file_path = project_path.join(format!("{}.yml", numeric_id));
                if file_path.exists() {
                    return Some(file_path);
                }
            }
        }
        None
    }

    /// Get the current highest task ID by scanning the project directory
    pub fn get_current_id(project_path: &Path) -> u64 {
        if let Ok(entries) = fs::read_dir(project_path) {
            entries
                .filter_map(|entry| entry.ok())
                .filter_map(|entry| {
                    let file_name = entry.file_name();
                    let name_str = file_name.to_string_lossy();
                    if name_str.ends_with(".yml") {
                        name_str.strip_suffix(".yml")?.parse::<u64>().ok()
                    } else {
                        None
                    }
                })
                .max()
                .unwrap_or(0)
        } else {
            0
        }
    }

    /// Get the actual project folder name for a given task ID
    pub fn get_project_for_task(task_id: &str) -> Option<String> {
        // Extract the prefix from the task ID (e.g., "STAT-001" -> "STAT")
        task_id.split('-').next().map(|s| s.to_string())
    }

    /// Get or create a project prefix, ensuring it's unique and consistent
    pub fn get_or_create_project_prefix(root_path: &Path, project_name: &str) -> Result<String, String> {
        // Check if we already have a folder for this exact project name
        let direct_path = root_path.join(project_name);
        if direct_path.exists() && direct_path.is_dir() {
            return Ok(project_name.to_string());
        }

        // Check existing project folders to avoid collisions
        if let Ok(entries) = fs::read_dir(root_path) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_dir() && !path.file_name().unwrap_or_default().to_string_lossy().starts_with('.') {
                    // This folder already exists, so generate a different prefix
                    continue;
                }
            }
        }

        // Generate a new prefix for this project name
        Self::generate_unique_folder_prefix(root_path, project_name)
    }

    /// Generate a unique folder name (prefix) for a project
    pub fn generate_unique_folder_prefix(root_path: &Path, project_name: &str) -> Result<String, String> {
        // Generate candidate prefix using the shared utility
        let candidate = generate_project_prefix(project_name);

        // Check if this folder name is available
        if !root_path.join(&candidate).exists() {
            return Ok(candidate);
        }

        // If collision, try variations with numbers
        for i in 1..=99 {
            let candidate_with_number = if candidate.len() >= 4 {
                format!("{}{:02}", &candidate[..2], i)
            } else {
                format!("{}{}", candidate, i)
            };

            if !root_path.join(&candidate_with_number).exists() {
                return Ok(candidate_with_number);
            }
        }

        Err(format!("Could not generate unique prefix for project '{}'", project_name))
    }
}

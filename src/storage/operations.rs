use crate::config::{ConfigManager, types::ProjectConfig};
use crate::storage::task::Task;
use crate::utils::generate_project_prefix;
use std::fs;
use std::path::{Path, PathBuf};

/// Core CRUD operations for task storage
pub struct StorageOperations;

impl StorageOperations {
    /// Add a new task to storage
    pub fn add(
        root_path: &Path,
        task: &Task,
        project_prefix: &str,
        original_project_name: Option<&str>,
    ) -> Result<String, Box<dyn std::error::Error>> {
        // Use the provided project prefix
        let project_folder = project_prefix.to_string();
        let project_path = root_path.join(&project_folder);

        // Use original project name for config initialization, fall back to prefix
        let config_project_name = original_project_name.unwrap_or(project_prefix);

        // Ensure project directory exists
        fs::create_dir_all(&project_path)?;

        // Create project config.yml if it doesn't exist and we have a project name
        let config_file_path = project_path.join("config.yml");
        if !config_file_path.exists() && original_project_name.is_some() {
            // Create a basic project config with the project name
            let project_config = ProjectConfig::new(config_project_name.to_string());

            // Save the project config
            if let Err(e) =
                ConfigManager::save_project_config(root_path, project_prefix, &project_config)
            {
                eprintln!("Warning: Failed to create project config: {}", e);
                // Continue execution - this is not a fatal error
            }
        }

        // Get the next numeric ID by finding the highest existing ID
        let next_numeric_id = Self::get_current_id(&project_path) + 1;

        // Create the formatted ID for external use
        let formatted_id = format!("{}-{}", project_folder, next_numeric_id);

        // Create a mutable copy of the task (no project field to set)
        let task_to_store = task.clone();

        // Get file path using the numeric ID
        let file_path = Self::get_file_path(&project_folder, next_numeric_id, root_path);
        let file_string = serde_yaml::to_string(&task_to_store)?;
        fs::create_dir_all(file_path.parent().unwrap())?;
        fs::write(&file_path, file_string)?;

        // No longer need to update index - simplified architecture

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
        id: &str,
        new_task: &Task,
    ) -> Result<(), Box<dyn std::error::Error>> {
        // Extract the project folder from the task ID
        let project_folder = match Self::get_project_for_task(id) {
            Some(folder) => folder,
            None => return Err("Invalid task ID format".into()),
        };

        // Get old task for index update
        let _old_task = Self::get(root_path, id, project_folder.clone());

        let project_path = root_path.join(&project_folder);

        // Use filesystem-based file path resolution
        let file_path = match Self::get_file_path_for_id(&project_path, id) {
            Some(path) => path,
            None => return Err("Task file not found".into()),
        };

        // Save the task (no project field to update)
        let task_to_save = new_task.clone();

        let file_string = serde_yaml::to_string(&task_to_save)?;
        fs::write(&file_path, file_string)?;

        // No longer need to update index - simplified architecture

        Ok(())
    }

    /// Delete a task
    pub fn delete(
        root_path: &Path,
        id: &str,
        project: String,
    ) -> Result<bool, Box<dyn std::error::Error>> {
        let project_path = root_path.join(&project);

        // Use filesystem-based file path resolution
        let file_path = match Self::get_file_path_for_id(&project_path, id) {
            Some(path) => path,
            None => return Ok(false), // Task file not found
        };

        match fs::remove_file(file_path) {
            Ok(_) => {
                // No longer need to update index - simplified architecture
                Ok(true)
            }
            Err(_) => Ok(false),
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
    #[allow(dead_code)]
    pub fn get_or_create_project_prefix(
        root_path: &Path,
        project_name: &str,
    ) -> Result<String, String> {
        // Check if we already have a folder for this exact project name
        let direct_path = root_path.join(project_name);
        if direct_path.exists() && direct_path.is_dir() {
            return Ok(project_name.to_string());
        }

        // Generate the expected prefix for this project name
        let expected_prefix = generate_project_prefix(project_name);

        // Check if a folder with this prefix already exists
        let prefix_path = root_path.join(&expected_prefix);
        if prefix_path.exists() && prefix_path.is_dir() {
            // Verify this is for the same project by checking config
            let config_path = prefix_path.join("config.yml");
            if config_path.exists() {
                if let Ok(content) = fs::read_to_string(&config_path) {
                    if let Ok(config) =
                        serde_yaml::from_str::<crate::config::types::ProjectConfig>(&content)
                    {
                        // Check if the project name in config matches (either exact or prefix)
                        if config.project_name == project_name
                            || config.project_name == expected_prefix
                        {
                            return Ok(expected_prefix);
                        }
                    }
                }
            }
        }

        // If no existing folder found, generate a new unique prefix
        Self::generate_unique_folder_prefix(root_path, project_name)
    }

    /// Generate a unique folder name (prefix) for a project
    #[allow(dead_code)]
    pub fn generate_unique_folder_prefix(
        root_path: &Path,
        project_name: &str,
    ) -> Result<String, String> {
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

        Err(format!(
            "Could not generate unique prefix for project '{}'",
            project_name
        ))
    }
}

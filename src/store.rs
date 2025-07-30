use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use serde::{Serialize, Deserialize};
use std::fmt;
use crate::types::{TaskStatus, TaskType, Priority, TaskRelationships, TaskComment, CustomFields};
use crate::index::{TaskIndex, TaskFilter};


#[derive(Clone, Serialize, Deserialize)]
pub struct Task {
    // Built-in standard fields (special handling in UI)
    // Note: ID is no longer stored in file - it's derived from folder+filename
    pub title: String,
    #[serde(skip_serializing_if = "TaskStatus::is_default", default)]
    pub status: TaskStatus,
    #[serde(skip_serializing_if = "Priority::is_default", default)]
    pub priority: Priority,
    #[serde(skip_serializing_if = "TaskType::is_default", default)]
    pub task_type: TaskType,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub assignee: Option<String>,
    pub project: String,
    pub created: String,
    pub modified: String,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub due_date: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub effort: Option<String>, // e.g., "5d", "2w", "3h"

    // Built-in structured fields (special UI components)
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub acceptance_criteria: Vec<String>,
    #[serde(skip_serializing_if = "TaskRelationships::is_empty", default)]
    pub relationships: TaskRelationships,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub comments: Vec<TaskComment>,

    // Legacy fields (keeping for backward compatibility)
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub subtitle: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub category: Option<String>,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub tags: Vec<String>,

    // Team-specific custom fields (generic UI treatment based on type)
    #[serde(skip_serializing_if = "std::collections::HashMap::is_empty", default)]
    pub custom_fields: CustomFields,
}

impl Task {
    pub fn new(_root_path: PathBuf, title: String, project: String, priority: Priority) -> Self {
        let now = chrono::Utc::now().to_rfc3339();

        Self {
            title,
            status: TaskStatus::default(),
            priority,
            task_type: TaskType::default(),
            assignee: None,
            project,
            created: now.clone(),
            modified: now,
            due_date: None,
            effort: None,
            acceptance_criteria: vec![],
            relationships: TaskRelationships::default(),
            comments: vec![],
            subtitle: None,
            description: None,
            category: None,
            tags: vec![],
            custom_fields: HashMap::new(),
        }
    }

    pub fn update_status(&mut self, new_status: TaskStatus) -> Result<(), String> {
        // TODO: Add transition validation once we implement transitions.yaml
        self.status = new_status;
        self.modified = chrono::Utc::now().to_rfc3339();
        Ok(())
    }

    pub fn update_modified(&mut self) {
        self.modified = chrono::Utc::now().to_rfc3339();
    }
}

impl fmt::Display for Task {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "title: {}\nstatus: {}\nsubtitle: {:?}\ndescription: {:?}\npriority: {}\nproject: {}\ncategory: {:?}\ncreated: {}\nmodified: {}\ndue_date: {:?}\ntags: {:?}",
               self.title, self.status, self.subtitle, self.description, self.priority, self.project, self.category, self.created, self.modified, self.due_date, self.tags)
    }
}

pub struct Storage {
    pub root_path: PathBuf,
    index: TaskIndex,
}

impl Storage {
    pub fn new(root_path: PathBuf) -> Self {
        fs::create_dir_all(&root_path).unwrap();

        // Load or create index - changed from index.json to index.yml
        let index_path = root_path.join("index.yml");
        let index = TaskIndex::load_from_file(&index_path)
            .unwrap_or_else(|_| TaskIndex::new());

        Self { root_path, index }
    }

    fn save_index(&self) -> Result<(), Box<dyn std::error::Error>> {
        let index_path = self.root_path.join("index.yml"); // Changed from .json to .yml
        self.index.save_to_file(&index_path)
    }

    fn get_file_path(&self, project_folder: &str, numeric_id: u64) -> PathBuf {
        let mut file_path = self.root_path.clone();
        file_path.push(project_folder);
        file_path.push(format!("{}.yml", numeric_id));
        file_path
    }

    pub fn add(&mut self, task: &Task) -> String {
        // For new architecture, we need to determine the prefix first
        let desired_prefix = if task.project.trim().is_empty() {
            "DEFAULT".to_string()
        } else {
            // Generate a prefix for this project if it doesn't exist as a folder
            self.get_or_create_project_prefix(&task.project).unwrap_or_else(|_| "TASK".to_string())
        };

        // The project folder name IS the prefix
        let project_folder = desired_prefix.clone();
        let project_path = self.root_path.join(&project_folder);

        // Ensure project directory exists
        fs::create_dir_all(&project_path).unwrap();

        // Get the next numeric ID by finding the highest existing ID
        let next_numeric_id = self.get_current_id(&project_path) + 1;

        // Create the formatted ID for external use
        let formatted_id = format!("{}-{}", project_folder, next_numeric_id);

        // Create a mutable copy of the task and update project
        let mut task_to_store = task.clone();
        task_to_store.project = project_folder.clone();

        // Get file path using the numeric ID
        let file_path = self.get_file_path(&project_folder, next_numeric_id);
        let file_string = serde_yaml::to_string(&task_to_store).unwrap();
        fs::create_dir_all(file_path.parent().unwrap()).unwrap();
        fs::write(&file_path, file_string).unwrap();

        // Update index
        let relative_path = file_path.strip_prefix(&self.root_path)
            .unwrap_or(&file_path)
            .to_string_lossy()
            .to_string();
        self.index.add_task_with_id(&formatted_id, &task_to_store, &relative_path);
        let _ = self.save_index();

        formatted_id
    }

    /// Get the actual project folder name for a given task ID
    pub fn get_project_for_task(&self, task_id: &str) -> Option<String> {
        // Extract the prefix from the task ID (e.g., "STAT-001" -> "STAT")
        task_id.split('-').next().map(|s| s.to_string())
    }

    /// Get or create a project prefix, ensuring it's unique and consistent
    fn get_or_create_project_prefix(&self, project_name: &str) -> Result<String, String> {
        // Check if we already have a folder for this exact project name
        let direct_path = self.root_path.join(project_name);
        if direct_path.exists() && direct_path.is_dir() {
            return Ok(project_name.to_string());
        }

        // Check existing project folders to avoid collisions
        if let Ok(entries) = fs::read_dir(&self.root_path) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_dir() && !path.file_name().unwrap_or_default().to_string_lossy().starts_with('.') {
                    // This folder already exists, so generate a different prefix
                    continue;
                }
            }
        }

        // Generate a new prefix for this project name
        self.generate_unique_folder_prefix(project_name)
    }

    /// Generate a unique folder name (prefix) for a project
    fn generate_unique_folder_prefix(&self, project_name: &str) -> Result<String, String> {
        // Generate candidate prefix using the same logic as before
        let candidate = if project_name.len() <= 4 {
            project_name.to_uppercase()
        } else {
            let normalized = project_name.to_uppercase();
            if normalized.contains('-') || normalized.contains('_') {
                // For hyphenated/underscored names, take first letters of each word
                normalized.split(&['-', '_'][..])
                    .filter_map(|word| word.chars().next())
                    .take(4)
                    .collect::<String>()
            } else {
                // For single words, take first 4 characters
                normalized.chars().take(4).collect::<String>()
            }
        };

        // Check if this folder name is available
        if !self.root_path.join(&candidate).exists() {
            return Ok(candidate);
        }

        // If collision, try variations with numbers
        for i in 1..=99 {
            let candidate_with_number = if candidate.len() >= 4 {
                format!("{}{:02}", &candidate[..2], i)
            } else {
                format!("{}{}", candidate, i)
            };

            if !self.root_path.join(&candidate_with_number).exists() {
                return Ok(candidate_with_number);
            }
        }

        Err(format!("Could not generate unique prefix for project '{}'", project_name))
    }

    pub fn edit(&mut self, id: &str, new_task: &Task) {
        // Get old task for index update and determine the actual project folder
        let old_task = self.get(id, new_task.project.clone());

        // Extract the project folder from the task ID
        let project_folder = match self.get_project_for_task(id) {
            Some(folder) => folder,
            None => return, // Invalid task ID format
        };

        let project_path = self.root_path.join(&project_folder);

        // Use filesystem-based file path resolution
        let file_path = match self.get_file_path_for_id(&project_path, id) {
            Some(path) => path,
            None => return, // Task file not found
        };

        // Update the task's project field to match the actual folder
        let mut task_to_save = new_task.clone();
        task_to_save.project = project_folder.clone();

        let file_string = serde_yaml::to_string(&task_to_save).unwrap();
        fs::write(&file_path, file_string).unwrap();

        // Update index using new method with explicit ID
        if let Some(old) = old_task {
            let relative_path = file_path.strip_prefix(&self.root_path)
                .unwrap_or(&file_path)
                .to_string_lossy()
                .to_string();
            self.index.update_task_with_id(id, &old, &task_to_save, &relative_path);
            let _ = self.save_index();
        }
    }

    pub fn delete(&mut self, id: &str, project: String) -> bool {
        // Get task for index update
        let task = self.get(id, project.clone());

        let project_path = self.root_path.join(&project);

        // Use filesystem-based file path resolution
        let file_path = match self.get_file_path_for_id(&project_path, id) {
            Some(path) => path,
            None => return false, // Task file not found
        };

        match fs::remove_file(file_path) {
            Ok(_) => {
                // Update index using new method with explicit ID
                if let Some(t) = task {
                    self.index.remove_task_with_id(id, &t);
                    let _ = self.save_index();
                }
                true
            }
            Err(_) => false
        }
    }

    pub fn search(&self, filter: &TaskFilter) -> Vec<(String, Task)> {
        let mut results = Vec::new();

        // If we have tag filters, use the index to get candidate task IDs
        let tag_candidates = if !filter.tags.is_empty() {
            self.index.find_by_filter(filter)
        } else {
            Vec::new()
        };

        // If we have a specific project filter, search only that project
        if let Some(project) = &filter.project {
            // Try to find the project folder (could be the project name itself or a mapped prefix)
            let project_folders = self.get_project_folders_for_name(project);

            for project_folder in project_folders {
                let project_path = self.root_path.join(&project_folder);
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
                                            if self.task_matches_filter(&task, filter) {
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
            if let Ok(entries) = fs::read_dir(&self.root_path) {
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
                                                    if self.task_matches_filter(&task, filter) {
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
    fn get_project_folders_for_name(&self, project_name: &str) -> Vec<String> {
        let mut folders = Vec::new();

        // First, try the project name as-is
        if self.root_path.join(project_name).exists() {
            folders.push(project_name.to_string());
        }

        // Then try to find folders that might correspond to this project
        if let Ok(entries) = fs::read_dir(&self.root_path) {
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
    fn task_matches_filter(&self, task: &Task, filter: &TaskFilter) -> bool {
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
        if !self.matches_text_filter(task, &filter.text_query) {
            return false;
        }

        // Tag filtering is handled at the index level before this method is called

        true
    }

    pub fn get(&self, id: &str, project: String) -> Option<Task> {
        // Extract project folder from the task ID if provided
        if let Some(folder_from_id) = self.get_project_for_task(id) {
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
            let project_path = self.root_path.join(&folder_from_id);
            if let Some(file_path) = self.get_file_path_for_id(&project_path, id) {
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

        let project_path = self.root_path.join(&project_name);
        if let Some(file_path) = self.get_file_path_for_id(&project_path, id) {
            if let Ok(file_string) = fs::read_to_string(&file_path) {
                if let Ok(task) = serde_yaml::from_str::<Task>(&file_string) {
                    return Some(task);
                }
            }
        }

        None
    }

    pub fn list_by_project(&self, project: &str) -> Vec<(String, Task)> {
        let filter = TaskFilter {
            project: Some(project.to_string()),
            ..Default::default()
        };
        self.search(&filter)
    }

    fn matches_text_filter(&self, task: &Task, text_query: &Option<String>) -> bool {
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

    pub fn rebuild_index(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        self.index = TaskIndex::rebuild_from_storage(&self.root_path)?;
        self.save_index()?;
        Ok(())
    }

    /// Set a custom prefix for a project by renaming the folder
    pub fn set_project_prefix(&mut self, old_project: &str, desired_prefix: &str) -> Result<(), String> {
        let desired_prefix = desired_prefix.to_uppercase().trim().to_string();

        // Validate prefix format
        if desired_prefix.is_empty() {
            return Err("Prefix cannot be empty".to_string());
        }

        if desired_prefix.len() > 6 {
            return Err("Prefix cannot be longer than 6 characters".to_string());
        }

        if !desired_prefix.chars().all(|c| c.is_ascii_alphanumeric()) {
            return Err("Prefix must contain only alphanumeric characters".to_string());
        }

        // Check if target folder already exists
        let target_path = self.root_path.join(&desired_prefix);
        if target_path.exists() {
            return Err(format!("Prefix '{}' is already in use", desired_prefix));
        }

        // Check if source project exists
        let source_path = self.root_path.join(old_project);
        if !source_path.exists() {
            return Err(format!("Project '{}' not found", old_project));
        }

        // Simply rename the folder - that's it! All task IDs automatically become valid
        // because the system derives the prefix from the folder name
        if let Err(e) = fs::rename(&source_path, &target_path) {
            return Err(format!("Failed to rename project folder: {}", e));
        }

        // Update only the task content to reflect the new project name (for consistency)
        self.update_task_project_field(&target_path, &desired_prefix)?;

        // Rebuild the index since folder paths have changed
        let _ = self.rebuild_index();

        Ok(())
    }

    /// Update task files to have the correct project field (but not filenames!)
    fn update_task_project_field(&self, project_path: &Path, new_prefix: &str) -> Result<(), String> {
        let entries = match fs::read_dir(project_path) {
            Ok(entries) => entries,
            Err(e) => return Err(format!("Failed to read project directory: {}", e)),
        };

        for entry in entries.flatten() {
            let file_path = entry.path();
            if file_path.extension().map_or(false, |ext| ext == "yml") {
                // Read the task
                if let Ok(file_content) = fs::read_to_string(&file_path) {
                    if let Ok(mut task) = serde_yaml::from_str::<Task>(&file_content) {
                        // Update only the project field - the ID stays the same structurally
                        // but gets its prefix from the folder name context
                        task.project = new_prefix.to_string();

                        // Write back the updated task (same filename)
                        if let Ok(updated_content) = serde_yaml::to_string(&task) {
                            let _ = fs::write(&file_path, updated_content);
                        }
                    }
                }
            }
        }

        Ok(())
    }

    /// Get the task count by counting .yml files in the project directory
    pub fn get_task_count(&self, project_path: &Path) -> u64 {
        if let Ok(entries) = fs::read_dir(project_path) {
            entries
                .filter_map(|entry| entry.ok())
                .filter(|entry| {
                    entry.path().is_file() &&
                    entry.path().extension().map_or(false, |ext| ext == "yml")
                })
                .count() as u64
        } else {
            0
        }
    }

    /// Get the current highest task ID by scanning the project directory
    pub fn get_current_id(&self, project_path: &Path) -> u64 {
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

    /// Get the file path for a task ID (relative to tasks root)
    pub fn get_file_path_for_id(&self, project_path: &Path, task_id: &str) -> Option<PathBuf> {
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
}

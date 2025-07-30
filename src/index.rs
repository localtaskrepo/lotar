use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use serde::{Serialize, Deserialize};
use crate::types::{TaskStatus, Priority};
use crate::store::{Task};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskFilter {
    pub status: Option<TaskStatus>,
    pub priority: Option<Priority>,  // Changed from u8 to Priority enum
    pub project: Option<String>,
    pub category: Option<String>,
    pub tags: Vec<String>,
    pub text_query: Option<String>,
}

impl Default for TaskFilter {
    fn default() -> Self {
        Self {
            status: None,
            priority: None,
            project: None,
            category: None,
            tags: vec![],
            text_query: None,
        }
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct TaskIndex {
    // Global cross-project indexes only
    pub tag2id: HashMap<String, Vec<String>>,
    pub last_updated: String,
}

impl TaskIndex {
    pub fn new() -> Self {
        Self {
            tag2id: HashMap::new(),
            last_updated: chrono::Utc::now().to_rfc3339(),
        }
    }

    pub fn add_task_with_id(&mut self, task_id: &str, task: &Task, _file_path: &str) {
        // Only update tag index (global cross-project data)
        for tag in &task.tags {
            self.tag2id.entry(tag.clone())
                .or_insert_with(Vec::new)
                .push(task_id.to_string());
        }

        self.last_updated = chrono::Utc::now().to_rfc3339();
    }

    pub fn remove_task_with_id(&mut self, task_id: &str, task: &Task) {
        // Remove from tag index only
        for tag in &task.tags {
            if let Some(ids) = self.tag2id.get_mut(tag) {
                ids.retain(|id| id != task_id);
                if ids.is_empty() {
                    self.tag2id.remove(tag);
                }
            }
        }

        self.last_updated = chrono::Utc::now().to_rfc3339();
    }

    pub fn update_task_with_id(&mut self, task_id: &str, old_task: &Task, new_task: &Task, file_path: &str) {
        self.remove_task_with_id(task_id, old_task);
        self.add_task_with_id(task_id, new_task, file_path);
    }

    pub fn find_by_filter(&self, filter: &TaskFilter) -> Vec<String> {
        // With the simplified index, we only handle tag filtering here
        // Other filters (status, priority, project) will be handled at the storage level
        let mut candidates: Option<Vec<String>> = None;

        // Filter by tags (intersection)
        for tag in &filter.tags {
            let tag_ids = self.tag2id.get(tag)
                .map(|ids| ids.clone())
                .unwrap_or_default();
            candidates = Some(match candidates {
                Some(existing) => existing.into_iter()
                    .filter(|id| tag_ids.contains(id))
                    .collect(),
                None => tag_ids,
            });
        }

        // If no tag filters, return empty vec - let storage handle other filters
        candidates.unwrap_or_default()
    }

    pub fn save_to_file(&self, path: &Path) -> Result<(), Box<dyn std::error::Error>> {
        let content = serde_yaml::to_string(self)?; // Changed from JSON to YAML
        fs::write(path, content)?;
        Ok(())
    }

    pub fn load_from_file(path: &Path) -> Result<Self, Box<dyn std::error::Error>> {
        if !path.exists() {
            return Ok(Self::new());
        }
        let content = fs::read_to_string(path)?;
        let index = serde_yaml::from_str(&content)?; // Changed from JSON to YAML
        Ok(index)
    }

    pub fn rebuild_from_storage(root_path: &PathBuf) -> Result<Self, Box<dyn std::error::Error>> {
        let mut index = Self::new();

        // Walk through all project directories
        if !root_path.exists() {
            return Ok(index);
        }

        for entry in fs::read_dir(root_path)? {
            let entry = entry?;
            let path = entry.path();

            if path.is_dir() {
                let project_name = path.file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or("")
                    .to_string();

                // Skip system directories
                if project_name.starts_with('.') {
                    continue;
                }

                // Walk through project directory looking for task files
                index.scan_project_directory(&path, &project_name, root_path)?;
            }
        }

        Ok(index)
    }

    fn scan_project_directory(&mut self, project_path: &Path, project_name: &str, root_path: &PathBuf) -> Result<(), Box<dyn std::error::Error>> {
        for entry in fs::read_dir(project_path)? {
            let entry = entry?;
            let path = entry.path();

            if path.is_file() {
                let file_name = path.file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or("");

                // Process task files (both .yaml and .yml) - removed metadata.yml skip since we're eliminating those files
                if file_name.ends_with(".yaml") || file_name.ends_with(".yml") {
                    if let Ok(content) = fs::read_to_string(&path) {
                        if let Ok(task) = serde_yaml::from_str::<Task>(&content) {
                            // Calculate relative path correctly from root_path
                            let relative_path = path.strip_prefix(root_path)
                                .unwrap_or(&path)
                                .to_string_lossy()
                                .to_string();

                            // Extract ID from folder+filename (e.g., AUTH/5.yml -> AUTH-5)
                            if let Some(stem) = path.file_stem().and_then(|s| s.to_str()) {
                                if let Ok(numeric_id) = stem.parse::<u64>() {
                                    let task_id = format!("{}-{}", project_name, numeric_id);
                                    self.add_task_with_id(&task_id, &task, &relative_path);
                                }
                            }
                        }
                    }
                } else if path.is_dir() {
                    // Recursively scan subdirectories (for categories)
                    self.scan_project_directory(&path, project_name, root_path)?;
                }
            }
        }

        Ok(())
    }
}

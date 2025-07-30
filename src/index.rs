use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use serde::{Serialize, Deserialize};
use crate::types::TaskStatus;
use crate::store::{Task};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskFilter {
    pub status: Option<TaskStatus>,
    pub priority: Option<u8>,
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
    pub id2file: HashMap<String, String>,
    pub tag2id: HashMap<String, Vec<String>>,
    pub status2id: HashMap<String, Vec<String>>,
    pub project2id: HashMap<String, Vec<String>>,
    pub priority2id: HashMap<String, Vec<String>>,
    pub last_updated: String,
}

impl TaskIndex {
    pub fn new() -> Self {
        Self {
            id2file: HashMap::new(),
            tag2id: HashMap::new(),
            status2id: HashMap::new(),
            project2id: HashMap::new(),
            priority2id: HashMap::new(),
            last_updated: chrono::Utc::now().to_rfc3339(),
        }
    }

    pub fn add_task(&mut self, task: &Task, file_path: &str) {
        let task_id = task.id.to_string();

        // Update id2file mapping
        self.id2file.insert(task_id.clone(), file_path.to_string());

        // Update tag index
        for tag in &task.tags {
            self.tag2id.entry(tag.clone())
                .or_insert_with(Vec::new)
                .push(task_id.clone());
        }

        // Update status index
        let status_key = task.status.to_string();
        self.status2id.entry(status_key)
            .or_insert_with(Vec::new)
            .push(task_id.clone());

        // Update project index
        self.project2id.entry(task.project.clone())
            .or_insert_with(Vec::new)
            .push(task_id.clone());

        // Update priority index
        let priority_key = task.priority.to_string();
        self.priority2id.entry(priority_key)
            .or_insert_with(Vec::new)
            .push(task_id.clone());

        self.last_updated = chrono::Utc::now().to_rfc3339();
    }

    pub fn remove_task(&mut self, task: &Task) {
        let task_id = task.id.to_string();

        // Remove from id2file
        self.id2file.remove(&task_id);

        // Remove from tag index
        for tag in &task.tags {
            if let Some(ids) = self.tag2id.get_mut(tag) {
                ids.retain(|id| id != &task_id);
                if ids.is_empty() {
                    self.tag2id.remove(tag);
                }
            }
        }

        // Remove from status index
        let status_key = task.status.to_string();
        if let Some(ids) = self.status2id.get_mut(&status_key) {
            ids.retain(|id| id != &task_id);
            if ids.is_empty() {
                self.status2id.remove(&status_key);
            }
        }

        // Remove from project index
        if let Some(ids) = self.project2id.get_mut(&task.project) {
            ids.retain(|id| id != &task_id);
            if ids.is_empty() {
                self.project2id.remove(&task.project);
            }
        }

        // Remove from priority index
        let priority_key = task.priority.to_string();
        if let Some(ids) = self.priority2id.get_mut(&priority_key) {
            ids.retain(|id| id != &task_id);
            if ids.is_empty() {
                self.priority2id.remove(&priority_key);
            }
        }

        self.last_updated = chrono::Utc::now().to_rfc3339();
    }

    pub fn update_task(&mut self, old_task: &Task, new_task: &Task, file_path: &str) {
        self.remove_task(old_task);
        self.add_task(new_task, file_path);
    }

    pub fn find_by_filter(&self, filter: &TaskFilter) -> Vec<String> {
        let mut candidates: Option<Vec<String>> = None;

        // Start with the most restrictive filter
        if let Some(status) = &filter.status {
            candidates = Some(self.status2id.get(&status.to_string())
                .map(|ids| ids.clone())
                .unwrap_or_default());
        }

        if let Some(project) = &filter.project {
            let project_ids = self.project2id.get(project)
                .map(|ids| ids.clone())
                .unwrap_or_default();
            candidates = Some(match candidates {
                Some(existing) => existing.into_iter()
                    .filter(|id| project_ids.contains(id))
                    .collect(),
                None => project_ids,
            });
        }

        if let Some(priority) = filter.priority {
            let priority_ids = self.priority2id.get(&priority.to_string())
                .map(|ids| ids.clone())
                .unwrap_or_default();
            candidates = Some(match candidates {
                Some(existing) => existing.into_iter()
                    .filter(|id| priority_ids.contains(id))
                    .collect(),
                None => priority_ids,
            });
        }

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

        candidates.unwrap_or_else(|| {
            // If no specific filters, return all task IDs
            self.id2file.keys().cloned().collect()
        })
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

                // Skip metadata files
                if file_name == "metadata.yml" {
                    continue;
                }

                // Process task files (both .yaml and .yml)
                if file_name.ends_with(".yaml") || file_name.ends_with(".yml") {
                    if let Ok(content) = fs::read_to_string(&path) {
                        if let Ok(task) = serde_yaml::from_str::<Task>(&content) {
                            // Fix: Calculate relative path correctly from root_path
                            let relative_path = path.strip_prefix(root_path)
                                .unwrap_or(&path)
                                .to_string_lossy()
                                .to_string();
                            self.add_task(&task, &relative_path);
                        }
                    }
                }
            } else if path.is_dir() {
                // Recursively scan subdirectories (for categories)
                self.scan_project_directory(&path, project_name, root_path)?;
            }
        }

        Ok(())
    }
}

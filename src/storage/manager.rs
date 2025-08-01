use crate::index::{TaskFilter, TaskIndex};
use crate::storage::operations::StorageOperations;
use crate::storage::search::StorageSearch;
use crate::storage::task::Task;
use std::fs;
use std::path::PathBuf;

/// Main storage manager that orchestrates all storage operations
pub struct Storage {
    pub root_path: PathBuf,
    index: TaskIndex,
}

impl Storage {
    pub fn new(root_path: PathBuf) -> Self {
        fs::create_dir_all(&root_path).unwrap();

        // Load or create index - changed from index.json to index.yml
        let index_path = root_path.join("index.yml");
        let index = TaskIndex::load_from_file(&index_path).unwrap_or_else(|_| TaskIndex::new());

        Self { root_path, index }
    }

    fn save_index(&self) -> Result<(), Box<dyn std::error::Error>> {
        let index_path = self.root_path.join("index.yml"); // Changed from .json to .yml
        self.index.save_to_file(&index_path)
    }

    pub fn add(&mut self, task: &Task, project_prefix: &str, original_project_name: Option<&str>) -> String {
        match StorageOperations::add(&self.root_path, &mut self.index, task, project_prefix, original_project_name) {
            Ok(formatted_id) => {
                let _ = self.save_index();
                formatted_id
            }
            Err(_) => "ERROR".to_string(), // TODO: Better error handling
        }
    }

    pub fn get(&self, id: &str, project: String) -> Option<Task> {
        StorageOperations::get(&self.root_path, id, project)
    }

    pub fn edit(&mut self, id: &str, new_task: &Task) {
        if let Ok(()) = StorageOperations::edit(&self.root_path, &mut self.index, id, new_task) {
            let _ = self.save_index();
        }
    }

    pub fn delete(&mut self, id: &str, project: String) -> bool {
        match StorageOperations::delete(&self.root_path, &mut self.index, id, project) {
            Ok(success) => {
                if success {
                    let _ = self.save_index();
                }
                success
            }
            Err(_) => false,
        }
    }

    pub fn search(&self, filter: &TaskFilter) -> Vec<(String, Task)> {
        StorageSearch::search(&self.root_path, &self.index, filter)
    }

    pub fn list_by_project(&self, project: &str) -> Vec<(String, Task)> {
        StorageSearch::list_by_project(&self.root_path, &self.index, project)
    }

    pub fn rebuild_index(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        self.index = TaskIndex::rebuild_from_storage(&self.root_path)?;
        self.save_index()?;
        Ok(())
    }
}

use std::path::Path;

use super::task::Task;

pub trait StorageBackend {
    fn add(
        &self,
        root: &Path,
        task: &Task,
        project: &str,
        original_project: Option<&str>,
    ) -> Result<String, Box<dyn std::error::Error>>;
    fn get(&self, root: &Path, id: &str, project: &str) -> Option<Task>;
    fn edit(&self, root: &Path, id: &str, task: &Task) -> Result<(), Box<dyn std::error::Error>>;
    fn delete(
        &self,
        root: &Path,
        id: &str,
        project: &str,
    ) -> Result<bool, Box<dyn std::error::Error>>;
}

pub struct FsBackend;

impl StorageBackend for FsBackend {
    fn add(
        &self,
        root: &Path,
        task: &Task,
        project: &str,
        original_project: Option<&str>,
    ) -> Result<String, Box<dyn std::error::Error>> {
        super::operations::StorageOperations::add(root, task, project, original_project)
    }

    fn get(&self, root: &Path, id: &str, project: &str) -> Option<Task> {
        super::operations::StorageOperations::get(root, id, project)
    }

    fn edit(&self, root: &Path, id: &str, task: &Task) -> Result<(), Box<dyn std::error::Error>> {
        super::operations::StorageOperations::edit(root, id, task)
    }

    fn delete(
        &self,
        root: &Path,
        id: &str,
        project: &str,
    ) -> Result<bool, Box<dyn std::error::Error>> {
        super::operations::StorageOperations::delete(root, id, project)
    }
}

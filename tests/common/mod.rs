use std::path::PathBuf;
use tempfile::TempDir;
use std::fs;
use lotar::storage::{Task, Storage};
use lotar::types::Priority;
use std::fs::File;
use std::io::Write;
use std::path::Path;

/// Test utilities for LoTaR testing
pub struct TestFixtures {
    pub temp_dir: TempDir,
    pub tasks_root: PathBuf,
}

impl TestFixtures {
    pub fn new() -> Self {
        let temp_dir = TempDir::new().expect("Failed to create temp directory");
        let tasks_root = temp_dir.path().join(".tasks");
        fs::create_dir_all(&tasks_root).expect("Failed to create tasks directory");

        Self {
            temp_dir,
            tasks_root,
        }
    }

    pub fn create_sample_task(&self, _project: &str) -> Task {
        Task::new(
            self.tasks_root.clone(),
            "Sample Test Task".to_string(),
            Priority::Medium  // Changed from numeric 2 to Priority::Medium
        )
    }

    #[allow(dead_code)] // Used across multiple test modules
    pub fn create_storage(&self) -> Storage {
        Storage::new(self.tasks_root.clone())
    }

    #[allow(dead_code)] // Used across multiple test modules
    pub fn get_temp_path(&self) -> &std::path::Path {
        self.temp_dir.path()
    }

    /// Create test files with TODO comments for scanner testing
    pub fn create_test_source_files(&self) -> Vec<String> {
        let mut files = Vec::new();

        // Create Rust file with TODO containing UUID
        let rust_file_path = self.temp_dir.path().join("test.rs");
        let mut rust_file = File::create(&rust_file_path).unwrap();
        writeln!(rust_file, "fn main() {{").unwrap();
        writeln!(rust_file, "    // TODO (uuid-1234): Test Rust with UUID").unwrap();
        writeln!(rust_file, "    // TODO: Implement main functionality").unwrap();
        writeln!(rust_file, "}}").unwrap();
        files.push(rust_file_path.to_string_lossy().to_string());

        // Create JavaScript file with TODO
        let js_file_path = self.temp_dir.path().join("test.js");
        let mut js_file = File::create(&js_file_path).unwrap();
        writeln!(js_file, "function test() {{").unwrap();
        writeln!(js_file, "    // TODO: Test JavaScript").unwrap();
        writeln!(js_file, "}}").unwrap();
        files.push(js_file_path.to_string_lossy().to_string());

        // Create Python file with TODO
        let py_file_path = self.temp_dir.path().join("test.py");
        let mut py_file = File::create(&py_file_path).unwrap();
        writeln!(py_file, "def test():").unwrap();
        writeln!(py_file, "    # TODO: Test Python").unwrap();
        writeln!(py_file, "    pass").unwrap();
        files.push(py_file_path.to_string_lossy().to_string());

        files
    }
}

/// Test utility functions
pub mod utils {
    /// Extract project prefix from task ID (e.g., "PROJ-123" -> "PROJ")
    #[allow(dead_code)] // Used across multiple test modules
    pub fn get_project_for_task(task_id: &str) -> Option<String> {
        task_id.split('-').next().map(|s| s.to_string())
    }
}

/// Assertion helpers for testing
pub mod assertions {
    use std::path::Path;

    #[allow(dead_code)] // Used in storage_crud_test.rs
    pub fn assert_task_exists(tasks_root: &Path, project: &str, _task_id: &str) {
        // Look for .yml files since we changed the extension
        let task_files = std::fs::read_dir(tasks_root.join(project))
            .expect("Project directory should exist")
            .filter_map(|entry| entry.ok())
            .filter(|entry| entry.path().extension().map_or(false, |ext| ext == "yml"))
            .collect::<Vec<_>>();

        assert!(!task_files.is_empty(), "Should have at least one task file in project {}", project);
    }

    #[allow(dead_code)] // Used in storage_crud_test.rs
    pub fn assert_metadata_updated(tasks_root: &Path, project: &str, task_count: u64, current_id: u64) {
        // Removed metadata file existence check since we've eliminated metadata.yml files
        // With the new filesystem-based approach, we verify the data by counting files and finding max ID
        let project_path = tasks_root.join(project);

        // Count actual task files in the directory (exclude config.yml)
        let actual_task_count = if let Ok(entries) = std::fs::read_dir(&project_path) {
            entries
                .filter_map(|entry| entry.ok())
                .filter(|entry| {
                    let path = entry.path();
                    let file_name = path.file_name()
                        .and_then(|name| name.to_str())
                        .unwrap_or("");
                    path.is_file() &&
                    path.extension().map_or(false, |ext| ext == "yml") &&
                    file_name != "config.yml" // Exclude config files from task count
                })
                .count() as u64
        } else {
            0
        };

        // Find the highest numbered file to verify current_id
        let actual_current_id = if let Ok(entries) = std::fs::read_dir(&project_path) {
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
        };

        assert_eq!(actual_task_count, task_count, "Task count mismatch for project {}", project);
        assert_eq!(actual_current_id, current_id, "Current ID mismatch for project {}", project);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_fixtures_creation() {
        let fixtures = TestFixtures::new();
        assert!(fixtures.tasks_root.exists());

        let task = fixtures.create_sample_task("test-project");
        assert_eq!(task.title, "Sample Test Task");
    }

    #[test]
    fn test_source_file_creation() {
        let fixtures = TestFixtures::new();
        let files = fixtures.create_test_source_files();

        assert_eq!(files.len(), 3);
        for file in &files {
            assert!(Path::new(file).exists());
        }

        // Verify content of one file
        let rust_content = fs::read_to_string(&files[0]).unwrap();
        assert!(rust_content.contains("TODO: Implement main functionality"));
        assert!(rust_content.contains("TODO (uuid-1234)"));
    }
}

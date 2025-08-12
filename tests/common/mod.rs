use lotar::types::Priority;
use lotar::{Storage, Task};
use std::fs;
use std::fs::File;
use std::io::Write;
use std::path::PathBuf;
use tempfile::TempDir;

// Re-export shared environment mutex for tests that mutate global env vars
pub mod env_mutex;

/// Test utilities for LoTaR testing
pub struct TestFixtures {
    pub temp_dir: TempDir,
    pub tasks_root: PathBuf,
}

impl TestFixtures {
    #[allow(dead_code)]
    pub fn new() -> Self {
        let temp_dir = TempDir::new().expect("Failed to create temp directory");
        let tasks_root = temp_dir.path().join(".tasks");
        fs::create_dir_all(&tasks_root).expect("Failed to create tasks directory");

        Self {
            temp_dir,
            tasks_root,
        }
    }

    // Clean up after test
    #[allow(dead_code)]
    pub fn cleanup(&self) {
        // Temporary directory is automatically cleaned up when TestFixtures is dropped
    }

    #[allow(dead_code)] // Used across multiple test modules
    pub fn create_storage(&self) -> Storage {
        Storage::new(self.tasks_root.clone())
    }

    #[allow(dead_code)] // Used across multiple test modules
    pub fn get_temp_path(&self) -> &std::path::Path {
        self.temp_dir.path()
    }

    /// Run a lotar command with the given arguments and return the output
    #[allow(dead_code)] // Used by output format consistency tests
    pub fn run_command(&self, args: &[&str]) -> Result<String, Box<dyn std::error::Error>> {
        use assert_cmd::Command;

        // Create a basic Cargo.toml if it doesn't exist for project detection
        let cargo_toml_path = self.temp_dir.path().join("Cargo.toml");
        if !cargo_toml_path.exists() {
            std::fs::write(
                &cargo_toml_path,
                "[package]\nname = \"test-project\"\nversion = \"0.1.0\"\nedition = \"2021\"",
            )?;

            // Create src directory and main.rs for a valid Rust project
            let src_dir = self.temp_dir.path().join("src");
            std::fs::create_dir_all(&src_dir)?;
            std::fs::write(
                src_dir.join("main.rs"),
                "fn main() { println!(\"Hello, world!\"); }",
            )?;
        }

        let output = Command::cargo_bin("lotar")?
            .args(args)
            .current_dir(self.temp_dir.path())
            .output()?;

        if output.status.success() {
            Ok(String::from_utf8_lossy(&output.stdout).to_string())
        } else {
            Err(format!(
                "Command failed: {}",
                String::from_utf8_lossy(&output.stderr)
            )
            .into())
        }
    }

    /// Create test files with TODO comments for scanner testing
    #[allow(dead_code)] // Used by scanner tests
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

    /// Create a sample task for testing
    #[allow(dead_code)]
    pub fn create_sample_task(&self, _project: &str) -> Task {
        Task::new(
            self.tasks_root.clone(),
            "Sample Test Task".to_string(),
            Priority::Medium,
        )
    }

    /// Create a config file in the specified directory
    #[allow(dead_code)]
    pub fn create_config_in_dir(&self, dir: &std::path::Path, content: &str) {
        let config_path = dir.join("config.yml");
        std::fs::write(&config_path, content).expect("Failed to create config file");
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
            .filter(|entry| entry.path().extension().is_some_and(|ext| ext == "yml"))
            .collect::<Vec<_>>();

        assert!(
            !task_files.is_empty(),
            "Should have at least one task file in project {project}"
        );
    }

    #[allow(dead_code)] // Used in storage_crud_test.rs
    pub fn assert_metadata_updated(
        tasks_root: &Path,
        project: &str,
        task_count: u64,
        current_id: u64,
    ) {
        // Removed metadata file existence check since we've eliminated metadata.yml files
        // With the new filesystem-based approach, we verify the data by counting files and finding max ID
        let project_path = tasks_root.join(project);

        // Count actual task files in the directory (exclude config.yml)
        let actual_task_count = if let Ok(entries) = std::fs::read_dir(&project_path) {
            entries
                .filter_map(|entry| entry.ok())
                .filter(|entry| {
                    let path = entry.path();
                    let file_name = path
                        .file_name()
                        .and_then(|name| name.to_str())
                        .unwrap_or("");
                    path.is_file()
                        && path.extension().is_some_and(|ext| ext == "yml")
                        && file_name != "config.yml" // Exclude config files from task count
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

        assert_eq!(
            actual_task_count, task_count,
            "Task count mismatch for project {project}"
        );
        assert_eq!(
            actual_current_id, current_id,
            "Current ID mismatch for project {project}"
        );
    }
}

// Note: Test functions for common utilities have been removed to prevent duplication
// across all test files that import this module. Each test file should test its own functionality.

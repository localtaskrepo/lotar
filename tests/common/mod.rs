use std::path::PathBuf;
use tempfile::TempDir;
use std::fs;
use local_task_repo::store::{Task, Storage};
use local_task_repo::types::Priority;
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

    pub fn create_sample_task(&self, project: &str) -> Task {
        Task::new(
            self.tasks_root.clone(),
            "Sample Test Task".to_string(),
            project.to_string(),
            Priority::Medium  // Changed from numeric 2 to Priority::Medium
        )
    }

    pub fn create_storage(&self) -> Storage {
        Storage::new(self.tasks_root.clone())
    }

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

    /// Create a project structure with multiple tasks
    pub fn create_sample_project(&self, project_name: &str) -> Vec<Task> {
        let mut storage = self.create_storage();
        let mut tasks = Vec::new();

        // Create different types of tasks
        let task_configs = vec![
            ("Implement authentication", Priority::High, vec!["security", "backend"]),
            ("Design user interface", Priority::Medium, vec!["ui", "frontend"]),
            ("Write unit tests", Priority::High, vec!["testing", "quality"]),
            ("Setup CI/CD pipeline", Priority::Medium, vec!["devops", "automation"]),
            ("Create documentation", Priority::Low, vec!["docs", "onboarding"]),
        ];

        for (title, priority, tags) in task_configs {
            let mut task = Task::new(
                self.tasks_root.clone(),
                title.to_string(),
                project_name.to_string(),
                priority
            );
            task.tags = tags.into_iter().map(|s| s.to_string()).collect();

            let task_id = storage.add(&task);
            task.id = task_id;
            tasks.push(task);
        }

        tasks
    }
}

/// Assertion helpers for testing
pub mod assertions {
    use local_task_repo::store::Task;
    use std::path::Path;

    pub fn assert_task_exists(tasks_root: &Path, project: &str, task_id: &str) {
        // Look for .yml files since we changed the extension
        let task_files = std::fs::read_dir(tasks_root.join(project))
            .expect("Project directory should exist")
            .filter_map(|entry| entry.ok())
            .filter(|entry| entry.path().extension().map_or(false, |ext| ext == "yml"))
            .collect::<Vec<_>>();

        assert!(!task_files.is_empty(), "Should have at least one task file in project {}", project);
    }

    pub fn assert_task_has_field(task: &Task, field: &str, expected_value: &str) {
        match field {
            "title" => assert_eq!(task.title, expected_value),
            "project" => assert_eq!(task.project, expected_value),
            "id" => assert_eq!(task.id, expected_value),
            _ => panic!("Unknown field: {}", field),
        }
    }

    pub fn assert_task_count(tasks: &[Task], expected_count: usize) {
        assert_eq!(tasks.len(), expected_count,
                  "Expected {} tasks, found {}", expected_count, tasks.len());
    }

    pub fn assert_metadata_updated(tasks_root: &Path, project: &str, task_count: u64, current_id: u64) {
        let metadata_file = tasks_root.join(format!("{}/metadata.yml", project));
        assert!(metadata_file.exists(), "Metadata file should exist for project {}", project);

        let metadata_content = std::fs::read_to_string(&metadata_file)
            .expect("Should be able to read metadata file");
        assert!(metadata_content.contains(&format!("task_count: {}", task_count)));
        assert!(metadata_content.contains(&format!("current_id: {}", current_id)));
    }
}

/// Performance testing utilities
pub mod performance {
    use std::time::{Duration, Instant};

    pub fn measure_execution_time<F, R>(operation: F) -> (R, Duration)
    where
        F: FnOnce() -> R,
    {
        let start = Instant::now();
        let result = operation();
        let duration = start.elapsed();
        (result, duration)
    }

    pub fn assert_performance_threshold<F, R>(
        operation: F,
        max_duration: Duration,
        operation_name: &str
    ) -> R
    where
        F: FnOnce() -> R,
    {
        let (result, duration) = measure_execution_time(operation);
        assert!(
            duration <= max_duration,
            "{} took {:?}, expected <= {:?}",
            operation_name,
            duration,
            max_duration
        );
        result
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
        assert_eq!(task.project, "test-project");
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

use tempfile::TempDir;
use std::fs;
use std::path::{Path, PathBuf};
use local_task_repo::store::{Task, Storage};

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
            2
        )
    }

    pub fn create_storage(&self) -> Storage {
        Storage::new(self.tasks_root.clone())
    }

    pub fn get_temp_path(&self) -> &std::path::Path {
        self.temp_dir.path()
    }

    /// Create test files with TODO comments for scanner testing
    pub fn create_test_source_files(&self) -> Vec<PathBuf> {
        let mut files = Vec::new();

        // Rust file
        let rust_file = self.temp_dir.path().join("test.rs");
        fs::write(&rust_file, r#"
fn main() {
    // TODO: Implement main functionality
    println!("Hello world");

    /* TODO: Add error handling */
    let result = process();
}

fn process() -> Result<(), String> {
    // TODO (uuid-1234): Refactor this function
    Ok(())
}
"#).expect("Failed to write Rust test file");
        files.push(rust_file);

        // JavaScript file
        let js_file = self.temp_dir.path().join("test.js");
        fs::write(&js_file, r#"
function main() {
    // TODO: Add input validation
    const input = getUserInput();

    /* TODO: Implement proper error handling */
    return processInput(input);
}
"#).expect("Failed to write JavaScript test file");
        files.push(js_file);

        // Python file
        let py_file = self.temp_dir.path().join("test.py");
        fs::write(&py_file, r#"
def main():
    # TODO: Add docstring
    pass

    # TODO (uuid-5678): Implement functionality
    return None
"#).expect("Failed to write Python test file");
        files.push(py_file);

        files
    }

    /// Create a project structure with multiple tasks
    pub fn create_sample_project(&self, project_name: &str) -> Vec<Task> {
        let mut storage = self.create_storage();
        let mut tasks = Vec::new();

        // Create different types of tasks
        let task_configs = vec![
            ("Implement authentication", 1, vec!["security", "backend"]),
            ("Design user interface", 2, vec!["ui", "frontend"]),
            ("Write unit tests", 3, vec!["testing", "quality"]),
            ("Setup CI/CD pipeline", 2, vec!["devops", "automation"]),
            ("Create documentation", 1, vec!["docs", "onboarding"]),
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

    pub fn assert_task_exists(tasks_root: &Path, project: &str, task_id: u64) {
        let task_file = tasks_root
            .join(format!("{}/task_{:04}.yaml", project, task_id));
        assert!(task_file.exists(), "Task file should exist: {:?}", task_file);
    }

    pub fn assert_task_has_field(task: &Task, field: &str, expected_value: &str) {
        match field {
            "title" => assert_eq!(task.title, expected_value),
            "project" => assert_eq!(task.project, expected_value),
            _ => panic!("Unknown field: {}", field),
        }
    }

    pub fn assert_task_count(tasks: &[Task], expected_count: usize) {
        assert_eq!(tasks.len(), expected_count,
                  "Expected {} tasks, found {}", expected_count, tasks.len());
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
            assert!(file.exists());
        }

        // Verify content of one file
        let rust_content = fs::read_to_string(&files[0]).unwrap();
        assert!(rust_content.contains("TODO: Implement main functionality"));
        assert!(rust_content.contains("TODO (uuid-1234)"));
    }
}

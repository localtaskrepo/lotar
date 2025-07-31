use assert_cmd::Command;
use tempfile::TempDir;

mod common;

/// File structure and directory creation verification tests
#[cfg(test)]
mod file_structure_tests {
    use super::*;

    #[test]
    fn test_tasks_directory_creation() {
        let temp_dir = TempDir::new().unwrap();

        // Create a task
        let mut cmd = Command::cargo_bin("lotar").unwrap();
        cmd.current_dir(&temp_dir)
            .args(&[
                "task", "add",
                "--title=Structure Test Task",
                "--project=structure-test"
            ])
            .assert()
            .success();

        // Verify .tasks directory was created
        let tasks_root = temp_dir.path().join(".tasks");
        assert!(tasks_root.exists(), ".tasks directory should be created");
        assert!(tasks_root.is_dir(), ".tasks should be a directory");
    }

    #[test]
    fn test_project_directory_creation() {
        let temp_dir = TempDir::new().unwrap();

        // Create a task
        let mut cmd = Command::cargo_bin("lotar").unwrap();
        cmd.current_dir(&temp_dir)
            .args(&[
                "task", "add",
                "--title=Project Dir Test Task",
                "--project=project-dir-test"
            ])
            .assert()
            .success();

        // Verify project directory was created (with abbreviated name)
        let tasks_root = temp_dir.path().join(".tasks");
        let project_dirs: Vec<_> = std::fs::read_dir(&tasks_root)
            .unwrap()
            .filter_map(|entry| entry.ok())
            .filter(|entry| entry.path().is_dir())
            .filter(|entry| !entry.file_name().to_string_lossy().starts_with('.'))
            .collect();

        assert!(!project_dirs.is_empty(), "At least one project directory should be created");
    }

    #[test]
    fn test_task_files_creation() {
        let temp_dir = TempDir::new().unwrap();

        // Create multiple tasks
        for i in 1..=3 {
            let mut cmd = Command::cargo_bin("lotar").unwrap();
            cmd.current_dir(&temp_dir)
                .args(&[
                    "task", "add",
                    &format!("--title=File Test Task {}", i),
                    "--project=file-test"
                ])
                .assert()
                .success();
        }

        // Find the project directory (abbreviated name)
        let tasks_root = temp_dir.path().join(".tasks");
        let project_dirs: Vec<_> = std::fs::read_dir(&tasks_root)
            .unwrap()
            .filter_map(|entry| entry.ok())
            .filter(|entry| entry.path().is_dir())
            .filter(|entry| !entry.file_name().to_string_lossy().starts_with('.'))
            .collect();

        assert!(!project_dirs.is_empty(), "Project directory should exist");

        // Check for task files in the project directory
        let project_dir = &project_dirs[0];
        let task_files: Vec<_> = std::fs::read_dir(project_dir.path())
            .unwrap()
            .filter_map(|entry| entry.ok())
            .filter(|entry| {
                entry.path().is_file() &&
                entry.path().extension().and_then(|s| s.to_str()) == Some("yml") &&
                entry.file_name().to_string_lossy() != "metadata.yml"
            })
            .collect();

        // Verify that task files are being created
        assert!(!task_files.is_empty(),
               "At least one task file should be created, found: {}", task_files.len());
    }

    #[test]
    fn test_index_file_creation() {
        let temp_dir = TempDir::new().unwrap();

        // Create a task
        let mut cmd = Command::cargo_bin("lotar").unwrap();
        cmd.current_dir(&temp_dir)
            .args(&[
                "task", "add",
                "--title=Index Test Task",
                "--project=index-test"
            ])
            .assert()
            .success();

        // Check for index.yml file
        let tasks_root = temp_dir.path().join(".tasks");
        let index_file = tasks_root.join("index.yml");
        assert!(index_file.exists(), "index.yml should be created");
        assert!(index_file.is_file(), "index.yml should be a file");
    }
}

use assert_cmd::Command;
use tempfile::TempDir;

mod common;
use common::TestFixtures;

/// Integration workflow tests - complex multi-step scenarios
#[cfg(test)]
mod workflow_tests {
    use super::*;

    #[test]
    fn test_complete_task_workflow() {
        let temp_dir = TempDir::new().unwrap();

        // Create a task
        let mut cmd = Command::cargo_bin("lotar").unwrap();
        let output = cmd.current_dir(&temp_dir)
            .args(&[
                "task", "add",
                "--title=Workflow Test Task",
                "--description=Testing complete workflow",
                "--priority=1"
            ])
            .output()
            .unwrap();

        assert!(output.status.success());
        let stdout = String::from_utf8(output.stdout).unwrap();
        assert!(stdout.contains("Added task with id"));

        // Extract task ID
        let task_id = stdout
            .split("Added task with id: ")
            .nth(1)
            .unwrap()
            .trim()
            .split_whitespace()
            .next()
            .unwrap();

        // Edit the task
        let mut cmd = Command::cargo_bin("lotar").unwrap();
        cmd.current_dir(&temp_dir)
            .args(&[
                "task", "edit", task_id,
                "--title=Updated Workflow Task"
            ])
            .assert()
            .success();
    }

    #[test]
    fn test_project_isolation() {
        let temp_dir = TempDir::new().unwrap();

        // Create tasks in different projects
        let mut cmd1 = Command::cargo_bin("lotar").unwrap();
        cmd1.current_dir(&temp_dir)
            .args(&[
                "task", "add",
                "--title=Project A Task",
                "--project=project-a"
            ])
            .assert()
            .success();

        let mut cmd2 = Command::cargo_bin("lotar").unwrap();
        cmd2.current_dir(&temp_dir)
            .args(&[
                "task", "add",
                "--title=Project B Task",
                "--project=project-b"
            ])
            .assert()
            .success();

        // Verify task directories were created
        let tasks_root = temp_dir.path().join(".tasks");
        assert!(tasks_root.exists(), "Tasks root directory should be created");

        let tasks_dirs: Vec<_> = std::fs::read_dir(&tasks_root)
            .unwrap()
            .filter_map(|entry| entry.ok())
            .filter(|entry| entry.path().is_dir())
            .collect();
        assert!(tasks_dirs.len() >= 2, "At least two project directories should be created");
    }

    #[test]
    fn test_scan_and_task_integration() {
        let fixtures = TestFixtures::new();
        let _source_files = fixtures.create_test_source_files();

        // First, scan for TODOs
        let mut scan_cmd = Command::cargo_bin("lotar").unwrap();
        let scan_output = scan_cmd
            .current_dir(fixtures.get_temp_path())
            .args(&["scan"])
            .output()
            .unwrap();

        assert!(scan_output.status.success());
        let scan_stdout = String::from_utf8(scan_output.stdout).unwrap();
        assert!(scan_stdout.contains("TODO"));

        // Then create a task in the same directory
        let mut task_cmd = Command::cargo_bin("lotar").unwrap();
        task_cmd.current_dir(fixtures.get_temp_path())
            .args(&[
                "task", "add",
                "--title=Task for scanned project",
                "--project=scanned-project"
            ])
            .assert()
            .success();

        // Verify task directory was created
        let tasks_root = fixtures.get_temp_path().join(".tasks");
        assert!(tasks_root.exists(), "Tasks root directory should be created");

        let tasks_dirs: Vec<_> = std::fs::read_dir(&tasks_root)
            .unwrap()
            .filter_map(|entry| entry.ok())
            .filter(|entry| entry.path().is_dir())
            .collect();
        assert!(!tasks_dirs.is_empty(), "At least one project directory should be created");
    }
}

use assert_cmd::Command;
use predicates::prelude::*;
use tempfile::TempDir;

mod common;

/// Task creation tests
#[cfg(test)]
mod task_creation {
    use super::*;

    #[test]
    fn test_task_add_basic() {
        let temp_dir = TempDir::new().unwrap();

        let mut cmd = Command::cargo_bin("lotar").unwrap();
        cmd.current_dir(&temp_dir)
            .args(&[
                "task", "add",
                "--title=Basic Test Task",
                "--priority=2",
                "--project=basic-test"
            ])
            .assert()
            .success()
            .stdout(predicate::str::contains("Added task with id"));

        // Verify task directory was created
        let tasks_root = temp_dir.path().join(".tasks");
        assert!(tasks_root.exists(), "Tasks root directory should be created");

        let tasks_dirs: Vec<_> = std::fs::read_dir(&tasks_root)
            .unwrap()
            .filter_map(|entry| entry.ok())
            .filter(|entry| entry.path().is_dir())
            .collect();
        assert!(!tasks_dirs.is_empty(), "At least one project directory should be created");
    }

    #[test]
    fn test_task_add_with_tags() {
        let temp_dir = TempDir::new().unwrap();

        let mut cmd = Command::cargo_bin("lotar").unwrap();
        cmd.current_dir(&temp_dir)
            .args(&[
                "task", "add",
                "--title=Tagged Task",
                "--tag=frontend",
                "--tag=urgent"
            ])
            .assert()
            .success()
            .stdout(predicate::str::contains("Added task with id"));
    }

    #[test]
    fn test_task_add_missing_title() {
        let temp_dir = TempDir::new().unwrap();

        let mut cmd = Command::cargo_bin("lotar").unwrap();
        cmd.current_dir(&temp_dir)
            .args(&["task", "add"])
            .assert()
            .failure()
            .stdout(predicate::str::contains("Error"));
    }
}

/// Task management operations
#[cfg(test)]
mod task_operations {
    use super::*;

    #[test]
    fn test_task_list() {
        let temp_dir = TempDir::new().unwrap();

        // Create a task first
        let mut cmd = Command::cargo_bin("lotar").unwrap();
        cmd.current_dir(&temp_dir)
            .args(&[
                "task", "add",
                "--title=List Test Task",
                "--project=list-test"
            ])
            .assert()
            .success();

        // Then list tasks
        let mut cmd = Command::cargo_bin("lotar").unwrap();
        cmd.current_dir(&temp_dir)
            .args(&["task", "list", "--project=list-test"])
            .assert()
            .success()
            .stdout(predicate::str::contains("Listing tasks for project"));
    }

    #[test]
    fn test_task_search() {
        let temp_dir = TempDir::new().unwrap();

        // Create a task first
        let mut cmd = Command::cargo_bin("lotar").unwrap();
        cmd.current_dir(&temp_dir)
            .args(&[
                "task", "add",
                "--title=Search Test Task",
                "--project=search-test"
            ])
            .assert()
            .success();

        // Then search for tasks
        let mut cmd = Command::cargo_bin("lotar").unwrap();
        cmd.current_dir(&temp_dir)
            .args(&["task", "search", "Search", "--project=search-test"])
            .assert()
            .success()
            .stdout(predicate::str::contains("Searching for"));
    }

    #[test]
    fn test_task_edit() {
        let temp_dir = TempDir::new().unwrap();

        // Create a task and get its ID
        let mut cmd = Command::cargo_bin("lotar").unwrap();
        let output = cmd.current_dir(&temp_dir)
            .args(&[
                "task", "add",
                "--title=Edit Test Task",
                "--project=edit-test"
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
                "--description=Updated description"
            ])
            .assert()
            .success()
            .stdout(predicate::str::contains("updated successfully"));
    }

    #[test]
    fn test_task_status_update() {
        let temp_dir = TempDir::new().unwrap();

        // Create a task and get its ID
        let mut cmd = Command::cargo_bin("lotar").unwrap();
        let output = cmd.current_dir(&temp_dir)
            .args(&[
                "task", "add",
                "--title=Status Test Task",
                "--project=status-test"
            ])
            .output()
            .unwrap();

        assert!(output.status.success());
        let stdout = String::from_utf8(output.stdout).unwrap();

        // Extract task ID
        let task_id = stdout
            .split("Added task with id: ")
            .nth(1)
            .unwrap()
            .trim()
            .split_whitespace()
            .next()
            .unwrap();

        // Update task status
        let mut cmd = Command::cargo_bin("lotar").unwrap();
        cmd.current_dir(&temp_dir)
            .args(&[
                "task", "status", task_id, "IN_PROGRESS"
            ])
            .assert()
            .success()
            .stdout(predicate::str::contains("status updated"));
    }

    #[test]
    fn test_task_search_with_filters() {
        let temp_dir = TempDir::new().unwrap();

        // Create a task and update its status
        let mut cmd = Command::cargo_bin("lotar").unwrap();
        let output = cmd.current_dir(&temp_dir)
            .args(&[
                "task", "add",
                "--title=Filter Test Task",
                "--project=filter-test"
            ])
            .output()
            .unwrap();

        let stdout = String::from_utf8(output.stdout).unwrap();
        let task_id = stdout
            .split("Added task with id: ")
            .nth(1)
            .unwrap()
            .trim()
            .split_whitespace()
            .next()
            .unwrap();

        // Update status
        let mut cmd = Command::cargo_bin("lotar").unwrap();
        cmd.current_dir(&temp_dir)
            .args(&["task", "status", task_id, "IN_PROGRESS"])
            .assert()
            .success();

        // Search with status filter
        let mut cmd = Command::cargo_bin("lotar").unwrap();
        cmd.current_dir(&temp_dir)
            .args(&[
                "task", "search", "Filter",
                "--status=IN_PROGRESS",
                "--project=filter-test"
            ])
            .assert()
            .success()
            .stdout(predicate::str::contains("Searching for"));
    }
}

/// Task error handling tests
#[cfg(test)]
mod task_errors {
    use super::*;

    #[test]
    fn test_edit_nonexistent_task() {
        let temp_dir = TempDir::new().unwrap();

        let mut cmd = Command::cargo_bin("lotar").unwrap();
        cmd.current_dir(&temp_dir)
            .args(&["task", "edit", "99999"])
            .assert()
            .failure()
            .stdout(predicate::str::contains("not found"));
    }

    #[test]
    fn test_edit_invalid_task_id() {
        let temp_dir = TempDir::new().unwrap();

        let mut cmd = Command::cargo_bin("lotar").unwrap();
        cmd.current_dir(&temp_dir)
            .args(&["task", "edit", "invalid"])
            .assert()
            .failure()
            .stdout(predicate::str::contains("not found"));
    }
}

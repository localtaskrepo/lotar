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
                "--priority=Low",
                "--project=basic-test"
            ])
            .assert()
            .success()
            .stdout(predicate::str::contains("✅ Created task:"));

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
            .stdout(predicate::str::contains("✅ Created task:"));
    }

    #[test]
    fn test_task_add_missing_title() {
        let temp_dir = TempDir::new().unwrap();

        let mut cmd = Command::cargo_bin("lotar").unwrap();
        cmd.current_dir(&temp_dir)
            .args(&["task", "add"])
            .assert()
            .failure()
            .stderr(predicate::str::contains("required arguments were not provided"));
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
            .stdout(predicate::str::contains("Found"));
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
            .stdout(predicate::str::contains("Found"));
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
        assert!(stdout.contains("✅ Created task:"));

        // Extract task ID
        let task_id = stdout
            .lines()
            .find(|line| line.contains("✅ Created task:"))
            .unwrap()
            .split("✅ Created task: ")
            .nth(1)
            .unwrap()
            .trim();

        // Edit the task
        let mut cmd = Command::cargo_bin("lotar").unwrap();
        cmd.current_dir(&temp_dir)
            .args(&[
                "task", "edit", task_id,
                "--description=Updated description",
                "--project=edit-test"
            ])
            .assert()
            .success()
            .stdout(predicate::str::contains("✅ Task"));
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
            .lines()
            .find(|line| line.contains("✅ Created task:"))
            .unwrap()
            .split("✅ Created task: ")
            .nth(1)
            .unwrap()
            .trim();

        // Update task status
        let mut cmd = Command::cargo_bin("lotar").unwrap();
        cmd.current_dir(&temp_dir)
            .args(&[
                "task", "status", task_id, "IN_PROGRESS"
            ])
            .assert()
            .success()
            .stdout(predicate::str::contains("status changed"));
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
            .lines()
            .find(|line| line.contains("✅ Created task:"))
            .unwrap()
            .split("✅ Created task: ")
            .nth(1)
            .unwrap()
            .trim();

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
            .stdout(predicate::str::contains("Found"));
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

/// Tests for smart project parameter parsing in task operations
#[cfg(test)]
mod smart_project_parameters {
    use super::*;

    #[test]
    fn test_task_add_with_full_project_name() {
        let temp_dir = TempDir::new().unwrap();

        // Add task with full project name
        let mut cmd = Command::cargo_bin("lotar").unwrap();
        cmd.current_dir(&temp_dir)
            .args(&[
                "task", "add",
                "--title=Test Task with Full Name",
                "--project=MY-AWESOME-PROJECT"
            ])
            .assert()
            .success()
            .stdout(predicate::str::contains("✅ Created task:"));

        // Verify the task can be listed using the generated prefix
        let mut cmd = Command::cargo_bin("lotar").unwrap();
        cmd.current_dir(&temp_dir)
            .args(&["task", "list", "--project=MAP"])
            .assert()
            .success()
            .stdout(predicate::str::contains("Test Task with Full Name"));
    }

    #[test]
    fn test_task_list_with_both_prefix_and_full_name() {
        let temp_dir = TempDir::new().unwrap();

        // Add a task
        let mut cmd = Command::cargo_bin("lotar").unwrap();
        cmd.current_dir(&temp_dir)
            .args(&[
                "task", "add",
                "--title=Smart Resolution Test",
                "--project=FRONTEND-COMPONENTS"
            ])
            .assert()
            .success();

        // List using prefix
        let mut cmd = Command::cargo_bin("lotar").unwrap();
        cmd.current_dir(&temp_dir)
            .args(&["task", "list", "--project=FC"])
            .assert()
            .success()
            .stdout(predicate::str::contains("Smart Resolution Test"));

        // List using full name
        let mut cmd = Command::cargo_bin("lotar").unwrap();
        cmd.current_dir(&temp_dir)
            .args(&["task", "list", "--project=FRONTEND-COMPONENTS"])
            .assert()
            .success()
            .stdout(predicate::str::contains("Smart Resolution Test"));
    }

    #[test]
    fn test_task_edit_with_smart_project_resolution() {
        let temp_dir = TempDir::new().unwrap();

        // Add a task
        let mut cmd = Command::cargo_bin("lotar").unwrap();
        cmd.current_dir(&temp_dir)
            .args(&[
                "task", "add",
                "--title=Editable Task",
                "--project=API-GATEWAY"
            ])
            .assert()
            .success();

        // Edit using full project name
        let mut cmd = Command::cargo_bin("lotar").unwrap();
        cmd.current_dir(&temp_dir)
            .args(&[
                "task", "edit", "AG-1",
                "--title=Edited Task Title",
                "--project=API-GATEWAY"
            ])
            .assert()
            .success()
            .stdout(predicate::str::contains("updated successfully"));

        // Verify the edit worked by listing with prefix
        let mut cmd = Command::cargo_bin("lotar").unwrap();
        cmd.current_dir(&temp_dir)
            .args(&["task", "list", "--project=AG"])
            .assert()
            .success()
            .stdout(predicate::str::contains("Edited Task Title"));
    }

    #[test]
    fn test_task_delete_with_smart_project_resolution() {
        let temp_dir = TempDir::new().unwrap();

        // Add a task
        let mut cmd = Command::cargo_bin("lotar").unwrap();
        cmd.current_dir(&temp_dir)
            .args(&[
                "task", "add",
                "--title=Task to Delete",
                "--project=USER-INTERFACE"
            ])
            .assert()
            .success();

        // Delete using full project name
        let mut cmd = Command::cargo_bin("lotar").unwrap();
        cmd.current_dir(&temp_dir)
            .args(&[
                "task", "delete", "UI-1",
                "--project=USER-INTERFACE",
                "--force"
            ])
            .assert()
            .success()
            .stdout(predicate::str::contains("✅ Task"));

        // Verify the task is gone
        let mut cmd = Command::cargo_bin("lotar").unwrap();
        cmd.current_dir(&temp_dir)
            .args(&["task", "list", "--project=UI"])
            .assert()
            .success()
            .stdout(predicate::str::contains("No tasks found"));
    }

    #[test]
    fn test_task_status_update_with_smart_project_resolution() {
        let temp_dir = TempDir::new().unwrap();

        // Add a task
        let mut cmd = Command::cargo_bin("lotar").unwrap();
        cmd.current_dir(&temp_dir)
            .args(&[
                "task", "add",
                "--title=Status Test Task",
                "--project=DATABASE-LAYER"
            ])
            .assert()
            .success();

        // Update status using full project name in search to verify it worked
        let mut cmd = Command::cargo_bin("lotar").unwrap();
        cmd.current_dir(&temp_dir)
            .args(&["task", "status", "DL-1", "IN_PROGRESS"])
            .assert()
            .success()
            .stdout(predicate::str::contains("status changed"));

        // Verify status change by searching with full project name
        let mut cmd = Command::cargo_bin("lotar").unwrap();
        cmd.current_dir(&temp_dir)
            .args(&["task", "search", "Status", "--project=DATABASE-LAYER", "--status=IN_PROGRESS"])
            .assert()
            .success()
            .stdout(predicate::str::contains("Status Test Task"));
    }
}

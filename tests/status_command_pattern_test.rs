//! Tests for standardized property command pattern - status get/set functionality
//! 
//! This module tests the new standardized pattern where property commands
//! can both get and set values:
//! - `lotar status TASK_ID` → shows current status
//! - `lotar status TASK_ID NEW_STATUS` → changes status

use assert_cmd::Command;
use predicates::prelude::*;

mod common;
use common::TestFixtures;

#[cfg(test)]
mod status_command_pattern {
    use super::*;

    #[test]
    fn test_status_get_operation() {
        let test_fixtures = TestFixtures::new();
        let temp_dir = test_fixtures.temp_dir.path();

        // Create a task first using the new CLI with explicit project
        let mut cmd = Command::cargo_bin("lotar").unwrap();
        cmd.current_dir(temp_dir)
            .arg("task")
            .arg("add")
            .arg("Test task for status get")
            .arg("--project=test-project")
            .assert()
            .success();

        // Get the status (should be TODO by default)
        let mut cmd = Command::cargo_bin("lotar").unwrap();
        cmd.current_dir(temp_dir)
            .arg("status")
            .arg("1")
            .arg("--project=test-project")
            .assert()
            .success()
            .stdout(predicate::str::contains("status: TODO"));
    }

    #[test]
    fn test_status_set_operation() {
        let test_fixtures = TestFixtures::new();
        let temp_dir = test_fixtures.temp_dir.path();

        // Create a task first using the new CLI with explicit project
        let mut cmd = Command::cargo_bin("lotar").unwrap();
        cmd.current_dir(temp_dir)
            .arg("task")
            .arg("add")
            .arg("Test task for status set")
            .arg("--project=test-project")
            .assert()
            .success();

        // Set the status to in_progress
        let mut cmd = Command::cargo_bin("lotar").unwrap();
        cmd.current_dir(temp_dir)
            .arg("status")
            .arg("1")
            .arg("in_progress")
            .arg("--project=test-project")
            .assert()
            .success()
            .stdout(predicate::str::contains("status changed from TODO to IN_PROGRESS"));
    }

    #[test]
    fn test_status_get_after_set() {
        let test_fixtures = TestFixtures::new();
        let temp_dir = test_fixtures.temp_dir.path();

        // Create a task using the new CLI with explicit project
        let mut cmd = Command::cargo_bin("lotar").unwrap();
        cmd.current_dir(temp_dir)
            .arg("task")
            .arg("add")
            .arg("Test task for get after set")
            .arg("--project=test-project")
            .assert()
            .success();

        // Set status to done
        let mut cmd = Command::cargo_bin("lotar").unwrap();
        cmd.current_dir(temp_dir)
            .arg("status")
            .arg("1")
            .arg("done")
            .arg("--project=test-project")
            .assert()
            .success();

        // Verify the status was changed
        let mut cmd = Command::cargo_bin("lotar").unwrap();
        cmd.current_dir(temp_dir)
            .arg("status")
            .arg("1")
            .arg("--project=test-project")
            .assert()
            .success()
            .stdout(predicate::str::contains("status: DONE"));
    }

    #[test]
    fn test_status_full_command_get_operation() {
        let test_fixtures = TestFixtures::new();
        let temp_dir = test_fixtures.temp_dir.path();

        // Create a task using the new CLI with explicit project
        let mut cmd = Command::cargo_bin("lotar").unwrap();
        cmd.current_dir(temp_dir)
            .arg("task")
            .arg("add")
            .arg("Test task for alias get")
            .arg("--project=test-project")
            .assert()
            .success();

        // Use status command to get status (testing that it works without alias)
        let mut cmd = Command::cargo_bin("lotar").unwrap();
        cmd.current_dir(temp_dir)
            .arg("status")
            .arg("1")
            .arg("--project=test-project")
            .assert()
            .success()
            .stdout(predicate::str::contains("status: TODO"));
    }

    #[test]
    fn test_status_full_command_set_operation() {
        let test_fixtures = TestFixtures::new();
        let temp_dir = test_fixtures.temp_dir.path();

        // Create a task using the new CLI with explicit project
        let mut cmd = Command::cargo_bin("lotar").unwrap();
        cmd.current_dir(temp_dir)
            .arg("task")
            .arg("add")
            .arg("Test task for alias set")
            .arg("--project=test-project")
            .assert()
            .success();

        // Use status command to set status (testing that it works without alias)
        let mut cmd = Command::cargo_bin("lotar").unwrap();
        cmd.current_dir(temp_dir)
            .arg("status")
            .arg("1")
            .arg("in_progress")
            .arg("--project=test-project")
            .assert()
            .success()
            .stdout(predicate::str::contains("status changed from TODO to IN_PROGRESS"));
    }

    #[test]
    fn test_status_set_same_value_warning() {
        let test_fixtures = TestFixtures::new();
        let temp_dir = test_fixtures.temp_dir.path();

        // Create a task
        let mut cmd = Command::cargo_bin("lotar").unwrap();
        cmd.current_dir(temp_dir)
            .arg("add")
            .arg("Test task for same status warning")
            .assert()
            .success();

        // Try to set status to the same value (TODO)
        let mut cmd = Command::cargo_bin("lotar").unwrap();
        cmd.current_dir(temp_dir)
            .arg("status")
            .arg("1")
            .arg("todo")
            .assert()
            .success()
            .stdout(predicate::str::contains("already has status 'TODO'"));
    }

    #[test]
    fn test_status_invalid_status_error() {
        let test_fixtures = TestFixtures::new();
        let temp_dir = test_fixtures.temp_dir.path();

        // Create a task
        let mut cmd = Command::cargo_bin("lotar").unwrap();
        cmd.current_dir(temp_dir)
            .arg("add")
            .arg("Test task for invalid status")
            .assert()
            .success();

        // Try to set an invalid status
        let mut cmd = Command::cargo_bin("lotar").unwrap();
        cmd.current_dir(temp_dir)
            .arg("status")
            .arg("1")
            .arg("invalid_status")
            .assert()
            .failure()
            .stdout(predicate::str::contains("Status validation failed"));
    }

    #[test]
    fn test_status_nonexistent_task_error() {
        let test_fixtures = TestFixtures::new();
        let temp_dir = test_fixtures.temp_dir.path();

        // Try to get status of non-existent task
        let mut cmd = Command::cargo_bin("lotar").unwrap();
        cmd.current_dir(temp_dir)
            .arg("status")
            .arg("999")
            .assert()
            .failure()
            .stdout(predicate::str::contains("not found"));
    }

    #[test]
    fn test_status_with_project_prefix() {
        let test_fixtures = TestFixtures::new();
        let temp_dir = test_fixtures.temp_dir.path();

        // Create a task (will get auto-generated prefix)
        let mut cmd = Command::cargo_bin("lotar").unwrap();
        let output = cmd.current_dir(temp_dir)
            .arg("add")
            .arg("Test task with prefix")
            .assert()
            .success()
            .get_output()
            .stdout
            .clone();

        // Extract the task ID from the output
        let output_str = String::from_utf8_lossy(&output);
        let task_id = output_str
            .lines()
            .find(|line| line.contains("Created task:"))
            .and_then(|line| line.split("Created task: ").nth(1))
            .expect("Should find task ID in output");

        // Use full task ID to get status
        let mut cmd = Command::cargo_bin("lotar").unwrap();
        cmd.current_dir(temp_dir)
            .arg("status")
            .arg(task_id)
            .assert()
            .success()
            .stdout(predicate::str::contains(format!("{} status: TODO", task_id)));
    }

    #[test]
    fn test_dual_interface_compatibility() {
        let test_fixtures = TestFixtures::new();
        let temp_dir = test_fixtures.temp_dir.path();

        // Create a task
        let mut cmd = Command::cargo_bin("lotar").unwrap();
        cmd.current_dir(temp_dir)
            .arg("add")
            .arg("Test dual interface")
            .assert()
            .success();

        // Set status using quick command
        let mut cmd = Command::cargo_bin("lotar").unwrap();
        cmd.current_dir(temp_dir)
            .arg("status")
            .arg("1")
            .arg("in_progress")
            .assert()
            .success();

        // Set status using full command (should still work)
        let mut cmd = Command::cargo_bin("lotar").unwrap();
        cmd.current_dir(temp_dir)
            .arg("task")
            .arg("status")
            .arg("1")
            .arg("done")
            .assert()
            .success();

        // Verify final status with quick command
        let mut cmd = Command::cargo_bin("lotar").unwrap();
        cmd.current_dir(temp_dir)
            .arg("status")
            .arg("1")
            .assert()
            .success()
            .stdout(predicate::str::contains("status: DONE"));
    }
}

// Test for default status assignment feature
// Tests that new tasks use the first status from config as default

use assert_cmd::Command;
use predicates::prelude::*;

mod common;
use common::TestFixtures;

#[cfg(test)]
mod default_status_tests {
    use super::*;

    #[test]
    fn test_new_task_uses_first_configured_status() {
        let test_fixtures = TestFixtures::new();
        let temp_dir = test_fixtures.temp_dir.path();

        // Initialize project with custom status order (InProgress first)
        let mut cmd = Command::cargo_bin("lotar").unwrap();
        cmd.current_dir(temp_dir)
            .arg("config")
            .arg("init")
            .arg("--project=custom-status")
            .assert()
            .success();

        // Set custom issue states with InProgress first
        let mut cmd = Command::cargo_bin("lotar").unwrap();
        cmd.current_dir(temp_dir)
            .arg("config")
            .arg("set")
            .arg("issue_states")
            .arg("IN_PROGRESS,TODO,DONE")
            .arg("--project=custom-status")
            .assert()
            .success();

        // Create a task (should use InProgress as default)
        let mut cmd = Command::cargo_bin("lotar").unwrap();
        cmd.current_dir(temp_dir)
            .arg("add")
            .arg("Test default status")
            .arg("--project=custom-status")
            .assert()
            .success();

        // Check that the task has InProgress status (first in config)
        let mut cmd = Command::cargo_bin("lotar").unwrap();
        cmd.current_dir(temp_dir)
            .arg("status")
            .arg("1")
            .arg("--project=custom-status")
            .assert()
            .success()
            .stdout(predicate::str::contains("status: IN_PROGRESS"));
    }

    #[test]
    fn test_explicit_default_status_config() {
        let test_fixtures = TestFixtures::new();
        let temp_dir = test_fixtures.temp_dir.path();

        // Initialize project
        let mut cmd = Command::cargo_bin("lotar").unwrap();
        cmd.current_dir(temp_dir)
            .arg("config")
            .arg("init")
            .arg("--project=explicit-default")
            .assert()
            .success();

        // Set explicit default status (different from first in list)
        let mut cmd = Command::cargo_bin("lotar").unwrap();
        cmd.current_dir(temp_dir)
            .arg("config")
            .arg("set")
            .arg("issue_states")
            .arg("TODO,IN_PROGRESS,DONE")
            .arg("--project=explicit-default")
            .assert()
            .success();

        let mut cmd = Command::cargo_bin("lotar").unwrap();
        cmd.current_dir(temp_dir)
            .arg("config")
            .arg("set")
            .arg("default_status")
            .arg("DONE")
            .arg("--project=explicit-default")
            .assert()
            .success();

        // Create a task (should use Done as default)
        let mut cmd = Command::cargo_bin("lotar").unwrap();
        cmd.current_dir(temp_dir)
            .arg("add")
            .arg("Test explicit default")
            .arg("--project=explicit-default")
            .assert()
            .success();

        // Check that the task has Done status (explicitly configured default)
        let mut cmd = Command::cargo_bin("lotar").unwrap();
        cmd.current_dir(temp_dir)
            .arg("status")
            .arg("1")
            .arg("--project=explicit-default")
            .assert()
            .success()
            .stdout(predicate::str::contains("status: DONE"));
    }

    #[test]
    fn test_fallback_to_todo_when_no_config() {
        let test_fixtures = TestFixtures::new();
        let temp_dir = test_fixtures.temp_dir.path();

        // Create a task without any special config (should fall back to Todo)
        let mut cmd = Command::cargo_bin("lotar").unwrap();
        cmd.current_dir(temp_dir)
            .arg("add")
            .arg("Test fallback default")
            .assert()
            .success();

        // Check that the task has Todo status (system default fallback)
        let mut cmd = Command::cargo_bin("lotar").unwrap();
        cmd.current_dir(temp_dir)
            .arg("status")
            .arg("1")
            .assert()
            .success()
            .stdout(predicate::str::contains("status: TODO"));
    }
}

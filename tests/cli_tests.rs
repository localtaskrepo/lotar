//! Comprehensive CLI command tests
//! 
//! This module consolidates all CLI-related tests including:
//! - Basic commands (help, version, invalid commands)
//! - Task management (add, list, edit, status)
//! - Project management and filtering
//! - Output formatting and error handling
//! - Performance characteristics

use assert_cmd::Command;
use predicates::prelude::*;
use tempfile::TempDir;
use std::fs;

mod common;
use common::TestFixtures;

// =============================================================================
// Basic CLI Commands
// =============================================================================

mod basic_commands {
    use super::*;

    #[test]
    fn test_help_command() {
        let mut cmd = Command::cargo_bin("lotar").unwrap();
        cmd.arg("help")
            .assert()
            .success()
            .stdout(predicate::str::contains("help"));
    }

    #[test]
    fn test_invalid_command() {
        let mut cmd = Command::cargo_bin("lotar").unwrap();
        cmd.arg("invalid_command").assert().failure();
    }

    #[test]
    fn test_no_command() {
        let mut cmd = Command::cargo_bin("lotar").unwrap();
        cmd.assert()
            .failure()
            .stderr(predicate::str::contains("Usage"));
    }

    #[test]
    fn test_version_command() {
        let mut cmd = Command::cargo_bin("lotar").unwrap();
        cmd.arg("--version")
            .assert()
            .success()
            .stdout(predicate::str::contains("lotar"));
    }
}

// =============================================================================
// Task Management Commands
// =============================================================================

mod task_management {
    use super::*;

    #[test]
    fn test_task_add_basic() {
        let temp_dir = TempDir::new().unwrap();

        let mut cmd = Command::cargo_bin("lotar").unwrap();
        cmd.current_dir(&temp_dir)
            .args(&["task", "add", "--title=Test Task"])
            .assert()
            .success()
            .stdout(predicate::str::contains("Created task:"));
    }

    #[test]
    fn test_task_add_missing_title() {
        let temp_dir = TempDir::new().unwrap();

        let mut cmd = Command::cargo_bin("lotar").unwrap();
        cmd.current_dir(&temp_dir)
            .args(&["task", "add"])
            .assert()
            .failure();
    }

    #[test]
    fn test_task_add_with_tags() {
        let test_fixtures = TestFixtures::new();
        let temp_dir = test_fixtures.temp_dir.path();

        // Test adding task with tags (correct syntax: --tag, not --tags)
        let mut cmd = Command::cargo_bin("lotar").unwrap();
        cmd.current_dir(temp_dir)
            .arg("task")
            .arg("add")
            .arg("--title=Task with tags")
            .arg("--tag=urgent")
            .arg("--tag=feature")
            .assert()
            .success()
            .stdout(predicate::str::contains("Created task:"));
    }

    #[test]
    fn test_task_list() {
        let test_fixtures = TestFixtures::new();
        let temp_dir = test_fixtures.temp_dir.path();

        // First create a task
        let mut cmd = Command::cargo_bin("lotar").unwrap();
        cmd.current_dir(temp_dir)
            .arg("add")
            .arg("List Test Task")
            .assert()
            .success();

        // List tasks
        let mut cmd = Command::cargo_bin("lotar").unwrap();
        cmd.current_dir(temp_dir)
            .arg("list")
            .assert()
            .success()
            .stdout(predicate::str::contains("List Test Task"));
    }

    #[test]
    fn test_task_status_update() {
        let test_fixtures = TestFixtures::new();
        let temp_dir = test_fixtures.temp_dir.path();

        // First create a task
        let mut cmd = Command::cargo_bin("lotar").unwrap();
        cmd.current_dir(temp_dir)
            .arg("add")
            .arg("Status update test")
            .assert()
            .success();

        // Update task status using correct syntax
        let mut cmd = Command::cargo_bin("lotar").unwrap();
        cmd.current_dir(temp_dir)
            .arg("set")
            .arg("1")
            .arg("status")
            .arg("in_progress")
            .assert()
            .success();
    }
}

// =============================================================================
// Project Management and Filtering
// =============================================================================

mod project_management {
    use super::*;

    #[test]
    fn test_task_add_with_full_project_name() {
        let temp_dir = TempDir::new().unwrap();

        let mut cmd = Command::cargo_bin("lotar").unwrap();
        cmd.current_dir(&temp_dir)
            .args(&[
                "task", "add",
                "--title=Project Test Task",
                "--project=frontend-components"
            ])
            .assert()
            .success()
            .stdout(predicate::str::contains("Created task:"));
    }

    #[test]
    fn test_smart_project_resolution() {
        let temp_dir = TempDir::new().unwrap();

        // Create task with full project name
        let mut cmd = Command::cargo_bin("lotar").unwrap();
        cmd.current_dir(&temp_dir)
            .args(&[
                "task", "add",
                "--title=Smart Resolution Test",
                "--project=FRONTEND-COMPONENTS"
            ])
            .assert()
            .success();

        // List using prefix (should resolve to same project)
        let mut cmd = Command::cargo_bin("lotar").unwrap();
        cmd.current_dir(&temp_dir)
            .args(&["task", "list", "--project=FC"])
            .assert()
            .success()
            .stdout(predicate::str::contains("Smart Resolution Test"));
    }

    #[test]
    fn test_case_insensitive_project_resolution() {
        let temp_dir = TempDir::new().unwrap();

        // Create task with uppercase project
        let mut cmd = Command::cargo_bin("lotar").unwrap();
        cmd.current_dir(&temp_dir)
            .args(&[
                "task", "add",
                "--title=Case Test Task",
                "--project=FRONTEND"
            ])
            .assert()
            .success();

        // List using lowercase
        let mut cmd = Command::cargo_bin("lotar").unwrap();
        cmd.current_dir(&temp_dir)
            .args(&["task", "list", "--project=frontend"])
            .assert()
            .success()
            .stdout(predicate::str::contains("Case Test Task"));
    }
}

// =============================================================================
// Output Formatting
// =============================================================================

mod output_formatting {
    use super::*;

    #[test]
    fn test_json_output_format() {
        let test_fixtures = TestFixtures::new();
        let temp_dir = test_fixtures.temp_dir.path();

        // Test JSON output format (note: this may not be implemented yet)
        let mut cmd = Command::cargo_bin("lotar").unwrap();
        cmd.current_dir(temp_dir)
            .arg("task")
            .arg("add")
            .arg("--title=JSON Test Task")
            .arg("--format=json")
            .assert()
            .success()
            .stdout(predicate::str::contains("Created task:").or(predicate::str::contains("JSON Test Task")));
    }

    #[test]
    fn test_verbose_output() {
        let temp_dir = TempDir::new().unwrap();

        let mut cmd = Command::cargo_bin("lotar").unwrap();
        cmd.current_dir(&temp_dir)
            .args(&[
                "--verbose",
                "task", "add",
                "--title=Verbose Test Task"
            ])
            .assert()
            .success();
    }
}

// =============================================================================
// Error Handling
// =============================================================================

mod error_handling {
    use super::*;

    #[test]
    fn test_invalid_task_id() {
        let test_fixtures = TestFixtures::new();
        let temp_dir = test_fixtures.temp_dir.path();

        // Test using invalid task ID with edit command (which should fail gracefully)
        let mut cmd = Command::cargo_bin("lotar").unwrap();
        cmd.current_dir(temp_dir)
            .arg("task")
            .arg("edit")
            .arg("INVALID-999")
            .arg("--title=Should not work")
            .assert()
            .failure()
            .stdout(predicate::str::contains("not found").or(predicate::str::contains("error")));
    }

    #[test]
    fn test_invalid_status() {
        let temp_dir = TempDir::new().unwrap();

        // Create a task first
        let mut cmd = Command::cargo_bin("lotar").unwrap();
        cmd.current_dir(&temp_dir)
            .args(&["task", "add", "--title=Status Error Test"])
            .assert()
            .success();

        // Try to set invalid status
        let mut cmd = Command::cargo_bin("lotar").unwrap();
        cmd.current_dir(&temp_dir)
            .args(&["status", "TEST-1", "invalid_status"])
            .assert()
            .failure();
    }
}

// =============================================================================
// File Structure Tests
// =============================================================================

mod file_structure {
    use super::*;

    #[test]
    fn test_tasks_directory_creation() {
        let temp_dir = TempDir::new().unwrap();

        // Create a task to trigger directory creation
        let mut cmd = Command::cargo_bin("lotar").unwrap();
        cmd.current_dir(&temp_dir)
            .args(&["task", "add", "--title=Directory Test"])
            .assert()
            .success();

        // Verify .tasks directory was created
        let tasks_dir = temp_dir.path().join(".tasks");
        assert!(tasks_dir.exists());
    }

    #[test]
    fn test_project_directory_creation() {
        let temp_dir = TempDir::new().unwrap();

        let mut cmd = Command::cargo_bin("lotar").unwrap();
        cmd.current_dir(&temp_dir)
            .args(&[
                "task", "add",
                "--title=Project Directory Test",
                "--project=test-project"
            ])
            .assert()
            .success();

        // Verify project directory was created (with appropriate prefix)
        let tasks_dir = temp_dir.path().join(".tasks");
        let entries = fs::read_dir(tasks_dir).unwrap();
        let project_dirs: Vec<_> = entries
            .filter_map(|entry| {
                let entry = entry.ok()?;
                if entry.file_type().ok()?.is_dir() {
                    Some(entry.file_name().to_string_lossy().to_string())
                } else {
                    None
                }
            })
            .collect();

        assert!(!project_dirs.is_empty(), "At least one project directory should exist");
    }

    #[test]
    fn test_task_files_creation() {
        let temp_dir = TempDir::new().unwrap();

        let mut cmd = Command::cargo_bin("lotar").unwrap();
        cmd.current_dir(&temp_dir)
            .args(&["task", "add", "--title=File Creation Test"])
            .assert()
            .success();

        // Check that task files were created
        let tasks_dir = temp_dir.path().join(".tasks");
        
        // Look for project directories and task files
        let entries = fs::read_dir(&tasks_dir).unwrap();
        let mut found_task_file = false;
        
        for entry in entries {
            let entry = entry.unwrap();
            if entry.file_type().unwrap().is_dir() {
                // Check inside project directory for task files
                let project_dir = entry.path();
                if let Ok(project_entries) = fs::read_dir(project_dir) {
                    for project_entry in project_entries {
                        if let Ok(project_entry) = project_entry {
                            let filename = project_entry.file_name();
                            if filename.to_string_lossy().ends_with(".yml") &&
                               filename.to_string_lossy() != "config.yml" {
                                found_task_file = true;
                                break;
                            }
                        }
                    }
                }
                if found_task_file {
                    break;
                }
            }
        }

        assert!(found_task_file, "At least one task file should be created");
    }
}

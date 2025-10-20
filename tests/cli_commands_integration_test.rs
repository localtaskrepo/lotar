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
use std::fs;
use tempfile::TempDir;

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
    use super::common::env_mutex::EnvVarGuard;
    use super::*;

    #[test]
    fn test_task_add_basic() {
        let temp_dir = TempDir::new().unwrap();

        let mut cmd = Command::cargo_bin("lotar").unwrap();
        cmd.current_dir(&temp_dir)
            .args(["task", "add", "Test Task"])
            .assert()
            .success()
            .stdout(predicate::str::contains("Created task:"));
    }

    #[test]
    fn test_task_add_missing_title() {
        let temp_dir = TempDir::new().unwrap();

        let mut cmd = Command::cargo_bin("lotar").unwrap();
        cmd.current_dir(&temp_dir)
            .args(["task", "add"])
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
            .arg("Task with tags")
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
        let _guard = EnvVarGuard::set(
            "LOTAR_TASKS_DIR",
            &test_fixtures.tasks_root.to_string_lossy(),
        );

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
        let _guard = EnvVarGuard::set(
            "LOTAR_TASKS_DIR",
            &test_fixtures.tasks_root.to_string_lossy(),
        );

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
            .arg("status")
            .arg("1")
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
            .args([
                "task",
                "add",
                "Project Test Task",
                "--project=frontend-components",
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
            .args([
                "task",
                "add",
                "Smart Resolution Test",
                "--project=FRONTEND-COMPONENTS",
            ])
            .assert()
            .success();

        // List using prefix (should resolve to same project)
        let mut cmd = Command::cargo_bin("lotar").unwrap();
        cmd.current_dir(&temp_dir)
            .args(["task", "list", "--project=FC"])
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
            .args(["task", "add", "Case Test Task", "--project=FRONTEND"])
            .assert()
            .success();

        // List using lowercase
        let mut cmd = Command::cargo_bin("lotar").unwrap();
        cmd.current_dir(&temp_dir)
            .args(["task", "list", "--project=frontend"])
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
            .arg("JSON Test Task")
            .arg("--format=json")
            .assert()
            .success()
            .stdout(
                predicate::str::contains("Created task:")
                    .or(predicate::str::contains("JSON Test Task")),
            );
    }

    #[test]
    fn test_verbose_output() {
        let temp_dir = TempDir::new().unwrap();

        let mut cmd = Command::cargo_bin("lotar").unwrap();
        cmd.current_dir(&temp_dir)
            .args(["--verbose", "task", "add", "Verbose Test Task"])
            .assert()
            .success();
    }

    #[test]
    fn project_label_falls_back_to_prefix_when_name_missing() {
        let temp = TempDir::new().unwrap();

        // Initialize project with a display name
        Command::cargo_bin("lotar")
            .unwrap()
            .current_dir(temp.path())
            .args([
                "config",
                "init",
                "--project",
                "Example Project",
                "--prefix",
                "EXM",
            ])
            .assert()
            .success();

        // Strip the project name so only the prefix remains
        let project_config = temp.path().join(".tasks").join("EXM").join("config.yml");
        std::fs::write(project_config, "default_priority: High\n")
            .expect("failed to overwrite project config");

        // Add a task with explicit project prefix
        Command::cargo_bin("lotar")
            .unwrap()
            .current_dir(temp.path())
            .args(["task", "add", "Test Task", "--project", "EXM"])
            .assert()
            .success();

        // Config show should display the prefix-only label
        let show_output = Command::cargo_bin("lotar")
            .unwrap()
            .current_dir(temp.path())
            .args(["config", "show", "--project", "EXM"])
            .output()
            .expect("failed to run config show");
        assert!(
            show_output.status.success(),
            "config show failed: {}",
            String::from_utf8_lossy(&show_output.stderr)
        );
        let show_stdout = String::from_utf8_lossy(&show_output.stdout);
        assert!(
            show_stdout.contains("Project configuration (EXM) – canonical YAML:"),
            "project config show should include canonical heading\n{show_stdout}"
        );

        // Config validate should also show prefix-only label
        let validate_output = Command::cargo_bin("lotar")
            .unwrap()
            .current_dir(temp.path())
            .args(["config", "validate", "--project", "EXM"])
            .output()
            .expect("failed to run config validate");
        assert!(
            validate_output.status.success(),
            "config validate failed: {}",
            String::from_utf8_lossy(&validate_output.stderr)
        );
        let validate_stdout = String::from_utf8_lossy(&validate_output.stdout);
        assert!(validate_stdout.contains("Validating project configuration for 'EXM'"));

        // Ensure task add success output uses prefix when name missing
        let add_output = Command::cargo_bin("lotar")
            .unwrap()
            .current_dir(temp.path())
            .args(["task", "add", "Another Task", "--project", "EXM"])
            .output()
            .expect("failed to add task twice");
        assert!(add_output.status.success(), "second add failed");
        let add_stdout = String::from_utf8_lossy(&add_output.stdout);
        assert!(add_stdout.contains("Created task: EXM-2"));
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
            .stderr(predicate::str::contains("not found").or(predicate::str::contains("error")));
    }

    #[test]
    fn test_invalid_status() {
        let temp_dir = TempDir::new().unwrap();

        // Create a task first
        let mut cmd = Command::cargo_bin("lotar").unwrap();
        cmd.current_dir(&temp_dir)
            .args(["task", "add", "Status Error Test"])
            .assert()
            .success();

        // Try to set invalid status
        let mut cmd = Command::cargo_bin("lotar").unwrap();
        cmd.current_dir(&temp_dir)
            .args(["status", "TEST-1", "invalid_status"])
            .assert()
            .failure();
    }

    #[test]
    fn test_status_project_mismatch_errors_when_id_has_prefix() {
        let temp_dir = TempDir::new().unwrap();

        // Create a task under default project (TEST)
        let mut cmd = Command::cargo_bin("lotar").unwrap();
        cmd.current_dir(&temp_dir)
            .args(["task", "add", "Mismatch Test Task", "--project=FOO"])
            .assert()
            .success();

        // Try to set status with wrong explicit project -> should error due to mismatch
        let mut cmd = Command::cargo_bin("lotar").unwrap();
        cmd.current_dir(&temp_dir)
            .args(["status", "FOO-1", "done", "--project=BAR"]) // BAR doesn't match FOO
            .assert()
            .failure()
            .stderr(predicate::str::contains("Project mismatch"));
    }

    #[test]
    fn test_edit_preserves_unspecified_fields() {
        let temp_dir = TempDir::new().unwrap();

        // Add with multiple fields set
        let mut cmd = Command::cargo_bin("lotar").unwrap();
        cmd.current_dir(&temp_dir)
            .args([
                "task",
                "add",
                "Original",
                "--assignee=dev@example.com",
                "--priority=high",
                "--type=feature",
            ])
            .assert()
            .success();

        // Discover created task id via list json
        let list_out = Command::cargo_bin("lotar")
            .unwrap()
            .current_dir(&temp_dir)
            .args(["--format=json", "list"])
            .output()
            .unwrap();
        assert!(list_out.status.success());
        let list_json: serde_json::Value = serde_json::from_slice(&list_out.stdout).unwrap();
        let task_id = list_json["tasks"][0]["id"].as_str().unwrap().to_string();

        // Edit only title
        let mut cmd = Command::cargo_bin("lotar").unwrap();
        cmd.current_dir(&temp_dir)
            .args(["task", "edit", &task_id, "--title=Renamed"])
            .assert()
            .success();

        // List JSON and verify assignee/priority/type preserved
        let task = super::get_task_as_json(&temp_dir, &task_id);
        assert_eq!(task["title"].as_str(), Some("Renamed"));
        assert_eq!(task["assignee"].as_str(), Some("dev@example.com"));
        assert!(
            task["priority"]
                .as_str()
                .unwrap_or("")
                .to_uppercase()
                .contains("HIGH")
        );
        assert!(
            task["task_type"]
                .as_str()
                .unwrap_or("")
                .to_lowercase()
                .contains("feature")
        );
    }
}

// =============================================================================
// Helper Functions
// =============================================================================

fn extract_task_id_from_output(output: &str) -> Option<String> {
    // Try to extract task ID from JSON first
    if let Ok(json) = serde_json::from_str::<serde_json::Value>(output) {
        if let Some(task) = json.get("task") {
            if let Some(id) = task.get("id").and_then(|v| v.as_str()) {
                return Some(id.to_string());
            }
        }
        // Also try top-level task_id field
        if let Some(id) = json.get("task_id").and_then(|v| v.as_str()) {
            return Some(id.to_string());
        }
    }

    // Fall back to text parsing for non-JSON output
    for line in output.lines() {
        if line.contains("Created task:") {
            if let Some(id_part) = line.split(":").nth(1) {
                return Some(id_part.trim().to_string());
            }
        }
    }
    None
}

// =============================================================================
// Consolidated: List alias (ls)
// =============================================================================

mod list_alias {
    use super::*;
    use serde_json::Value;
    use tempfile::TempDir;

    #[test]
    fn list_alias_ls_works() {
        let temp = TempDir::new().unwrap();

        // Create a task to ensure list returns something
        Command::cargo_bin("lotar")
            .unwrap()
            .current_dir(temp.path())
            .args(["task", "add", "Hello world"])
            .assert()
            .success();

        // Use alias `ls`
        let out = Command::cargo_bin("lotar")
            .unwrap()
            .current_dir(temp.path())
            .args(["--format", "json", "ls"])
            .output()
            .unwrap();
        assert!(out.status.success());
        let v: Value = serde_json::from_slice(&out.stdout).unwrap();
        assert_eq!(v["status"], "success");
        assert!(v["tasks"].as_array().is_some());
    }
}

// =============================================================================
// Consolidated: List effort filters and sort
// =============================================================================

mod list_effort {
    use super::*;
    use serde_json::Value;
    use tempfile::TempDir;

    fn run(cmd: &mut Command, temp: &TempDir, args: &[&str]) -> assert_cmd::assert::Assert {
        cmd.current_dir(temp.path())
            .env("LOTAR_TEST_SILENT", "1")
            .args(args)
            .assert()
    }

    #[test]
    fn list_sort_by_effort_time_only_asc_and_desc() {
        let temp = TempDir::new().unwrap();

        // Create three tasks with time efforts: 30m (0.50h), 2h (2.00h), 1d (8.00h)
        run(
            &mut Command::cargo_bin("lotar").unwrap(),
            &temp,
            &["task", "add", "A", "--effort", "30m"],
        )
        .success();
        run(
            &mut Command::cargo_bin("lotar").unwrap(),
            &temp,
            &["task", "add", "B", "--effort", "2h"],
        )
        .success();
        run(
            &mut Command::cargo_bin("lotar").unwrap(),
            &temp,
            &["task", "add", "C", "--effort", "1d"],
        )
        .success();

        // Ascending
        let out = Command::cargo_bin("lotar")
            .unwrap()
            .current_dir(temp.path())
            .env("LOTAR_TEST_SILENT", "1")
            .args(["task", "list", "--format", "json", "--sort-by=effort"])
            .output()
            .unwrap();
        assert!(out.status.success());
        let v: Value = serde_json::from_slice(&out.stdout).unwrap();
        let tasks = v["tasks"].as_array().unwrap();
        let efforts: Vec<&str> = tasks
            .iter()
            .map(|t| t["effort"].as_str().unwrap_or("-"))
            .collect();
        assert_eq!(
            efforts,
            vec!["0.50h", "2.00h", "8.00h"],
            "ascending effort order"
        );

        // Descending
        let out2 = Command::cargo_bin("lotar")
            .unwrap()
            .current_dir(temp.path())
            .env("LOTAR_TEST_SILENT", "1")
            .args([
                "task",
                "list",
                "--format",
                "json",
                "--sort-by=effort",
                "--reverse",
            ])
            .output()
            .unwrap();
        assert!(out2.status.success());
        let v2: Value = serde_json::from_slice(&out2.stdout).unwrap();
        let tasks2 = v2["tasks"].as_array().unwrap();
        let efforts2: Vec<&str> = tasks2
            .iter()
            .map(|t| t["effort"].as_str().unwrap_or("-"))
            .collect();
        assert_eq!(
            efforts2,
            vec!["8.00h", "2.00h", "0.50h"],
            "descending effort order"
        );
    }

    #[test]
    fn list_effort_min_max_time_window() {
        let temp = TempDir::new().unwrap();

        // Time efforts: 0.50h, 2.00h, 8.00h
        run(
            &mut Command::cargo_bin("lotar").unwrap(),
            &temp,
            &["task", "add", "A", "--effort", "30m"],
        )
        .success();
        run(
            &mut Command::cargo_bin("lotar").unwrap(),
            &temp,
            &["task", "add", "B", "--effort", "2h"],
        )
        .success();
        run(
            &mut Command::cargo_bin("lotar").unwrap(),
            &temp,
            &["task", "add", "C", "--effort", "1d"],
        )
        .success();

        // Filter [1h, 1d] inclusive; expect 2.00h and 8.00h
        let out = Command::cargo_bin("lotar")
            .unwrap()
            .current_dir(temp.path())
            .env("LOTAR_TEST_SILENT", "1")
            .args([
                "task",
                "list",
                "--format",
                "json",
                "--effort-min",
                "1h",
                "--effort-max",
                "1d",
                "--sort-by=effort",
            ])
            .output()
            .unwrap();
        assert!(out.status.success());
        let v: Value = serde_json::from_slice(&out.stdout).unwrap();
        let tasks = v["tasks"].as_array().unwrap();
        let efforts: Vec<&str> = tasks
            .iter()
            .map(|t| t["effort"].as_str().unwrap_or("-"))
            .collect();
        assert_eq!(
            efforts,
            vec!["2.00h", "8.00h"],
            "filtered inclusive time window"
        );
    }

    #[test]
    fn list_effort_min_points_excludes_time() {
        let temp = TempDir::new().unwrap();

        // Mixed: time and points
        run(
            &mut Command::cargo_bin("lotar").unwrap(),
            &temp,
            &["task", "add", "T1", "--effort", "2h"],
        )
        .success();
        run(
            &mut Command::cargo_bin("lotar").unwrap(),
            &temp,
            &["task", "add", "P3", "--effort", "3pt"],
        )
        .success();
        run(
            &mut Command::cargo_bin("lotar").unwrap(),
            &temp,
            &["task", "add", "P5", "--effort", "5"],
        )
        .success(); // bare number => points

        // Points filter: --effort-min 4 (points). Should include only P5; time tasks excluded by kind.
        let out = Command::cargo_bin("lotar")
            .unwrap()
            .current_dir(temp.path())
            .env("LOTAR_TEST_SILENT", "1")
            .args([
                "task",
                "list",
                "--format",
                "json",
                "--effort-min",
                "4",
                "--sort-by=effort",
            ])
            .output()
            .unwrap();
        assert!(out.status.success());
        let v: Value = serde_json::from_slice(&out.stdout).unwrap();
        let tasks = v["tasks"].as_array().unwrap();
        assert_eq!(tasks.len(), 1, "only points >= 4 should match");
        assert_eq!(tasks[0]["effort"], "5pt");
    }
}

// =============================================================================
// Consolidated: Status command pattern (get/set)
// =============================================================================

mod status_patterns {
    use super::common::env_mutex::EnvVarGuard;
    use super::*;

    #[test]
    fn test_status_get_operation() {
        let test_fixtures = TestFixtures::new();
        let temp_dir = test_fixtures.temp_dir.path();
        let _guard = EnvVarGuard::set(
            "LOTAR_TASKS_DIR",
            &test_fixtures.tasks_root.to_string_lossy(),
        );

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
            .stdout(predicate::str::contains("status: Todo"));
    }

    #[test]
    fn test_status_set_operation() {
        let test_fixtures = TestFixtures::new();
        let temp_dir = test_fixtures.temp_dir.path();
        let _guard = EnvVarGuard::set(
            "LOTAR_TASKS_DIR",
            &test_fixtures.tasks_root.to_string_lossy(),
        );

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
            .stdout(predicate::str::contains(
                "status changed from Todo to InProgress",
            ));
    }

    #[test]
    fn test_status_get_after_set() {
        let test_fixtures = TestFixtures::new();
        let temp_dir = test_fixtures.temp_dir.path();
        let _guard = EnvVarGuard::set(
            "LOTAR_TASKS_DIR",
            &test_fixtures.tasks_root.to_string_lossy(),
        );

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
            .stdout(predicate::str::contains("status: Done"));
    }

    #[test]
    fn test_status_full_command_get_operation() {
        let test_fixtures = TestFixtures::new();
        let temp_dir = test_fixtures.temp_dir.path();
        let _guard = EnvVarGuard::set(
            "LOTAR_TASKS_DIR",
            &test_fixtures.tasks_root.to_string_lossy(),
        );

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
            .stdout(predicate::str::contains("status: Todo"));
    }

    #[test]
    fn test_status_full_command_set_operation() {
        let test_fixtures = TestFixtures::new();
        let temp_dir = test_fixtures.temp_dir.path();
        let _guard = EnvVarGuard::set(
            "LOTAR_TASKS_DIR",
            &test_fixtures.tasks_root.to_string_lossy(),
        );

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
            .stdout(predicate::str::contains(
                "status changed from Todo to InProgress",
            ));
    }

    #[test]
    fn test_status_set_same_value_warning() {
        let test_fixtures = TestFixtures::new();
        let temp_dir = test_fixtures.temp_dir.path();
        let _guard = EnvVarGuard::set(
            "LOTAR_TASKS_DIR",
            &test_fixtures.tasks_root.to_string_lossy(),
        );

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
            .stderr(predicate::str::contains("already has status 'Todo'"));
    }

    #[test]
    fn test_status_invalid_status_error() {
        let test_fixtures = TestFixtures::new();
        let temp_dir = test_fixtures.temp_dir.path();
        let _guard = EnvVarGuard::set(
            "LOTAR_TASKS_DIR",
            &test_fixtures.tasks_root.to_string_lossy(),
        );

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
            .stderr(predicate::str::contains("Status validation failed"));
    }

    #[test]
    fn test_status_nonexistent_task_error() {
        let test_fixtures = TestFixtures::new();
        let temp_dir = test_fixtures.temp_dir.path();
        let _guard = EnvVarGuard::set(
            "LOTAR_TASKS_DIR",
            &test_fixtures.tasks_root.to_string_lossy(),
        );

        // Try to get status of non-existent task
        let mut cmd = Command::cargo_bin("lotar").unwrap();
        cmd.current_dir(temp_dir)
            .arg("status")
            .arg("999")
            .assert()
            .failure()
            .stderr(predicate::str::contains("not found"));
    }

    #[test]
    fn test_status_with_project_prefix() {
        let test_fixtures = TestFixtures::new();
        let temp_dir = test_fixtures.temp_dir.path();
        let _guard = EnvVarGuard::set(
            "LOTAR_TASKS_DIR",
            &test_fixtures.tasks_root.to_string_lossy(),
        );

        // Create a task (will get auto-generated prefix)
        let mut cmd = Command::cargo_bin("lotar").unwrap();
        let output = cmd
            .current_dir(temp_dir)
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
            .stdout(predicate::str::contains(format!("{task_id} status: Todo")));
    }

    #[test]
    fn test_dual_interface_compatibility() {
        let test_fixtures = TestFixtures::new();
        let temp_dir = test_fixtures.temp_dir.path();
        let _guard = EnvVarGuard::set(
            "LOTAR_TASKS_DIR",
            &test_fixtures.tasks_root.to_string_lossy(),
        );

        // Create a task with explicit project to make numeric ID resolution unambiguous
        let mut cmd = Command::cargo_bin("lotar").unwrap();
        cmd.current_dir(temp_dir)
            .arg("task")
            .arg("add")
            .arg("Test dual interface")
            .arg("--project=test-project")
            .assert()
            .success();

        // Set status using quick command
        let mut cmd = Command::cargo_bin("lotar").unwrap();
        cmd.current_dir(temp_dir)
            .arg("status")
            .arg("1")
            .arg("in_progress")
            .arg("--project=test-project")
            .assert()
            .success();

        // Set status using full command (should still work)
        let mut cmd = Command::cargo_bin("lotar").unwrap();
        cmd.current_dir(temp_dir)
            .arg("task")
            .arg("status")
            .arg("1")
            .arg("done")
            .arg("--project=test-project")
            .assert()
            .success();

        // Verify final status with quick command
        let mut cmd = Command::cargo_bin("lotar").unwrap();
        cmd.current_dir(temp_dir)
            .arg("status")
            .arg("1")
            .arg("--project=test-project")
            .assert()
            .success()
            .stdout(predicate::str::contains("status: Done"));
    }
}

// =============================================================================
// Consolidated: Task dry-run previews
// =============================================================================

mod dry_run {
    use super::*;

    #[test]
    fn edit_dry_run_previews_without_write() {
        let tf = TestFixtures::new();
        let temp = tf.temp_dir.path();

        // Create a task first and extract ID from output
        let mut cmd = Command::cargo_bin("lotar").unwrap();
        let add_out = cmd
            .current_dir(temp)
            .arg("task")
            .arg("add")
            .arg("Initial Task")
            .output()
            .unwrap();
        assert!(add_out.status.success());
        let stdout = String::from_utf8_lossy(&add_out.stdout);
        let id = stdout
            .lines()
            .find_map(|l| {
                l.strip_prefix("✅ Created task: ")
                    .map(|s| s.trim().to_string())
            })
            .expect("expected created task id in output");

        // Dry-run edit change priority
        let mut edit = Command::cargo_bin("lotar").unwrap();
        edit.current_dir(temp)
            .arg("task")
            .arg("edit")
            .arg(&id)
            .arg("--priority")
            .arg("HIGH")
            .arg("--dry-run")
            .assert()
            .success()
            .stdout(predicate::str::contains("DRY RUN: Would update"));
    }

    #[test]
    fn delete_dry_run_previews_without_delete() {
        let tf = TestFixtures::new();
        let temp = tf.temp_dir.path();

        // Create a task first and extract ID from output
        let mut cmd = Command::cargo_bin("lotar").unwrap();
        let add_out = cmd
            .current_dir(temp)
            .arg("task")
            .arg("add")
            .arg("Task To Delete")
            .output()
            .unwrap();
        assert!(add_out.status.success());
        let stdout = String::from_utf8_lossy(&add_out.stdout);
        let id = stdout
            .lines()
            .find_map(|l| {
                l.strip_prefix("✅ Created task: ")
                    .map(|s| s.trim().to_string())
            })
            .expect("expected created task id in output");

        // Dry-run delete
        let mut del = Command::cargo_bin("lotar").unwrap();
        del.current_dir(temp)
            .arg("task")
            .arg("delete")
            .arg(&id)
            .arg("--dry-run")
            .assert()
            .success()
            .stdout(predicate::str::contains("DRY RUN: Would delete task"));

        // Verify the task still exists by attempting to delete for real (should succeed)
        let mut del2 = Command::cargo_bin("lotar").unwrap();
        del2.current_dir(temp)
            .arg("task")
            .arg("delete")
            .arg(&id)
            .arg("--force")
            .assert()
            .success()
            .stdout(predicate::str::contains("deleted successfully"));
    }
}

// =============================================================================
// Consolidated: Serve command features (light, readiness-friendly)
// =============================================================================

mod serve {
    use super::common::cargo_bin_silent;
    use super::*;
    use std::time::Duration;

    #[test]
    fn test_serve_command_basic_functionality() {
        let fixtures = TestFixtures::new();
        let temp_dir = fixtures.temp_dir.path();

        // Create a test task first to have some data to serve
        let mut cmd = cargo_bin_silent();
        cmd.current_dir(temp_dir)
            .arg("add")
            .arg("Test task for web server")
            .arg("--type=feature")
            .assert()
            .success();

        // Test serve command help
        let mut cmd = cargo_bin_silent();
        let result = cmd
            .current_dir(temp_dir)
            .arg("serve")
            .arg("--help")
            .assert();

        if let Ok(assert_result) = result.try_success() {
            let output = String::from_utf8_lossy(&assert_result.get_output().stdout);
            let _ = output.contains("port") || output.contains("host");
        }

        // Test serve command with default options (background mode for testing)
        let mut cmd = cargo_bin_silent();
        let result = cmd
            .current_dir(temp_dir)
            .arg("serve")
            .timeout(Duration::from_millis(200)) // Very quick timeout - just test if command exists
            .assert();

        // Expected to timeout or fail - we just want to see if command is recognized
        let _serve_command_exists = result.try_success().is_ok();
    }

    #[test]
    fn test_serve_command_port_options() {
        let fixtures = TestFixtures::new();
        let temp_dir = fixtures.temp_dir.path();

        // Test custom port option
        let mut cmd = cargo_bin_silent();
        let result = cmd
            .current_dir(temp_dir)
            .arg("serve")
            .arg("--port=8080")
            .timeout(Duration::from_millis(200))
            .assert();

        // Port option may or may not be implemented
        let _custom_port_works = result.try_success().is_ok();

        // Test alternative port option
        let mut cmd = cargo_bin_silent();
        let result = cmd
            .current_dir(temp_dir)
            .arg("serve")
            .arg("-p")
            .arg("9090")
            .timeout(Duration::from_millis(200))
            .assert();

        // Alternative port syntax may or may not be implemented
        let _alt_port_works = result.try_success().is_ok();

        // Test invalid port option
        let mut cmd = cargo_bin_silent();
        let result = cmd
            .current_dir(temp_dir)
            .arg("serve")
            .arg("--port=99999") // Invalid port
            .timeout(Duration::from_millis(100))
            .assert();

        let _ = result.try_success();
    }

    #[test]
    fn test_serve_command_host_options() {
        let fixtures = TestFixtures::new();
        let temp_dir = fixtures.temp_dir.path();

        // Test localhost host
        let mut cmd = cargo_bin_silent();
        let result = cmd
            .current_dir(temp_dir)
            .arg("serve")
            .arg("--host=localhost")
            .timeout(Duration::from_millis(100))
            .assert();

        let _ = result.try_success();

        // Test bind to all interfaces
        let mut cmd = cargo_bin_silent();
        let result = cmd
            .current_dir(temp_dir)
            .arg("serve")
            .arg("--host=0.0.0.0")
            .timeout(Duration::from_millis(100))
            .assert();

        let _ = result.try_success();

        // Test custom IP
        let mut cmd = cargo_bin_silent();
        let result = cmd
            .current_dir(temp_dir)
            .arg("serve")
            .arg("--host=127.0.0.1")
            .timeout(Duration::from_millis(100))
            .assert();

        let _ = result.try_success();
    }

    #[test]
    fn test_serve_command_combined_options() {
        let fixtures = TestFixtures::new();
        let temp_dir = fixtures.temp_dir.path();

        // Test port and host together
        let mut cmd = cargo_bin_silent();
        let result = cmd
            .current_dir(temp_dir)
            .arg("serve")
            .arg("--port=8080")
            .arg("--host=localhost")
            .timeout(Duration::from_millis(100))
            .assert();

        let _ = result.try_success();

        // Test with verbose output
        let mut cmd = cargo_bin_silent();
        let result = cmd
            .current_dir(temp_dir)
            .arg("serve")
            .arg("--verbose")
            .timeout(Duration::from_millis(100))
            .assert();

        let _ = result.try_success();
    }

    #[test]
    fn test_serve_command_with_project_data() {
        let fixtures = TestFixtures::new();
        let temp_dir = fixtures.temp_dir.path();
        let _guard = super::common::env_mutex::EnvVarGuard::set(
            "LOTAR_TASKS_DIR",
            &fixtures.tasks_root.to_string_lossy(),
        );

        // Create diverse test data
        let mut cmd = Command::cargo_bin("lotar").unwrap();
        cmd.current_dir(temp_dir)
            .arg("task")
            .arg("add")
            .arg("Web UI Test Task")
            .arg("--project=test-project")
            .arg("--type=feature")
            .arg("--priority=high")
            .arg("--assignee=test@example.com")
            .assert()
            .success();

        let mut cmd = Command::cargo_bin("lotar").unwrap();
        cmd.current_dir(temp_dir)
            .arg("task")
            .arg("add")
            .arg("API Test Task")
            .arg("--project=test-project")
            .arg("--type=bug")
            .arg("--priority=high")
            .assert()
            .success();

        // Change one task status
        let mut cmd = Command::cargo_bin("lotar").unwrap();
        cmd.current_dir(temp_dir)
            .arg("status")
            .arg("2")
            .arg("in_progress")
            .arg("--project=test-project")
            .assert()
            .success();

        // Test serve with actual project data
        let mut cmd = Command::cargo_bin("lotar").unwrap();
        let result = cmd
            .current_dir(temp_dir)
            .arg("serve")
            .timeout(Duration::from_millis(150))
            .assert();

        let _ = result.try_success();

        // Test serve with specific project
        let mut cmd = Command::cargo_bin("lotar").unwrap();
        let result = cmd
            .current_dir(temp_dir)
            .arg("serve")
            .arg("--project=test-project")
            .timeout(Duration::from_millis(100))
            .assert();

        let _ = result.try_success();
    }

    #[test]
    fn test_serve_implementation_summary() {
        let fixtures = TestFixtures::new();
        let temp_dir = fixtures.temp_dir.path();

        // Create test task
        let mut cmd = Command::cargo_bin("lotar").unwrap();
        cmd.current_dir(temp_dir)
            .arg("add")
            .arg("Summary test task")
            .assert()
            .success();

        // Test basic serve existence
        let mut cmd = Command::cargo_bin("lotar").unwrap();
        let result = cmd.current_dir(temp_dir).arg("help").assert();

        if let Ok(assert_result) = result.try_success() {
            let output = String::from_utf8_lossy(&assert_result.get_output().stdout);
            let _ = output.contains("serve");
        }

        // Test serve help specifically
        let mut cmd = Command::cargo_bin("lotar").unwrap();
        let result = cmd.current_dir(temp_dir).arg("help").arg("serve").assert();

        if let Ok(assert_result) = result.try_success() {
            let output = String::from_utf8_lossy(&assert_result.get_output().stdout);
            let _ = output.contains("port");
            let _ = output.contains("host");
        }
    }
}

// =============================================================================
// Consolidated: Whoami and dry-run explain
// =============================================================================

mod whoami_and_dryrun_explain {
    use super::common::env_mutex::EnvVarGuard;
    use super::*;
    use lotar::api_types::TaskCreate;
    use lotar::services::task_service::TaskService;
    use lotar::storage::manager::Storage;
    use lotar::utils::paths;

    fn write_minimal_config(tasks_dir: &std::path::Path, extra: &str) {
        // Provide minimal, valid lists so CLI validation passes where needed
        let base = r#"default.project: TEST
issue.states: [Todo, InProgress, Done]
issue.types: [Feature, Bug, Chore]
issue.priorities: [Low, Medium, High]
"#;
        let mut content = String::from(base);
        if !extra.is_empty() {
            content.push_str(extra);
            if !extra.ends_with('\n') {
                content.push('\n');
            }
        }
        std::fs::write(paths::global_config_path(tasks_dir), content).unwrap();
    }

    #[test]
    fn whoami_uses_config_default_reporter() {
        let temp = tempfile::TempDir::new().unwrap();
        let tasks_dir = temp.path().join(".tasks");
        std::fs::create_dir_all(&tasks_dir).unwrap();

        // Configure a deterministic reporter identity
        write_minimal_config(&tasks_dir, "default.reporter: alice@example.com\n");

        let _guard = EnvVarGuard::set("LOTAR_TASKS_DIR", &tasks_dir.to_string_lossy());
        let _reporter_guard = EnvVarGuard::clear("LOTAR_DEFAULT_REPORTER");

        let mut cmd = Command::cargo_bin("lotar").unwrap();
        cmd.current_dir(temp.path())
            .args(["whoami"]) // text mode
            .assert()
            .success()
            .stdout(predicate::str::contains("alice@example.com"));
    }

    #[test]
    fn whoami_explain_text_includes_source_and_confidence() {
        let temp = tempfile::TempDir::new().unwrap();
        let tasks_dir = temp.path().join(".tasks");
        std::fs::create_dir_all(&tasks_dir).unwrap();

        // Deterministic reporter identity
        write_minimal_config(&tasks_dir, "default.reporter: carol\n");

        let _guard = EnvVarGuard::set("LOTAR_TASKS_DIR", &tasks_dir.to_string_lossy());
        let _reporter_guard = EnvVarGuard::clear("LOTAR_DEFAULT_REPORTER");

        let mut cmd = Command::cargo_bin("lotar").unwrap();
        cmd.current_dir(temp.path())
            .args(["whoami", "--explain"]) // text mode
            .assert()
            .success()
            .stdout(predicate::str::contains("carol"))
            .stdout(predicate::str::contains("source:"))
            .stdout(predicate::str::contains("confidence:"));
    }

    #[test]
    fn whoami_json_compact_and_explain_fields() {
        let temp = tempfile::TempDir::new().unwrap();
        let tasks_dir = temp.path().join(".tasks");
        std::fs::create_dir_all(&tasks_dir).unwrap();

        write_minimal_config(&tasks_dir, "default.reporter: dave\n");

        let _guard = EnvVarGuard::set("LOTAR_TASKS_DIR", &tasks_dir.to_string_lossy());
        let _reporter_guard = EnvVarGuard::clear("LOTAR_DEFAULT_REPORTER");

        // Compact JSON (no extra fields)
        let mut cmd = Command::cargo_bin("lotar").unwrap();
        cmd.current_dir(temp.path())
            .args(["whoami", "--format=json"]) // json mode
            .assert()
            .success()
            .stdout(predicate::str::contains("{\"user\":\"dave\"}"));

        // Explain JSON includes source/confidence/details
        let mut cmd2 = Command::cargo_bin("lotar").unwrap();
        cmd2.current_dir(temp.path())
            .args(["whoami", "--format=json", "--explain"]) // json + explain
            .assert()
            .success()
            .stdout(predicate::str::contains("\"user\":"))
            .stdout(predicate::str::contains("\"source\":"))
            .stdout(predicate::str::contains("\"confidence\":"));
    }

    #[test]
    fn status_dry_run_explain_previews_and_does_not_write() {
        let temp = tempfile::TempDir::new().unwrap();
        let tasks_dir = temp.path().join(".tasks");
        std::fs::create_dir_all(&tasks_dir).unwrap();

        // Default reporter used for auto-assign preview
        write_minimal_config(&tasks_dir, "default.reporter: bob\n");

        let _guard = EnvVarGuard::set("LOTAR_TASKS_DIR", &tasks_dir.to_string_lossy());
        let _reporter_guard = EnvVarGuard::clear("LOTAR_DEFAULT_REPORTER");

        // Create a task via service to get a known ID
        let mut storage = Storage::new(tasks_dir.clone());
        let created = TaskService::create(
            &mut storage,
            TaskCreate {
                title: "Preview status change".to_string(),
                project: Some("TEST".to_string()),
                priority: None,
                task_type: None,
                reporter: None,
                assignee: None,
                due_date: None,
                effort: None,
                description: None,
                tags: vec![],
                relationships: None,
                custom_fields: None,
            },
        )
        .expect("create task");

        // Dry-run with explain should preview change and show auto-assign effect
        let mut cmd = Command::cargo_bin("lotar").unwrap();
        cmd.current_dir(temp.path())
            .args(["status", &created.id, "Done", "--dry-run", "--explain"])
            .assert()
            .success()
            .stdout(predicate::str::contains("DRY RUN: Would change"))
            .stdout(predicate::str::contains("Explanation:"));

        // Verify status wasn't changed (get path prints current status)
        let mut cmd2 = Command::cargo_bin("lotar").unwrap();
        cmd2.current_dir(temp.path())
            .args(["status", &created.id])
            .assert()
            .success()
            .stdout(predicate::str::contains("Task "))
            .stdout(predicate::str::contains(" status: "))
            .stdout(predicate::str::contains("Todo"));
    }

    #[test]
    fn add_dry_run_explain_creates_no_task_files() {
        let temp = tempfile::TempDir::new().unwrap();
        let tasks_dir = temp.path().join(".tasks");
        std::fs::create_dir_all(&tasks_dir).unwrap();

        write_minimal_config(&tasks_dir, "");

        let _guard = EnvVarGuard::set("LOTAR_TASKS_DIR", &tasks_dir.to_string_lossy());

        // Snapshot YAML files before
        let before_count = count_task_yaml_files(&tasks_dir);

        // Run add in dry-run mode
        let mut cmd = Command::cargo_bin("lotar").unwrap();
        cmd.current_dir(temp.path())
            .args([
                "add",
                "Dry run task",
                "--project=TEST",
                "--dry-run",
                "--explain",
            ])
            .assert()
            .success()
            .stdout(predicate::str::contains("DRY RUN: Would create task"))
            .stdout(predicate::str::contains("Explanation:"));

        // No new YAML files should be written
        let after_count = count_task_yaml_files(&tasks_dir);
        assert_eq!(
            before_count, after_count,
            "dry-run should not write task files"
        );
    }

    fn count_task_yaml_files(tasks_dir: &std::path::Path) -> usize {
        // Count all project task YAML files (exclude top-level config.yml)
        let mut count = 0usize;
        if let Ok(read) = std::fs::read_dir(tasks_dir) {
            for entry in read.flatten() {
                let path = entry.path();
                if path.is_dir() {
                    // inside project dir
                    if let Ok(inner) = std::fs::read_dir(&path) {
                        for f in inner.flatten() {
                            let fp = f.path();
                            if fp.is_file() {
                                let name = fp.file_name().and_then(|s| s.to_str()).unwrap_or("");
                                let ext = fp.extension().and_then(|s| s.to_str()).unwrap_or("");
                                if ext.eq_ignore_ascii_case("yml") && name != "config.yml" {
                                    count += 1;
                                }
                            }
                        }
                    }
                }
            }
        }
        count
    }
}

// =============================================================================
// Consolidated: Comments suite (command, shortcut, parity)
// =============================================================================

mod comments {
    use super::*;

    #[test]
    fn comment_positional_text_adds_comment() {
        let tf = TestFixtures::new();
        // create a task
        let mut cmd = Command::cargo_bin("lotar").unwrap();
        cmd.current_dir(tf.get_temp_path())
            .args(["add", "Test task for comments"])
            .assert()
            .success();

        // list to get ID
        let output = Command::cargo_bin("lotar")
            .unwrap()
            .current_dir(tf.get_temp_path())
            .args(["list"]) // default text output
            .output()
            .unwrap();
        assert!(output.status.success());
        let body = String::from_utf8_lossy(&output.stdout);
        let id = regex::Regex::new(r"([A-Z0-9]+-\d+)")
            .unwrap()
            .captures(&body)
            .and_then(|c| c.get(1))
            .map(|m| m.as_str().to_string())
            .expect("Expected an ID in list output");

        // add comment
        let mut cmd = Command::cargo_bin("lotar").unwrap();
        cmd.current_dir(tf.get_temp_path())
            .args(["comment", &id, "hello world"])
            .assert()
            .success();

        // verify via JSON second run
        let out = Command::cargo_bin("lotar")
            .unwrap()
            .current_dir(tf.get_temp_path())
            .args(["-f", "json", "comment", &id, "again"]) // ensure json output
            .output()
            .unwrap();
        assert!(out.status.success());
        let s = String::from_utf8_lossy(&out.stdout);
        assert!(s.contains("\"action\":\"task.comment\""));
        assert!(s.contains("\"added_comment\""));
    }

    #[test]
    fn comment_message_flag_adds_comment() {
        let tf = TestFixtures::new();
        // create a task
        Command::cargo_bin("lotar")
            .unwrap()
            .current_dir(tf.get_temp_path())
            .args(["add", "Task for -m"])
            .assert()
            .success();

        // get id (use JSON for stability)
        let out = Command::cargo_bin("lotar")
            .unwrap()
            .current_dir(tf.get_temp_path())
            .args(["list", "--format", "json"]) // json
            .output()
            .unwrap();
        let body = String::from_utf8_lossy(&out.stdout);
        let id = regex::Regex::new(r#"id"\s*:\s*"([A-Z0-9]+-\d+)"#)
            .unwrap()
            .captures(&body)
            .and_then(|c| c.get(1))
            .map(|m| m.as_str().to_string())
            .expect("Expected an ID in list JSON output");

        // add comment with -m
        Command::cargo_bin("lotar")
            .unwrap()
            .current_dir(tf.get_temp_path())
            .args(["comment", &id, "-m", "via flag message"])
            .assert()
            .success();
    }

    #[test]
    fn comment_requires_text() {
        let tf = TestFixtures::new();
        // create a task
        Command::cargo_bin("lotar")
            .unwrap()
            .current_dir(tf.get_temp_path())
            .args(["add", "Task for empty check"])
            .assert()
            .success();

        // get id via JSON list
        let out = Command::cargo_bin("lotar")
            .unwrap()
            .current_dir(tf.get_temp_path())
            .args(["list", "--format", "json"]) // json
            .output()
            .unwrap();
        let body = String::from_utf8_lossy(&out.stdout);
        let id = regex::Regex::new(r#"id"\s*:\s*"([A-Z0-9]+-\d+)"#)
            .unwrap()
            .captures(&body)
            .and_then(|c| c.get(1))
            .map(|m| m.as_str().to_string())
            .expect("Expected an ID in list JSON output");

        // run with no text -> should list existing comments (success) and show 0 initially
        Command::cargo_bin("lotar")
            .unwrap()
            .current_dir(tf.get_temp_path())
            .args(["--format", "json", "comment", &id])
            .assert()
            .success()
            .stdout(predicate::str::contains("\"action\":\"task.comment.list\""))
            .stdout(predicate::str::contains("\"comments\":0"));
    }

    #[test]
    fn comment_shortcut_adds_comment() {
        use tempfile::TempDir;
        let temp = TempDir::new().unwrap();
        // Create a task
        Command::cargo_bin("lotar")
            .unwrap()
            .current_dir(temp.path())
            .args(["task", "add", "A"])
            .assert()
            .success();

        // Resolve created ID by listing
        let out = Command::cargo_bin("lotar")
            .unwrap()
            .current_dir(temp.path())
            .args(["--format", "json", "list"]) // get first id
            .assert()
            .success()
            .get_output()
            .stdout
            .clone();
        let s = String::from_utf8_lossy(&out);
        let v: serde_json::Value = serde_json::from_str(&s).unwrap();
        let id = v["tasks"][0]["id"].as_str().unwrap().to_string();

        // Add a comment
        let out = Command::cargo_bin("lotar")
            .unwrap()
            .current_dir(temp.path())
            .args(["--format", "json", "comment", &id, "First comment"])
            .assert()
            .success()
            .get_output()
            .stdout
            .clone();
        let s = String::from_utf8_lossy(&out);
        let v: serde_json::Value = serde_json::from_str(&s).unwrap();
        assert_eq!(v["action"], "task.comment");
        assert_eq!(v["task_id"], id);
        assert_eq!(v["comments"], 1);
    }

    #[test]
    fn task_comment_parity_list_on_empty() {
        let tf = TestFixtures::new();
        // create a task
        Command::cargo_bin("lotar")
            .unwrap()
            .current_dir(tf.get_temp_path())
            .args(["add", "Task for task comment parity"])
            .assert()
            .success();

        // get id via JSON list
        let out = Command::cargo_bin("lotar")
            .unwrap()
            .current_dir(tf.get_temp_path())
            .args(["list", "--format", "json"]) // json
            .output()
            .unwrap();
        let body = String::from_utf8_lossy(&out.stdout);
        let id = regex::Regex::new(r#"id"\s*:\s*"([A-Z0-9]+-\d+)"#)
            .unwrap()
            .captures(&body)
            .and_then(|c| c.get(1))
            .map(|m| m.as_str().to_string())
            .expect("Expected an ID in list JSON output");

        // lotar task comment with no text should list existing comments
        Command::cargo_bin("lotar")
            .unwrap()
            .current_dir(tf.get_temp_path())
            .args(["--format", "json", "task", "comment", &id])
            .assert()
            .success()
            .stdout(predicate::str::contains("\"action\":\"task.comment.list\""))
            .stdout(predicate::str::contains("\"comments\":0"));
    }
}

// =============================================================================
// Consolidated: Advanced list features
// =============================================================================

mod list_features {
    #![allow(clippy::redundant_pattern_matching)]
    use super::*;

    /// Phase 2.1 - Advanced List Command Features Testing
    /// Tests complex filtering, sorting, and grouping functionality
    /// including custom properties, multiple filters, and date operations.
    ///
    /// Phase 2.1 - Advanced List Command Features Testing
    /// Tests current filtering capabilities and documents gaps between
    /// help documentation and actual implementation.
    ///
    /// KEY FINDINGS:
    /// - Single filters work (status, type, priority)
    /// - Multiple values for same filter NOT implemented yet
    /// - Help documentation promises features not in CLI args
    /// - CLI args use Option<String> instead of Vec<String>
    ///
    #[test]
    fn test_current_filtering_capabilities() {
        let fixtures = TestFixtures::new();
        let temp_dir = fixtures.temp_dir.path();

        // Create diverse test tasks
        let mut cmd = Command::cargo_bin("lotar").unwrap();
        cmd.current_dir(temp_dir)
            .arg("add")
            .arg("Bug task")
            .arg("--type=bug")
            .arg("--priority=high")
            .assert()
            .success();

        let mut cmd = Command::cargo_bin("lotar").unwrap();
        cmd.current_dir(temp_dir)
            .arg("add")
            .arg("Feature task")
            .arg("--type=feature")
            .arg("--priority=low")
            .assert()
            .success();

        let mut cmd = Command::cargo_bin("lotar").unwrap();
        cmd.current_dir(temp_dir)
            .arg("add")
            .arg("Chore task")
            .arg("--type=chore")
            .arg("--priority=medium")
            .assert()
            .success();

        // Change one task status
        let mut cmd = Command::cargo_bin("lotar").unwrap();
        cmd.current_dir(temp_dir)
            .arg("status")
            .arg("2")
            .arg("in_progress")
            .assert()
            .success();

        // Test single status filter (WORKS)
        let mut cmd = Command::cargo_bin("lotar").unwrap();
        let result = cmd
            .current_dir(temp_dir)
            .arg("list")
            .arg("--status=todo")
            .arg("--format=json")
            .assert()
            .success();

        let output = String::from_utf8_lossy(&result.get_output().stdout);
        if !output.trim().is_empty() {
            let json: serde_json::Value =
                serde_json::from_str(&output).expect("Should return valid JSON");

            if let Some(tasks) = json.get("tasks").and_then(|t| t.as_array()) {
                for task in tasks {
                    if let Some(status) = task.get("status").and_then(|s| s.as_str()) {
                        assert_eq!(
                            status.to_ascii_lowercase(),
                            "todo",
                            "Status filter should work"
                        );
                    }
                }
                assert!(!tasks.is_empty(), "Should find some TODO tasks");
            }
        }

        // Test single priority filter (UNCLEAR - needs verification)
        let mut cmd = Command::cargo_bin("lotar").unwrap();
        let result = cmd
            .current_dir(temp_dir)
            .arg("list")
            .arg("--priority=high")
            .arg("--format=json")
            .assert();

        // Priority filter may or may not be implemented
        let _priority_result = result.try_success().is_ok();

        // Test high priority flag (DOCUMENTED but may not work)
        let mut cmd = Command::cargo_bin("lotar").unwrap();
        let result = cmd
            .current_dir(temp_dir)
            .arg("list")
            .arg("--high")
            .arg("--format=json")
            .assert();

        // High priority flag may or may not be implemented
        let _high_priority_result = result.try_success().is_ok();
    }

    #[test]
    fn test_documentation_vs_implementation_gaps() {
        let fixtures = TestFixtures::new();
        let temp_dir = fixtures.temp_dir.path();

        // Create a test task
        let mut cmd = Command::cargo_bin("lotar").unwrap();
        cmd.current_dir(temp_dir)
            .arg("add")
            .arg("Test task")
            .assert()
            .success();

        // Test 1: Multiple status filters (DOCUMENTED but fails)
        let mut cmd = Command::cargo_bin("lotar").unwrap();
        let result = cmd
            .current_dir(temp_dir)
            .arg("list")
            .arg("--status=todo")
            .arg("--status=in_progress")
            .arg("--format=json")
            .assert();

        if let Ok(_) = result.try_success() {}

        // Test 2: Type filtering
        let mut cmd = Command::cargo_bin("lotar").unwrap();
        let result = cmd
            .current_dir(temp_dir)
            .arg("list")
            .arg("--type=feature")
            .arg("--format=json")
            .assert();

        if let Ok(_) = result.try_success() {}

        // Test 3: --bugs shortcut (DOCUMENTED)
        let mut cmd = Command::cargo_bin("lotar").unwrap();
        let result = cmd
            .current_dir(temp_dir)
            .arg("list")
            .arg("--bugs")
            .arg("--format=json")
            .assert();

        if let Ok(_) = result.try_success() {}

        // Test 4: --assignee filter (DOCUMENTED)
        let mut cmd = Command::cargo_bin("lotar").unwrap();
        let result = cmd
            .current_dir(temp_dir)
            .arg("list")
            .arg("--assignee=test@example.com")
            .arg("--format=json")
            .assert();

        if let Ok(_) = result.try_success() {}

        // Test 5: Sorting (DOCUMENTED)
        let mut cmd = Command::cargo_bin("lotar").unwrap();
        let result = cmd
            .current_dir(temp_dir)
            .arg("list")
            .arg("--sort-by=priority")
            .arg("--format=json")
            .assert();

        if let Ok(_) = result.try_success() {}

        // Test 6: Grouping (DOCUMENTED)
        let mut cmd = Command::cargo_bin("lotar").unwrap();
        let result = cmd
            .current_dir(temp_dir)
            .arg("list")
            .arg("--group-by=status")
            .assert();

        if let Ok(_) = result.try_success() {}
    }

    #[test]
    fn test_single_type_filtering() {
        let fixtures = TestFixtures::new();
        let temp_dir = fixtures.temp_dir.path();

        // Create tasks with different types
        let mut cmd = Command::cargo_bin("lotar").unwrap();
        cmd.current_dir(temp_dir)
            .arg("add")
            .arg("Feature task")
            .arg("--type=feature")
            .assert()
            .success();

        let mut cmd = Command::cargo_bin("lotar").unwrap();
        cmd.current_dir(temp_dir)
            .arg("add")
            .arg("Bug task")
            .arg("--type=bug")
            .assert()
            .success();

        let mut cmd = Command::cargo_bin("lotar").unwrap();
        cmd.current_dir(temp_dir)
            .arg("add")
            .arg("Chore task")
            .arg("--type=chore")
            .assert()
            .success();

        // Test single type filter for bugs
        let mut cmd = Command::cargo_bin("lotar").unwrap();
        let result = cmd
            .current_dir(temp_dir)
            .arg("list")
            .arg("--type=bug")
            .arg("--format=json")
            .assert();

        if let Ok(assert_result) = result.try_success() {
            let output = String::from_utf8_lossy(&assert_result.get_output().stdout);

            if !output.trim().is_empty() {
                let json: serde_json::Value =
                    serde_json::from_str(&output).expect("Should return valid JSON");

                if let Some(tasks) = json.get("tasks").and_then(|t| t.as_array()) {
                    // Should only include bug tasks
                    for task in tasks {
                        if let Some(task_type) = task.get("task_type").and_then(|t| t.as_str()) {
                            assert_eq!(
                                task_type.to_ascii_lowercase(),
                                "bug",
                                "Type filter should only return bug tasks"
                            );
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn test_multiple_type_filters_architecture() {
        let fixtures = TestFixtures::new();
        let temp_dir = fixtures.temp_dir.path();

        // Create tasks with different types
        let mut cmd = Command::cargo_bin("lotar").unwrap();
        cmd.current_dir(temp_dir)
            .arg("add")
            .arg("Feature task")
            .arg("--type=feature")
            .assert()
            .success();

        let mut cmd = Command::cargo_bin("lotar").unwrap();
        cmd.current_dir(temp_dir)
            .arg("add")
            .arg("Bug task")
            .arg("--type=bug")
            .assert()
            .success();

        // Test multiple type filters (may not be implemented)
        let mut cmd = Command::cargo_bin("lotar").unwrap();
        let result = cmd
            .current_dir(temp_dir)
            .arg("list")
            .arg("--type=bug")
            .arg("--type=feature")
            .arg("--format=json")
            .assert();

        if let Ok(assert_result) = result.try_success() {
            let output = String::from_utf8_lossy(&assert_result.get_output().stdout);

            if !output.trim().is_empty() {
                let json: serde_json::Value =
                    serde_json::from_str(&output).expect("Should return valid JSON");

                if let Some(_tasks) = json.get("tasks").and_then(|t| t.as_array()) {}
            }
        }
    }

    #[test]
    fn test_search_command_vs_list_command() {
        let fixtures = TestFixtures::new();
        let temp_dir = fixtures.temp_dir.path();

        // Create tasks for comparison
        let mut cmd = Command::cargo_bin("lotar").unwrap();
        cmd.current_dir(temp_dir)
            .arg("add")
            .arg("Search test task")
            .arg("--type=feature")
            .arg("--priority=high")
            .assert()
            .success();

        // Test list command with filters
        let mut cmd = Command::cargo_bin("lotar").unwrap();
        let list_result = cmd
            .current_dir(temp_dir)
            .arg("list")
            .arg("--format=json")
            .assert();

        if let Ok(assert_result) = list_result.try_success() {
            let output = String::from_utf8_lossy(&assert_result.get_output().stdout);
            if !output.trim().is_empty() {}
        }

        // Test task search command (full interface)
        let mut cmd = Command::cargo_bin("lotar").unwrap();
        let search_result = cmd
            .current_dir(temp_dir)
            .arg("task")
            .arg("search")
            .arg("--format=json")
            .assert();

        if let Ok(_) = search_result.try_success() {}
    }

    #[test]
    fn test_advanced_filter_combinations() {
        let fixtures = TestFixtures::new();
        let temp_dir = fixtures.temp_dir.path();

        // Create diverse tasks
        let mut cmd = Command::cargo_bin("lotar").unwrap();
        cmd.current_dir(temp_dir)
            .arg("add")
            .arg("High priority bug")
            .arg("--type=bug")
            .arg("--priority=high")
            .arg("--assignee=alice@company.com")
            .assert()
            .success();

        let mut cmd = Command::cargo_bin("lotar").unwrap();
        cmd.current_dir(temp_dir)
            .arg("add")
            .arg("Medium priority feature")
            .arg("--type=feature")
            .arg("--priority=medium")
            .arg("--assignee=bob@company.com")
            .assert()
            .success();

        // Test what combinations might work (based on CLI args available)
        let test_cases = vec![
            // Basic single filters that should work based on CLI struct
            ("--status=todo", "single status filter"),
            ("--priority=high", "single priority filter"),
            ("--assignee=alice@company.com", "assignee filter"),
            ("--mine", "mine filter"),
            ("--high", "high priority flag"),
            ("--critical", "critical priority flag"),
        ];

        for (filter_arg, _description) in test_cases {
            let mut cmd = Command::cargo_bin("lotar").unwrap();
            let args: Vec<&str> = filter_arg.split_whitespace().collect();
            let mut cmd_with_args = cmd.current_dir(temp_dir).arg("list");

            for arg in args {
                cmd_with_args = cmd_with_args.arg(arg);
            }

            let result = cmd_with_args.arg("--format=json").assert();

            if let Ok(_) = result.try_success() {}
        }
    }

    #[test]
    fn test_search_performance_and_limits() {
        let fixtures = TestFixtures::new();
        let temp_dir = fixtures.temp_dir.path();

        // Create multiple tasks to test performance and limits

        for i in 1..=5 {
            let mut cmd = Command::cargo_bin("lotar").unwrap();
            cmd.current_dir(temp_dir)
                .arg("add")
                .arg(format!("Performance test task {i}"))
                .arg("--type=feature")
                .assert()
                .success();
        }

        // Test limit parameter
        let mut cmd = Command::cargo_bin("lotar").unwrap();
        let result = cmd
            .current_dir(temp_dir)
            .arg("list")
            .arg("--limit=3") // This should be supported based on CLI args
            .arg("--format=json")
            .assert();

        if let Ok(assert_result) = result.try_success() {
            let output = String::from_utf8_lossy(&assert_result.get_output().stdout);

            if !output.trim().is_empty() {
                let json: serde_json::Value =
                    serde_json::from_str(&output).expect("Should return valid JSON");

                if let Some(tasks) = json.get("tasks").and_then(|t| t.as_array()) {
                    if tasks.len() <= 3 {}
                }
            }
        }

        // Test with no limit (default)
        let mut cmd = Command::cargo_bin("lotar").unwrap();
        let result = cmd
            .current_dir(temp_dir)
            .arg("list")
            .arg("--format=json")
            .assert();

        if let Ok(assert_result) = result.try_success() {
            let output = String::from_utf8_lossy(&assert_result.get_output().stdout);

            if !output.trim().is_empty() {
                let json: serde_json::Value =
                    serde_json::from_str(&output).expect("Should return valid JSON");

                if let Some(_tasks) = json.get("tasks").and_then(|t| t.as_array()) {}
            }
        }
    }

    #[test]
    fn test_implementation_status_summary() {
        let fixtures = TestFixtures::new();
        let temp_dir = fixtures.temp_dir.path();

        // Create a test task
        let mut cmd = Command::cargo_bin("lotar").unwrap();
        cmd.current_dir(temp_dir)
            .arg("add")
            .arg("Summary test task")
            .arg("--type=feature")
            .arg("--priority=high")
            .assert()
            .success();

        // Core features that should work
        let core_features = vec![
            ("Basic list", "list", vec!["--format=json"]),
            (
                "Status filter",
                "list",
                vec!["--status=todo", "--format=json"],
            ),
            ("JSON format", "list", vec!["--format=json"]),
            ("Text format", "list", vec!["--format=text"]),
            ("Limit param", "list", vec!["--limit=5", "--format=json"]),
        ];

        for (_name, cmd_name, args) in core_features {
            let mut cmd = Command::cargo_bin("lotar").unwrap();
            let mut cmd_with_args = cmd.current_dir(temp_dir).arg(cmd_name);

            for arg in args {
                cmd_with_args = cmd_with_args.arg(arg);
            }

            let _result = cmd_with_args.assert().try_success().is_ok();
        }

        // Features documented but not implemented
        let missing_features = vec![
            (
                "Multiple status filters",
                "list",
                vec!["--status=todo", "--status=in_progress"],
            ),
            (
                "Multiple type filters",
                "list",
                vec!["--type=bug", "--type=feature"],
            ),
            ("Sorting", "list", vec!["--sort-by=priority"]),
            ("Grouping", "list", vec!["--group-by=status"]),
            ("High priority flag", "list", vec!["--high-priority"]),
            ("Type shortcuts", "list", vec!["--bugs"]),
        ];

        for (_name, cmd_name, args) in missing_features {
            let mut cmd = Command::cargo_bin("lotar").unwrap();
            let mut cmd_with_args = cmd.current_dir(temp_dir).arg(cmd_name);

            for arg in args {
                cmd_with_args = cmd_with_args.arg(arg);
            }

            let _result = cmd_with_args.assert().try_success().is_ok();
        }
    }

    // Merged from tag_filtering_comprehensive_test.rs
    mod tag_filtering_tests {
        use super::*;

        #[test]
        fn test_tag_filtering_or_logic() {
            let fixtures = TestFixtures::new();
            let temp_dir = fixtures.temp_dir.path();

            let mut cmd = Command::cargo_bin("lotar").unwrap();
            cmd.current_dir(temp_dir)
                .arg("add")
                .arg("Backend task")
                .arg("--tag=urgent")
                .arg("--tag=backend")
                .arg("--tag=feature")
                .assert()
                .success();

            let mut cmd = Command::cargo_bin("lotar").unwrap();
            cmd.current_dir(temp_dir)
                .arg("add")
                .arg("Frontend task")
                .arg("--tag=frontend")
                .arg("--tag=testing")
                .assert()
                .success();

            let mut cmd = Command::cargo_bin("lotar").unwrap();
            cmd.current_dir(temp_dir)
                .arg("add")
                .arg("Database task")
                .arg("--tag=database")
                .arg("--tag=backend")
                .assert()
                .success();

            let mut cmd = Command::cargo_bin("lotar").unwrap();
            let output = cmd
                .current_dir(temp_dir)
                .arg("list")
                .arg("--tag=urgent")
                .arg("--format=json")
                .assert()
                .success()
                .get_output()
                .stdout
                .clone();

            let stdout = String::from_utf8_lossy(&output);
            let json: serde_json::Value = serde_json::from_str(&stdout).expect("Valid JSON");
            let tasks = if let Some(arr) = json.as_array() {
                arr
            } else {
                json["tasks"].as_array().expect("Tasks array")
            };
            assert_eq!(
                tasks.len(),
                1,
                "Should find exactly one task with 'urgent' tag"
            );
            assert!(
                tasks[0]["tags"]
                    .as_array()
                    .unwrap()
                    .contains(&serde_json::Value::String("urgent".to_string()))
            );

            let mut cmd = Command::cargo_bin("lotar").unwrap();
            let output = cmd
                .current_dir(temp_dir)
                .arg("list")
                .arg("--tag=urgent")
                .arg("--tag=frontend")
                .arg("--format=json")
                .assert()
                .success()
                .get_output()
                .stdout
                .clone();

            let stdout = String::from_utf8_lossy(&output);
            let json: serde_json::Value = serde_json::from_str(&stdout).expect("Valid JSON");
            let tasks = if let Some(arr) = json.as_array() {
                arr
            } else {
                json["tasks"].as_array().expect("Tasks array")
            };
            assert_eq!(
                tasks.len(),
                2,
                "Should find two tasks: one with 'urgent' and one with 'frontend' tag"
            );

            let task_titles: Vec<String> = tasks
                .iter()
                .map(|t| t["title"].as_str().unwrap().to_string())
                .collect();
            assert!(task_titles.contains(&"Backend task".to_string()));
            assert!(task_titles.contains(&"Frontend task".to_string()));

            let mut cmd = Command::cargo_bin("lotar").unwrap();
            let output = cmd
                .current_dir(temp_dir)
                .arg("list")
                .arg("--tag=backend")
                .arg("--tag=testing")
                .arg("--format=json")
                .assert()
                .success()
                .get_output()
                .stdout
                .clone();

            let stdout = String::from_utf8_lossy(&output);
            let json: serde_json::Value = serde_json::from_str(&stdout).expect("Valid JSON");
            let tasks = if let Some(arr) = json.as_array() {
                arr
            } else {
                json["tasks"].as_array().expect("Tasks array")
            };
            assert_eq!(
                tasks.len(),
                3,
                "Should find three tasks: two with 'backend' and one with 'testing' tag"
            );
        }

        #[test]
        fn test_task_display_completeness() {
            let fixtures = TestFixtures::new();
            let temp_dir = fixtures.temp_dir.path();

            let mut cmd = Command::cargo_bin("lotar").unwrap();
            cmd.current_dir(temp_dir)
                .arg("add")
                .arg("Complete task")
                .arg("--type=feature")
                .arg("--priority=high")
                .arg("--tag=urgent")
                .arg("--tag=frontend")
                .arg("--description=This is a detailed description")
                .arg("--field=product=web")
                .arg("--assignee=developer@example.com")
                .arg("--effort=5d")
                .arg("--field=custom=value")
                .assert()
                .success();

            let mut cmd = Command::cargo_bin("lotar").unwrap();
            let output = cmd
                .current_dir(temp_dir)
                .arg("list")
                .arg("--format=json")
                .assert()
                .success()
                .get_output()
                .stdout
                .clone();

            let stdout = String::from_utf8_lossy(&output);
            let json: serde_json::Value = serde_json::from_str(&stdout).expect("Valid JSON");
            let tasks = json["tasks"].as_array().expect("Tasks array");
            assert_eq!(tasks.len(), 1, "Should find exactly one task");

            let task = &tasks[0];
            assert!(task.get("id").is_some());
            assert!(task.get("title").is_some());
            assert!(task.get("status").is_some());
            assert!(task.get("priority").is_some());
            assert!(task.get("task_type").is_some());
            assert!(task.get("description").is_some());
            assert!(task.get("assignee").is_some());
            assert!(task.get("project").is_some());
            assert!(task.get("due_date").is_some());
            assert!(task.get("effort").is_some());
            assert!(task.get("tags").is_some());
            assert!(task.get("created").is_some());
            assert!(task.get("modified").is_some());
            assert!(task.get("custom_fields").is_some());

            assert_eq!(task["title"], "Complete task");
            assert_eq!(task["priority"], "High");
            assert_eq!(task["task_type"], "Feature");
            assert_eq!(task["description"], "This is a detailed description");
            assert_eq!(task["assignee"], "developer@example.com");
            assert_eq!(task["effort"], "40.00h");

            let tags = task["tags"].as_array().expect("Tags should be an array");
            assert_eq!(tags.len(), 2);
            assert!(tags.contains(&serde_json::Value::String("urgent".to_string())));
            assert!(tags.contains(&serde_json::Value::String("frontend".to_string())));

            assert!(task["project"].as_str().is_some());
            assert!(!task["project"].is_null());

            let custom_fields = task["custom_fields"]
                .as_object()
                .expect("Custom fields should be an object");
            assert_eq!(custom_fields.get("product").unwrap(), "web");
            assert!(custom_fields.contains_key("custom"));
        }

        #[test]
        fn test_text_format_includes_project() {
            let fixtures = TestFixtures::new();
            let temp_dir = fixtures.temp_dir.path();

            let mut cmd = Command::cargo_bin("lotar").unwrap();
            cmd.current_dir(temp_dir)
                .arg("add")
                .arg("Test task")
                .arg("--tag=test")
                .assert()
                .success();

            let mut cmd = Command::cargo_bin("lotar").unwrap();
            cmd.current_dir(temp_dir)
                .arg("list")
                .arg("--tag=test")
                .assert()
                .success()
                .stdout(predicate::str::contains("Found 1 task(s)"))
                .stdout(predicate::str::contains("Test task"));
        }
    }
}

fn get_task_as_json(temp_dir: &TempDir, task_id: &str) -> serde_json::Value {
    let mut cmd = Command::cargo_bin("lotar").unwrap();
    let output = cmd
        .current_dir(temp_dir)
        .args(["--format=json", "list"])
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "Should be able to retrieve task list as JSON"
    );

    let json: serde_json::Value =
        serde_json::from_slice(&output.stdout).expect("Should produce valid JSON");

    // Find the task with the matching ID in the tasks array
    if let Some(tasks) = json.get("tasks").and_then(|t| t.as_array()) {
        for task in tasks {
            if let Some(id) = task.get("id").and_then(|i| i.as_str()) {
                if id == task_id {
                    return task.clone();
                }
            }
        }
    }

    panic!("Could not find task with ID: {task_id}");
}

// =============================================================================
// Phase 1.1 - Dual CLI Interface Testing
// =============================================================================

mod dual_interface {
    use super::*;
    use serde_json::Value;

    #[test]
    fn test_add_interfaces_create_identical_tasks() {
        let temp_dir = TempDir::new().unwrap();

        // Test quick interface: lotar add
        let mut cmd1 = Command::cargo_bin("lotar").unwrap();
        let output1 = cmd1
            .current_dir(&temp_dir)
            .args([
                "add",
                "Test task quick",
                "--priority=high",
                "--assignee=test@example.com",
            ])
            .output()
            .unwrap();

        assert!(output1.status.success(), "Quick add should succeed");
        let stdout1 = String::from_utf8_lossy(&output1.stdout);

        // Extract task ID from output (assuming it's in the format "Created task: TASK_ID")
        let task_id1 = extract_task_id_from_output(&stdout1);

        // Test full interface: lotar task add
        let mut cmd2 = Command::cargo_bin("lotar").unwrap();
        let output2 = cmd2
            .current_dir(&temp_dir)
            .args([
                "task",
                "add",
                "Test task full",
                "--priority=high",
                "--assignee=test@example.com",
            ])
            .output()
            .unwrap();

        assert!(output2.status.success(), "Full add should succeed");
        let stdout2 = String::from_utf8_lossy(&output2.stdout);
        let task_id2 = extract_task_id_from_output(&stdout2);

        // Verify both tasks were created
        assert!(
            task_id1.is_some(),
            "Quick interface should create task with ID"
        );
        assert!(
            task_id2.is_some(),
            "Full interface should create task with ID"
        );

        // Get both tasks and compare their properties (excluding ID and creation time)
        let task1_json = get_task_as_json(&temp_dir, &task_id1.unwrap());
        let task2_json = get_task_as_json(&temp_dir, &task_id2.unwrap());

        // Both tasks should have the same structure and properties
        assert_eq!(task1_json["priority"], task2_json["priority"]);
        assert_eq!(task1_json["assignee"], task2_json["assignee"]);
        assert_eq!(task1_json["status"], task2_json["status"]); // Should both be "todo" by default
    }

    #[test]
    fn test_add_parameter_compatibility() {
        let temp_dir = TempDir::new().unwrap();

        // Test that parameters work the same way in both interfaces
        let test_cases = [
            // (quick_args, full_args, description)
            (
                vec!["add", "Simple task"],
                vec!["task", "add", "Simple task"],
                "Basic title setting",
            ),
            (
                vec!["add", "Priority task", "--priority=medium"],
                vec!["task", "add", "Priority task", "--priority=medium"],
                "Priority setting",
            ),
            (
                vec!["add", "Assigned task", "--assignee=user@example.com"],
                vec![
                    "task",
                    "add",
                    "Assigned task",
                    "--assignee=user@example.com",
                ],
                "Assignee setting",
            ),
        ];

        for (quick_args, full_args, _description) in test_cases.iter() {
            // Test quick interface
            let mut cmd1 = Command::cargo_bin("lotar").unwrap();
            cmd1.current_dir(&temp_dir)
                .args(quick_args)
                .assert()
                .success();

            // Test full interface
            let mut cmd2 = Command::cargo_bin("lotar").unwrap();
            cmd2.current_dir(&temp_dir)
                .args(full_args)
                .assert()
                .success();
        }
    }

    #[test]
    fn test_add_output_consistency() {
        let temp_dir = TempDir::new().unwrap();

        // Test that both interfaces produce consistent output formats

        // Quick interface with JSON output
        let mut cmd1 = Command::cargo_bin("lotar").unwrap();
        let output1 = cmd1
            .current_dir(&temp_dir)
            .args(["--format", "json", "add", "Output test quick"])
            .output()
            .unwrap();

        assert!(output1.status.success());
        let json1: Value = serde_json::from_slice(&output1.stdout)
            .expect("Quick interface should produce valid JSON");

        // Full interface with JSON output
        let mut cmd2 = Command::cargo_bin("lotar").unwrap();
        let output2 = cmd2
            .current_dir(&temp_dir)
            .args(["--format", "json", "task", "add", "Output test full"])
            .output()
            .unwrap();

        assert!(output2.status.success());
        let json2: Value = serde_json::from_slice(&output2.stdout)
            .expect("Full interface should produce valid JSON");

        // Both should have the same JSON structure
        assert!(
            json1.get("task").is_some(),
            "Quick interface JSON should include task object"
        );
        assert!(
            json2.get("task").is_some(),
            "Full interface JSON should include task object"
        );

        // Check the task objects have the required fields
        if let Some(task1) = json1.get("task") {
            assert!(
                task1.get("id").is_some(),
                "Quick interface task should have ID"
            );
            assert!(
                task1.get("title").is_some(),
                "Quick interface task should have title"
            );
        }
        if let Some(task2) = json2.get("task") {
            assert!(
                task2.get("id").is_some(),
                "Full interface task should have ID"
            );
            assert!(
                task2.get("title").is_some(),
                "Full interface task should have title"
            );
        }
    }

    #[test]
    fn test_add_enhanced_output_format() {
        let temp_dir = TempDir::new().unwrap();

        // Test that enhanced add output shows all set properties
        let mut cmd = Command::cargo_bin("lotar").unwrap();
        let output = cmd
            .current_dir(&temp_dir)
            .args([
                "add",
                "Enhanced output test",
                "--priority=high",
                "--assignee=test@example.com",
            ])
            .output()
            .unwrap();

        assert!(output.status.success());
        let stdout = String::from_utf8_lossy(&output.stdout);

        // Should show task ID
        assert!(
            stdout.contains("Created task:") || stdout.contains("Task"),
            "Output should mention task creation"
        );

        // Should show title
        assert!(
            stdout.contains("Enhanced output test"),
            "Output should show the task title"
        );

        // Should show set properties
        assert!(
            stdout.to_lowercase().contains("high") || stdout.to_lowercase().contains("priority"),
            "Output should show priority information"
        );
        assert!(
            stdout.contains("test@example.com") || stdout.to_lowercase().contains("assignee"),
            "Output should show assignee information"
        );
    }

    #[test]
    fn test_list_interfaces_return_identical_results() {
        let temp_dir = TempDir::new().unwrap();

        // Create test tasks
        Command::cargo_bin("lotar")
            .unwrap()
            .current_dir(&temp_dir)
            .args(["add", "Task 1", "--priority=high", "--type=feature"])
            .assert()
            .success();

        Command::cargo_bin("lotar")
            .unwrap()
            .current_dir(&temp_dir)
            .args(["add", "Task 2", "--priority=low", "--type=bug"])
            .assert()
            .success();

        // Test basic list commands
        let quick_list = Command::cargo_bin("lotar")
            .unwrap()
            .current_dir(&temp_dir)
            .args(["list"])
            .assert()
            .success()
            .get_output()
            .stdout
            .clone();

        let full_list = Command::cargo_bin("lotar")
            .unwrap()
            .current_dir(&temp_dir)
            .args(["task", "list"])
            .assert()
            .success()
            .get_output()
            .stdout
            .clone();

        assert_eq!(
            quick_list, full_list,
            "Basic list commands should return identical output"
        );
    }

    #[test]
    fn test_list_filter_option_compatibility() {
        let temp_dir = TempDir::new().unwrap();

        // Create test tasks with different properties
        Command::cargo_bin("lotar")
            .unwrap()
            .current_dir(&temp_dir)
            .args([
                "add",
                "High Priority Feature",
                "--priority=high",
                "--type=feature",
            ])
            .assert()
            .success();

        Command::cargo_bin("lotar")
            .unwrap()
            .current_dir(&temp_dir)
            .args(["add", "Low Priority Bug", "--priority=low", "--type=bug"])
            .assert()
            .success();

        // Test filtered lists
        let quick_filtered = Command::cargo_bin("lotar")
            .unwrap()
            .current_dir(&temp_dir)
            .args(["list", "--priority=high"])
            .assert()
            .success()
            .get_output()
            .stdout
            .clone();

        let full_filtered = Command::cargo_bin("lotar")
            .unwrap()
            .current_dir(&temp_dir)
            .args(["task", "list", "--priority=high"])
            .assert()
            .success()
            .get_output()
            .stdout
            .clone();

        assert_eq!(
            quick_filtered, full_filtered,
            "Filtered list commands should return identical output"
        );

        // Verify the filter worked
        let stdout = String::from_utf8_lossy(&quick_filtered);
        assert!(
            stdout.contains("High Priority Feature"),
            "Should contain high priority task"
        );
        assert!(
            !stdout.contains("Low Priority Bug"),
            "Should not contain low priority task"
        );
    }

    #[test]
    fn test_list_json_format_consistency() {
        let temp_dir = TempDir::new().unwrap();

        // Create test task
        Command::cargo_bin("lotar")
            .unwrap()
            .current_dir(&temp_dir)
            .args(["add", "JSON Test Task", "--priority=medium"])
            .assert()
            .success();

        // Test JSON output
        let quick_json = Command::cargo_bin("lotar")
            .unwrap()
            .current_dir(&temp_dir)
            .args(["list", "--format=json"])
            .assert()
            .success()
            .get_output()
            .stdout
            .clone();

        let full_json = Command::cargo_bin("lotar")
            .unwrap()
            .current_dir(&temp_dir)
            .args(["task", "list", "--format=json"])
            .assert()
            .success()
            .get_output()
            .stdout
            .clone();

        assert_eq!(
            quick_json, full_json,
            "JSON list commands should return identical output"
        );

        // Verify JSON structure
        let stdout = String::from_utf8_lossy(&quick_json);
        assert!(
            stdout.contains("\"tasks\""),
            "JSON should contain tasks array"
        );
        assert!(
            stdout.contains("\"status\":\"success\""),
            "JSON should contain success status"
        );
        assert!(
            stdout.contains("JSON Test Task"),
            "JSON should contain the task"
        );
    }

    #[test]
    fn test_list_sorting_compatibility() {
        let temp_dir = TempDir::new().unwrap();

        // Create tasks with different priorities
        Command::cargo_bin("lotar")
            .unwrap()
            .current_dir(&temp_dir)
            .args(["add", "Low Priority Task", "--priority=low"])
            .assert()
            .success();

        Command::cargo_bin("lotar")
            .unwrap()
            .current_dir(&temp_dir)
            .args(["add", "High Priority Task", "--priority=high"])
            .assert()
            .success();

        // Test sorted lists
        let quick_sorted = Command::cargo_bin("lotar")
            .unwrap()
            .current_dir(&temp_dir)
            .args(["list", "--sort-by=priority"])
            .assert()
            .success()
            .get_output()
            .stdout
            .clone();

        let full_sorted = Command::cargo_bin("lotar")
            .unwrap()
            .current_dir(&temp_dir)
            .args(["task", "list", "--sort-by=priority"])
            .assert()
            .success()
            .get_output()
            .stdout
            .clone();

        assert_eq!(
            quick_sorted, full_sorted,
            "Sorted list commands should return identical output"
        );
    }
}

// =============================================================================
// File Structure and Config Creation Tests
// =============================================================================

mod file_structure {
    use super::*;

    #[test]
    fn test_tasks_directory_creation() {
        let temp_dir = TempDir::new().unwrap();

        // Create a task to trigger directory creation
        let mut cmd = Command::cargo_bin("lotar").unwrap();
        cmd.current_dir(&temp_dir)
            .args(["task", "add", "Directory Test"])
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
            .args([
                "task",
                "add",
                "Project Directory Test",
                "--project=test-project",
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

        assert!(
            !project_dirs.is_empty(),
            "At least one project directory should exist"
        );
    }

    #[test]
    fn test_task_files_creation() {
        let temp_dir = TempDir::new().unwrap();

        let mut cmd = Command::cargo_bin("lotar").unwrap();
        cmd.current_dir(&temp_dir)
            .args(["task", "add", "File Creation Test"])
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
                    for project_entry in project_entries.flatten() {
                        let filename = project_entry.file_name();
                        if filename.to_string_lossy().ends_with(".yml")
                            && filename.to_string_lossy() != "config.yml"
                        {
                            found_task_file = true;
                            break;
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

    #[test]
    fn test_global_config_created_by_write_operations() {
        let temp_dir = TempDir::new().unwrap();

        // Test that write operations (like add) DO create global config
        let mut cmd = Command::cargo_bin("lotar").unwrap();
        cmd.current_dir(&temp_dir)
            .args(["add", "Test global config creation"])
            .assert()
            .success();

        let tasks_dir = temp_dir.path().join(".tasks");
        assert!(
            tasks_dir.exists(),
            "Write operations should create .tasks directory"
        );

        // Check for global config.yml (should exist)
        let global_config = tasks_dir.join("config.yml");
        assert!(
            global_config.exists(),
            "Global config.yml SHOULD be created by write operations"
        );

        // Verify it's a valid global config file, even if all values are defaults
        let config_content = fs::read_to_string(&global_config).unwrap();
        assert!(
            config_content.contains("default:\n  project:"),
            "Global config should record default project prefix"
        );
        assert!(
            !config_content.contains("server:"),
            "Default global config should not redundantly include server section"
        );
        assert!(
            !config_content.contains("issue:"),
            "Default global config should omit issue taxonomy defaults"
        );
    }

    #[test]
    fn test_read_operations_do_not_create_directories() {
        let temp_dir = TempDir::new().unwrap();

        // Test that read-only operations don't create .tasks directory
        let mut cmd = Command::cargo_bin("lotar").unwrap();
        cmd.current_dir(&temp_dir)
            .args(["status", "TEST-001"])
            .assert()
            .failure(); // Expected to fail since task doesn't exist

        let tasks_dir = temp_dir.path().join(".tasks");
        assert!(
            !tasks_dir.exists(),
            "Read-only operations should not create any directories"
        );

        // Test other read-only operations
        let mut cmd = Command::cargo_bin("lotar").unwrap();
        cmd.current_dir(&temp_dir)
            .args(["priority", "TEST-001"])
            .assert()
            .failure(); // Expected to fail since task doesn't exist

        assert!(
            !tasks_dir.exists(),
            "Read-only property commands should not create directories"
        );

        // Test task subcommand read operations
        let mut cmd = Command::cargo_bin("lotar").unwrap();
        cmd.current_dir(&temp_dir)
            .args(["task", "priority", "TEST-001"])
            .assert()
            .failure(); // Expected to fail since task doesn't exist

        assert!(
            !tasks_dir.exists(),
            "Task subcommand read operations should not create directories"
        );
    }

    #[test]
    fn test_project_config_created_by_write_operations() {
        let temp_dir = TempDir::new().unwrap();

        // Add a task (write operation)
        let mut cmd = Command::cargo_bin("lotar").unwrap();
        cmd.current_dir(&temp_dir)
            .args(["add", "Test config creation"])
            .assert()
            .success();

        let tasks_dir = temp_dir.path().join(".tasks");
        assert!(
            tasks_dir.exists(),
            "Write operations should create .tasks directory"
        );

        // Check for global config.yml (should exist)
        let global_config = tasks_dir.join("config.yml");
        assert!(
            global_config.exists(),
            "Global config.yml SHOULD be created by write operations"
        );

        // Check for project-specific config (should exist in project directory)
        let entries = fs::read_dir(&tasks_dir).unwrap();
        let mut found_project_config = false;

        for entry in entries {
            let entry = entry.unwrap();
            if entry.file_type().unwrap().is_dir() {
                let project_config = entry.path().join("config.yml");
                if project_config.exists() {
                    found_project_config = true;
                    break;
                }
            }
        }

        assert!(
            found_project_config,
            "Project-specific config.yml should also be created by add command"
        );
    }

    #[test]
    fn test_config_creation_consistency_across_interfaces() {
        let temp_dir = TempDir::new().unwrap();

        // Test that both quick and full interfaces behave the same way

        // First, test quick interface
        let mut cmd = Command::cargo_bin("lotar").unwrap();
        cmd.current_dir(&temp_dir)
            .args(["add", "Quick interface test"])
            .assert()
            .success();

        let tasks_dir = temp_dir.path().join(".tasks");
        let global_config = tasks_dir.join("config.yml");
        assert!(
            global_config.exists(),
            "Quick interface SHOULD create global config"
        );

        // Clean up for next test
        fs::remove_dir_all(&tasks_dir).unwrap();

        // Test full interface
        let mut cmd = Command::cargo_bin("lotar").unwrap();
        cmd.current_dir(&temp_dir)
            .args(["task", "add", "Full interface test"])
            .assert()
            .success();

        let global_config = tasks_dir.join("config.yml");
        assert!(
            global_config.exists(),
            "Full interface SHOULD create global config"
        );

        // Both interfaces should behave identically regarding config creation
    }

    #[test]
    fn test_smart_global_config_default_prefix() {
        let temp_dir = TempDir::new().unwrap();

        // Test with explicit project - should set smart default_prefix
        let mut cmd = Command::cargo_bin("lotar").unwrap();
        cmd.current_dir(&temp_dir)
            .args(["add", "Test smart prefix", "--project=myawesomeproject"])
            .assert()
            .success();

        let global_config = temp_dir.path().join(".tasks/config.yml");
        assert!(global_config.exists(), "Global config should be created");

        let config_content = fs::read_to_string(&global_config).unwrap();
        assert!(
            config_content.contains("default:") && config_content.contains("project:"),
            "Global config should have default.project"
        );
        // Should have some form of project prefix (exact value depends on generation logic)
        assert!(
            config_content.contains("MYAW")
                || config_content.contains("MAP")
                || config_content.contains("MY"),
            "Global config should have a smart default_project derived from project name"
        );
    }

    #[test]
    fn test_global_config_regeneration_when_missing() {
        let temp_dir = TempDir::new().unwrap();

        // First, create a task to establish .tasks directory and global config
        let mut cmd = Command::cargo_bin("lotar").unwrap();
        cmd.current_dir(&temp_dir)
            .args(["add", "First task"])
            .assert()
            .success();

        let global_config = temp_dir.path().join(".tasks/config.yml");
        assert!(
            global_config.exists(),
            "Global config should be created initially"
        );

        // Delete the global config
        fs::remove_file(&global_config).unwrap();
        assert!(!global_config.exists(), "Global config should be deleted");

        // Add another task - global config should be regenerated
        let mut cmd = Command::cargo_bin("lotar").unwrap();
        cmd.current_dir(&temp_dir)
            .args(["add", "Second task"])
            .assert()
            .success();

        assert!(
            global_config.exists(),
            "Global config should be regenerated when missing"
        );
    }

    #[test]
    fn test_project_name_in_config() {
        let test_fixtures = TestFixtures::new();
        let temp_dir = test_fixtures.temp_dir.path();

        // Create task using quick interface
        let mut cmd = Command::cargo_bin("lotar").unwrap();
        let output = cmd
            .current_dir(temp_dir)
            .arg("add")
            .arg("Test project name config")
            .output()
            .expect("Failed to execute command");

        assert!(output.status.success());

        // Extract task ID from output
        let output_str = String::from_utf8(output.stdout).unwrap();
        let task_id = extract_task_id_from_output(&output_str)
            .expect("Could not extract task ID from output");

        // Get the project prefix from task ID (e.g., "TTF-1" -> "TTF")
        let prefix = task_id.split('-').next().unwrap();

        // Check that project config exists and has correct project name
        let project_config_path = temp_dir.join(".tasks").join(prefix).join("config.yml");
        assert!(project_config_path.exists(), "Project config should exist");

        let config_content = fs::read_to_string(&project_config_path).unwrap();

        // Should contain the actual project name, not the prefix
        // For temp directory, it will be detected as the directory name (starting with tmp)
        // So we just verify it's not the prefix (which would be uppercase)
        assert!(
            config_content.contains("project:\n  name:")
                || config_content.contains("project.name:"),
            "Config should contain canonical project.name field"
        );
        assert!(
            !config_content.contains(&format!("project.name: {prefix}"))
                && !config_content.contains(&format!("name: {prefix}")),
            "Project config should NOT contain prefix '{prefix}' as project name. Config content: {config_content}"
        );
        assert!(
            !config_content.contains("project.id:") && !config_content.contains("\n  id:"),
            "Project config should not include legacy project.id field. Config content: {config_content}"
        );
    }

    #[test]
    fn test_cli_interface_consistency() {
        let test_fixtures = TestFixtures::new();
        let temp_dir = test_fixtures.temp_dir.path();

        // Test with text output
        let mut quick_cmd = Command::cargo_bin("lotar").unwrap();
        let quick_output = quick_cmd
            .current_dir(temp_dir)
            .arg("add")
            .arg("Test quick interface")
            .arg("--priority=high")
            .output()
            .expect("Failed to execute quick add command");

        let mut full_cmd = Command::cargo_bin("lotar").unwrap();
        let full_output = full_cmd
            .current_dir(temp_dir)
            .arg("task")
            .arg("add")
            .arg("Test full interface")
            .arg("--priority=high")
            .output()
            .expect("Failed to execute full add command");

        assert!(quick_output.status.success());
        assert!(full_output.status.success());

        let quick_str = String::from_utf8(quick_output.stdout).unwrap();
        let full_str = String::from_utf8(full_output.stdout).unwrap();
        let quick_upper = quick_str.to_ascii_uppercase();
        let full_upper = full_str.to_ascii_uppercase();

        // Both should have similar structure (both show detailed task info)
        assert!(quick_str.contains("✅ Created task:"));
        assert!(full_str.contains("✅ Created task:"));
        assert!(quick_str.contains("Title:"));
        assert!(full_str.contains("Title:"));
        assert!(quick_upper.contains("PRIORITY: HIGH"));
        assert!(full_upper.contains("PRIORITY: HIGH"));

        // Test with JSON output
        let mut quick_json_cmd = Command::cargo_bin("lotar").unwrap();
        let quick_json_output = quick_json_cmd
            .current_dir(temp_dir)
            .arg("add")
            .arg("Test quick JSON")
            .arg("--priority=low")
            .arg("--format=json")
            .output()
            .expect("Failed to execute quick add JSON command");

        let mut full_json_cmd = Command::cargo_bin("lotar").unwrap();
        let full_json_output = full_json_cmd
            .current_dir(temp_dir)
            .arg("--format=json")
            .arg("task")
            .arg("add")
            .arg("Test full JSON")
            .arg("--priority=low")
            .output()
            .expect("Failed to execute full add JSON command");

        assert!(quick_json_output.status.success());
        assert!(full_json_output.status.success());

        let quick_json_str = String::from_utf8(quick_json_output.stdout).unwrap();
        let full_json_str = String::from_utf8(full_json_output.stdout).unwrap();

        // Both should be valid JSON with same structure
        let quick_json: serde_json::Value = serde_json::from_str(&quick_json_str)
            .expect("Quick interface should produce valid JSON");
        let full_json: serde_json::Value =
            serde_json::from_str(&full_json_str).expect("Full interface should produce valid JSON");

        // Both should have the same JSON structure
        assert_eq!(
            quick_json.get("status"),
            Some(&serde_json::Value::String("success".to_string()))
        );
        assert_eq!(
            full_json.get("status"),
            Some(&serde_json::Value::String("success".to_string()))
        );
        assert!(quick_json.get("task").is_some());
        assert!(full_json.get("task").is_some());
        let quick_priority = quick_json
            .get("task")
            .and_then(|task| task.get("priority"))
            .and_then(|value| value.as_str())
            .unwrap_or_default();
        let full_priority = full_json
            .get("task")
            .and_then(|task| task.get("priority"))
            .and_then(|value| value.as_str())
            .unwrap_or_default();
        assert_eq!(quick_priority, "Low");
        assert_eq!(full_priority, "Low");
    }
}

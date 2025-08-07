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
            .args(["--format=json", "add", "Output test quick"])
            .output()
            .unwrap();

        assert!(output1.status.success());
        let json1: Value = serde_json::from_slice(&output1.stdout)
            .expect("Quick interface should produce valid JSON");

        // Full interface with JSON output
        let mut cmd2 = Command::cargo_bin("lotar").unwrap();
        let output2 = cmd2
            .current_dir(&temp_dir)
            .args(["--format=json", "task", "add", "Output test full"])
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

        // Verify it's a valid global config with default settings
        let config_content = fs::read_to_string(&global_config).unwrap();
        assert!(
            config_content.contains("server_port:"),
            "Global config should have server_port"
        );
        assert!(
            config_content.contains("issue_states:"),
            "Global config should have issue_states"
        );
        assert!(
            config_content.contains("default_project:")
                || config_content.contains("default_prefix:"),
            "Global config should have default_project setting"
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
            config_content.contains("default_project:"),
            "Global config should have default_project"
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
            config_content.contains("project_name:"),
            "Config should contain project_name field"
        );
        assert!(
            !config_content.contains(&format!("project_name: {prefix}")),
            "Project config should NOT contain prefix '{prefix}' as project name. Config content: {config_content}"
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

        // Both should have similar structure (both show detailed task info)
        assert!(quick_str.contains("✅ Created task:"));
        assert!(full_str.contains("✅ Created task:"));
        assert!(quick_str.contains("Title:"));
        assert!(full_str.contains("Title:"));
        assert!(quick_str.contains("Priority: HIGH"));
        assert!(full_str.contains("Priority: HIGH"));

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
        assert!(
            quick_json.get("task").unwrap().get("priority")
                == Some(&serde_json::Value::String("LOW".to_string()))
        );
        assert!(
            full_json.get("task").unwrap().get("priority")
                == Some(&serde_json::Value::String("LOW".to_string()))
        );
    }
}

use assert_cmd::Command;
use predicates::prelude::*;
use tempfile::TempDir;
use std::fs;

/// Test that tasks are created with correct project prefix, not project name
#[test]
fn test_project_prefix_resolution_not_project_name() {
    let temp_dir = TempDir::new().unwrap();
    let tasks_dir = temp_dir.path().join(".tasks");
    
    // Create a predictable project environment that generates DEFA prefix
    fs::create_dir_all(temp_dir.path()).unwrap();
    fs::write(temp_dir.path().join("Cargo.toml"), 
        "[package]\nname = \"default-example-feature-app\"\nversion = \"0.1.0\"\n").unwrap();
    
    // Run add command without explicit project - should use default prefix
    let mut cmd = Command::cargo_bin("lotar").unwrap();
    cmd.current_dir(temp_dir.path())
       .args(&["--experimental", "add", "Test task", "--task-type=feature"])
       .assert()
       .success()
       .stdout(predicate::str::contains("Created task: DEFA-1"));
    
    // Verify that DEFA directory is created, not 'default'
    assert!(tasks_dir.join("DEFA").exists(), "DEFA directory should exist");
    assert!(!tasks_dir.join("default").exists(), "default directory should NOT exist");
    
    // Verify the task file is in the correct location
    let task_file = tasks_dir.join("DEFA").join("1.yml");
    assert!(task_file.exists(), "Task file should exist in DEFA directory");
    
    // Verify task content
    let task_content = fs::read_to_string(&task_file).unwrap();
    assert!(task_content.contains("title: Test task"));
}

/// Test that JSON output includes task ID in the expected format
#[test]
fn test_json_output_includes_task_id() {
    let temp_dir = TempDir::new().unwrap();
    
    // Create a predictable project environment that generates DEFA prefix
    fs::write(temp_dir.path().join("Cargo.toml"), 
        "[package]\nname = \"default-example-feature-app\"\nversion = \"0.1.0\"\n").unwrap();
    
    let mut cmd = Command::cargo_bin("lotar").unwrap();
    let output = cmd.current_dir(temp_dir.path())
       .args(&["--experimental", "--format=json", "add", "JSON test task"])
       .assert()
       .success()
       .get_output()
       .stdout
       .clone();
    
    let stdout = String::from_utf8(output).unwrap();
    
    // Find the JSON line in the output
    let json_line = stdout.lines()
        .find(|line| line.trim_start().starts_with('{'))
        .expect("Should find a JSON line in output");
    
    let json: serde_json::Value = serde_json::from_str(json_line).unwrap();
    
    // Test that the JSON output includes task_id
    let task_id = json["task_id"].as_str().unwrap();
    assert!(task_id.starts_with("DEFA-"), "Task ID should start with DEFA-, got: {}", task_id);
}

/// Test that JSON error messages are properly formatted
#[test]
fn test_json_error_output_format() {
    let temp_dir = TempDir::new().unwrap();
    
    // Run add command with invalid priority to trigger validation error
    let mut cmd = Command::cargo_bin("lotar").unwrap();
    let output = cmd.current_dir(temp_dir.path())
       .args(&["--experimental", "--format=json", "add", "Error test", "--task-type=epic"])
       .assert()
       .failure() // Should fail due to validation
       .get_output()
       .stdout
       .clone();
    
    let stdout = String::from_utf8(output).unwrap();
    
    // Extract JSON line (last non-empty line should be the JSON output)
    let json_line = stdout.lines()
        .filter(|line| !line.is_empty() && line.trim_start().starts_with('{'))
        .last()
        .expect("Should find a JSON line in output");
    
    // Parse JSON to verify error structure
    let json_value: serde_json::Value = serde_json::from_str(json_line)
        .expect("Error output should be valid JSON");
    
    // Verify error fields
    assert_eq!(json_value["status"].as_str().unwrap(), "error");
    assert!(json_value.get("message").is_some(), "Error JSON should contain message field");
    
    // Verify error message content
    let message = json_value["message"].as_str().unwrap();
    assert!(message.contains("not allowed"), "Error should mention validation failure");
}

/// Test that multiple tasks get sequential IDs with correct prefix
#[test]
fn test_sequential_task_ids_with_prefix() {
    let temp_dir = TempDir::new().unwrap();
    
    // Create a predictable project environment that generates DEFA prefix
    fs::write(temp_dir.path().join("Cargo.toml"), 
        "[package]\nname = \"default-example-feature-app\"\nversion = \"0.1.0\"\n").unwrap();
    
    // Create first task
    let mut cmd = Command::cargo_bin("lotar").unwrap();
    cmd.current_dir(temp_dir.path())
       .args(&["--experimental", "--format=json", "add", "First task", "--task-type=feature"])
       .assert()
       .success()
       .stdout(predicate::str::contains(r#""task_id":"DEFA-1""#));
    
    // Create second task
    let mut cmd = Command::cargo_bin("lotar").unwrap();
    cmd.current_dir(temp_dir.path())
       .args(&["--experimental", "--format=json", "add", "Second task", "--task-type=bug"])
       .assert()
       .success()
       .stdout(predicate::str::contains(r#""task_id":"DEFA-2""#));
    
    // Verify both directories and files exist
    let tasks_dir = temp_dir.path().join(".tasks");
    assert!(tasks_dir.join("DEFA").join("1.yml").exists());
    assert!(tasks_dir.join("DEFA").join("2.yml").exists());
}

/// Test explicit project resolution works correctly
#[test]
fn test_explicit_project_resolution() {
    let temp_dir = TempDir::new().unwrap();
    
    // Create task with explicit project
    let mut cmd = Command::cargo_bin("lotar").unwrap();
    cmd.current_dir(temp_dir.path())
       .args(&["--experimental", "--project=test", "--format=json", "add", "Explicit project task"])
       .assert()
       .success()
       .stdout(predicate::str::contains(r#""task_id":"TEST-1""#));
    
    // Verify TEST directory is created
    let tasks_dir = temp_dir.path().join(".tasks");
    assert!(tasks_dir.join("TEST").exists(), "TEST directory should exist");
    assert!(tasks_dir.join("TEST").join("1.yml").exists(), "Task file should exist in TEST directory");
}

/// Test that output format consistency works across commands
#[test]
fn test_output_format_consistency() {
    let temp_dir = TempDir::new().unwrap();
    
    // Create a predictable project environment that generates DEFA prefix
    fs::write(temp_dir.path().join("Cargo.toml"), 
        "[package]\nname = \"default-example-feature-app\"\nversion = \"0.1.0\"\n").unwrap();
    
    // Create a task first
    let mut cmd = Command::cargo_bin("lotar").unwrap();
    cmd.current_dir(temp_dir.path())
       .args(&["--experimental", "add", "Test task for list"])
       .assert()
       .success();
    
    // Test list command with JSON format
    let mut cmd = Command::cargo_bin("lotar").unwrap();
    let output = cmd.current_dir(temp_dir.path())
       .args(&["--experimental", "--format=json", "list"])
       .assert()
       .success()
       .get_output()
       .stdout
       .clone();
    
    let stdout = String::from_utf8(output).unwrap();
    
    // Should be valid JSON array
    let json_value: serde_json::Value = serde_json::from_str(&stdout)
        .expect("List output should be valid JSON");
    
    assert!(json_value.is_array(), "JSON list should be an array");
    assert!(!json_value.as_array().unwrap().is_empty(), "Should contain at least one task");
}

/// Test that project prefix is used consistently in task operations
#[test]
fn test_project_prefix_consistency_in_operations() {
    let temp_dir = TempDir::new().unwrap();
    
    // Create a predictable project environment that generates DEFA prefix
    fs::write(temp_dir.path().join("Cargo.toml"), 
        "[package]\nname = \"default-example-feature-app\"\nversion = \"0.1.0\"\n").unwrap();
    
    // Create task
    let mut cmd = Command::cargo_bin("lotar").unwrap();
    cmd.current_dir(temp_dir.path())
       .args(&["--experimental", "add", "Status test task"])
       .assert()
       .success()
       .stdout(predicate::str::contains("DEFA-1"));
    
    // Update status using the prefix-based ID
    let mut cmd = Command::cargo_bin("lotar").unwrap();
    cmd.current_dir(temp_dir.path())
       .args(&["--experimental", "status", "DEFA-1", "in_progress"])
       .assert()
       .success()
       .stdout(predicate::str::contains("Status changed successfully"));
    
    // Verify task file was updated in the correct directory
    let task_file = temp_dir.path().join(".tasks").join("DEFA").join("1.yml");
    let content = fs::read_to_string(&task_file).unwrap();
    assert!(content.contains("status: InProgress"));
}

/// Test regression: ensure no "default" directories are created
#[test]
fn test_no_default_directory_regression() {
    let temp_dir = TempDir::new().unwrap();
    
    // Create a predictable project environment that generates DEFA prefix
    fs::write(temp_dir.path().join("Cargo.toml"), 
        "[package]\nname = \"default-example-feature-app\"\nversion = \"0.1.0\"\n").unwrap();
    
    // Create multiple tasks
    for i in 1..=3 {
        let mut cmd = Command::cargo_bin("lotar").unwrap();
        cmd.current_dir(temp_dir.path())
           .args(&["--experimental", "add", &format!("Task {}", i)])
           .assert()
           .success();
    }
    
    let tasks_dir = temp_dir.path().join(".tasks");
    
    // Check that no "default" directory exists
    assert!(!tasks_dir.join("default").exists(), 
           "No 'default' directory should ever be created");
    
    // Verify DEFA directory exists with all tasks
    assert!(tasks_dir.join("DEFA").exists());
    assert!(tasks_dir.join("DEFA").join("1.yml").exists());
    assert!(tasks_dir.join("DEFA").join("2.yml").exists());
    assert!(tasks_dir.join("DEFA").join("3.yml").exists());
}

/// Test that JSON output fields match expected schema
#[test]
fn test_json_output_schema_compliance() {
    let temp_dir = TempDir::new().unwrap();
    
    // Create a predictable project environment that generates DEFA prefix
    fs::write(temp_dir.path().join("Cargo.toml"), 
        "[package]\nname = \"default-example-feature-app\"\nversion = \"0.1.0\"\n").unwrap();
    
    // Test successful add
    let mut cmd = Command::cargo_bin("lotar").unwrap();
    let output = cmd.current_dir(temp_dir.path())
       .args(&["--experimental", "--format=json", "add", "Schema test"])
       .assert()
       .success()
       .get_output()
       .stdout
       .clone();
    
    let stdout = String::from_utf8(output).unwrap();
    
    // Extract JSON line (last non-empty line should be the JSON output)
    let json_line = stdout.lines()
        .filter(|line| !line.is_empty() && line.trim_start().starts_with('{'))
        .last()
        .expect("Should find a JSON line in output");
    
    let json: serde_json::Value = serde_json::from_str(json_line).unwrap();
    
    // Verify exact schema for success response
    assert_eq!(json["status"].as_str().unwrap(), "success");
    assert!(json["message"].as_str().unwrap().starts_with("Created task:"));
    assert!(json["task_id"].as_str().unwrap().contains("DEFA-"));
    
    // Verify no unexpected fields
    let expected_fields = ["status", "message", "task_id"];
    for field in json.as_object().unwrap().keys() {
        assert!(expected_fields.contains(&field.as_str()), 
               "Unexpected field in JSON response: {}", field);
    }
}

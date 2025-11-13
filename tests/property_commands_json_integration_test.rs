//! JSON envelope tests for property commands (assignee, due date, priority, delete)

use predicates::prelude::*;
use serde_json::Value;

mod common;
use common::TestFixtures;

#[test]
fn test_assignee_get_set_json() {
    let fixtures = TestFixtures::new();
    let temp_dir = fixtures.temp_dir.path();

    // Create a task
    let mut cmd = crate::common::lotar_cmd().unwrap();
    let assert_result = cmd
        .current_dir(temp_dir)
        .env("LOTAR_TEST_SILENT", "1")
        .arg("add")
        .arg("Assignee JSON task")
        .arg("--project=test-project")
        .assert()
        .success()
        .stdout(predicate::str::contains("Created task:"));
    let output = assert_result.get_output();
    let created = String::from_utf8_lossy(&output.stdout);
    assert!(created.contains("Created task:"));

    // GET current assignee (expect null)
    let mut cmd = crate::common::lotar_cmd().unwrap();
    let output = cmd
        .current_dir(temp_dir)
        .env("LOTAR_TEST_SILENT", "1")
        .args(["assignee", "1", "--project=test-project", "--format=json"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let json: Value = serde_json::from_slice(&output).expect("valid JSON from assignee get");
    assert_eq!(json["status"], "success");
    assert!(json["task_id"].is_string());
    assert!(json.get("assignee").is_some());
    assert!(json["assignee"].is_null());

    // SET new assignee
    let mut cmd = crate::common::lotar_cmd().unwrap();
    let output = cmd
        .current_dir(temp_dir)
        .env("LOTAR_TEST_SILENT", "1")
        .args([
            "assignee",
            "1",
            "john.doe@example.com",
            "--project=test-project",
            "--format=json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let json: Value = serde_json::from_slice(&output).expect("valid JSON from assignee set");
    assert_eq!(json["status"], "success");
    assert!(json["message"].as_str().unwrap_or("").contains("assignee"));
    assert!(json["task_id"].is_string());
    assert!(json["old_assignee"].is_null());
    assert_eq!(json["new_assignee"], "john.doe@example.com");
}

#[test]
fn test_duedate_get_set_json() {
    let fixtures = TestFixtures::new();
    let temp_dir = fixtures.temp_dir.path();

    // Create a task
    let mut cmd = crate::common::lotar_cmd().unwrap();
    cmd.current_dir(temp_dir)
        .env("LOTAR_TEST_SILENT", "1")
        .args(["add", "DueDate JSON task", "--project=test-project"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Created task:"));

    // GET current due date (expect null)
    let mut cmd = crate::common::lotar_cmd().unwrap();
    let output = cmd
        .current_dir(temp_dir)
        .env("LOTAR_TEST_SILENT", "1")
        .args(["due-date", "1", "--project=test-project", "--format=json"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let json: Value = serde_json::from_slice(&output).expect("valid JSON from due date get");
    assert_eq!(json["status"], "success");
    assert!(json["task_id"].is_string());
    assert!(json["due_date"].is_null());

    // SET new due date
    let mut cmd = crate::common::lotar_cmd().unwrap();
    let output = cmd
        .current_dir(temp_dir)
        .env("LOTAR_TEST_SILENT", "1")
        .args([
            "due-date",
            "1",
            "2025-12-31",
            "--project=test-project",
            "--format=json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let json: Value = serde_json::from_slice(&output).expect("valid JSON from due date set");
    assert_eq!(json["status"], "success");
    assert!(json["message"].as_str().unwrap_or("").contains("due date"));
    assert!(json["task_id"].is_string());
    assert!(json["old_due_date"].is_null());
    assert_eq!(json["new_due_date"], "2025-12-31");
}

#[test]
fn test_priority_unchanged_json() {
    let fixtures = TestFixtures::new();
    let temp_dir = fixtures.temp_dir.path();

    // Create a task (default priority applies)
    let mut cmd = crate::common::lotar_cmd().unwrap();
    cmd.current_dir(temp_dir)
        .env("LOTAR_TEST_SILENT", "1")
        .args(["add", "Priority JSON task", "--project=test-project"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Created task:"));

    // Set the same priority to trigger unchanged path
    let mut cmd = crate::common::lotar_cmd().unwrap();
    let out = cmd
        .current_dir(temp_dir)
        .env("LOTAR_TEST_SILENT", "1")
        .args([
            "priority",
            "1",
            "medium",
            "--project=test-project",
            "--format=json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let json: Value = serde_json::from_slice(&out).expect("valid JSON from priority unchanged");
    assert_eq!(json["status"], "success");
    assert!(
        json["message"]
            .as_str()
            .unwrap_or("")
            .to_lowercase()
            .contains("unchanged")
    );
    assert!(json["task_id"].is_string());
    // Priority value should be present; compare case-insensitive with the requested value
    let p = json["priority"].as_str().unwrap_or("").to_lowercase();
    assert_eq!(p, "medium");
}

#[test]
fn test_task_delete_success_json() {
    let fixtures = TestFixtures::new();
    let temp_dir = fixtures.temp_dir.path();

    // Create a task
    let mut cmd = crate::common::lotar_cmd().unwrap();
    cmd.current_dir(temp_dir)
        .env("LOTAR_TEST_SILENT", "1")
        .args(["add", "Delete JSON task", "--project=test-project"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Created task:"));

    // Delete with --yes and JSON format
    let mut cmd = crate::common::lotar_cmd().unwrap();
    let out = cmd
        .current_dir(temp_dir)
        .env("LOTAR_TEST_SILENT", "1")
        .args([
            "task",
            "delete",
            "1",
            "--project=test-project",
            "--yes",
            "--format=json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let json: Value = serde_json::from_slice(&out).expect("valid JSON from delete");
    assert_eq!(json["status"], "success");
    assert!(
        json["message"]
            .as_str()
            .unwrap_or("")
            .to_lowercase()
            .contains("deleted")
    );
    // Task ID is the CLI-supplied ID for delete
    assert_eq!(json["task_id"], "1");

    // Optional: verify list is empty
    let mut cmd = crate::common::lotar_cmd().unwrap();
    let out = cmd
        .current_dir(temp_dir)
        .env("LOTAR_TEST_SILENT", "1")
        .args(["task", "list", "--project=test-project", "--format=json"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let json: Value = serde_json::from_slice(&out).expect("valid JSON from list after delete");
    assert!(json["tasks"].as_array().is_some());
    assert_eq!(json["tasks"].as_array().unwrap().len(), 0);
}

use crate::common::{cargo_bin_silent, extract_task_id_from_output};
use predicates::prelude::*;

mod common;
use common::TestFixtures;

#[test]
fn add_command_emits_info_logs() {
    let fixtures = TestFixtures::new();
    let temp_dir = fixtures.temp_dir.path();

    let mut cmd = cargo_bin_silent();
    let assert = cmd
        .current_dir(temp_dir)
        .arg("add")
        .arg("Logging test task")
        .arg("--type=feature")
        .arg("--priority=high")
        .arg("--log-level=info")
        .arg("--format=text")
        .assert()
        .success();

    let out = assert.get_output();
    let stdout = String::from_utf8_lossy(&out.stdout);
    let stderr = String::from_utf8_lossy(&out.stderr);

    assert!(
        stdout.contains("Created task"),
        "stdout should contain creation message: {stdout}"
    );
    assert!(
        stderr.contains("BEGIN ADD"),
        "stderr should contain BEGIN ADD: {stderr}"
    );
    assert!(
        stderr.contains("END ADD"),
        "stderr should contain END ADD: {stderr}"
    );
}

#[test]
fn list_command_emits_info_logs() {
    let fixtures = TestFixtures::new();
    let temp_dir = fixtures.temp_dir.path();

    // Seed a task
    cargo_bin_silent()
        .current_dir(temp_dir)
        .arg("add")
        .arg("List logging task")
        .arg("--type=feature")
        .arg("--priority=medium")
        .assert()
        .success();

    let mut cmd = cargo_bin_silent();
    let assert = cmd
        .current_dir(temp_dir)
        .arg("list")
        .arg("--log-level=info")
        .arg("--format=text")
        .assert()
        .success();

    let out = assert.get_output();
    let stdout = String::from_utf8_lossy(&out.stdout);
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(!stdout.trim().is_empty(), "list stdout should not be empty");
    assert!(
        stderr.contains("BEGIN LIST"),
        "stderr should contain BEGIN LIST: {stderr}"
    );
    assert!(
        stderr.contains("END LIST"),
        "stderr should contain END LIST: {stderr}"
    );
}

// Merged from logging_additional_coverage_test.rs
#[test]
fn config_templates_emits_stdout_and_logs() {
    let fixtures = TestFixtures::new();
    let temp_dir = fixtures.temp_dir.path();

    let mut cmd = cargo_bin_silent();
    cmd.current_dir(temp_dir)
        .arg("config")
        .arg("templates")
        .arg("--log-level=info")
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "Available Configuration Templates:",
        ))
        .stdout(predicate::str::contains("default"))
        .stderr(predicate::str::contains("BEGIN CONFIG"))
        .stderr(predicate::str::contains("END CONFIG status=ok"));
}

#[test]
fn config_show_with_tasks_dir_emits_expected_output_and_logs() {
    let fixtures = TestFixtures::new();
    let temp_dir = fixtures.temp_dir.path();
    let custom_tasks = temp_dir.join("custom_tasks");
    std::fs::create_dir_all(&custom_tasks).unwrap();

    let mut cmd = cargo_bin_silent();
    cmd.current_dir(temp_dir)
        .arg("config")
        .arg("show")
        .arg("--tasks-dir")
        .arg(custom_tasks.to_str().unwrap())
        .arg("--log-level=info")
        .assert()
        .success()
        .stdout(predicate::str::contains("Tasks directory:"))
        .stdout(predicate::str::contains(
            custom_tasks.to_string_lossy().to_string(),
        ))
        .stderr(predicate::str::contains("BEGIN CONFIG"))
        .stderr(predicate::str::contains("END CONFIG status=ok"));
}

#[test]
fn status_command_logs_and_json_notice_on_noop() {
    let fixtures = TestFixtures::new();
    let temp_dir = fixtures.temp_dir.path();

    // Create a task
    let add = cargo_bin_silent()
        .current_dir(temp_dir)
        .arg("add")
        .arg("Status logging task")
        .arg("--type=feature")
        .arg("--priority=low")
        .assert()
        .success();
    let stdout = String::from_utf8_lossy(&add.get_output().stdout).to_string();
    let task_id = extract_task_id_from_output(&stdout).expect("should extract task id");

    // Set to a new status first to ensure known state
    cargo_bin_silent()
        .current_dir(temp_dir)
        .args([
            "status",
            &task_id,
            "in_progress",
            "--log-level=info",
            "--format=text",
        ])
        .assert()
        .success();

    // Repeat with JSON format to trigger no-op path and JSON notice
    let mut cmd = cargo_bin_silent();
    let assert = cmd
        .current_dir(temp_dir)
        .args([
            "status",
            &task_id,
            "in_progress",
            "--log-level=info",
            "--format=json",
        ])
        .assert()
        .success();

    let out = assert.get_output();
    let stdout = String::from_utf8_lossy(&out.stdout);
    let stderr = String::from_utf8_lossy(&out.stderr);

    // JSON stdout should contain a minimal JSON object (notice)
    assert!(
        stdout.contains("status") && stdout.contains("unchanged"),
        "stdout should include JSON notice about unchanged status: {stdout}"
    );
    // Logs should include no-op info on stderr
    assert!(
        stderr.contains("BEGIN STATUS"),
        "stderr should contain BEGIN STATUS: {stderr}"
    );
    assert!(
        stderr.contains("no-op"),
        "stderr should mention no-op: {stderr}"
    );
}

#[test]
fn scan_command_emits_debug_logs() {
    let fixtures = TestFixtures::new();
    let temp_dir = fixtures.temp_dir.path();

    // Create a file to scan
    std::fs::write(
        temp_dir.join("scan_test.rs"),
        "// TODO: log it\nfn main() {}\n",
    )
    .unwrap();

    let mut cmd = cargo_bin_silent();
    let assert = cmd
        .current_dir(temp_dir)
        .args([
            "scan",
            "--log-level=debug",
            "--format=text",
            temp_dir.to_str().unwrap(),
        ])
        .assert()
        .success();

    let out = assert.get_output();
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("scan: begin"),
        "stderr should contain scan begin log: {stderr}"
    );
    assert!(
        stderr.contains("scan: scanning path=") || stderr.to_lowercase().contains("scan"),
        "stderr should contain scan path log: {stderr}"
    );
}

#[test]
fn priority_command_emits_info_logs() {
    let fixtures = TestFixtures::new();
    let temp_dir = fixtures.temp_dir.path();

    // Create a task to modify
    let add = cargo_bin_silent()
        .current_dir(temp_dir)
        .arg("add")
        .arg("Priority logging task")
        .arg("--type=feature")
        .arg("--priority=low")
        .assert()
        .success();
    let stdout = String::from_utf8_lossy(&add.get_output().stdout).to_string();
    let task_id = extract_task_id_from_output(&stdout).expect("should extract task id");

    let mut cmd = cargo_bin_silent();
    let assert = cmd
        .current_dir(temp_dir)
        .args([
            "priority",
            &task_id,
            "high",
            "--log-level=info",
            "--format=text",
        ])
        .assert()
        .success();

    let out = assert.get_output();
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("BEGIN PRIORITY"),
        "stderr should contain BEGIN PRIORITY: {stderr}"
    );
    assert!(
        stderr.contains("END PRIORITY"),
        "stderr should contain END PRIORITY: {stderr}"
    );
}

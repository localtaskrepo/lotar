use assert_cmd::Command;
use predicates::prelude::*;

mod common;
use common::TestFixtures;

// Additional coverage for logging hygiene across config commands

#[test]
fn config_templates_emits_stdout_and_logs() {
    let fixtures = TestFixtures::new();
    let temp_dir = fixtures.temp_dir.path();

    let mut cmd = Command::cargo_bin("lotar").unwrap();
    cmd.current_dir(temp_dir)
        .arg("config")
        .arg("templates")
        .arg("--log-level=info")
        .assert()
        .success()
        // stdout should have the templates list header
        .stdout(predicate::str::contains(
            "Available Configuration Templates:",
        ))
        .stdout(predicate::str::contains("default"))
        // stderr should carry logs
        .stderr(predicate::str::contains("BEGIN CONFIG"))
        .stderr(predicate::str::contains("END CONFIG status=ok"));
}

#[test]
fn config_show_with_tasks_dir_emits_expected_output_and_logs() {
    let fixtures = TestFixtures::new();
    let temp_dir = fixtures.temp_dir.path();
    let custom_tasks = temp_dir.join("custom_tasks");
    std::fs::create_dir_all(&custom_tasks).unwrap();

    let mut cmd = Command::cargo_bin("lotar").unwrap();
    cmd.current_dir(temp_dir)
        .arg("config")
        .arg("show")
        .arg("--tasks-dir")
        .arg(custom_tasks.to_str().unwrap())
        .arg("--log-level=info")
        .assert()
        .success()
        // stdout should show tasks directory path
        .stdout(predicate::str::contains("Tasks directory:"))
        .stdout(predicate::str::contains(
            custom_tasks.to_string_lossy().to_string(),
        ))
        // stderr should carry logs
        .stderr(predicate::str::contains("BEGIN CONFIG"))
        .stderr(predicate::str::contains("END CONFIG status=ok"));
}

use assert_cmd::Command;
use lotar::help::HelpSystem;
use lotar::output::OutputFormat;
use lotar::workspace::TasksDirectoryResolver;
use predicates::prelude::*;
use std::fs;
use tempfile::TempDir;

#[test]
fn test_relative_path_finds_parent_directory() {
    let temp_dir = TempDir::new().unwrap();

    // Create a parent directory with .tasks
    let parent_tasks_dir = temp_dir.path().join(".tasks");
    fs::create_dir_all(&parent_tasks_dir).unwrap();

    // Create config in parent .tasks directory
    let parent_config = parent_tasks_dir.join("config.yml");
    fs::write(&parent_config, "default.project: parent-project\n").unwrap();

    // Create a subdirectory
    let sub_dir = temp_dir.path().join("subproject");
    fs::create_dir_all(&sub_dir).unwrap();

    // From the subdirectory, test that LOTAR_TASKS_DIR=.tasks finds the parent
    let mut cmd = Command::cargo_bin("lotar").unwrap();
    cmd.current_dir(&sub_dir)
        .env("LOTAR_TASKS_DIR", ".tasks")
        .arg("config")
        .arg("show")
        .assert()
        .success()
        .stdout(predicate::str::contains("parent-project"));

    // Verify no .tasks directory was created in the subdirectory
    let sub_tasks_dir = sub_dir.join(".tasks");
    assert!(
        !sub_tasks_dir.exists(),
        "Should not create .tasks in subdirectory"
    );
}

#[test]
fn test_relative_path_creates_new_when_no_parent_exists() {
    let temp_dir = TempDir::new().unwrap();

    // Create a subdirectory (but no parent .tasks directory)
    let sub_dir = temp_dir.path().join("subproject");
    fs::create_dir_all(&sub_dir).unwrap();

    // From the subdirectory, test that LOTAR_TASKS_DIR=.tasks creates new directory
    let mut cmd = Command::cargo_bin("lotar").unwrap();
    cmd.current_dir(&sub_dir)
        .env("LOTAR_TASKS_DIR", ".tasks")
        .arg("config")
        .arg("show")
        .assert()
        .success();

    // Verify .tasks directory was created in the subdirectory (current directory)
    let sub_tasks_dir = sub_dir.join(".tasks");
    assert!(
        sub_tasks_dir.exists(),
        "Should create .tasks in current directory when no parent exists"
    );
}

#[test]
fn test_complex_relative_path_no_parent_search() {
    let temp_dir = TempDir::new().unwrap();

    // Create a parent directory with .tasks (should NOT be found for complex paths)
    let parent_tasks_dir = temp_dir.path().join(".tasks");
    fs::create_dir_all(&parent_tasks_dir).unwrap();

    // Create a subdirectory
    let sub_dir = temp_dir.path().join("subproject");
    fs::create_dir_all(&sub_dir).unwrap();

    // From the subdirectory, test complex relative path doesn't trigger parent search
    let mut cmd = Command::cargo_bin("lotar").unwrap();
    cmd.current_dir(&sub_dir)
        .env("LOTAR_TASKS_DIR", "../other/tasks")
        .arg("config")
        .arg("show")
        .assert()
        .success()
        .stdout(predicate::str::contains("Tasks directory: ../other/tasks"));

    // Verify the complex path directory was created
    let complex_path_dir = sub_dir.join("../other/tasks");
    assert!(
        complex_path_dir.exists(),
        "Should create directory at complex relative path"
    );
}

// Merged from misc_smoke_unit_test.rs
#[test]
fn help_system_smoke() {
    let help = HelpSystem::new(OutputFormat::Text, false);
    let _ = help;
    let result = help.list_available_help();
    assert!(result.is_ok());
}

#[test]
fn explicit_path_resolution_creates_dir_and_resolves() {
    let temp_dir = tempfile::TempDir::new().unwrap();
    let tasks_dir = temp_dir.path().join("custom_tasks");
    std::fs::create_dir_all(&tasks_dir).unwrap();
    let resolver = TasksDirectoryResolver::resolve(tasks_dir.to_str(), None).unwrap();
    assert_eq!(resolver.path, tasks_dir);
}

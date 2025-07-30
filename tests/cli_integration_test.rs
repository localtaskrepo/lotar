use assert_cmd::Command;
use predicates::prelude::*;
use tempfile::TempDir;

mod common;
use common::TestFixtures;

/// Test the CLI commands work correctly
#[test]
fn test_cli_help_command() {
    let mut cmd = Command::cargo_bin("local_task_repo").unwrap();
    cmd.arg("help")
        .assert()
        .success()
        .stdout(predicate::str::contains("help"));
}

#[test]
fn test_cli_task_add_command() {
    let temp_dir = TempDir::new().unwrap();

    let mut cmd = Command::cargo_bin("local_task_repo").unwrap();
    cmd.current_dir(&temp_dir)
        .args(&[
            "task", "add",
            "--title=CLI Test Task",
            "--priority=2",
            "--project=cli-test"
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("Added task with id"));

    // Verify task file was created
    let tasks_dir = temp_dir.path().join(".tasks/cli-test");
    assert!(tasks_dir.exists(), "Tasks directory should be created");
}

#[test]
fn test_cli_task_add_with_tags() {
    let temp_dir = TempDir::new().unwrap();

    let mut cmd = Command::cargo_bin("local_task_repo").unwrap();
    cmd.current_dir(&temp_dir)
        .args(&[
            "task", "add",
            "--title=Tagged Task",
            "--tag=frontend",
            "--tag=urgent"
        ])
        .assert()
        .success();
}

#[test]
fn test_cli_task_add_missing_title() {
    let temp_dir = TempDir::new().unwrap();

    let mut cmd = Command::cargo_bin("local_task_repo").unwrap();
    cmd.current_dir(&temp_dir)
        .args(&["task", "add", "--priority=1"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("Title is required"));
}

#[test]
fn test_cli_scan_command() {
    let fixtures = TestFixtures::new();
    fixtures.create_test_source_files();

    let mut cmd = Command::cargo_bin("local_task_repo").unwrap();
    cmd.current_dir(fixtures.get_temp_path())
        .args(&["scan"])
        .assert()
        .success()
        .stdout(predicate::str::contains("TODO"));
}

#[test]
fn test_cli_scan_with_path() {
    let fixtures = TestFixtures::new();
    fixtures.create_test_source_files();

    let mut cmd = Command::cargo_bin("local_task_repo").unwrap();
    cmd.args(&["scan", fixtures.get_temp_path().to_str().unwrap()])
        .assert()
        .success();
}

#[test]
fn test_cli_serve_command_starts() {
    // Test that serve command starts successfully (it should keep running)
    // We spawn the process and check it starts, then kill it
    use std::process::{Command, Stdio};
    use std::time::Duration;
    use std::thread;
    
    let mut child = Command::new("cargo")
        .args(&["run", "--", "serve", "8002"]) // Use different port to avoid conflicts
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("Failed to start server");
    
    // Give the server time to start
    thread::sleep(Duration::from_millis(500));
    
    // Check if the process is still running (which means it started successfully)
    match child.try_wait() {
        Ok(Some(_status)) => {
            // Process has exited - this is unexpected for a server
            panic!("Server process exited unexpectedly");
        }
        Ok(None) => {
            // Process is still running - this is what we expect
            // Server started successfully, now terminate it
            child.kill().expect("Failed to kill server process");
            child.wait().expect("Failed to wait for server process");
        }
        Err(e) => {
            panic!("Error checking server process status: {}", e);
        }
    }
}

#[test]
fn test_cli_invalid_command() {
    let mut cmd = Command::cargo_bin("local_task_repo").unwrap();
    cmd.arg("invalid_command")
        .assert()
        .failure()
        .stdout(predicate::str::contains("Invalid command"));
}

#[test]
fn test_cli_no_command() {
    let mut cmd = Command::cargo_bin("local_task_repo").unwrap();
    cmd.assert()
        .failure()
        .stdout(predicate::str::contains("No command specified"));
}

#[test]
fn test_cli_config_operations() {
    let temp_dir = TempDir::new().unwrap();

    // Test config set
    let mut cmd = Command::cargo_bin("local_task_repo").unwrap();
    cmd.current_dir(&temp_dir)
        .args(&["config", "set", "test_key", "test_value"])
        .assert()
        .success();

    // Test config get
    let mut cmd = Command::cargo_bin("local_task_repo").unwrap();
    cmd.current_dir(&temp_dir)
        .args(&["config", "get", "test_key"])
        .assert()
        .success();
}

/// Integration tests for complex CLI workflows
#[cfg(test)]
mod integration_workflows {
    use super::*;

    #[test]
    fn test_complete_task_workflow() {
        let temp_dir = TempDir::new().unwrap();

        // Create a task
        let mut cmd = Command::cargo_bin("local_task_repo").unwrap();
        let output = cmd.current_dir(&temp_dir)
            .args(&[
                "task", "add",
                "--title=Workflow Test Task",
                "--description=Testing complete workflow",
                "--priority=1"
            ])
            .output()
            .unwrap();

        assert!(output.status.success());
        let stdout = String::from_utf8(output.stdout).unwrap();
        assert!(stdout.contains("Added task with id"));

        // Extract task ID from output (assuming format "Added task with id: 1")
        let task_id = stdout
            .split("Added task with id: ")
            .nth(1)
            .unwrap()
            .trim()
            .split_whitespace()
            .next()
            .unwrap();

        // Edit the task
        let mut cmd = Command::cargo_bin("local_task_repo").unwrap();
        cmd.current_dir(&temp_dir)
            .args(&[
                "task", "edit", task_id,
                "--title=Updated Workflow Task"
            ])
            .assert()
            .success();
    }

    #[test]
    fn test_project_isolation() {
        let temp_dir = TempDir::new().unwrap();

        // Create tasks in different projects
        let mut cmd1 = Command::cargo_bin("local_task_repo").unwrap();
        cmd1.current_dir(&temp_dir)
            .args(&[
                "task", "add",
                "--title=Project A Task",
                "--project=project-a"
            ])
            .assert()
            .success();

        let mut cmd2 = Command::cargo_bin("local_task_repo").unwrap();
        cmd2.current_dir(&temp_dir)
            .args(&[
                "task", "add",
                "--title=Project B Task",
                "--project=project-b"
            ])
            .assert()
            .success();

        // Verify both projects have task directories
        assert!(temp_dir.path().join(".tasks/project-a").exists());
        assert!(temp_dir.path().join(".tasks/project-b").exists());
    }

    #[test]
    fn test_scan_and_task_integration() {
        let fixtures = TestFixtures::new();
        let _source_files = fixtures.create_test_source_files();

        // First, scan for TODOs
        let mut scan_cmd = Command::cargo_bin("local_task_repo").unwrap();
        let scan_output = scan_cmd
            .current_dir(fixtures.get_temp_path())
            .args(&["scan"])
            .output()
            .unwrap();

        assert!(scan_output.status.success());
        let scan_stdout = String::from_utf8(scan_output.stdout).unwrap();
        assert!(scan_stdout.contains("TODO"));

        // Then create a task in the same directory
        let mut task_cmd = Command::cargo_bin("local_task_repo").unwrap();
        task_cmd.current_dir(fixtures.get_temp_path())
            .args(&[
                "task", "add",
                "--title=Task for scanned project",
                "--project=scanned-project"
            ])
            .assert()
            .success();

        // Verify both scan results and task creation work in same directory
        assert!(fixtures.get_temp_path().join(".tasks/scanned-project").exists());
    }
}

/// Error handling and edge cases
#[cfg(test)]
mod error_handling {
    use super::*;

    #[test]
    fn test_task_edit_nonexistent_id() {
        let temp_dir = TempDir::new().unwrap();

        let mut cmd = Command::cargo_bin("local_task_repo").unwrap();
        cmd.current_dir(&temp_dir)
            .args(&["task", "edit", "99999"])
            .assert()
            .failure()
            .stdout(predicate::str::contains("not found"));
    }

    #[test]
    fn test_task_edit_invalid_id() {
        let temp_dir = TempDir::new().unwrap();

        let mut cmd = Command::cargo_bin("local_task_repo").unwrap();
        cmd.current_dir(&temp_dir)
            .args(&["task", "edit", "invalid"])
            .assert()
            .failure()
            .stderr(predicate::str::contains("Invalid id"));
    }

    #[test]
    fn test_scan_nonexistent_directory() {
        let mut cmd = Command::cargo_bin("local_task_repo").unwrap();
        cmd.args(&["scan", "/nonexistent/path"])
            .assert()
            .failure();
    }

    #[test]
    fn test_config_missing_arguments() {
        let mut cmd = Command::cargo_bin("local_task_repo").unwrap();
        cmd.args(&["config"])
            .assert()
            .failure()
            .stdout(predicate::str::contains("No config operation specified"));
    }
}

/// Performance tests for CLI operations
#[cfg(test)]
mod cli_performance {
    use super::*;
    use std::time::Instant;

    #[test]
    fn test_cli_startup_time() {
        let start = Instant::now();

        let mut cmd = Command::cargo_bin("local_task_repo").unwrap();
        cmd.arg("help")
            .assert()
            .success();

        let duration = start.elapsed();
        assert!(duration.as_millis() < 1000, "CLI should start within 1 second");
    }

    #[test]
    fn test_bulk_task_creation_performance() {
        let temp_dir = TempDir::new().unwrap();
        let start = Instant::now();

        // Create 10 tasks via CLI
        for i in 0..10 {
            let mut cmd = Command::cargo_bin("local_task_repo").unwrap();
            cmd.current_dir(&temp_dir)
                .args(&[
                    "task", "add",
                    &format!("--title=Bulk Task {}", i),
                    "--priority=2"
                ])
                .assert()
                .success();
        }

        let duration = start.elapsed();
        assert!(duration.as_millis() < 5000,
               "10 task creations should complete within 5 seconds, took: {:?}", duration);
    }
}

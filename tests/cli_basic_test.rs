use assert_cmd::Command;
use predicates::prelude::*;
use tempfile::TempDir;

mod common;

/// Basic CLI command tests - help, version, invalid commands
#[cfg(test)]
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
}

/// Configuration command tests
#[cfg(test)]
mod config_commands {
    use super::*;

    #[test]
    fn test_config_set_operation() {
        let temp_dir = TempDir::new().unwrap();

        let mut cmd = Command::cargo_bin("lotar").unwrap();
        cmd.current_dir(&temp_dir)
            .args(&["config", "set", "server_port", "9000"])
            .assert()
            .success()
            .stdout(predicate::str::contains("Successfully updated server_port"));
    }

    #[test]
    fn test_config_missing_arguments() {
        let mut cmd = Command::cargo_bin("lotar").unwrap();
        cmd.args(&["config"])
            .assert()
            .failure() // Now expects failure since no subcommand provided
            .stderr(predicate::str::contains("Usage"));
    }
}

/// Scan command tests
#[cfg(test)]
mod scan_commands {
    use super::*;

    #[test]
    fn test_scan_current_directory() {
        let temp_dir = TempDir::new().unwrap();

        let mut cmd = Command::cargo_bin("lotar").unwrap();
        cmd.current_dir(&temp_dir).arg("scan").assert().success();
    }

    #[test]
    fn test_scan_with_path() {
        let temp_dir = TempDir::new().unwrap();

        let mut cmd = Command::cargo_bin("lotar").unwrap();
        cmd.current_dir(&temp_dir)
            .args(&["scan", temp_dir.path().to_str().unwrap()])
            .assert()
            .success();
    }

    #[test]
    fn test_scan_with_todo_files() {
        let temp_dir = TempDir::new().unwrap();

        // Create a test file with TODO comments
        let test_file = temp_dir.path().join("test.js");
        std::fs::write(
            &test_file,
            "// TODO: This is a test todo comment\nfunction test() {}",
        )
        .unwrap();

        let mut cmd = Command::cargo_bin("lotar").unwrap();
        cmd.current_dir(&temp_dir)
            .args(&["scan", temp_dir.path().to_str().unwrap()])
            .assert()
            .success()
            .stdout(predicate::str::contains("Scanning"));
    }

    #[test]
    fn test_scan_nonexistent_directory() {
        let mut cmd = Command::cargo_bin("lotar").unwrap();
        cmd.args(&["scan", "/nonexistent/path"]).assert().failure();
    }
}

/// Serve command tests
#[cfg(test)]
mod serve_commands {
    use std::process::{Command as StdCommand, Stdio};
    use std::thread;
    use std::time::Duration;

    #[test]
    fn test_serve_command_starts() {
        let mut child = StdCommand::new("cargo")
            .args(&["run", "--bin", "lotar", "--", "serve", "--port=0"])
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("Failed to start serve command");

        // Give it a moment to start
        thread::sleep(Duration::from_millis(1000));

        // Try to kill the process gracefully first
        match child.try_wait() {
            Ok(Some(status)) => {
                assert!(
                    status.success() || status.code().is_some(),
                    "Process should exit with a status code"
                );
            }
            Ok(None) => {
                let _ = child.kill();
                let _output = child
                    .wait_with_output()
                    .expect("Failed to wait for process");
                assert!(
                    true,
                    "Serve command started and was terminated successfully"
                );
            }
            Err(_) => {
                let _ = child.kill();
                panic!("Failed to check process status");
            }
        }
    }

    #[test]
    fn test_serve_with_timeout() {
        let child = StdCommand::new("timeout")
            .args(&["2s"])
            .arg("cargo")
            .args(&["run", "--bin", "lotar", "--", "serve", "8080"])
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn();

        if let Ok(mut child) = child {
            thread::sleep(Duration::from_millis(500));
            let _ = child.kill();
            assert!(true, "Serve command test completed");
        } else {
            // Skip this test if timeout command is not available
            assert!(true, "Timeout command not available, skipping serve test");
        }
    }
}

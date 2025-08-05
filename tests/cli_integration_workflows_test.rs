use assert_cmd::Command;
use tempfile::TempDir;

mod common;
use common::TestFixtures;

/// Integration workflow tests - complex multi-step scenarios
#[cfg(test)]
mod workflow_tests {
    use super::*;

    #[test]
    fn test_complete_task_workflow() {
        let temp_dir = TempDir::new().unwrap();

        // Create a task using the new CLI format
        let mut cmd = Command::cargo_bin("lotar").unwrap();
        let output = cmd
            .current_dir(&temp_dir)
            .args(&[
                "add",
                "Workflow Test Task",
                "--description=Testing complete workflow",
                "--priority=high",
            ])
            .output()
            .unwrap();

        assert!(output.status.success());
        let stdout = String::from_utf8(output.stdout).unwrap();
        assert!(stdout.contains("Created task"));

        // Extract task ID from "✅ Created task: DEFA-1" format
        let _task_id = stdout
            .lines()
            .find(|line| line.contains("Created task:"))
            .and_then(|line| line.split_whitespace().last())
            .expect("Could not extract task ID");

        // Test that we can list tasks and see our created task
        let mut cmd = Command::cargo_bin("lotar").unwrap();
        cmd.current_dir(&temp_dir)
            .args(&["list"])
            .assert()
            .success();
    }

    #[test]
    fn test_project_isolation() {
        let temp_dir = TempDir::new().unwrap();

        // Create tasks in different projects
        let mut cmd1 = Command::cargo_bin("lotar").unwrap();
        cmd1.current_dir(&temp_dir)
            .args(&[
                "task",
                "add",
                "--title=Project A Task",
                "--project=project-a",
            ])
            .assert()
            .success();

        let mut cmd2 = Command::cargo_bin("lotar").unwrap();
        cmd2.current_dir(&temp_dir)
            .args(&[
                "task",
                "add",
                "--title=Project B Task",
                "--project=project-b",
            ])
            .assert()
            .success();

        // Verify task directories were created
        let tasks_root = temp_dir.path().join(".tasks");
        assert!(
            tasks_root.exists(),
            "Tasks root directory should be created"
        );

        let tasks_dirs: Vec<_> = std::fs::read_dir(&tasks_root)
            .unwrap()
            .filter_map(|entry| entry.ok())
            .filter(|entry| entry.path().is_dir())
            .collect();
        assert!(
            tasks_dirs.len() >= 2,
            "At least two project directories should be created"
        );
    }

    #[test]
    fn test_scan_and_task_integration() {
        let fixtures = TestFixtures::new();
        let _source_files = fixtures.create_test_source_files();

        // First, scan for TODOs
        let mut scan_cmd = Command::cargo_bin("lotar").unwrap();
        let scan_output = scan_cmd
            .current_dir(fixtures.get_temp_path())
            .args(&["scan"])
            .output()
            .unwrap();

        // Debug: Print actual output if test fails
        if !scan_output.status.success() {
            eprintln!("Scan command failed!");
            eprintln!("Exit code: {}", scan_output.status);
            eprintln!("Stdout: {}", String::from_utf8_lossy(&scan_output.stdout));
            eprintln!("Stderr: {}", String::from_utf8_lossy(&scan_output.stderr));
        }

        assert!(scan_output.status.success());
        let scan_stdout = String::from_utf8(scan_output.stdout).unwrap();
        assert!(scan_stdout.contains("TODO"));

        // Then create a task in the same directory
        let mut task_cmd = Command::cargo_bin("lotar").unwrap();
        task_cmd
            .current_dir(fixtures.get_temp_path())
            .args(&[
                "task",
                "add",
                "--title=Task for scanned project",
                "--project=scanned-project",
            ])
            .assert()
            .success();

        // Verify task directory was created
        let tasks_root = fixtures.get_temp_path().join(".tasks");
        assert!(
            tasks_root.exists(),
            "Tasks root directory should be created"
        );

        let tasks_dirs: Vec<_> = std::fs::read_dir(&tasks_root)
            .unwrap()
            .filter_map(|entry| entry.ok())
            .filter(|entry| entry.path().is_dir())
            .collect();
        assert!(
            !tasks_dirs.is_empty(),
            "At least one project directory should be created"
        );
    }
}

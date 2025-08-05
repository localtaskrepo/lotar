use assert_cmd::Command;
use predicates::prelude::*;
use tempfile::TempDir;
use std::fs;

mod common;

/// Tests for smart project parameter parsing and filtering functionality
#[cfg(test)]
mod smart_project_filtering_tests {
    use super::*;

    /// Helper function to create a test environment with multiple projects and tasks
    fn setup_multi_project_environment() -> TempDir {
        let temp_dir = TempDir::new().unwrap();
        let tasks_root = temp_dir.path().join(".tasks");
        
        // Create project structures
        let frontend_dir = tasks_root.join("FRON");
        let backend_dir = tasks_root.join("AB");
        
        fs::create_dir_all(&frontend_dir).unwrap();
        fs::create_dir_all(&backend_dir).unwrap();
        
        // Create project configs
        let frontend_config = r#"project_name: FRONTEND"#;
        let backend_config = r#"project_name: API-BACKEND"#;
        
        fs::write(frontend_dir.join("config.yml"), frontend_config).unwrap();
        fs::write(backend_dir.join("config.yml"), backend_config).unwrap();
        
        // Create some test tasks
        let frontend_task = r#"title: Frontend Task 1
created: "2024-01-01T00:00:00Z"
modified: "2024-01-01T00:00:00Z"
"#;
        
        let backend_task = r#"title: Backend Task 1
created: "2024-01-01T00:00:00Z"
modified: "2024-01-01T00:00:00Z"
"#;
        
        fs::write(frontend_dir.join("1.yml"), frontend_task).unwrap();
        fs::write(backend_dir.join("1.yml"), backend_task).unwrap();
        
        // Create global config
        let global_config = r#"default_prefix: FRON"#;
        fs::write(tasks_root.join("config.yml"), global_config).unwrap();
        
        temp_dir
    }

    #[test]
    fn test_task_list_filtering_with_prefix() {
        let temp_dir = setup_multi_project_environment();

        let mut cmd = Command::cargo_bin("lotar").unwrap();
        cmd.current_dir(&temp_dir)
            .args(&["task", "list", "--project=FRON"])
            .assert()
            .success()
            .stdout(predicate::str::contains("Frontend Task 1"))
            .stdout(predicate::str::contains("FRON-1")); // Should show task ID in list (same as search)
    }

    #[test]
    fn test_task_list_filtering_with_full_project_name() {
        let temp_dir = setup_multi_project_environment();

        let mut cmd = Command::cargo_bin("lotar").unwrap();
        cmd.current_dir(&temp_dir)
            .args(&["task", "list", "--project=FRONTEND"])
            .assert()
            .success()
            .stdout(predicate::str::contains("Frontend Task 1"));
    }

    #[test]
    fn test_task_list_filtering_backend_with_prefix() {
        let temp_dir = setup_multi_project_environment();

        let mut cmd = Command::cargo_bin("lotar").unwrap();
        cmd.current_dir(&temp_dir)
            .args(&["task", "list", "--project=AB"])
            .assert()
            .success()
            .stdout(predicate::str::contains("Backend Task 1"))
            .stdout(predicate::str::contains("Frontend Task 1").not()); // Should not show frontend tasks
    }

    #[test]
    fn test_task_list_filtering_backend_with_full_name() {
        let temp_dir = setup_multi_project_environment();

        let mut cmd = Command::cargo_bin("lotar").unwrap();
        cmd.current_dir(&temp_dir)
            .args(&["task", "list", "--project=API-BACKEND"])
            .assert()
            .success()
            .stdout(predicate::str::contains("Backend Task 1"))
            .stdout(predicate::str::contains("Frontend Task 1").not());
    }

    #[test]
    fn test_task_search_filtering_with_prefix() {
        let temp_dir = setup_multi_project_environment();

        let mut cmd = Command::cargo_bin("lotar").unwrap();
        cmd.current_dir(&temp_dir)
            .args(&["task", "search", "Task", "--project=FRON"])
            .assert()
            .success()
            .stdout(predicate::str::contains("Frontend Task 1"))
            .stdout(predicate::str::contains("Backend Task 1").not());
    }

    #[test]
    fn test_task_search_filtering_with_full_project_name() {
        let temp_dir = setup_multi_project_environment();

        let mut cmd = Command::cargo_bin("lotar").unwrap();
        cmd.current_dir(&temp_dir)
            .args(&["task", "search", "Task", "--project=API-BACKEND"])
            .assert()
            .success()
            .stdout(predicate::str::contains("Backend Task 1"))
            .stdout(predicate::str::contains("Frontend Task 1").not());
    }

    #[test]
    fn test_task_search_shows_full_project_names() {
        let temp_dir = setup_multi_project_environment();

        let mut cmd = Command::cargo_bin("lotar").unwrap();
        cmd.current_dir(&temp_dir)
            .args(&["task", "search", "Task", "--project=AB"])
            .assert()
            .success()
            .stdout(predicate::str::contains("AB-1")); // Should show task ID (same format as list)
    }

    #[test]
    fn test_task_search_without_project_filter() {
        let temp_dir = setup_multi_project_environment();

        let mut cmd = Command::cargo_bin("lotar").unwrap();
        cmd.current_dir(&temp_dir)
            .args(&["task", "search", "Task"])
            .assert()
            .success()
            .stdout(predicate::str::contains("Frontend Task 1"))
            .stdout(predicate::str::contains("Backend Task 1"));
    }

    #[test]
    fn test_task_list_with_nonexistent_project() {
        let temp_dir = setup_multi_project_environment();

        let mut cmd = Command::cargo_bin("lotar").unwrap();
        cmd.current_dir(&temp_dir)
            .args(&["task", "list", "--project=NONEXISTENT"])
            .assert()
            .success()
            .stdout(predicate::str::contains("No tasks found"));
    }

    #[test]
    fn test_task_search_with_nonexistent_project() {
        let temp_dir = setup_multi_project_environment();

        let mut cmd = Command::cargo_bin("lotar").unwrap();
        cmd.current_dir(&temp_dir)
            .args(&["task", "search", "Task", "--project=NONEXISTENT"])
            .assert()
            .success()
            .stdout(predicate::str::contains("No tasks found"));
    }

    #[test]
    fn test_mixed_filtering_search_with_status_and_project() {
        let temp_dir = setup_multi_project_environment();

        // First, add a task with different status to test combined filtering
        let mut cmd = Command::cargo_bin("lotar").unwrap();
        cmd.current_dir(&temp_dir)
            .args(&["task", "add", "--title=In Progress Task", "--project=FRON"])
            .assert()
            .success();

        // Update the status of the new task
        let mut cmd = Command::cargo_bin("lotar").unwrap();
        cmd.current_dir(&temp_dir)
            .args(&["task", "status", "FRON-2", "IN_PROGRESS"])
            .assert()
            .success();

        // Search for tasks with specific status and project
        let mut cmd = Command::cargo_bin("lotar").unwrap();
        cmd.current_dir(&temp_dir)
            .args(&["task", "search", "Task", "--project=FRON", "--status=IN_PROGRESS"])
            .assert()
            .success()
            .stdout(predicate::str::contains("In Progress Task"))
            .stdout(predicate::str::contains("Frontend Task 1").not()); // Should not show TODO tasks
    }

    #[test]
    fn test_case_insensitive_project_name_resolution() {
        let temp_dir = setup_multi_project_environment();

        // Test with lowercase full project name
        let mut cmd = Command::cargo_bin("lotar").unwrap();
        cmd.current_dir(&temp_dir)
            .args(&["task", "list", "--project=frontend"])
            .assert()
            .success()
            .stdout(predicate::str::contains("Frontend Task 1"));
    }
}

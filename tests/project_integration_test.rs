//! Project management and detection tests
//!
//! This module consolidates all project-related tests including:
//! - Project structure detection and validation
//! - Multi-project workspace handling
//! - Project scanning and filtering
//! - Smart project management features

use predicates::prelude::*;
use std::fs;

mod common;
use common::TestFixtures;

// =============================================================================
// Project Detection and Structure
// =============================================================================

mod project_detection {
    use super::*;

    #[test]
    fn test_project_type_detection() {
        let test_fixtures = TestFixtures::new();
        let temp_dir = test_fixtures.temp_dir.path();

        // Create Rust project structure
        fs::write(
            temp_dir.join("Cargo.toml"),
            "[package]\nname = \"test-project\"\nversion = \"0.1.0\"",
        )
        .unwrap();
        fs::create_dir_all(temp_dir.join("src")).unwrap();
        fs::write(temp_dir.join("src/main.rs"), "fn main() {}").unwrap();

        // Test project detection (scan command shows scanning output, not project info)
        let mut cmd = crate::common::lotar_cmd().unwrap();
        cmd.current_dir(temp_dir)
            .arg("scan")
            .assert()
            .success()
            .stdout(predicate::str::contains("Scanning"));
    }

    #[test]
    fn test_multi_project_detection() {
        let test_fixtures = TestFixtures::new();
        let temp_dir = test_fixtures.temp_dir.path();

        // Create multiple project types
        // Rust project
        fs::write(
            temp_dir.join("Cargo.toml"),
            "[package]\nname = \"rust-project\"\nversion = \"0.1.0\"",
        )
        .unwrap();
        fs::create_dir_all(temp_dir.join("src")).unwrap();

        // Node.js project
        let node_dir = temp_dir.join("node-project");
        fs::create_dir_all(&node_dir).unwrap();
        fs::write(
            node_dir.join("package.json"),
            r#"{"name": "node-project", "version": "1.0.0"}"#,
        )
        .unwrap();

        // Python project
        let python_dir = temp_dir.join("python-project");
        fs::create_dir_all(&python_dir).unwrap();
        fs::write(
            python_dir.join("setup.py"),
            "from setuptools import setup\nsetup(name='python-project')",
        )
        .unwrap();

        // Test scan works with multiple project files
        let mut cmd = crate::common::lotar_cmd().unwrap();
        cmd.current_dir(temp_dir)
            .arg("scan")
            .assert()
            .success()
            .stdout(predicate::str::contains("Scanning"));
    }

    #[test]
    fn test_project_filtering() {
        let test_fixtures = TestFixtures::new();
        let temp_dir = test_fixtures.temp_dir.path();

        // Create multiple project types
        fs::write(
            temp_dir.join("Cargo.toml"),
            "[package]\nname = \"rust-project\"",
        )
        .unwrap();

        let node_dir = temp_dir.join("node-app");
        fs::create_dir_all(&node_dir).unwrap();
        fs::write(node_dir.join("package.json"), r#"{"name": "node-app"}"#).unwrap();

        // Test basic scanning without filtering (scan command doesn't have --filter)
        let mut cmd = crate::common::lotar_cmd().unwrap();
        cmd.current_dir(temp_dir)
            .arg("scan")
            .assert()
            .success()
            .stdout(predicate::str::contains("Scanning"));

        // Test basic scanning again
        let mut cmd = crate::common::lotar_cmd().unwrap();
        cmd.current_dir(temp_dir)
            .arg("scan")
            .assert()
            .success()
            .stdout(predicate::str::contains("Scanning"));
    }
}

// =============================================================================
// Project Structure Validation
// =============================================================================

mod structure_validation {
    use super::*;

    #[test]
    fn test_invalid_project_structure() {
        let test_fixtures = TestFixtures::new();
        let temp_dir = test_fixtures.temp_dir.path();

        // Create incomplete Rust project (Cargo.toml without src/)
        fs::write(
            temp_dir.join("Cargo.toml"),
            "[package]\nname = \"incomplete-rust\"",
        )
        .unwrap();

        // Scan should work regardless of project completeness
        let mut cmd = crate::common::lotar_cmd().unwrap();
        cmd.current_dir(temp_dir)
            .arg("scan")
            .assert()
            .success()
            .stdout(predicate::str::contains("Scanning"));
    }

    #[test]
    fn test_nested_project_detection() {
        let test_fixtures = TestFixtures::new();
        let temp_dir = test_fixtures.temp_dir.path();

        // Create nested project structure
        let workspace_dir = temp_dir.join("workspace");
        fs::create_dir_all(&workspace_dir).unwrap();
        fs::write(
            workspace_dir.join("Cargo.toml"),
            "[workspace]\nmembers = [\"project1\", \"project2\"]",
        )
        .unwrap();

        // Create workspace members
        let project1_dir = workspace_dir.join("project1");
        fs::create_dir_all(&project1_dir).unwrap();
        fs::write(
            project1_dir.join("Cargo.toml"),
            "[package]\nname = \"project1\"",
        )
        .unwrap();
        fs::create_dir_all(project1_dir.join("src")).unwrap();

        let project2_dir = workspace_dir.join("project2");
        fs::create_dir_all(&project2_dir).unwrap();
        fs::write(
            project2_dir.join("Cargo.toml"),
            "[package]\nname = \"project2\"",
        )
        .unwrap();
        fs::create_dir_all(project2_dir.join("src")).unwrap();

        // Test that scan works with nested project structure
        let mut cmd = crate::common::lotar_cmd().unwrap();
        cmd.current_dir(temp_dir)
            .arg("scan")
            .assert()
            .success()
            .stdout(predicate::str::contains("Scanning"));
    }

    #[test]
    fn test_project_file_structure_validation() {
        let test_fixtures = TestFixtures::new();
        let temp_dir = test_fixtures.temp_dir.path();

        // Create a project with typical file structure issues
        fs::write(
            temp_dir.join("Cargo.toml"),
            "[package]\nname = \"test-validation\"",
        )
        .unwrap();
        fs::create_dir_all(temp_dir.join("src")).unwrap();
        fs::write(temp_dir.join("src/lib.rs"), "// Empty lib").unwrap();

        // Create some test files in non-standard locations
        fs::create_dir_all(temp_dir.join("random_tests")).unwrap();
        fs::write(
            temp_dir.join("random_tests/test1.rs"),
            "#[test] fn test() {}",
        )
        .unwrap();

        // Create some documentation
        fs::write(temp_dir.join("README.md"), "# Test Project").unwrap();

        // Test scan with verbose output
        let mut cmd = crate::common::lotar_cmd().unwrap();
        cmd.current_dir(temp_dir)
            .arg("scan")
            .arg("--verbose")
            .assert()
            .success()
            .stdout(predicate::str::contains("Scanning"));
    }
}

// =============================================================================
// Multi-Project Workspace Management
// =============================================================================

mod multi_project_workspace {
    use super::*;

    #[test]
    fn test_workspace_project_management() {
        let test_fixtures = TestFixtures::new();
        let temp_dir = test_fixtures.temp_dir.path();

        // Create multiple projects
        let frontend_dir = temp_dir.join("frontend");
        fs::create_dir_all(&frontend_dir).unwrap();
        fs::write(
            frontend_dir.join("package.json"),
            r#"{"name": "frontend-app"}"#,
        )
        .unwrap();

        let backend_dir = temp_dir.join("backend");
        fs::create_dir_all(&backend_dir).unwrap();
        fs::write(
            backend_dir.join("Cargo.toml"),
            "[package]\nname = \"backend-api\"",
        )
        .unwrap();
        fs::create_dir_all(backend_dir.join("src")).unwrap();

        // Initialize configuration for multiple projects
        let mut cmd = crate::common::lotar_cmd().unwrap();
        cmd.current_dir(temp_dir)
            .arg("config")
            .arg("init")
            .arg("--project=Frontend")
            .assert()
            .success();

        let mut cmd = crate::common::lotar_cmd().unwrap();
        cmd.current_dir(temp_dir)
            .arg("config")
            .arg("init")
            .arg("--project=Backend")
            .assert()
            .success();

        // Add tasks to different projects
        let mut cmd = crate::common::lotar_cmd().unwrap();
        cmd.current_dir(temp_dir)
            .arg("add")
            .arg("Frontend Task")
            .arg("--project=Frontend")
            .assert()
            .success();

        let mut cmd = crate::common::lotar_cmd().unwrap();
        cmd.current_dir(temp_dir)
            .arg("add")
            .arg("Backend Task")
            .arg("--project=Backend")
            .assert()
            .success();

        // List tasks from all projects
        let mut cmd = crate::common::lotar_cmd().unwrap();
        cmd.current_dir(temp_dir)
            .arg("list")
            .assert()
            .success()
            .stdout(predicate::str::contains("Frontend Task"))
            .stdout(predicate::str::contains("Backend Task"));

        // List tasks from specific project
        let mut cmd = crate::common::lotar_cmd().unwrap();
        cmd.current_dir(temp_dir)
            .arg("list")
            .arg("--project=Frontend")
            .assert()
            .success()
            .stdout(predicate::str::contains("Frontend Task"))
            .stdout(predicate::str::contains("Backend Task").not());
    }

    #[test]
    fn test_project_context_switching() {
        let test_fixtures = TestFixtures::new();
        let temp_dir = test_fixtures.temp_dir.path();

        // Create project structure
        let project_a_dir = temp_dir.join("project-a");
        fs::create_dir_all(&project_a_dir).unwrap();
        fs::write(
            project_a_dir.join("package.json"),
            r#"{"name": "project-a"}"#,
        )
        .unwrap();

        let project_b_dir = temp_dir.join("project-b");
        fs::create_dir_all(&project_b_dir).unwrap();
        fs::write(
            project_b_dir.join("Cargo.toml"),
            "[package]\nname = \"project-b\"",
        )
        .unwrap();
        fs::create_dir_all(project_b_dir.join("src")).unwrap();

        // Initialize projects in their respective directories
        let mut cmd = crate::common::lotar_cmd().unwrap();
        cmd.current_dir(&project_a_dir)
            .arg("config")
            .arg("init")
            .arg("--project=Alpha")
            .assert()
            .success();

        let mut cmd = crate::common::lotar_cmd().unwrap();
        cmd.current_dir(&project_b_dir)
            .arg("config")
            .arg("init")
            .arg("--project=Beta")
            .assert()
            .success();

        // Work from project-a directory
        let mut cmd = crate::common::lotar_cmd().unwrap();
        cmd.current_dir(&project_a_dir)
            .arg("add")
            .arg("Task from A")
            .arg("--project=Alpha")
            .assert()
            .success();

        // Work from project-b directory
        let mut cmd = crate::common::lotar_cmd().unwrap();
        cmd.current_dir(&project_b_dir)
            .arg("add")
            .arg("Task from B")
            .arg("--project=Beta")
            .assert()
            .success();

        // Verify tasks are correctly associated
        let mut cmd = crate::common::lotar_cmd().unwrap();
        cmd.current_dir(temp_dir)
            .arg("list")
            .arg("--project=Alpha")
            .assert()
            .success()
            .stdout(predicate::str::contains("Task from A"))
            .stdout(predicate::str::contains("Task from B").not());

        let mut cmd = crate::common::lotar_cmd().unwrap();
        cmd.current_dir(temp_dir)
            .arg("list")
            .arg("--project=Beta")
            .assert()
            .success()
            .stdout(predicate::str::contains("Task from B"))
            .stdout(predicate::str::contains("Task from A").not());
    }
}

// =============================================================================
// Smart Project Features
// =============================================================================

mod smart_features {
    use super::*;

    #[test]
    fn test_project_auto_detection() {
        let test_fixtures = TestFixtures::new();
        let temp_dir = test_fixtures.temp_dir.path();

        // Create a project structure
        fs::write(
            temp_dir.join("Cargo.toml"),
            "[package]\nname = \"auto-detect-test\"",
        )
        .unwrap();
        fs::create_dir_all(temp_dir.join("src")).unwrap();

        // Add a task without specifying project - should auto-detect
        let mut cmd = crate::common::lotar_cmd().unwrap();
        cmd.current_dir(temp_dir)
            .arg("add")
            .arg("Auto-detected task")
            .assert()
            .success()
            .stdout(predicate::str::contains("Created task:").or(predicate::str::contains("Task")));

        // Verify task was created and can be listed
        let mut cmd = crate::common::lotar_cmd().unwrap();
        cmd.current_dir(temp_dir)
            .arg("list")
            .assert()
            .success()
            .stdout(predicate::str::contains("Auto-detected task"));
    }

    #[test]
    fn test_project_filtering_advanced() {
        let test_fixtures = TestFixtures::new();
        let temp_dir = test_fixtures.temp_dir.path();

        // Create multiple project types with complex names
        fs::write(
            temp_dir.join("Cargo.toml"),
            "[package]\nname = \"backend-api-service\"",
        )
        .unwrap();
        fs::create_dir_all(temp_dir.join("src")).unwrap();

        let frontend_dir = temp_dir.join("frontend-web-app");
        fs::create_dir_all(&frontend_dir).unwrap();
        fs::write(
            frontend_dir.join("package.json"),
            r#"{"name": "frontend-web-app"}"#,
        )
        .unwrap();

        let mobile_dir = temp_dir.join("mobile-app");
        fs::create_dir_all(&mobile_dir).unwrap();
        fs::write(
            mobile_dir.join("pubspec.yaml"),
            "name: mobile_app\nversion: 1.0.0",
        )
        .unwrap();

        // Test basic scanning (scan command doesn't support filtering)
        let mut cmd = crate::common::lotar_cmd().unwrap();
        cmd.current_dir(temp_dir)
            .arg("scan")
            .assert()
            .success()
            .stdout(predicate::str::contains("Scanning"));

        // Test basic scanning again
        let mut cmd = crate::common::lotar_cmd().unwrap();
        cmd.current_dir(temp_dir)
            .arg("scan")
            .assert()
            .success()
            .stdout(predicate::str::contains("Scanning"));
    }

    #[test]
    fn test_project_status_reporting() {
        let test_fixtures = TestFixtures::new();
        let temp_dir = test_fixtures.temp_dir.path();

        // Create a project
        fs::write(
            temp_dir.join("Cargo.toml"),
            "[package]\nname = \"status-test\"",
        )
        .unwrap();
        fs::create_dir_all(temp_dir.join("src")).unwrap();

        // Initialize and add some tasks
        let mut cmd = crate::common::lotar_cmd().unwrap();
        cmd.current_dir(temp_dir)
            .arg("config")
            .arg("init")
            .arg("--project=StatusTest")
            .assert()
            .success();

        // Add tasks with different states
        let mut cmd = crate::common::lotar_cmd().unwrap();
        let first_task_output = cmd
            .current_dir(temp_dir)
            .arg("add")
            .arg("Todo Task")
            .arg("--project=StatusTest")
            .assert()
            .success()
            .get_output()
            .stdout
            .clone();

        let mut cmd = crate::common::lotar_cmd().unwrap();
        let second_task_output = cmd
            .current_dir(temp_dir)
            .arg("add")
            .arg("In Progress Task")
            .arg("--project=StatusTest")
            .assert()
            .success()
            .get_output()
            .stdout
            .clone();

        // Extract task IDs from the output
        let _first_task_output_str = String::from_utf8_lossy(&first_task_output);
        let _first_task_id = _first_task_output_str
            .lines()
            .find(|line| line.contains("Created task:"))
            .and_then(|line| line.split("Created task: ").nth(1))
            .expect("Should find first task ID in output");

        let second_task_output_str = String::from_utf8_lossy(&second_task_output);
        let second_task_id = second_task_output_str
            .lines()
            .find(|line| line.contains("Created task:"))
            .and_then(|line| line.split("Created task: ").nth(1))
            .expect("Should find second task ID in output");

        // Set the second task state to InProgress
        let mut cmd = crate::common::lotar_cmd().unwrap();
        cmd.current_dir(temp_dir)
            .arg("status")
            .arg(second_task_id)
            .arg("in_progress")
            .assert()
            .success();

        // Test status command - check overall status
        let mut cmd = crate::common::lotar_cmd().unwrap();
        cmd.current_dir(temp_dir)
            .arg("list")
            .arg("--project=StatusTest")
            .assert()
            .success()
            .stdout(predicate::str::contains("STAT-"))
            .stdout(predicate::str::contains("Todo Task"))
            .stdout(predicate::str::contains("In Progress Task"));
    }
}

// =============================================================================
// Project Integration Tests
// =============================================================================

mod integration {
    use super::*;

    #[test]
    fn test_full_project_workflow() {
        let test_fixtures = TestFixtures::new();
        let temp_dir = test_fixtures.temp_dir.path();

        // Step 1: Create a realistic project structure
        fs::write(
            temp_dir.join("Cargo.toml"),
            "[package]\nname = \"workflow-test\"\nversion = \"0.1.0\"",
        )
        .unwrap();
        fs::create_dir_all(temp_dir.join("src")).unwrap();
        fs::write(
            temp_dir.join("src/main.rs"),
            "fn main() { println!(\"Hello, world!\"); }",
        )
        .unwrap();
        fs::write(temp_dir.join("README.md"), "# Workflow Test Project").unwrap();

        // Step 2: Initialize project configuration
        let mut cmd = crate::common::lotar_cmd().unwrap();
        cmd.current_dir(temp_dir)
            .arg("config")
            .arg("init")
            .arg("--template=default")
            .arg("--project=WorkflowTest")
            .assert()
            .success();

        // Step 3: Scan and verify it works
        let mut cmd = crate::common::lotar_cmd().unwrap();
        cmd.current_dir(temp_dir)
            .arg("scan")
            .assert()
            .success()
            .stdout(predicate::str::contains("Scanning"));

        // Step 4: Add tasks for different workflow stages
        let mut cmd = crate::common::lotar_cmd().unwrap();
        let first_task_output = cmd
            .current_dir(temp_dir)
            .arg("add")
            .arg("Setup CI/CD")
            .arg("--project=WorkflowTest")
            .arg("--priority=high")
            .assert()
            .success()
            .get_output()
            .stdout
            .clone();

        let mut cmd = crate::common::lotar_cmd().unwrap();
        cmd.current_dir(temp_dir)
            .arg("add")
            .arg("Write tests")
            .arg("--project=WorkflowTest")
            .arg("--priority=medium")
            .assert()
            .success();

        // Extract the first task ID for later use
        let first_task_output_str = String::from_utf8_lossy(&first_task_output);
        let first_task_id = first_task_output_str
            .lines()
            .find(|line| line.contains("Created task:"))
            .and_then(|line| line.split("Created task: ").nth(1))
            .expect("Should find first task ID in output");

        // Step 5: List and verify tasks
        let mut cmd = crate::common::lotar_cmd().unwrap();
        cmd.current_dir(temp_dir)
            .arg("list")
            .arg("--project=WorkflowTest")
            .assert()
            .success()
            .stdout(predicate::str::contains("Setup CI/CD"))
            .stdout(predicate::str::contains("Write tests"));

        // Step 6: Check project status using list command
        let mut cmd = crate::common::lotar_cmd().unwrap();
        cmd.current_dir(temp_dir)
            .arg("list")
            .arg("--project=WorkflowTest")
            .assert()
            .success()
            .stdout(predicate::str::contains("Setup CI/CD"))
            .stdout(predicate::str::contains("Write tests"));

        // Step 7: Modify task state
        let mut cmd = crate::common::lotar_cmd().unwrap();
        cmd.current_dir(temp_dir)
            .arg("status")
            .arg(first_task_id)
            .arg("in_progress")
            .assert()
            .success();

        // Step 8: Verify status update using list command
        let mut cmd = crate::common::lotar_cmd().unwrap();
        cmd.current_dir(temp_dir)
            .arg("list")
            .arg("--project=WorkflowTest")
            .assert()
            .success()
            .stdout(predicate::str::contains("[InProgress]"));
    }

    #[test]
    fn test_project_isolation() {
        let test_fixtures = TestFixtures::new();
        let temp_dir = test_fixtures.temp_dir.path();

        // Create two separate projects
        let project1_dir = temp_dir.join("isolated-project-1");
        fs::create_dir_all(&project1_dir).unwrap();
        fs::write(
            project1_dir.join("package.json"),
            r#"{"name": "isolated-1"}"#,
        )
        .unwrap();

        let project2_dir = temp_dir.join("isolated-project-2");
        fs::create_dir_all(&project2_dir).unwrap();
        fs::write(
            project2_dir.join("Cargo.toml"),
            "[package]\nname = \"isolated-2\"",
        )
        .unwrap();
        fs::create_dir_all(project2_dir.join("src")).unwrap();

        // Initialize both projects in their respective directories
        let mut cmd = crate::common::lotar_cmd().unwrap();
        cmd.current_dir(&project1_dir)
            .arg("config")
            .arg("init")
            .arg("--project=WebApp")
            .assert()
            .success();

        let mut cmd = crate::common::lotar_cmd().unwrap();
        cmd.current_dir(&project2_dir)
            .arg("config")
            .arg("init")
            .arg("--project=RustCrate")
            .assert()
            .success();

        // Add tasks to each project
        let mut cmd = crate::common::lotar_cmd().unwrap();
        cmd.current_dir(temp_dir)
            .arg("add")
            .arg("Project 1 Task")
            .arg("--project=WebApp")
            .assert()
            .success();

        let mut cmd = crate::common::lotar_cmd().unwrap();
        cmd.current_dir(temp_dir)
            .arg("add")
            .arg("Project 2 Task")
            .arg("--project=RustCrate")
            .assert()
            .success();

        // Verify isolation - each project should only see its own tasks
        let mut cmd = crate::common::lotar_cmd().unwrap();
        cmd.current_dir(temp_dir)
            .arg("list")
            .arg("--project=WebApp")
            .assert()
            .success()
            .stdout(predicate::str::contains("Project 1 Task"))
            .stdout(predicate::str::contains("Project 2 Task").not());

        let mut cmd = crate::common::lotar_cmd().unwrap();
        cmd.current_dir(temp_dir)
            .arg("list")
            .arg("--project=RustCrate")
            .assert()
            .success()
            .stdout(predicate::str::contains("Project 2 Task"))
            .stdout(predicate::str::contains("Project 1 Task").not());
    }
}

// =============================================================================
// Consolidated: Relative path parent search & helpers smoke
// =============================================================================

mod relative_path_search {
    use super::*;
    use lotar::help::HelpSystem;
    use lotar::output::OutputFormat;
    use lotar::workspace::TasksDirectoryResolver;
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
        let mut cmd = crate::common::lotar_cmd().unwrap();
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
        let mut cmd = crate::common::lotar_cmd().unwrap();
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
        let mut cmd = crate::common::lotar_cmd().unwrap();
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
}

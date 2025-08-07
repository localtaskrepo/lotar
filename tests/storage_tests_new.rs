//! Storage system tests (New)
//!
//! This module consolidates all storage-related tests including:
//! - CRUD operations (Create, Read, Update, Delete)
//! - Multi-project storage
//! - Data validation and serialization
//! - File system operations

use assert_cmd::Command;
use predicates::prelude::*;

mod common;
use common::TestFixtures;

// =============================================================================
// CRUD Operations
// =============================================================================

mod crud_operations {
    use super::*;

    #[test]
    fn test_task_creation() {
        let test_fixtures = TestFixtures::new();
        let temp_dir = test_fixtures.temp_dir.path();

        // Initialize project
        let mut cmd = Command::cargo_bin("lotar").unwrap();
        cmd.current_dir(temp_dir)
            .arg("config")
            .arg("init")
            .arg("--project=NewCrudTest")
            .assert()
            .success();

        // Create a task using modern syntax
        let mut cmd = Command::cargo_bin("lotar").unwrap();
        cmd.current_dir(temp_dir)
            .arg("add")
            .arg("Test task creation new")
            .arg("--description=Testing CRUD operations new")
            .arg("--priority=high")
            .arg("--project=NewCrudTest")
            .assert()
            .success()
            .stdout(predicate::str::contains("Created task:"));

        // Verify task exists
        let mut cmd = Command::cargo_bin("lotar").unwrap();
        cmd.current_dir(temp_dir)
            .arg("list")
            .arg("--project=NewCrudTest")
            .assert()
            .success()
            .stdout(predicate::str::contains("Test task creation new"));
    }

    #[test]
    fn test_task_retrieval() {
        let test_fixtures = TestFixtures::new();
        let temp_dir = test_fixtures.temp_dir.path();

        // Initialize project and create task
        let mut cmd = Command::cargo_bin("lotar").unwrap();
        cmd.current_dir(temp_dir)
            .arg("config")
            .arg("init")
            .arg("--project=NewRetrievalTest")
            .assert()
            .success();

        let mut cmd = Command::cargo_bin("lotar").unwrap();
        cmd.current_dir(temp_dir)
            .arg("add")
            .arg("Retrievable task new")
            .arg("--description=Task for retrieval testing new")
            .arg("--project=NewRetrievalTest")
            .assert()
            .success();

        // Retrieve task details using list command
        let mut cmd = Command::cargo_bin("lotar").unwrap();
        cmd.current_dir(temp_dir)
            .arg("list")
            .arg("--project=NewRetrievalTest")
            .assert()
            .success()
            .stdout(predicate::str::contains("Retrievable task new"))
            .stdout(predicate::str::contains("Task for retrieval testing new"));
    }

    #[test]
    fn test_task_modification() {
        let test_fixtures = TestFixtures::new();
        let temp_dir = test_fixtures.temp_dir.path();

        // Initialize project
        let mut cmd = Command::cargo_bin("lotar").unwrap();
        cmd.current_dir(temp_dir)
            .arg("config")
            .arg("init")
            .arg("--project=NewModifyTest")
            .assert()
            .success();

        // Create a task
        let mut cmd = Command::cargo_bin("lotar").unwrap();
        cmd.current_dir(temp_dir)
            .arg("add")
            .arg("Original task new")
            .arg("--description=Initial task new")
            .arg("--priority=medium")
            .arg("--project=NewModifyTest")
            .assert()
            .success();

        // Update task status (task ID should be NEWM-1 based on project prefix)
        let mut cmd = Command::cargo_bin("lotar").unwrap();
        cmd.current_dir(temp_dir)
            .arg("status")
            .arg("NEWM-1")
            .arg("in_progress")
            .arg("--project=NewModifyTest")
            .assert()
            .success();

        // Verify update
        let mut cmd = Command::cargo_bin("lotar").unwrap();
        cmd.current_dir(temp_dir)
            .arg("list")
            .arg("--project=NewModifyTest")
            .assert()
            .success()
            .stdout(predicate::str::contains("IN_PROGRESS"));
    }

    #[test]
    fn test_task_deletion() {
        let test_fixtures = TestFixtures::new();
        let temp_dir = test_fixtures.temp_dir.path();

        // Initialize project and create task
        let mut cmd = Command::cargo_bin("lotar").unwrap();
        cmd.current_dir(temp_dir)
            .arg("config")
            .arg("init")
            .arg("--project=NewDeleteTest")
            .assert()
            .success();

        let mut cmd = Command::cargo_bin("lotar").unwrap();
        cmd.current_dir(temp_dir)
            .arg("add")
            .arg("Task to mark complete")
            .arg("--project=NewDeleteTest")
            .assert()
            .success();

        // Mark task as completed (instead of deletion, since delete command doesn't exist)
        let mut cmd = Command::cargo_bin("lotar").unwrap();
        cmd.current_dir(temp_dir)
            .arg("status")
            .arg("NEWD-1")
            .arg("done")
            .arg("--project=NewDeleteTest")
            .assert()
            .success();

        // Verify task is marked as complete
        let mut cmd = Command::cargo_bin("lotar").unwrap();
        cmd.current_dir(temp_dir)
            .arg("list")
            .arg("--project=NewDeleteTest")
            .assert()
            .success()
            .stdout(predicate::str::contains("DONE"));
    }
}

// =============================================================================
// Multi-Project Storage
// =============================================================================

mod multi_project {
    use super::*;

    #[test]
    fn test_project_isolation() {
        let test_fixtures = TestFixtures::new();
        let temp_dir = test_fixtures.temp_dir.path();

        // Initialize two projects with distinct names to avoid conflicts
        let mut cmd = Command::cargo_bin("lotar").unwrap();
        cmd.current_dir(temp_dir)
            .arg("config")
            .arg("init")
            .arg("--project=Echo")
            .assert()
            .success();

        let mut cmd = Command::cargo_bin("lotar").unwrap();
        cmd.current_dir(temp_dir)
            .arg("config")
            .arg("init")
            .arg("--project=Foxtrot")
            .assert()
            .success();

        // Add tasks to each project
        let mut cmd = Command::cargo_bin("lotar").unwrap();
        cmd.current_dir(temp_dir)
            .arg("add")
            .arg("Task in Echo")
            .arg("--project=Echo")
            .assert()
            .success();

        let mut cmd = Command::cargo_bin("lotar").unwrap();
        cmd.current_dir(temp_dir)
            .arg("add")
            .arg("Task in Foxtrot")
            .arg("--project=Foxtrot")
            .assert()
            .success();

        // Verify project isolation
        let mut cmd = Command::cargo_bin("lotar").unwrap();
        cmd.current_dir(temp_dir)
            .arg("list")
            .arg("--project=Echo")
            .assert()
            .success()
            .stdout(predicate::str::contains("Task in Echo"))
            .stdout(predicate::str::contains("Task in Foxtrot").not());

        let mut cmd = Command::cargo_bin("lotar").unwrap();
        cmd.current_dir(temp_dir)
            .arg("list")
            .arg("--project=Foxtrot")
            .assert()
            .success()
            .stdout(predicate::str::contains("Task in Foxtrot"))
            .stdout(predicate::str::contains("Task in Echo").not());
    }

    #[test]
    fn test_cross_project_operations() {
        let test_fixtures = TestFixtures::new();
        let temp_dir = test_fixtures.temp_dir.path();

        // Initialize projects with completely distinct names
        let mut cmd = Command::cargo_bin("lotar").unwrap();
        cmd.current_dir(temp_dir)
            .arg("config")
            .arg("init")
            .arg("--project=Hotel")
            .assert()
            .success();

        let mut cmd = Command::cargo_bin("lotar").unwrap();
        cmd.current_dir(temp_dir)
            .arg("config")
            .arg("init")
            .arg("--project=India")
            .assert()
            .success();

        // Add tasks to both projects
        for i in 1..=3 {
            let mut cmd = Command::cargo_bin("lotar").unwrap();
            cmd.current_dir(temp_dir)
                .arg("add")
                .arg(format!("Hotel Task {i}"))
                .arg("--project=Hotel")
                .assert()
                .success();

            let mut cmd = Command::cargo_bin("lotar").unwrap();
            cmd.current_dir(temp_dir)
                .arg("add")
                .arg(format!("India Task {i}"))
                .arg("--project=India")
                .assert()
                .success();
        }

        // List all tasks across projects
        let mut cmd = Command::cargo_bin("lotar").unwrap();
        cmd.current_dir(temp_dir)
            .arg("list")
            .assert()
            .success()
            .stdout(predicate::str::contains("Hotel Task"))
            .stdout(predicate::str::contains("India Task"));

        // Verify individual project listings
        let mut cmd = Command::cargo_bin("lotar").unwrap();
        cmd.current_dir(temp_dir)
            .arg("list")
            .arg("--project=Hotel")
            .assert()
            .success()
            .stdout(predicate::str::contains("Hotel Task 1"))
            .stdout(predicate::str::contains("Hotel Task 2"))
            .stdout(predicate::str::contains("Hotel Task 3"));

        let mut cmd = Command::cargo_bin("lotar").unwrap();
        cmd.current_dir(temp_dir)
            .arg("list")
            .arg("--project=India")
            .assert()
            .success()
            .stdout(predicate::str::contains("India Task 1"))
            .stdout(predicate::str::contains("India Task 2"))
            .stdout(predicate::str::contains("India Task 3"));
    }
}

// =============================================================================
// Data Validation and Serialization
// =============================================================================

mod validation_and_serialization {
    use super::*;

    #[test]
    fn test_task_data_validation() {
        let test_fixtures = TestFixtures::new();
        let temp_dir = test_fixtures.temp_dir.path();

        // Initialize project
        let mut cmd = Command::cargo_bin("lotar").unwrap();
        cmd.current_dir(temp_dir)
            .arg("config")
            .arg("init")
            .arg("--project=NewValidationTest")
            .assert()
            .success();

        // Test valid task creation
        let mut cmd = Command::cargo_bin("lotar").unwrap();
        cmd.current_dir(temp_dir)
            .arg("add")
            .arg("Valid task new")
            .arg("--description=This is a valid task description new")
            .arg("--priority=medium")
            .arg("--project=NewValidationTest")
            .assert()
            .success();

        // Test task creation with special characters
        let mut cmd = Command::cargo_bin("lotar").unwrap();
        cmd.current_dir(temp_dir)
            .arg("add")
            .arg("Task with special chars new: @#$%^&*()")
            .arg("--description=Description with unicode new: 🚀 🎯 ✅")
            .arg("--project=NewValidationTest")
            .assert()
            .success();

        // Verify both tasks exist and display correctly
        let mut cmd = Command::cargo_bin("lotar").unwrap();
        cmd.current_dir(temp_dir)
            .arg("list")
            .arg("--project=NewValidationTest")
            .assert()
            .success()
            .stdout(predicate::str::contains("Valid task new"))
            .stdout(predicate::str::contains("special chars new"));
    }

    #[test]
    fn test_task_serialization_formats() {
        let test_fixtures = TestFixtures::new();
        let temp_dir = test_fixtures.temp_dir.path();

        // Initialize project
        let mut cmd = Command::cargo_bin("lotar").unwrap();
        cmd.current_dir(temp_dir)
            .arg("config")
            .arg("init")
            .arg("--project=NewSerializationTest")
            .assert()
            .success();

        // Create task with complex data
        let mut cmd = Command::cargo_bin("lotar").unwrap();
        cmd.current_dir(temp_dir)
            .arg("add")
            .arg("Complex serialization test new")
            .arg("--description=Multi-line description new\nwith newlines and\nspecial formatting")
            .arg("--priority=high")
            .arg("--project=NewSerializationTest")
            .assert()
            .success();

        // Verify task is properly serialized and can be read back
        let mut cmd = Command::cargo_bin("lotar").unwrap();
        cmd.current_dir(temp_dir)
            .arg("list")
            .arg("--project=NewSerializationTest")
            .assert()
            .success()
            .stdout(predicate::str::contains("Complex serialization test new"))
            .stdout(predicate::str::contains("Multi-line description new"));
    }
}

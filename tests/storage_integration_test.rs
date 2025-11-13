//! Storage system tests
//!
//! This module consolidates all storage-related tests including:
//! - CRUD operations (Create, Read, Update, Delete)
//! - Multi-project storage
//! - Data validation and serialization
//! - File system operations

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
        let mut cmd = crate::common::lotar_cmd().unwrap();
        cmd.current_dir(temp_dir)
            .arg("config")
            .arg("init")
            .arg("--project=CrudTest")
            .assert()
            .success();

        // Create a task using modern syntax
        let mut cmd = crate::common::lotar_cmd().unwrap();
        cmd.current_dir(temp_dir)
            .arg("add")
            .arg("Test task creation")
            .arg("--description=Testing CRUD operations")
            .arg("--priority=high")
            .arg("--project=CrudTest")
            .assert()
            .success()
            .stdout(predicate::str::contains("Created task:"));

        // Verify task exists
        let mut cmd = crate::common::lotar_cmd().unwrap();
        cmd.current_dir(temp_dir)
            .arg("list")
            .arg("--project=CrudTest")
            .assert()
            .success()
            .stdout(predicate::str::contains("Test task creation"));
    }

    #[test]
    fn test_task_retrieval() {
        let test_fixtures = TestFixtures::new();
        let temp_dir = test_fixtures.temp_dir.path();

        // Initialize project and create task
        let mut cmd = crate::common::lotar_cmd().unwrap();
        cmd.current_dir(temp_dir)
            .arg("config")
            .arg("init")
            .arg("--project=RetrievalTest")
            .assert()
            .success();

        let mut cmd = crate::common::lotar_cmd().unwrap();
        cmd.current_dir(temp_dir)
            .arg("add")
            .arg("Retrievable task")
            .arg("--description=Task for retrieval testing")
            .arg("--project=RetrievalTest")
            .assert()
            .success();

        // Retrieve task details using list command
        let mut cmd = crate::common::lotar_cmd().unwrap();
        cmd.current_dir(temp_dir)
            .arg("list")
            .arg("--project=RetrievalTest")
            .assert()
            .success()
            .stdout(predicate::str::contains("Retrievable task"))
            .stdout(predicate::str::contains("Task for retrieval testing"));
    }

    #[test]
    fn test_task_update() {
        let test_fixtures = TestFixtures::new();
        let temp_dir = test_fixtures.temp_dir.path();

        // Initialize project
        let mut cmd = crate::common::lotar_cmd().unwrap();
        cmd.current_dir(temp_dir)
            .arg("config")
            .arg("init")
            .arg("--project=UpdateTest")
            .assert()
            .success();

        // Create a task
        let mut cmd = crate::common::lotar_cmd().unwrap();
        cmd.current_dir(temp_dir)
            .arg("add")
            .arg("Original task")
            .arg("--description=Initial task")
            .arg("--priority=medium")
            .arg("--project=UpdateTest")
            .assert()
            .success();

        // Update task status (task ID should be UPDA-1 based on project prefix)
        let mut cmd = crate::common::lotar_cmd().unwrap();
        cmd.current_dir(temp_dir)
            .arg("status")
            .arg("UPDA-1")
            .arg("in_progress")
            .arg("--project=UpdateTest")
            .assert()
            .success();

        // Verify update
        let mut cmd = crate::common::lotar_cmd().unwrap();
        cmd.current_dir(temp_dir)
            .arg("list")
            .arg("--project=UpdateTest")
            .assert()
            .success()
            .stdout(predicate::str::contains("[InProgress]"));
    }

    #[test]
    fn test_task_completion() {
        let test_fixtures = TestFixtures::new();
        let temp_dir = test_fixtures.temp_dir.path();

        // Initialize project and create task
        let mut cmd = crate::common::lotar_cmd().unwrap();
        cmd.current_dir(temp_dir)
            .arg("config")
            .arg("init")
            .arg("--project=CompleteTest")
            .assert()
            .success();

        let mut cmd = crate::common::lotar_cmd().unwrap();
        cmd.current_dir(temp_dir)
            .arg("add")
            .arg("Task to complete")
            .arg("--project=CompleteTest")
            .assert()
            .success();

        // Mark task as completed (task ID should be COMP-1 based on project prefix)
        let mut cmd = crate::common::lotar_cmd().unwrap();
        cmd.current_dir(temp_dir)
            .arg("status")
            .arg("COMP-1")
            .arg("done")
            .arg("--project=CompleteTest")
            .assert()
            .success();

        // Verify task is marked as complete
        let mut cmd = crate::common::lotar_cmd().unwrap();
        cmd.current_dir(temp_dir)
            .arg("list")
            .arg("--project=CompleteTest")
            .assert()
            .success()
            .stdout(predicate::str::contains("[Done]"));
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
        let mut cmd = crate::common::lotar_cmd().unwrap();
        cmd.current_dir(temp_dir)
            .arg("config")
            .arg("init")
            .arg("--project=Alpha")
            .assert()
            .success();

        let mut cmd = crate::common::lotar_cmd().unwrap();
        cmd.current_dir(temp_dir)
            .arg("config")
            .arg("init")
            .arg("--project=Beta")
            .assert()
            .success();

        // Add tasks to each project
        let mut cmd = crate::common::lotar_cmd().unwrap();
        cmd.current_dir(temp_dir)
            .arg("add")
            .arg("Task in Alpha")
            .arg("--project=Alpha")
            .assert()
            .success();

        let mut cmd = crate::common::lotar_cmd().unwrap();
        cmd.current_dir(temp_dir)
            .arg("add")
            .arg("Task in Beta")
            .arg("--project=Beta")
            .assert()
            .success();

        // Verify project isolation
        let mut cmd = crate::common::lotar_cmd().unwrap();
        cmd.current_dir(temp_dir)
            .arg("list")
            .arg("--project=Alpha")
            .assert()
            .success()
            .stdout(predicate::str::contains("Task in Alpha"))
            .stdout(predicate::str::contains("Task in Beta").not());

        let mut cmd = crate::common::lotar_cmd().unwrap();
        cmd.current_dir(temp_dir)
            .arg("list")
            .arg("--project=Beta")
            .assert()
            .success()
            .stdout(predicate::str::contains("Task in Beta"))
            .stdout(predicate::str::contains("Task in Alpha").not());
    }

    #[test]
    fn test_cross_project_operations() {
        let test_fixtures = TestFixtures::new();
        let temp_dir = test_fixtures.temp_dir.path();

        // Initialize projects with completely distinct names
        let mut cmd = crate::common::lotar_cmd().unwrap();
        cmd.current_dir(temp_dir)
            .arg("config")
            .arg("init")
            .arg("--project=Gamma")
            .assert()
            .success();

        let mut cmd = crate::common::lotar_cmd().unwrap();
        cmd.current_dir(temp_dir)
            .arg("config")
            .arg("init")
            .arg("--project=Delta")
            .assert()
            .success();

        // Add tasks to both projects
        for i in 1..=3 {
            let mut cmd = crate::common::lotar_cmd().unwrap();
            cmd.current_dir(temp_dir)
                .arg("add")
                .arg(format!("Gamma Task {i}"))
                .arg("--project=Gamma")
                .assert()
                .success();

            let mut cmd = crate::common::lotar_cmd().unwrap();
            cmd.current_dir(temp_dir)
                .arg("add")
                .arg(format!("Delta Task {i}"))
                .arg("--project=Delta")
                .assert()
                .success();
        }

        // List all tasks across projects
        let mut cmd = crate::common::lotar_cmd().unwrap();
        cmd.current_dir(temp_dir)
            .arg("list")
            .assert()
            .success()
            .stdout(predicate::str::contains("Gamma Task"))
            .stdout(predicate::str::contains("Delta Task"));

        // Verify individual project listings
        let mut cmd = crate::common::lotar_cmd().unwrap();
        cmd.current_dir(temp_dir)
            .arg("list")
            .arg("--project=Gamma")
            .assert()
            .success()
            .stdout(predicate::str::contains("Gamma Task 1"))
            .stdout(predicate::str::contains("Gamma Task 2"))
            .stdout(predicate::str::contains("Gamma Task 3"));

        let mut cmd = crate::common::lotar_cmd().unwrap();
        cmd.current_dir(temp_dir)
            .arg("list")
            .arg("--project=Delta")
            .assert()
            .success()
            .stdout(predicate::str::contains("Delta Task 1"))
            .stdout(predicate::str::contains("Delta Task 2"))
            .stdout(predicate::str::contains("Delta Task 3"));
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
        let mut cmd = crate::common::lotar_cmd().unwrap();
        cmd.current_dir(temp_dir)
            .arg("config")
            .arg("init")
            .arg("--project=ValidationTest")
            .assert()
            .success();

        // Test valid task creation
        let mut cmd = crate::common::lotar_cmd().unwrap();
        cmd.current_dir(temp_dir)
            .arg("add")
            .arg("Valid task")
            .arg("--description=This is a valid task description")
            .arg("--priority=medium")
            .arg("--project=ValidationTest")
            .assert()
            .success();

        // Test task creation with special characters
        let mut cmd = crate::common::lotar_cmd().unwrap();
        cmd.current_dir(temp_dir)
            .arg("add")
            .arg("Task with special chars: @#$%^&*()")
            .arg("--description=Description with unicode: 🚀 🎯 ✅")
            .arg("--project=ValidationTest")
            .assert()
            .success();

        // Verify both tasks exist and display correctly
        let mut cmd = crate::common::lotar_cmd().unwrap();
        cmd.current_dir(temp_dir)
            .arg("list")
            .arg("--project=ValidationTest")
            .assert()
            .success()
            .stdout(predicate::str::contains("Valid task"))
            .stdout(predicate::str::contains("special chars"));
    }

    #[test]
    fn test_task_serialization_formats() {
        let test_fixtures = TestFixtures::new();
        let temp_dir = test_fixtures.temp_dir.path();

        // Initialize project
        let mut cmd = crate::common::lotar_cmd().unwrap();
        cmd.current_dir(temp_dir)
            .arg("config")
            .arg("init")
            .arg("--project=SerializationTest")
            .assert()
            .success();

        // Create task with complex data
        let mut cmd = crate::common::lotar_cmd().unwrap();
        cmd.current_dir(temp_dir)
            .arg("add")
            .arg("Complex serialization test")
            .arg("--description=Multi-line description\nwith newlines and\nspecial formatting")
            .arg("--priority=high")
            .arg("--project=SerializationTest")
            .assert()
            .success();

        // Verify task is properly serialized and can be read back
        let mut cmd = crate::common::lotar_cmd().unwrap();
        cmd.current_dir(temp_dir)
            .arg("list")
            .arg("--project=SerializationTest")
            .assert()
            .success()
            .stdout(predicate::str::contains("Complex serialization test"))
            .stdout(predicate::str::contains("Multi-line description"));
    }
}

//! Performance and experimental tests
//!
//! This module consolidates all performance-related tests including:
//! - CLI performance benchmarks
//! - Large dataset handling
//! - Memory usage optimization
//! - Experimental features testing

use predicates::prelude::*;
use std::fs;
use std::time::Instant;

mod common;
use common::TestFixtures;

// =============================================================================
// CLI Performance Tests
// =============================================================================

mod cli_performance {
    use super::*;

    #[test]
    fn test_basic_command_performance() {
        let test_fixtures = TestFixtures::new();
        let temp_dir = test_fixtures.temp_dir.path();

        // Measure help command performance
        let start = Instant::now();
        let mut cmd = crate::common::lotar_cmd().unwrap();
        cmd.current_dir(temp_dir).arg("help").assert().success();
        let help_duration = start.elapsed();

        // Help should be very fast (< 1 second)
        assert!(
            help_duration.as_secs() < 1,
            "Help command took too long: {help_duration:?}"
        );

        // Measure config show performance (read-only operation)
        let start = Instant::now();
        let mut cmd = crate::common::lotar_cmd().unwrap();
        cmd.current_dir(temp_dir)
            .arg("config")
            .arg("show")
            .assert()
            .success();
        let config_duration = start.elapsed();

        // Config show should be fast (< 2 seconds)
        assert!(
            config_duration.as_secs() < 2,
            "Config show took too long: {config_duration:?}"
        );
    }

    #[test]
    fn test_task_operations_performance() {
        let test_fixtures = TestFixtures::new();
        let temp_dir = test_fixtures.temp_dir.path();

        // Initialize project first
        let mut cmd = crate::common::lotar_cmd().unwrap();
        cmd.current_dir(temp_dir)
            .arg("config")
            .arg("init")
            .arg("--project=PerfTest")
            .assert()
            .success();

        // Measure task creation performance
        let start = Instant::now();
        for i in 0..10 {
            let mut cmd = crate::common::lotar_cmd().unwrap();
            cmd.current_dir(temp_dir)
                .arg("task")
                .arg("add")
                .arg(format!("Performance test task {i}"))
                .arg("--project=PerfTest")
                .assert()
                .success();
        }
        let creation_duration = start.elapsed();

        // 10 task creations should complete reasonably quickly (< 10 seconds)
        assert!(
            creation_duration.as_secs() < 10,
            "Task creation took too long: {creation_duration:?}"
        );

        // Measure list performance
        let start = Instant::now();
        let mut cmd = crate::common::lotar_cmd().unwrap();
        cmd.current_dir(temp_dir)
            .arg("list")
            .arg("--project=PerfTest")
            .assert()
            .success()
            .stdout(predicate::str::contains("Performance test task"));
        let list_duration = start.elapsed();

        // List should be fast even with multiple tasks (< 2 seconds)
        assert!(
            list_duration.as_secs() < 2,
            "List command took too long: {list_duration:?}"
        );
    }

    #[test]
    fn test_scan_performance_medium_project() {
        let test_fixtures = TestFixtures::new();
        let temp_dir = test_fixtures.temp_dir.path();

        // Create a medium-sized project structure
        fs::write(
            temp_dir.join("Cargo.toml"),
            "[package]\nname = \"perf-test\"",
        )
        .unwrap();
        fs::create_dir_all(temp_dir.join("src")).unwrap();

        // Create multiple source files
        for i in 0..25 {
            let file_path = temp_dir.join("src").join(format!("module_{i}.rs"));
            fs::write(
                &file_path,
                format!(
                    "// Module {i}\npub fn function_{i}() {{\n    println!(\"Function {i}\");\n}}"
                ),
            )
            .unwrap();
        }

        // Create tests directory with test files
        fs::create_dir_all(temp_dir.join("tests")).unwrap();
        for i in 0..10 {
            let test_path = temp_dir.join("tests").join(format!("test_{i}.rs"));
            fs::write(
                &test_path,
                format!("#[test]\nfn test_{i}() {{\n    assert_eq!(2 + 2, 4);\n}}"),
            )
            .unwrap();
        }

        // Create documentation files
        fs::create_dir_all(temp_dir.join("docs")).unwrap();
        for i in 0..5 {
            let doc_path = temp_dir.join("docs").join(format!("doc_{i}.md"));
            fs::write(
                &doc_path,
                format!("# Documentation {i}\n\nThis is documentation file {i}."),
            )
            .unwrap();
        }

        // Measure scan performance
        let start = Instant::now();
        let mut cmd = crate::common::lotar_cmd().unwrap();
        cmd.current_dir(temp_dir)
            .arg("scan")
            .assert()
            .success()
            .stdout(predicate::str::contains("Scanning"));
        let scan_duration = start.elapsed();

        // Scan should complete reasonably quickly even with many files (< 5 seconds)
        assert!(
            scan_duration.as_secs() < 5,
            "Scan took too long: {scan_duration:?}"
        );
    }
}

// =============================================================================
// Large Dataset Handling
// =============================================================================

mod large_datasets {
    use super::*;

    #[test]
    fn test_many_small_tasks_performance() {
        let test_fixtures = TestFixtures::new();
        let temp_dir = test_fixtures.temp_dir.path();

        // Initialize project
        let mut cmd = crate::common::lotar_cmd().unwrap();
        cmd.current_dir(temp_dir)
            .arg("config")
            .arg("init")
            .arg("--project=LargeDataset")
            .assert()
            .success();

        // Create many small tasks
        let task_count = 50;
        let start = Instant::now();

        for i in 0..task_count {
            let mut cmd = crate::common::lotar_cmd().unwrap();
            cmd.current_dir(temp_dir)
                .arg("task")
                .arg("add")
                .arg(format!("Task {i}"))
                .arg("--description")
                .arg(format!("Description for task number {i}"))
                .arg("--project=LargeDataset")
                .assert()
                .success();
        }

        let creation_duration = start.elapsed();
        println!("Created {task_count} tasks in {creation_duration:?}");

        // Measure list performance with many tasks
        let start = Instant::now();
        let mut cmd = crate::common::lotar_cmd().unwrap();
        cmd.current_dir(temp_dir)
            .arg("list")
            .arg("--project=LargeDataset")
            .assert()
            .success();
        let list_duration = start.elapsed();

        println!("Listed {task_count} tasks in {list_duration:?}");

        // Performance should be reasonable (< 20 seconds for creation, < 3 seconds for listing)
        assert!(
            creation_duration.as_secs() < 20,
            "Task creation took too long: {creation_duration:?}"
        );
        assert!(
            list_duration.as_secs() < 3,
            "Task listing took too long: {list_duration:?}"
        );
    }

    #[test]
    fn test_complex_task_data_handling() {
        let test_fixtures = TestFixtures::new();
        let temp_dir = test_fixtures.temp_dir.path();

        // Initialize project
        let mut cmd = crate::common::lotar_cmd().unwrap();
        cmd.current_dir(temp_dir)
            .arg("config")
            .arg("init")
            .arg("--project=ComplexData")
            .assert()
            .success();

        // Create tasks with complex descriptions and metadata
        let start = Instant::now();

        for i in 0..20 {
            let long_description = format!(
                "This is a very detailed description for task {i}. \
                It contains multiple sentences and explains the task in great detail. \
                The description includes technical requirements, acceptance criteria, \
                and various implementation notes that span multiple lines. \
                Task {i} requires careful consideration of performance implications."
            );

            let mut cmd = crate::common::lotar_cmd().unwrap();
            cmd.current_dir(temp_dir)
                .arg("task")
                .arg("add")
                .arg(format!(
                    "Complex Task {i} with Long Title That Describes Everything"
                ))
                .arg("--description")
                .arg(&long_description)
                .arg("--priority=high")
                .arg("--project=ComplexData")
                .assert()
                .success();
        }

        let creation_duration = start.elapsed();

        // Measure search performance with complex data
        let start = Instant::now();
        let mut cmd = crate::common::lotar_cmd().unwrap();
        cmd.current_dir(temp_dir)
            .arg("list")
            .arg("--project=ComplexData")
            .assert()
            .success();
        let search_duration = start.elapsed();

        println!("Complex task creation took: {creation_duration:?}");
        println!("Search took: {search_duration:?}");

        // Should handle complex data reasonably well
        assert!(
            creation_duration.as_secs() < 15,
            "Complex task creation took too long: {creation_duration:?}"
        );
        assert!(
            search_duration.as_secs() < 3,
            "Search took too long: {search_duration:?}"
        );
    }

    #[test]
    fn test_large_file_scanning_performance() {
        let test_fixtures = TestFixtures::new();
        let temp_dir = test_fixtures.temp_dir.path();

        // Create a large file (simulating a substantial codebase)
        let large_file_content = (0..1000)
            .map(|i| format!("// Line {i} of large file\nfn function_{i}() {{\n    println!(\"Function {i}\");\n}}\n"))
            .collect::<Vec<_>>()
            .join("\n");

        fs::write(temp_dir.join("large_file.rs"), &large_file_content).unwrap();

        // Create many smaller files
        fs::create_dir_all(temp_dir.join("src")).unwrap();
        for i in 0..100 {
            let content = format!("// Small file {i}\npub const VALUE_{i}: i32 = {i};");
            fs::write(temp_dir.join("src").join(format!("small_{i}.rs")), content).unwrap();
        }

        // Measure scanning performance
        let start = Instant::now();
        let mut cmd = crate::common::lotar_cmd().unwrap();
        cmd.current_dir(temp_dir)
            .arg("scan")
            .assert()
            .success()
            .stdout(predicate::str::contains("Scanning"));
        let scan_duration = start.elapsed();

        println!("Scanned large dataset in: {scan_duration:?}");

        // Should handle large files and many small files (< 8 seconds)
        assert!(
            scan_duration.as_secs() < 8,
            "Large file scanning took too long: {scan_duration:?}"
        );
    }
}

// =============================================================================
// Memory Usage and Optimization
// =============================================================================

mod memory_optimization {
    use super::*;

    #[test]
    fn test_memory_efficient_operations() {
        let test_fixtures = TestFixtures::new();
        let temp_dir = test_fixtures.temp_dir.path();

        // Create a project with substantial data
        let mut cmd = crate::common::lotar_cmd().unwrap();
        cmd.current_dir(temp_dir)
            .arg("config")
            .arg("init")
            .arg("--project=MemoryTest")
            .assert()
            .success();

        // Create tasks in batches to test memory handling
        for batch in 0..5 {
            for i in 0..10 {
                let task_id = batch * 10 + i;
                let mut cmd = crate::common::lotar_cmd().unwrap();
                cmd.current_dir(temp_dir)
                    .arg("task")
                    .arg("add")
                    .arg(format!("Memory test task {task_id}"))
                    .arg("--project=MemoryTest")
                    .assert()
                    .success();
            }

            // Periodically list tasks to ensure memory usage remains reasonable
            let mut cmd = crate::common::lotar_cmd().unwrap();
            cmd.current_dir(temp_dir)
                .arg("list")
                .arg("--project=MemoryTest")
                .assert()
                .success();
        }

        // Final verification - all operations should complete successfully
        let mut cmd = crate::common::lotar_cmd().unwrap();
        cmd.current_dir(temp_dir)
            .arg("list")
            .arg("--project=MemoryTest")
            .assert()
            .success()
            .stdout(predicate::str::contains("Found"));
    }

    #[test]
    fn test_concurrent_operation_simulation() {
        let test_fixtures = TestFixtures::new();
        let temp_dir = test_fixtures.temp_dir.path();

        // Initialize project
        let mut cmd = crate::common::lotar_cmd().unwrap();
        cmd.current_dir(temp_dir)
            .arg("config")
            .arg("init")
            .arg("--project=ConcurrentTest")
            .assert()
            .success();

        // Simulate rapid successive operations (like a user working quickly)
        let start = Instant::now();

        // Add task
        let mut cmd = crate::common::lotar_cmd().unwrap();
        cmd.current_dir(temp_dir)
            .arg("task")
            .arg("add")
            .arg("Quick task 1")
            .arg("--project=ConcurrentTest")
            .assert()
            .success();

        // Immediately list
        let mut cmd = crate::common::lotar_cmd().unwrap();
        cmd.current_dir(temp_dir)
            .arg("list")
            .arg("--project=ConcurrentTest")
            .assert()
            .success();

        // Add another task
        let mut cmd = crate::common::lotar_cmd().unwrap();
        cmd.current_dir(temp_dir)
            .arg("task")
            .arg("add")
            .arg("Quick task 2")
            .arg("--project=ConcurrentTest")
            .assert()
            .success();

        // Check list again
        let mut cmd = crate::common::lotar_cmd().unwrap();
        cmd.current_dir(temp_dir)
            .arg("list")
            .arg("--project=ConcurrentTest")
            .assert()
            .success();

        // Modify task status
        let mut cmd = crate::common::lotar_cmd().unwrap();
        cmd.current_dir(temp_dir)
            .arg("status")
            .arg("CONC-1")
            .arg("in_progress")
            .assert()
            .success();

        // Final list
        let mut cmd = crate::common::lotar_cmd().unwrap();
        cmd.current_dir(temp_dir)
            .arg("list")
            .arg("--project=ConcurrentTest")
            .assert()
            .success();

        let total_duration = start.elapsed();
        println!("Rapid operations completed in: {total_duration:?}");

        // All operations should complete quickly (< 10 seconds)
        assert!(
            total_duration.as_secs() < 10,
            "Rapid operations took too long: {total_duration:?}"
        );
    }
}

// =============================================================================
// Experimental Features
// =============================================================================

mod experimental_features {
    use super::*;

    #[test]
    fn test_experimental_cli_commands() {
        let test_fixtures = TestFixtures::new();
        let temp_dir = test_fixtures.temp_dir.path();

        // Test experimental flags and options that might be added
        let mut cmd = crate::common::lotar_cmd().unwrap();
        cmd.current_dir(temp_dir)
            .arg("help")
            .assert()
            .success()
            .stdout(predicate::str::contains("LoTaR Overview"));

        // Test edge cases in command parsing
        let mut cmd = crate::common::lotar_cmd().unwrap();
        cmd.current_dir(temp_dir)
            .arg("config")
            .arg("show")
            .arg("--help")
            .assert()
            .success()
            .stdout(predicate::str::contains("config"));
    }

    #[test]
    fn test_stress_testing_operations() {
        let test_fixtures = TestFixtures::new();
        let temp_dir = test_fixtures.temp_dir.path();

        // Initialize project
        let mut cmd = crate::common::lotar_cmd().unwrap();
        cmd.current_dir(temp_dir)
            .arg("config")
            .arg("init")
            .arg("--project=StressTest")
            .assert()
            .success();

        // Stress test: Create, list, modify, list cycle
        for cycle in 0..5 {
            // Create task
            let mut cmd = crate::common::lotar_cmd().unwrap();
            cmd.current_dir(temp_dir)
                .arg("task")
                .arg("add")
                .arg(format!("Stress test task {cycle}"))
                .arg("--project=StressTest")
                .assert()
                .success();

            // List tasks
            let mut cmd = crate::common::lotar_cmd().unwrap();
            cmd.current_dir(temp_dir)
                .arg("list")
                .arg("--project=StressTest")
                .assert()
                .success();

            // Modify task if it's not the first one
            if cycle > 0 {
                let mut cmd = crate::common::lotar_cmd().unwrap();
                cmd.current_dir(temp_dir)
                    .arg("status")
                    .arg(format!("STRE-{cycle}"))
                    .arg("done")
                    .assert()
                    .success();
            }

            // List tasks again to verify
            let mut cmd = crate::common::lotar_cmd().unwrap();
            cmd.current_dir(temp_dir)
                .arg("list")
                .arg("--project=StressTest")
                .assert()
                .success();
        }

        // Final verification
        let mut cmd = crate::common::lotar_cmd().unwrap();
        cmd.current_dir(temp_dir)
            .arg("list")
            .arg("--project=StressTest")
            .assert()
            .success();
    }

    #[test]
    fn test_error_recovery_performance() {
        let test_fixtures = TestFixtures::new();
        let temp_dir = test_fixtures.temp_dir.path();

        // Test error conditions and recovery
        let start = Instant::now();

        // Invalid project name - this should succeed but find no tasks
        let mut cmd = crate::common::lotar_cmd().unwrap();
        cmd.current_dir(temp_dir)
            .arg("list")
            .arg("--project=NonExistentProject")
            .assert()
            .success()
            .stderr(predicate::str::contains("No tasks found"));

        // Invalid task ID with status command
        let mut cmd = crate::common::lotar_cmd().unwrap();
        cmd.current_dir(temp_dir)
            .arg("status")
            .arg("INVALID-999")
            .arg("done")
            .assert()
            .failure();

        // Invalid template
        let mut cmd = crate::common::lotar_cmd().unwrap();
        cmd.current_dir(temp_dir)
            .arg("config")
            .arg("init")
            .arg("--template=invalid")
            .assert()
            .failure();

        // Valid operation after errors
        let mut cmd = crate::common::lotar_cmd().unwrap();
        cmd.current_dir(temp_dir).arg("help").assert().success();

        let error_recovery_duration = start.elapsed();

        // Error handling should be fast (< 5 seconds total)
        assert!(
            error_recovery_duration.as_secs() < 5,
            "Error recovery took too long: {error_recovery_duration:?}"
        );
    }

    #[test]
    fn test_boundary_conditions() {
        let test_fixtures = TestFixtures::new();
        let temp_dir = test_fixtures.temp_dir.path();

        // Test with very long project names
        let long_project_name = "A".repeat(100);
        let mut cmd = crate::common::lotar_cmd().unwrap();
        cmd.current_dir(temp_dir)
            .arg("config")
            .arg("init")
            .arg(format!("--project={long_project_name}"))
            .assert()
            .success(); // Should handle long names gracefully

        // Test with special characters in project names
        let mut cmd = crate::common::lotar_cmd().unwrap();
        cmd.current_dir(temp_dir)
            .arg("config")
            .arg("init")
            .arg("--project=Test-Project_2024")
            .assert()
            .success();

        // Test with minimal input
        let mut cmd = crate::common::lotar_cmd().unwrap();
        cmd.current_dir(temp_dir)
            .arg("config")
            .arg("init")
            .arg("--project=A")
            .assert()
            .success();

        // All boundary tests should complete quickly
        let mut cmd = crate::common::lotar_cmd().unwrap();
        cmd.current_dir(temp_dir)
            .arg("config")
            .arg("show")
            .assert()
            .success();
    }
}

// =============================================================================
// Integration Performance Tests
// =============================================================================

mod integration_performance {
    use super::*;

    #[test]
    fn test_full_workflow_performance() {
        let test_fixtures = TestFixtures::new();
        let temp_dir = test_fixtures.temp_dir.path();

        let start = Instant::now();

        // Step 1: Project initialization
        let mut cmd = crate::common::lotar_cmd().unwrap();
        cmd.current_dir(temp_dir)
            .arg("config")
            .arg("init")
            .arg("--project=FullWorkflow")
            .assert()
            .success();

        // Step 2: Create project structure
        fs::write(temp_dir.join("main.rs"), "fn main() {}").unwrap();
        fs::create_dir_all(temp_dir.join("src")).unwrap();

        // Step 3: Scan project
        let mut cmd = crate::common::lotar_cmd().unwrap();
        cmd.current_dir(temp_dir).arg("scan").assert().success();

        // Step 4: Add multiple tasks
        for i in 0..10 {
            let mut cmd = crate::common::lotar_cmd().unwrap();
            cmd.current_dir(temp_dir)
                .arg("task")
                .arg("add")
                .arg(format!("Workflow task {i}"))
                .arg("--project=FullWorkflow")
                .assert()
                .success();
        }

        // Step 5: Modify tasks
        for i in 1..=5 {
            let mut cmd = crate::common::lotar_cmd().unwrap();
            cmd.current_dir(temp_dir)
                .arg("status")
                .arg(format!("FULL-{i}"))
                .arg("in_progress")
                .assert()
                .success();
        }

        // Step 6: Complete tasks
        for i in 1..=3 {
            let mut cmd = crate::common::lotar_cmd().unwrap();
            cmd.current_dir(temp_dir)
                .arg("status")
                .arg(format!("FULL-{i}"))
                .arg("done")
                .assert()
                .success();
        }

        // Step 7: Final status check
        let mut cmd = crate::common::lotar_cmd().unwrap();
        cmd.current_dir(temp_dir)
            .arg("list")
            .arg("--project=FullWorkflow")
            .assert()
            .success()
            .stdout(predicate::str::contains("FULL"));

        let total_workflow_duration = start.elapsed();
        println!("Full workflow completed in: {total_workflow_duration:?}");

        // Complete workflow should finish in reasonable time (< 30 seconds)
        assert!(
            total_workflow_duration.as_secs() < 30,
            "Full workflow took too long: {total_workflow_duration:?}"
        );
    }
}

use assert_cmd::Command;
use predicates::prelude::*;
use tempfile::TempDir;
use std::time::Instant;

mod common;

/// Performance and scalability tests
#[cfg(test)]
mod performance_tests {
    use super::*;

    #[test]
    fn test_cli_startup_time() {
        let start = Instant::now();

        let mut cmd = Command::cargo_bin("lotar").unwrap();
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
            let mut cmd = Command::cargo_bin("lotar").unwrap();
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

    #[test]
    fn test_large_scale_performance() {
        let temp_dir = TempDir::new().unwrap();

        // Create 100 tasks for performance testing
        let start = Instant::now();
        for i in 1..=100 {
            let mut cmd = Command::cargo_bin("lotar").unwrap();
            cmd.current_dir(&temp_dir)
                .args(&[
                    "task", "add",
                    &format!("--title=Performance Test Task {}", i),
                    "--project=perf-test"
                ])
                .assert()
                .success();
        }
        let creation_duration = start.elapsed();

        // Test listing performance with 100+ tasks
        let start = Instant::now();
        let mut cmd = Command::cargo_bin("lotar").unwrap();
        cmd.current_dir(&temp_dir)
            .args(&["task", "list", "--project=perf-test"])
            .assert()
            .success()
            .stdout(predicate::str::contains("Listing tasks for project"));
        let list_duration = start.elapsed();

        // Performance assertions (generous thresholds)
        assert!(creation_duration.as_secs() < 30,
               "100 task creation should complete within 30 seconds, took: {:?}", creation_duration);
        assert!(list_duration.as_millis() < 2000,
               "List operation should complete within 2 seconds, took: {:?}", list_duration);
    }
}

/// Storage-level performance tests
#[cfg(test)]
mod storage_performance_tests {
    use super::*;
    use local_task_repo::storage::Task;
    use local_task_repo::types::Priority;
    use common::TestFixtures;

    #[test]
    fn test_task_creation_performance() {
        let fixtures = TestFixtures::new();
        let mut storage = fixtures.create_storage();

        let start = Instant::now();

        // Create 50 tasks and measure performance
        for i in 0..50 {
            let task = Task::new(
                fixtures.tasks_root.clone(),
                format!("Performance Task {}", i),
                "perf-test".to_string(),
                Priority::Medium
            );
            storage.add(&task);
        }

        let duration = start.elapsed();
        assert!(duration.as_millis() < 2000,
               "50 task creation should complete within 2 seconds, took: {:?}", duration);
    }

    #[test]
    fn test_bulk_task_creation() {
        let fixtures = TestFixtures::new();
        let mut storage = fixtures.create_storage();

        let start = Instant::now();

        // Create 100 tasks in bulk
        for i in 0..100 {
            let task = Task::new(
                fixtures.tasks_root.clone(),
                format!("Bulk Task {}", i),
                "bulk-test".to_string(),
                Priority::Low
            );
            storage.add(&task);
        }

        let duration = start.elapsed();
        assert!(duration.as_secs() < 5,
               "100 task bulk creation should complete within 5 seconds, took: {:?}", duration);
    }
}

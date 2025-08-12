use assert_cmd::Command;
use predicates::prelude::*;
use tempfile::TempDir;
mod common;

/// Focused test suite for smart defaults functionality
/// Smart Defaults Test: Phase 3.1.1 completion validation
/// Tests the core smart default behavior that was implemented
///
struct TestEnvironment {
    temp_dir: TempDir,
}

impl TestEnvironment {
    fn new() -> Self {
        TestEnvironment {
            temp_dir: TempDir::new().unwrap(),
        }
    }

    fn path(&self) -> &std::path::Path {
        self.temp_dir.path()
    }

    /// Helper to check task priority via list command
    fn assert_task_priority(&self, project: &str, expected_priority: &str) {
        Command::cargo_bin("lotar")
            .unwrap()
            .current_dir(self.path())
            .arg("list")
            .arg(format!("--project={project}"))
            .assert()
            .success()
            .stdout(predicate::str::contains(format!("({expected_priority})")));
    }

    /// Helper to check task status via list command
    fn assert_task_status(&self, project: &str, expected_status: &str) {
        Command::cargo_bin("lotar")
            .unwrap()
            .current_dir(self.path())
            .arg("list")
            .arg(format!("--project={project}"))
            .assert()
            .success()
            .stdout(predicate::str::contains(format!("[{expected_status}]")));
    }

    /// Helper to create task and expect warnings
    fn create_task_with_warning(&self, project: &str, title: &str, expected_warning: &str) {
        Command::cargo_bin("lotar")
            .unwrap()
            .current_dir(self.path())
            .arg("add")
            .arg(title)
            .arg(format!("--project={project}"))
            .assert()
            .success()
            .stderr(predicate::str::contains(expected_warning));
    }

    /// Helper to create task without warnings
    fn create_task(&self, project: &str, title: &str) {
        Command::cargo_bin("lotar")
            .unwrap()
            .current_dir(self.path())
            .arg("add")
            .arg(title)
            .arg(format!("--project={project}"))
            .assert()
            .success();
    }

    /// Helper to initialize project
    fn init_project(&self, project: &str) {
        Command::cargo_bin("lotar")
            .unwrap()
            .current_dir(self.path())
            .arg("config")
            .arg("init")
            .arg(format!("--project={project}"))
            .assert()
            .success();
    }

    /// Helper to set config value
    fn set_config(&self, project: &str, key: &str, value: &str) {
        Command::cargo_bin("lotar")
            .unwrap()
            .current_dir(self.path())
            .arg("config")
            .arg("set")
            .arg(key)
            .arg(value)
            .arg(format!("--project={project}"))
            .assert()
            .success();
    }
}

mod priority_smart_defaults {
    use super::*;

    #[test]
    fn test_priority_uses_global_default_when_valid() {
        let env = TestEnvironment::new();

        env.init_project("prio-global");
        env.set_config(
            "prio-global",
            "issue_priorities",
            "Critical,High,Medium,Low",
        );
        env.create_task("prio-global", "Test global default priority");

        // Should use global default (Medium) since it's in the project list
        env.assert_task_priority("prio-global", "MEDIUM");
    }

    #[test]
    fn test_priority_falls_back_to_first_when_global_invalid() {
        let env = TestEnvironment::new();

        env.init_project("prio-fallback");
        env.set_config("prio-fallback", "issue_priorities", "Critical,High"); // No Medium
        env.create_task_with_warning(
            "prio-fallback",
            "Test priority fallback",
            "Warning: Global default priority 'Medium' is not in project priority list",
        );

        // Should fall back to first in list (Critical)
        env.assert_task_priority("prio-fallback", "CRITICAL");
    }

    #[test]
    fn test_priority_explicit_project_default_wins() {
        let env = TestEnvironment::new();

        env.init_project("prio-explicit");
        env.set_config(
            "prio-explicit",
            "issue_priorities",
            "Critical,High,Medium,Low",
        );
        env.set_config("prio-explicit", "default_priority", "High");
        env.create_task("prio-explicit", "Test explicit priority");

        // Should use explicit project default (High)
        env.assert_task_priority("prio-explicit", "HIGH");
    }

    #[test]
    fn test_priority_explicit_invalid_falls_back() {
        let env = TestEnvironment::new();

        env.init_project("prio-invalid");
        env.set_config("prio-invalid", "issue_priorities", "Critical,High");
        env.set_config("prio-invalid", "default_priority", "Low"); // Not in list
        env.create_task_with_warning(
            "prio-invalid",
            "Test invalid explicit priority",
            "Warning", // Should show some warning about fallback
        );

        // Should fall back to first in list (Critical)
        env.assert_task_priority("prio-invalid", "CRITICAL");
    }
}

mod status_smart_defaults {
    use super::*;

    #[test]
    fn test_status_uses_first_when_no_explicit_default() {
        let env = TestEnvironment::new();

        env.init_project("status-first");
        env.set_config("status-first", "issue_states", "BLOCKED,TODO,DONE");
        env.create_task("status-first", "Test status first");

        // Should use first in list (BLOCKED)
        env.assert_task_status("status-first", "BLOCKED");
    }

    #[test]
    fn test_status_explicit_project_default_wins() {
        let env = TestEnvironment::new();

        env.init_project("status-explicit");
        env.set_config("status-explicit", "issue_states", "TODO,BLOCKED,DONE");
        env.set_config("status-explicit", "default_status", "BLOCKED");
        env.create_task("status-explicit", "Test explicit status");

        // Should use explicit project default (BLOCKED)
        env.assert_task_status("status-explicit", "BLOCKED");
    }

    #[test]
    fn test_status_explicit_invalid_falls_back_to_first() {
        let env = TestEnvironment::new();

        env.init_project("status-invalid");
        env.set_config("status-invalid", "issue_states", "TODO,DONE");
        env.set_config("status-invalid", "default_status", "BLOCKED"); // Not in list
        env.create_task_with_warning(
            "status-invalid",
            "Test invalid explicit status",
            "Warning", // Should show warning about fallback
        );

        // Should fall back to first in list (TODO)
        env.assert_task_status("status-invalid", "TODO");
    }

    #[test]
    fn test_status_default_template_behavior() {
        let env = TestEnvironment::new();

        env.init_project("status-default");
        // Don't customize issue_states - use template defaults
        env.create_task("status-default", "Test default status");

        // Should use first from default template (TODO)
        env.assert_task_status("status-default", "TODO");
    }
}

mod combined_scenarios {
    use super::*;

    #[test]
    fn test_mixed_priority_and_status_defaults() {
        let env = TestEnvironment::new();

        env.init_project("mixed");

        // Priority: global not in list → fallback to first
        env.set_config("mixed", "issue_priorities", "Critical,High"); // No Medium

        // Status: explicit default
        env.set_config("mixed", "issue_states", "TODO,BLOCKED,DONE");
        env.set_config("mixed", "default_status", "BLOCKED");

        env.create_task_with_warning(
            "mixed",
            "Test mixed defaults",
            "Warning: Global default priority 'Medium' is not in project priority list",
        );

        // Should get fallback priority + explicit status
        env.assert_task_priority("mixed", "CRITICAL");
        env.assert_task_status("mixed", "BLOCKED");
    }

    #[test]
    fn test_both_explicit_defaults_work_together() {
        let env = TestEnvironment::new();

        env.init_project("both-explicit");

        // Both priority and status have explicit defaults
        env.set_config(
            "both-explicit",
            "issue_priorities",
            "Low,Medium,High,Critical",
        );
        env.set_config("both-explicit", "default_priority", "High");

        env.set_config("both-explicit", "issue_states", "TODO,BLOCKED,VERIFY,DONE");
        env.set_config("both-explicit", "default_status", "VERIFY");

        env.create_task("both-explicit", "Test both explicit");

        // Should use both explicit defaults
        env.assert_task_priority("both-explicit", "HIGH");
        env.assert_task_status("both-explicit", "VERIFY");
    }

    #[test]
    fn test_global_default_status_invalid_but_project_list_valid_fallback() {
        let env = TestEnvironment::new();

        env.init_project("precedence");
        // Configure project states without the global default (valid enum names only)
        env.set_config("precedence", "issue_states", "TODO,IN_PROGRESS,DONE");

        // Set global default_status to value not present in project list
        Command::cargo_bin("lotar")
            .unwrap()
            .current_dir(env.path())
            .args(["config", "set", "default_status", "BLOCKED", "--global"])
            .assert()
            .success();

        // Create task and ensure it falls back to first project state
        env.create_task("precedence", "Test precedence fallback");
        // Should fall back to first project state (TODO)
        env.assert_task_status("precedence", "TODO");
    }
}

mod edge_cases {
    use super::*;

    #[test]
    fn test_single_priority_value() {
        let env = TestEnvironment::new();

        env.init_project("single-prio");
        env.set_config("single-prio", "issue_priorities", "Critical"); // Only one option
        env.create_task_with_warning(
            "single-prio",
            "Test single priority",
            "Warning: Global default priority 'Medium' is not in project priority list",
        );

        // Should use the only available priority
        env.assert_task_priority("single-prio", "CRITICAL");
    }

    #[test]
    fn test_single_status_value() {
        let env = TestEnvironment::new();

        env.init_project("single-status");
        env.set_config("single-status", "issue_states", "TODO"); // Only one option
        env.create_task("single-status", "Test single status");

        // Should use the only available status
        env.assert_task_status("single-status", "TODO");
    }

    #[test]
    fn test_multiple_tasks_consistent_defaults() {
        let env = TestEnvironment::new();

        env.init_project("consistent");
        env.set_config("consistent", "issue_priorities", "Critical,High");
        env.set_config("consistent", "issue_states", "VERIFY,TODO,DONE");

        // Create multiple tasks
        for i in 1..=3 {
            env.create_task_with_warning(
                "consistent",
                &format!("Test task {i}"),
                "Warning: Global default priority 'Medium' is not in project priority list",
            );
        }

        // All tasks should have consistent defaults
        let output = Command::cargo_bin("lotar")
            .unwrap()
            .current_dir(env.path())
            .arg("list")
            .arg("--project=consistent")
            .output()
            .unwrap();

        let stdout = String::from_utf8_lossy(&output.stdout);
        // Should have 3 tasks all with CRITICAL priority and VERIFY status
        assert_eq!(stdout.matches("(CRITICAL)").count(), 3);
        assert_eq!(stdout.matches("[VERIFY]").count(), 3);
    }
}

// Merged from default_status_assignment_test.rs
mod default_status_tests {
    use super::*;
    use common::TestFixtures;

    #[test]
    fn test_new_task_uses_first_configured_status() {
        let test_fixtures = TestFixtures::new();
        let temp_dir = test_fixtures.temp_dir.path();

        let mut cmd = Command::cargo_bin("lotar").unwrap();
        cmd.current_dir(temp_dir)
            .arg("config")
            .arg("init")
            .arg("--project=custom-status")
            .assert()
            .success();

        let mut cmd = Command::cargo_bin("lotar").unwrap();
        cmd.current_dir(temp_dir)
            .arg("config")
            .arg("set")
            .arg("issue_states")
            .arg("IN_PROGRESS,TODO,DONE")
            .arg("--project=custom-status")
            .assert()
            .success();

        let mut cmd = Command::cargo_bin("lotar").unwrap();
        cmd.current_dir(temp_dir)
            .arg("add")
            .arg("Test default status")
            .arg("--project=custom-status")
            .assert()
            .success();

        let mut cmd = Command::cargo_bin("lotar").unwrap();
        cmd.current_dir(temp_dir)
            .arg("status")
            .arg("1")
            .arg("--project=custom-status")
            .assert()
            .success()
            .stdout(predicate::str::contains("status: IN_PROGRESS"));
    }

    #[test]
    fn test_explicit_default_status_config() {
        let test_fixtures = TestFixtures::new();
        let temp_dir = test_fixtures.temp_dir.path();

        let mut cmd = Command::cargo_bin("lotar").unwrap();
        cmd.current_dir(temp_dir)
            .arg("config")
            .arg("init")
            .arg("--project=explicit-default")
            .assert()
            .success();

        let mut cmd = Command::cargo_bin("lotar").unwrap();
        cmd.current_dir(temp_dir)
            .arg("config")
            .arg("set")
            .arg("issue_states")
            .arg("TODO,IN_PROGRESS,DONE")
            .arg("--project=explicit-default")
            .assert()
            .success();

        let mut cmd = Command::cargo_bin("lotar").unwrap();
        cmd.current_dir(temp_dir)
            .arg("config")
            .arg("set")
            .arg("default_status")
            .arg("DONE")
            .arg("--project=explicit-default")
            .assert()
            .success();

        let mut cmd = Command::cargo_bin("lotar").unwrap();
        cmd.current_dir(temp_dir)
            .arg("add")
            .arg("Test explicit default")
            .arg("--project=explicit-default")
            .assert()
            .success();

        let mut cmd = Command::cargo_bin("lotar").unwrap();
        cmd.current_dir(temp_dir)
            .arg("status")
            .arg("1")
            .arg("--project=explicit-default")
            .assert()
            .success()
            .stdout(predicate::str::contains("status: DONE"));
    }

    #[test]
    fn test_fallback_to_todo_when_no_config() {
        let test_fixtures = TestFixtures::new();
        let temp_dir = test_fixtures.temp_dir.path();

        let mut cmd = Command::cargo_bin("lotar").unwrap();
        cmd.current_dir(temp_dir)
            .arg("add")
            .arg("Test fallback default")
            .assert()
            .success();

        let mut cmd = Command::cargo_bin("lotar").unwrap();
        cmd.current_dir(temp_dir)
            .arg("status")
            .arg("1")
            .assert()
            .success()
            .stdout(predicate::str::contains("status: TODO"));
    }
}

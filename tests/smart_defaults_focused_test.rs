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

// =============================================================================
// Assignment & Reporter auto behavior (merged)
// =============================================================================
mod assignment {
    use crate::common::env_mutex::EnvVarGuard;
    use lotar::api_types::{TaskCreate, TaskUpdate};
    use lotar::services::task_service::TaskService;
    use lotar::storage::manager::Storage;
    use lotar::types::{Priority, TaskStatus, TaskType};
    use lotar::utils::paths;

    #[test]
    fn reporter_is_auto_set_from_config_on_create() {
        let tmp = tempfile::tempdir().unwrap();
        let tasks_dir = tmp.path().join(".tasks");
        std::fs::create_dir_all(&tasks_dir).unwrap();
        let _guard = EnvVarGuard::set("LOTAR_TASKS_DIR", &tasks_dir.to_string_lossy());

        // Global config with default_reporter
        std::fs::write(
            paths::global_config_path(&tasks_dir),
            "default.project: TEST\nissue.states: [Todo, InProgress, Done]\nissue.types: [Feature, Bug, Chore]\nissue.priorities: [Low, Medium, High]\ndefault.reporter: alice@example.com\n",
        )
        .unwrap();

        let mut storage = Storage::new(tasks_dir.clone());
        let req = TaskCreate {
            title: "Auto reporter".to_string(),
            project: Some("TEST".to_string()),
            priority: Some(Priority::from("High")),
            task_type: Some(TaskType::from("Feature")),
            ..TaskCreate::default()
        };
        let created = TaskService::create(&mut storage, req).expect("service create");
        assert_eq!(created.reporter.as_deref(), Some("alice@example.com"));
    }

    #[test]
    fn reporter_respects_disable_flag() {
        let tmp = tempfile::tempdir().unwrap();
        let tasks_dir = tmp.path().join(".tasks");
        std::fs::create_dir_all(&tasks_dir).unwrap();
        let _guard = EnvVarGuard::set("LOTAR_TASKS_DIR", &tasks_dir.to_string_lossy());
        // Disable via config
        std::fs::write(
            paths::global_config_path(&tasks_dir),
            "default.project: TEST\nauto.set_reporter: false\n",
        )
        .unwrap();

        let mut storage = Storage::new(tasks_dir);
        let req = TaskCreate {
            title: "No reporter".to_string(),
            project: Some("TEST".to_string()),
            ..TaskCreate::default()
        };
        let created = TaskService::create(&mut storage, req).expect("service create");
        assert!(
            created.reporter.is_none(),
            "reporter should be None when disabled"
        );
    }

    #[test]
    fn reporter_falls_back_to_git_or_system_when_no_config() {
        let tmp = tempfile::tempdir().unwrap();
        let tasks_dir = tmp.path().join(".tasks");
        std::fs::create_dir_all(&tasks_dir).unwrap();
        let _guard = EnvVarGuard::set("LOTAR_TASKS_DIR", &tasks_dir.to_string_lossy());

        let mut storage = Storage::new(tasks_dir);
        let req = TaskCreate {
            title: "File reporter".to_string(),
            project: Some("TEST".to_string()),
            ..TaskCreate::default()
        };
        let created = TaskService::create(&mut storage, req).expect("service create");
        let _ = created.reporter; // may be Some or None; ensure no crash
    }

    #[test]
    fn reporter_default_me_alias_resolves_to_identity() {
        let tmp = tempfile::tempdir().unwrap();
        let tasks_dir = tmp.path().join(".tasks");
        std::fs::create_dir_all(&tasks_dir).unwrap();
        let _tasks_guard = EnvVarGuard::set("LOTAR_TASKS_DIR", &tasks_dir.to_string_lossy());
        let _user_guard = EnvVarGuard::set("USER", "alias-user");
        let _username_guard = EnvVarGuard::set("USERNAME", "alias-user");
        let _default_guard = EnvVarGuard::clear("LOTAR_DEFAULT_REPORTER");

        std::fs::write(
            paths::global_config_path(&tasks_dir),
            "default.project: TEST\nissue.states: [Todo, InProgress, Done]\nissue.types: [Feature, Bug, Chore]\nissue.priorities: [Low, Medium, High]\ndefault.reporter: \"@me\"\n",
        )
        .unwrap();

        lotar::utils::identity::invalidate_identity_cache(Some(&tasks_dir));

        let mut storage = Storage::new(tasks_dir.clone());
        let req = TaskCreate {
            title: "Alias reporter".to_string(),
            project: Some("TEST".to_string()),
            ..TaskCreate::default()
        };
        let created = TaskService::create(&mut storage, req).expect("service create");
        assert_eq!(created.reporter.as_deref(), Some("alias-user"));
    }

    #[test]
    fn assignee_auto_set_on_status_change_when_empty() {
        let tmp = tempfile::tempdir().unwrap();
        let tasks_dir = tmp.path().join(".tasks");
        std::fs::create_dir_all(&tasks_dir).unwrap();
        let _guard = EnvVarGuard::set("LOTAR_TASKS_DIR", &tasks_dir.to_string_lossy());

        // default_reporter=bob used for auto-assign
        std::fs::write(
            paths::global_config_path(&tasks_dir),
            "default.project: TEST\nissue.states: [Todo, InProgress, Done]\nissue.types: [Feature, Bug, Chore]\nissue.priorities: [Low, Medium, High]\ndefault.reporter: bob\n",
        )
        .unwrap();

        let merged = lotar::config::resolution::load_and_merge_configs(Some(&tasks_dir))
            .expect("load merged config");
        assert_eq!(merged.default_reporter.as_deref(), Some("bob"));

        let mut storage = Storage::new(tasks_dir.clone());
        let create = TaskCreate {
            title: "Needs assignee".to_string(),
            project: Some("TEST".to_string()),
            ..TaskCreate::default()
        };
        let created = TaskService::create(&mut storage, create).unwrap();
        assert!(created.assignee.is_none(), "assignee should start None");

        let mut storage = Storage::new(tasks_dir);
        let updated = TaskService::update(
            &mut storage,
            &created.id,
            TaskUpdate {
                status: Some(TaskStatus::from("InProgress")),
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(updated.assignee.as_deref(), Some("bob"));
    }

    #[test]
    fn assignee_auto_set_respects_disable_flag() {
        let tmp = tempfile::tempdir().unwrap();
        let tasks_dir = tmp.path().join(".tasks");
        std::fs::create_dir_all(&tasks_dir).unwrap();
        let _guard = EnvVarGuard::set("LOTAR_TASKS_DIR", &tasks_dir.to_string_lossy());
        std::fs::write(
            paths::global_config_path(&tasks_dir),
            "default.project: TEST\nauto.assign_on_status: false\n",
        )
        .unwrap();

        let mut storage = Storage::new(tasks_dir.clone());
        let create = TaskCreate {
            title: "No auto assign".to_string(),
            project: Some("TEST".to_string()),
            ..TaskCreate::default()
        };
        let created = TaskService::create(&mut storage, create).unwrap();
        assert!(created.assignee.is_none());

        let mut storage = Storage::new(tasks_dir);
        let updated = TaskService::update(
            &mut storage,
            &created.id,
            TaskUpdate {
                status: Some(TaskStatus::from("Done")),
                ..Default::default()
            },
        )
        .unwrap();
        assert!(
            updated.assignee.is_none(),
            "assignee should remain None when disabled"
        );
    }

    #[test]
    fn first_change_does_not_overwrite_existing_assignee() {
        let tmp = tempfile::tempdir().unwrap();
        let tasks_dir = tmp.path().join(".tasks");
        std::fs::create_dir_all(&tasks_dir).unwrap();
        let _guard = EnvVarGuard::set("LOTAR_TASKS_DIR", &tasks_dir.to_string_lossy());

        std::fs::write(
            paths::global_config_path(&tasks_dir),
            "default.project: AAA\nissue.states: [Todo, InProgress, Done]\nissue.types: [Feature, Bug, Chore]\nissue.priorities: [Low, Medium, High]\ndefault.reporter: ryan\n",
        )
        .unwrap();

        let mut storage = Storage::new(tasks_dir.clone());
        let created = TaskService::create(
            &mut storage,
            TaskCreate {
                title: "Preset assignee".into(),
                project: Some("AAA".into()),
                assignee: Some("sam".into()),
                ..TaskCreate::default()
            },
        )
        .unwrap();

        // Status change should not override existing assignee
        let mut storage = Storage::new(tasks_dir.clone());
        let updated = TaskService::update(
            &mut storage,
            &created.id,
            TaskUpdate {
                status: Some(TaskStatus::from("InProgress")),
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(updated.assignee.as_deref(), Some("sam"));
    }

    #[test]
    fn env_default_reporter_is_respected() {
        let tmp = tempfile::tempdir().unwrap();
        let tasks_dir = tmp.path().join(".tasks");
        std::fs::create_dir_all(&tasks_dir).unwrap();
        let _guard_tasks = EnvVarGuard::set("LOTAR_TASKS_DIR", &tasks_dir.to_string_lossy());
        let _guard_rep = EnvVarGuard::set("LOTAR_DEFAULT_REPORTER", "env-reporter@example.com");

        let mut storage = Storage::new(tasks_dir);
        let req = TaskCreate {
            title: "Env reporter".to_string(),
            project: Some("TEST".to_string()),
            ..TaskCreate::default()
        };
        let created = TaskService::create(&mut storage, req).expect("create");
        assert_eq!(
            created.reporter.as_deref(),
            Some("env-reporter@example.com")
        );
    }
}

impl TestEnvironment {
    fn new() -> Self {
        crate::common::reset_lotar_test_environment();

        TestEnvironment {
            temp_dir: TempDir::new().unwrap(),
        }
    }

    fn path(&self) -> &std::path::Path {
        self.temp_dir.path()
    }

    /// Helper to check task priority via list command
    fn assert_task_priority(&self, project: &str, expected_priority: &str) {
        crate::common::lotar_cmd()
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
        crate::common::lotar_cmd()
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
        crate::common::lotar_cmd()
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
        crate::common::lotar_cmd()
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
        crate::common::lotar_cmd()
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
        crate::common::lotar_cmd()
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
        env.assert_task_priority("prio-global", "Medium");
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
        env.assert_task_priority("prio-fallback", "Critical");
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
        env.assert_task_priority("prio-explicit", "High");
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
        env.assert_task_priority("prio-invalid", "Critical");
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

        // Should use first from default template (Todo)
        env.assert_task_status("status-default", "Todo");
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
        env.assert_task_priority("mixed", "Critical");
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
        env.assert_task_priority("both-explicit", "High");
        env.assert_task_status("both-explicit", "VERIFY");
    }

    #[test]
    fn test_global_default_status_invalid_but_project_list_valid_fallback() {
        let env = TestEnvironment::new();

        env.init_project("precedence");
        // Configure project states without the global default (valid enum names only)
        env.set_config("precedence", "issue_states", "TODO,IN_PROGRESS,DONE");

        // Set global default_status to value not present in project list
        crate::common::lotar_cmd()
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
        env.assert_task_priority("single-prio", "Critical");
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
        let output = crate::common::lotar_cmd()
            .unwrap()
            .current_dir(env.path())
            .arg("list")
            .arg("--project=consistent")
            .output()
            .unwrap();

        let stdout = String::from_utf8_lossy(&output.stdout);
        // Should have 3 tasks all with Critical priority and VERIFY status
        assert_eq!(stdout.matches("(Critical)").count(), 3);
        assert_eq!(stdout.matches("[VERIFY]").count(), 3);
    }
}

// Merged from default_status_assignment_test.rs
mod default_status_tests {
    use super::*;
    use common::TestFixtures;
    use common::env_mutex::EnvVarGuard;

    #[test]
    fn test_new_task_uses_first_configured_status() {
        let test_fixtures = TestFixtures::new();
        let temp_dir = test_fixtures.temp_dir.path();
        let _ignore_env = EnvVarGuard::set("LOTAR_IGNORE_ENV_TASKS_DIR", "1");

        let mut cmd = crate::common::lotar_cmd().unwrap();
        cmd.current_dir(temp_dir)
            .arg("config")
            .arg("init")
            .arg("--project=custom-status")
            .assert()
            .success();

        let mut cmd = crate::common::lotar_cmd().unwrap();
        cmd.current_dir(temp_dir)
            .arg("config")
            .arg("set")
            .arg("issue_states")
            .arg("IN_PROGRESS,TODO,DONE")
            .arg("--project=custom-status")
            .assert()
            .success();

        let mut cmd = crate::common::lotar_cmd().unwrap();
        cmd.current_dir(temp_dir)
            .arg("add")
            .arg("Test default status")
            .arg("--project=custom-status")
            .assert()
            .success();

        let mut cmd = crate::common::lotar_cmd().unwrap();
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
        let _ignore_env = EnvVarGuard::set("LOTAR_IGNORE_ENV_TASKS_DIR", "1");

        let mut cmd = crate::common::lotar_cmd().unwrap();
        cmd.current_dir(temp_dir)
            .arg("config")
            .arg("init")
            .arg("--project=explicit-default")
            .assert()
            .success();

        let mut cmd = crate::common::lotar_cmd().unwrap();
        cmd.current_dir(temp_dir)
            .arg("config")
            .arg("set")
            .arg("issue_states")
            .arg("TODO,IN_PROGRESS,DONE")
            .arg("--project=explicit-default")
            .assert()
            .success();

        let mut cmd = crate::common::lotar_cmd().unwrap();
        cmd.current_dir(temp_dir)
            .arg("config")
            .arg("set")
            .arg("default_status")
            .arg("DONE")
            .arg("--project=explicit-default")
            .assert()
            .success();

        let mut cmd = crate::common::lotar_cmd().unwrap();
        cmd.current_dir(temp_dir)
            .arg("add")
            .arg("Test explicit default")
            .arg("--project=explicit-default")
            .assert()
            .success();

        let mut cmd = crate::common::lotar_cmd().unwrap();
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
        let _ignore_env = EnvVarGuard::set("LOTAR_IGNORE_ENV_TASKS_DIR", "1");

        let mut cmd = crate::common::lotar_cmd().unwrap();
        cmd.current_dir(temp_dir)
            .arg("add")
            .arg("Test fallback default")
            .assert()
            .success();

        let mut cmd = crate::common::lotar_cmd().unwrap();
        cmd.current_dir(temp_dir)
            .arg("status")
            .arg("1")
            .assert()
            .success()
            .stdout(predicate::str::contains("status: Todo"));
    }
}

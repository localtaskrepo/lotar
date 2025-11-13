// Consolidated small CLI unit tests: priority and status args/handlers.

use lotar::cli::handlers::priority::{PriorityArgs, PriorityHandler};
use lotar::cli::handlers::status::StatusArgs;

mod common;

#[test]
fn priority_args_get_only() {
    let args = PriorityArgs::new("TEST-1".to_string(), None, Some("test-project".to_string()));
    assert_eq!(args.task_id, "TEST-1");
    assert_eq!(args.new_priority, None);
    assert_eq!(args.explicit_project, Some("test-project".to_string()));
}

#[test]
fn priority_handler_type_exists() {
    let _handler = PriorityHandler;
}

#[test]
fn status_args_creation() {
    let args = StatusArgs::new(
        "AUTH-123".to_string(),
        Some("InProgress".to_string()),
        Some("auth".to_string()),
    );
    assert_eq!(args.task_id, "AUTH-123");
    assert_eq!(args.new_status, Some("InProgress".to_string()));
    assert_eq!(args.explicit_project, Some("auth".to_string()));
}

#[test]
fn status_args_get_only() {
    let args = StatusArgs::new("AUTH-123".to_string(), None, Some("auth".to_string()));
    assert_eq!(args.task_id, "AUTH-123");
    assert_eq!(args.new_status, None);
    assert_eq!(args.explicit_project, Some("auth".to_string()));
}

// =============================================================================
// Effort normalization behavior (consolidated)
// =============================================================================

mod effort_normalization {
    use assert_cmd::Command;
    use serde_json::Value;
    use tempfile::TempDir;

    fn run(cmd: &mut Command, temp_dir: &TempDir, args: &[&str]) -> assert_cmd::assert::Assert {
        cmd.current_dir(temp_dir.path())
            .env("LOTAR_TESTS_SILENT", "1")
            .args(args)
            .assert()
    }

    #[test]
    fn effort_is_normalized_on_add_and_edit() {
        let temp = crate::common::temp_dir();

        // Create a task with a variety of effort spellings that should normalize to hours
        run(
            &mut crate::common::lotar_cmd().unwrap(),
            &temp,
            &["task", "add", "A", "--effort", "1 hr 30 min"],
        )
        .success();

        // List as JSON and verify the stored effort is canonical (1.50h)
        let out = crate::common::lotar_cmd()
            .unwrap()
            .current_dir(temp.path())
            .env("LOTAR_TEST_SILENT", "1")
            .args(["task", "list", "--format", "json"])
            .output()
            .unwrap();
        assert!(out.status.success());
        let v: Value = serde_json::from_slice(&out.stdout).unwrap();
        let tasks = v["tasks"].as_array().unwrap();
        assert_eq!(tasks.len(), 1);
        let effort = tasks[0]["effort"].as_str().unwrap();
        assert_eq!(effort, "1.50h");

        // Now edit with a different shape and confirm it re-normalizes
        let id = tasks[0]["id"].as_str().unwrap();
        run(
            &mut crate::common::lotar_cmd().unwrap(),
            &temp,
            &["task", "edit", id, "--effort", "2 days"],
        )
        .success();

        let out2 = crate::common::lotar_cmd()
            .unwrap()
            .current_dir(temp.path())
            .env("LOTAR_TEST_SILENT", "1")
            .args(["task", "list", "--format", "json"])
            .output()
            .unwrap();
        assert!(out2.status.success());
        let v2: Value = serde_json::from_slice(&out2.stdout).unwrap();
        let tasks2 = v2["tasks"].as_array().unwrap();
        assert_eq!(tasks2.len(), 1);
        let effort2 = tasks2[0]["effort"].as_str().unwrap();
        assert_eq!(effort2, "16.00h");
    }
}

// =============================================================================
// Help module unit tests (consolidated)
// =============================================================================

mod help_module {
    use lotar::help::HelpSystem;
    use lotar::output::OutputFormat;

    #[test]
    fn help_system_creation() {
        let help = HelpSystem::new(OutputFormat::Text, false);
        let _ = help;
    }

    #[test]
    fn help_list_available() {
        let help = HelpSystem::new(OutputFormat::Text, false);
        let result = help.list_available_help();
        assert!(result.is_ok());
    }
}

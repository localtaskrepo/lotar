// Consolidated small CLI unit tests: priority and status args/handlers.

use lotar::cli::handlers::priority::{PriorityArgs, PriorityHandler};
use lotar::cli::handlers::status::StatusArgs;

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

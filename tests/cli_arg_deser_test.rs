use lotar::cli;
use lotar::cli::handlers::priority::{PriorityArgs, PriorityHandler};
use lotar::cli::handlers::status::StatusArgs;
use serde_json::json;

#[test]
fn task_add_args_deserialize_full() {
    let payload = json!({
        "title": "Implement API",
        "type": "Feature",
        "priority": "High",
        "assignee": "dev@me",
        "effort": "3d",
        "due_date": "2025-09-01",
        "description": "Build the HTTP API",
        "category": "backend",
        "tags": ["api", "rust"],
        "fields": {"estimate": "5", "sprint": "24"}
    });

    let args: cli::TaskAddArgs = serde_json::from_value(payload).expect("valid add args");
    assert_eq!(args.title, "Implement API");
    assert_eq!(args.task_type.as_deref(), Some("Feature"));
    assert_eq!(args.priority.as_deref(), Some("High"));
    assert_eq!(args.assignee.as_deref(), Some("dev@me"));
    assert_eq!(args.effort.as_deref(), Some("3d"));
    assert_eq!(args.due.as_deref(), Some("2025-09-01"));
    assert_eq!(args.description.as_deref(), Some("Build the HTTP API"));
    assert_eq!(args.category.as_deref(), Some("backend"));
    assert_eq!(args.tags, vec!["api", "rust"]);
    assert!(args.fields.iter().any(|(k, v)| k == "estimate" && v == "5"));
    assert!(args.fields.iter().any(|(k, v)| k == "sprint" && v == "24"));
}

#[test]
fn task_add_args_fields_support_multiple_forms() {
    // Array of "k=v"
    let payload1 = json!({
        "title": "Task",
        "fields": ["a=1", "b=2"]
    });
    let a1: cli::TaskAddArgs = serde_json::from_value(payload1).unwrap();
    assert!(a1.fields.contains(&("a".into(), "1".into())));
    assert!(a1.fields.contains(&("b".into(), "2".into())));

    // Array of [k,v]
    let payload2 = json!({
        "title": "Task",
        "fields": [["x", "10"], ["y", "20"]]
    });
    let a2: cli::TaskAddArgs = serde_json::from_value(payload2).unwrap();
    assert!(a2.fields.contains(&("x".into(), "10".into())));
    assert!(a2.fields.contains(&("y".into(), "20".into())));

    // Object form
    let payload3 = json!({
        "title": "Task",
        "fields": {"p": "alpha", "q": "beta"}
    });
    let a3: cli::TaskAddArgs = serde_json::from_value(payload3).unwrap();
    assert!(a3.fields.contains(&("p".into(), "alpha".into())));
    assert!(a3.fields.contains(&("q".into(), "beta".into())));
}

#[test]
fn task_edit_args_deserialize_minimal() {
    let payload = json!({
        "id": "PROJ-1",
        "title": "New title",
        "due_date": "2025-10-10",
        "tags": ["one", "two"],
        "fields": {"k": "v"}
    });
    let args: cli::TaskEditArgs = serde_json::from_value(payload).expect("valid edit");
    assert_eq!(args.id, "PROJ-1");
    assert_eq!(args.title.as_deref(), Some("New title"));
    assert_eq!(args.due.as_deref(), Some("2025-10-10"));
    assert_eq!(args.tags, vec!["one", "two"]);
    assert!(args.fields.contains(&("k".into(), "v".into())));
}

#[test]
fn task_search_args_aliases_and_defaults() {
    let payload = json!({
        "query": "server",
        "status": ["Todo", "InProgress"],
        "priority": ["High"],
        "type": ["Bug"],
        "tags": ["api"],
        "reverse": true,
        "limit": 5
    });
    let args: cli::TaskSearchArgs = serde_json::from_value(payload).expect("valid search");
    assert_eq!(args.query.as_deref(), Some("server"));
    assert_eq!(args.status, vec!["Todo", "InProgress"]);
    assert_eq!(args.priority, vec!["High"]);
    assert_eq!(args.task_type, vec!["Bug"]);
    assert_eq!(args.tag, vec!["api"]);
    assert!(args.reverse);
    assert_eq!(args.limit, 5);
}

// Merged from cli_handlers_unit_test.rs
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
fn status_args_creation_and_get_only() {
    let args = StatusArgs::new(
        "AUTH-123".to_string(),
        Some("InProgress".to_string()),
        Some("auth".to_string()),
    );
    assert_eq!(args.task_id, "AUTH-123");
    assert_eq!(args.new_status, Some("InProgress".to_string()));
    assert_eq!(args.explicit_project, Some("auth".to_string()));

    let args2 = StatusArgs::new("AUTH-123".to_string(), None, Some("auth".to_string()));
    assert_eq!(args2.task_id, "AUTH-123");
    assert_eq!(args2.new_status, None);
    assert_eq!(args2.explicit_project, Some("auth".to_string()));
}

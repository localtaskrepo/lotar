use lotar::cli::handlers::status::StatusArgs;

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

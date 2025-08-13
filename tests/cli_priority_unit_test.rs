use lotar::cli::handlers::priority::{PriorityArgs, PriorityHandler};

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

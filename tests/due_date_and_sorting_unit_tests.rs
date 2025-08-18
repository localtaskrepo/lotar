use lotar::cli::validation::CliValidator;
use lotar::config::types::{ConfigurableField, ResolvedConfig, StringConfigField};
use lotar::types::{Priority, TaskStatus, TaskType};

fn cfg() -> ResolvedConfig {
    ResolvedConfig {
        server_port: 8080,
        default_prefix: "TEST".to_string(),
        issue_states: ConfigurableField {
            values: vec![
                TaskStatus::Todo,
                TaskStatus::InProgress,
                TaskStatus::Verify,
                TaskStatus::Blocked,
                TaskStatus::Done,
            ],
        },
        issue_types: ConfigurableField {
            values: vec![TaskType::Feature, TaskType::Bug, TaskType::Epic],
        },
        issue_priorities: ConfigurableField {
            values: vec![
                Priority::Low,
                Priority::Medium,
                Priority::High,
                Priority::Critical,
            ],
        },
        categories: StringConfigField::new_wildcard(),
        tags: StringConfigField::new_wildcard(),
        default_assignee: None,
        default_reporter: None,
        default_category: None,
        default_tags: vec![],
        auto_set_reporter: true,
        auto_assign_on_status: true,
        auto_codeowners_assign: true,
        default_priority: Priority::Medium,
        default_status: None,
        custom_fields: StringConfigField::new_wildcard(),
        scan_signal_words: vec![
            "TODO".to_string(),
            "FIXME".to_string(),
            "HACK".to_string(),
            "BUG".to_string(),
            "NOTE".to_string(),
        ],
        auto_identity: true,
        auto_identity_git: true,
        auto_tags_from_path: true,
        auto_branch_infer_type: true,
        auto_branch_infer_status: true,
        auto_branch_infer_priority: true,
        branch_type_aliases: std::collections::HashMap::new(),
        branch_status_aliases: std::collections::HashMap::new(),
        branch_priority_aliases: std::collections::HashMap::new(),
    }
}

#[test]
fn due_date_next_weekday_and_offsets() {
    let conf = cfg();
    let validator = CliValidator::new(&conf);
    // next weekday variants
    assert!(validator.parse_due_date("next monday").is_ok());
    assert!(validator.parse_due_date("next Friday").is_ok());
    // offset variants
    assert!(validator.parse_due_date("+2d").is_ok());
    assert!(validator.parse_due_date("+1 day").is_ok());
    assert!(validator.parse_due_date("+2w").is_ok());
    assert!(validator.parse_due_date("+3 weeks").is_ok());
}

#[test]
fn ordering_invariants_for_status_and_priority() {
    // Priority: Low < Medium < High < Critical
    assert!(Priority::Low < Priority::Medium);
    assert!(Priority::Medium < Priority::High);
    assert!(Priority::High < Priority::Critical);

    // TaskStatus: Todo < InProgress < Verify < Blocked < Done
    assert!(TaskStatus::Todo < TaskStatus::InProgress);
    assert!(TaskStatus::InProgress < TaskStatus::Verify);
    assert!(TaskStatus::Verify < TaskStatus::Blocked);
    assert!(TaskStatus::Blocked < TaskStatus::Done);
}

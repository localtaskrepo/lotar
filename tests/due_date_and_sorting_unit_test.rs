use lotar::cli::validation::CliValidator;
use lotar::config::types::{ConfigurableField, ResolvedConfig, StringConfigField};
use lotar::types::{Priority, TaskStatus, TaskType};

fn cfg() -> ResolvedConfig {
    ResolvedConfig {
        server_port: 8080,
        default_prefix: "TEST".to_string(),
        issue_states: ConfigurableField {
            values: vec![
                TaskStatus::from("Todo"),
                TaskStatus::from("InProgress"),
                TaskStatus::from("Verify"),
                TaskStatus::from("Blocked"),
                TaskStatus::from("Done"),
            ],
        },
        issue_types: ConfigurableField {
            values: vec![
                TaskType::from("Feature"),
                TaskType::from("Bug"),
                TaskType::from("Epic"),
            ],
        },
        issue_priorities: ConfigurableField {
            values: vec![
                Priority::from("Low"),
                Priority::from("Medium"),
                Priority::from("High"),
                Priority::from("Critical"),
            ],
        },
        tags: StringConfigField::new_wildcard(),
        default_assignee: None,
        default_reporter: None,
        default_tags: vec![],
        auto_set_reporter: true,
        auto_assign_on_status: true,
        auto_codeowners_assign: true,
        default_priority: Priority::from("Medium"),
        default_status: None,
        custom_fields: StringConfigField::new_wildcard(),
        scan_signal_words: vec![
            "TODO".to_string(),
            "FIXME".to_string(),
            "HACK".to_string(),
            "BUG".to_string(),
            "NOTE".to_string(),
        ],
        scan_strip_attributes: true,
        scan_ticket_patterns: None,
        scan_enable_ticket_words: false,
        scan_enable_mentions: true,
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
    assert!(Priority::from("Low") < Priority::from("Medium"));
    assert!(Priority::from("Medium") < Priority::from("High"));
    assert!(Priority::from("High") < Priority::from("Critical"));

    // TaskStatus: Todo < InProgress < Verify < Blocked < Done
    assert!(TaskStatus::from("Todo") < TaskStatus::from("InProgress"));
    assert!(TaskStatus::from("InProgress") < TaskStatus::from("Verify"));
    assert!(TaskStatus::from("Verify") < TaskStatus::from("Blocked"));
    assert!(TaskStatus::from("Blocked") < TaskStatus::from("Done"));
}

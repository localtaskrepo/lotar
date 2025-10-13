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
fn validate_status_variants() {
    let conf = cfg();
    let validator = CliValidator::new(&conf);
    assert!(validator.validate_status("TODO").is_ok());
    assert!(validator.validate_status("IN_PROGRESS").is_ok());
    assert!(validator.validate_status("InProgress").is_ok());
    assert!(validator.validate_status("in progress").is_ok());
    assert!(validator.validate_status("DONE").is_ok());
    assert!(validator.validate_status("todo").is_ok());
    assert!(validator.validate_status("in_progress").is_ok());
    assert!(validator.validate_status("done").is_ok());
    assert!(validator.validate_status("Invalid").is_err());
}

#[test]
fn validate_assignee_formats() {
    let conf = cfg();
    let validator = CliValidator::new(&conf);
    assert!(validator.validate_assignee("@me").is_ok());
    assert!(validator.validate_assignee("@john").is_ok());
    assert!(validator.validate_assignee("john@example.com").is_ok());
    assert!(validator.validate_assignee("not-an-email").is_err());
    assert!(validator.validate_assignee("@").is_err());
    assert!(validator.validate_assignee("john@").is_err());
}

#[test]
fn validate_tags_trim_and_reject_empty() {
    let conf = cfg();
    let validator = CliValidator::new(&conf);
    assert_eq!(validator.validate_tag(" backend ").unwrap(), "backend");
    assert!(validator.validate_tag("").is_err());
    assert!(validator.validate_tag("   ").is_err());
}

#[test]
fn parse_due_date_variants() {
    let conf = cfg();
    let validator = CliValidator::new(&conf);
    assert!(validator.parse_due_date("today").is_ok());
    assert!(validator.parse_due_date("tomorrow").is_ok());
    assert!(validator.parse_due_date("next week").is_ok());
    assert!(validator.parse_due_date("2024-12-25").is_ok());
    assert!(validator.parse_due_date("invalid-date").is_err());
    assert!(validator.parse_due_date("12/25/2024").is_err());
}

#[test]
fn validate_effort_formats() {
    let conf = cfg();
    let validator = CliValidator::new(&conf);
    assert!(validator.validate_effort("2h").is_ok());
    assert!(validator.validate_effort("1.5d").is_ok());
    assert!(validator.validate_effort("1w").is_ok());
    assert!(validator.validate_effort("2").is_ok()); // points unit (plain number) is now valid
    assert!(validator.validate_effort("2x").is_err());
    assert!(validator.validate_effort("abc").is_err());
}

#[test]
fn custom_field_collision_is_rejected() {
    let conf = cfg();
    let validator = CliValidator::new(&conf);
    // Built-in names should be rejected as custom fields (case/sep insensitive)
    for bad in ["status", "Status", "due-date", "Due_Date", "TAGS", "effort"] {
        assert!(
            validator.validate_custom_field_name(bad).is_err(),
            "expected collision for {bad}"
        );
    }
}

// Merged from types_priority_unit_test.rs
#[test]
fn priority_serialization() {
    let priority = Priority::from("High");
    assert_eq!(priority.to_string(), "High");
    let yaml = serde_yaml::to_string(&priority).unwrap();
    assert!(yaml.contains("High"));
    let json = serde_json::to_string(&priority).unwrap();
    assert_eq!(json, "\"High\"");
}

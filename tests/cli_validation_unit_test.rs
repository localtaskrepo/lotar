use lotar::cli::validation::CliValidator;
use lotar::config::types::{ConfigurableField, ResolvedConfig, StringConfigField};
use lotar::storage::task::Task;
use lotar::types::{Priority, TaskStatus, TaskType};
use std::path::PathBuf;

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
        tags: StringConfigField::new_wildcard(),
        default_assignee: None,
        default_reporter: None,
        default_tags: vec![],
        members: vec![],
        strict_members: false,
        auto_populate_members: true,
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
        sprint_defaults: Default::default(),
        sprint_notifications: Default::default(),
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
fn validate_assignee_enforces_members_when_strict() {
    let mut conf = cfg();
    conf.strict_members = true;
    conf.members = vec!["allowed@example.com".to_string()];
    let validator = CliValidator::new(&conf);

    assert!(validator.validate_assignee("allowed@example.com").is_ok());
    let err = validator
        .validate_assignee("someone@example.com")
        .unwrap_err();
    assert!(err.contains("not in configured members"));
}

#[test]
fn validate_reporter_formats() {
    let conf = cfg();
    let validator = CliValidator::new(&conf);
    assert!(validator.validate_reporter("@me").is_ok());
    assert!(validator.validate_reporter("@john").is_ok());
    assert!(validator.validate_reporter("john@example.com").is_ok());
    assert!(validator.validate_reporter("not-an-email").is_err());
    assert!(validator.validate_reporter("@").is_err());
    assert!(validator.validate_reporter("john@").is_err());
}

#[test]
fn validate_reporter_enforces_members_when_strict() {
    let mut conf = cfg();
    conf.strict_members = true;
    conf.members = vec!["allowed@example.com".to_string()];
    let validator = CliValidator::new(&conf);

    assert!(validator.validate_reporter("allowed@example.com").is_ok());
    let err = validator
        .validate_reporter("someone@example.com")
        .unwrap_err();
    assert!(err.contains("not in configured members"));
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

#[test]
fn ensure_task_membership_allows_listed_members() {
    let mut conf = cfg();
    conf.strict_members = true;
    conf.members = vec![
        "alice@example.com".to_string(),
        "bob@example.com".to_string(),
    ];
    let validator = CliValidator::new(&conf);
    let mut task = Task::new(
        PathBuf::from("."),
        "demo".to_string(),
        Priority::from("Medium"),
    );
    task.reporter = Some("alice@example.com".to_string());
    task.assignee = Some("bob@example.com".to_string());

    assert!(validator.ensure_task_membership(&task).is_ok());
}

#[test]
fn ensure_task_membership_rejects_unlisted_member() {
    let mut conf = cfg();
    conf.strict_members = true;
    conf.members = vec!["alice@example.com".to_string()];
    let validator = CliValidator::new(&conf);
    let mut task = Task::new(
        PathBuf::from("."),
        "demo".to_string(),
        Priority::from("Medium"),
    );
    task.reporter = Some("bob@example.com".to_string());

    let err = validator.ensure_task_membership(&task).unwrap_err();
    assert!(err.contains("Reporter 'bob@example.com'"));
}

#[test]
fn ensure_task_membership_requires_configured_members() {
    let mut conf = cfg();
    conf.strict_members = true;
    conf.members.clear();
    let validator = CliValidator::new(&conf);
    let task = Task::new(
        PathBuf::from("."),
        "demo".to_string(),
        Priority::from("Medium"),
    );

    let err = validator.ensure_task_membership(&task).unwrap_err();
    assert!(err.contains("Strict members are enabled"));
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

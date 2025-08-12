use lotar::cli::validation::CliValidator;
use lotar::config::types::{ConfigurableField, ResolvedConfig, StringConfigField};
use lotar::types::{Priority, TaskStatus, TaskType};

fn cfg() -> ResolvedConfig {
    ResolvedConfig {
        server_port: 8080,
        default_prefix: "TEST".to_string(),
        issue_states: ConfigurableField {
            values: vec![TaskStatus::Todo, TaskStatus::InProgress, TaskStatus::Done],
        },
        issue_types: ConfigurableField {
            values: vec![TaskType::Feature, TaskType::Bug, TaskType::Epic],
        },
        issue_priorities: ConfigurableField {
            values: vec![Priority::Low, Priority::Medium, Priority::High],
        },
        categories: StringConfigField::new_wildcard(),
        tags: StringConfigField::new_wildcard(),
        default_assignee: None,
        default_priority: Priority::Medium,
        default_status: None,
        custom_fields: StringConfigField::new_wildcard(),
    }
}

#[test]
fn validate_status_variants() {
    let conf = cfg();
    let validator = CliValidator::new(&conf);
    assert!(validator.validate_status("TODO").is_ok());
    assert!(validator.validate_status("IN_PROGRESS").is_ok());
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
    assert!(validator.validate_effort("2").is_err());
    assert!(validator.validate_effort("2x").is_err());
    assert!(validator.validate_effort("abc").is_err());
}

// Merged from types_priority_unit_test.rs
#[test]
fn priority_serialization() {
    let priority = Priority::High;
    assert_eq!(priority.to_string(), "HIGH");
    let yaml = serde_yaml::to_string(&priority).unwrap();
    assert!(yaml.contains("High"));
    let json = serde_json::to_string(&priority).unwrap();
    assert_eq!(json, "\"High\"");
}

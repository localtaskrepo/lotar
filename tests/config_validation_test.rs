use lotar::config::types::{ConfigurableField, GlobalConfig, ProjectConfig, StringConfigField};
use lotar::config::validation::errors::{ValidationError, ValidationResult};
use lotar::config::validation::{ConfigValidator, ValidationSeverity};
use lotar::types::{Priority, TaskStatus, TaskType};
use tempfile::TempDir;

#[test]
fn test_global_config_validation_valid() {
    let temp_dir = TempDir::new().unwrap();
    let validator = ConfigValidator::new(temp_dir.path());

    let config = GlobalConfig {
        server_port: 8080,
        default_prefix: "TEST".to_string(),
        issue_states: ConfigurableField {
            values: vec![TaskStatus::Todo, TaskStatus::InProgress, TaskStatus::Done],
        },
        issue_types: ConfigurableField {
            values: vec![TaskType::Feature, TaskType::Bug],
        },
        issue_priorities: ConfigurableField {
            values: vec![Priority::Low, Priority::Medium, Priority::High],
        },
        categories: StringConfigField::new_wildcard(),
        tags: StringConfigField::new_wildcard(),
        default_assignee: None,
        default_priority: Priority::Medium,
        default_status: Some(TaskStatus::Todo),
        custom_fields: StringConfigField::new_wildcard(),
    };

    let result = validator.validate_global_config(&config);
    assert!(!result.has_errors());
    assert!(!result.has_warnings());
}

#[test]
fn test_global_config_validation_privileged_port_warning() {
    let temp_dir = TempDir::new().unwrap();
    let validator = ConfigValidator::new(temp_dir.path());

    let config = GlobalConfig {
        server_port: 80, // Privileged port should trigger warning
        default_prefix: "TEST".to_string(),
        issue_states: ConfigurableField {
            values: vec![TaskStatus::Todo],
        },
        issue_types: ConfigurableField {
            values: vec![TaskType::Feature],
        },
        issue_priorities: ConfigurableField {
            values: vec![Priority::Medium],
        },
        categories: StringConfigField::new_wildcard(),
        tags: StringConfigField::new_wildcard(),
        default_assignee: None,
        default_priority: Priority::Medium,
        default_status: Some(TaskStatus::Todo),
        custom_fields: StringConfigField::new_wildcard(),
    };

    let result = validator.validate_global_config(&config);
    assert!(!result.has_errors());
    assert!(result.has_warnings());

    // Check the warning content
    assert_eq!(result.warnings.len(), 1);
    let warning = &result.warnings[0];
    assert_eq!(warning.severity, ValidationSeverity::Warning);
    assert!(
        warning
            .message
            .contains("Port 80 may require elevated privileges")
    );
    assert!(
        warning
            .fix_suggestion
            .as_ref()
            .unwrap()
            .contains("port >= 1024")
    );
}

#[test]
fn test_global_config_validation_empty_lists_error() {
    let temp_dir = TempDir::new().unwrap();
    let validator = ConfigValidator::new(temp_dir.path());

    let config = GlobalConfig {
        server_port: 8080,
        default_prefix: "TEST".to_string(),
        issue_states: ConfigurableField {
            values: vec![], // Empty list should trigger error
        },
        issue_types: ConfigurableField {
            values: vec![], // Empty list should trigger error
        },
        issue_priorities: ConfigurableField {
            values: vec![], // Empty list should trigger error
        },
        categories: StringConfigField::new_wildcard(),
        tags: StringConfigField::new_wildcard(),
        default_assignee: None,
        default_priority: Priority::Medium,
        default_status: None,
        custom_fields: StringConfigField::new_wildcard(),
    };

    let result = validator.validate_global_config(&config);
    assert!(result.has_errors());

    // Should have 3 errors for the 3 empty lists
    assert!(result.errors.len() >= 3);

    let error_messages: Vec<&str> = result.errors.iter().map(|e| e.message.as_str()).collect();
    assert!(
        error_messages
            .iter()
            .any(|msg| msg.contains("Issue states list cannot be empty"))
    );
    assert!(
        error_messages
            .iter()
            .any(|msg| msg.contains("Issue types list cannot be empty"))
    );
    assert!(
        error_messages
            .iter()
            .any(|msg| msg.contains("Issue priorities list cannot be empty"))
    );
}

#[test]
fn test_global_config_validation_invalid_defaults() {
    let temp_dir = TempDir::new().unwrap();
    let validator = ConfigValidator::new(temp_dir.path());

    let config = GlobalConfig {
        server_port: 8080,
        default_prefix: "TEST".to_string(),
        issue_states: ConfigurableField {
            values: vec![TaskStatus::Todo, TaskStatus::InProgress],
        },
        issue_types: ConfigurableField {
            values: vec![TaskType::Feature],
        },
        issue_priorities: ConfigurableField {
            values: vec![Priority::Low], // Default priority is Medium but only Low is in list
        },
        categories: StringConfigField::new_wildcard(),
        tags: StringConfigField::new_wildcard(),
        default_assignee: None,
        default_priority: Priority::Medium, // This should cause an error
        default_status: Some(TaskStatus::Done), // This should cause an error (Done not in states)
        custom_fields: StringConfigField::new_wildcard(),
    };

    let result = validator.validate_global_config(&config);
    assert!(result.has_errors());

    let error_messages: Vec<&str> = result.errors.iter().map(|e| e.message.as_str()).collect();
    assert!(
        error_messages
            .iter()
            .any(|msg| msg.contains("Default priority 'MEDIUM' not found"))
    );
    assert!(
        error_messages
            .iter()
            .any(|msg| msg.contains("Default status 'DONE' not found"))
    );
}

#[test]
fn test_project_config_validation_valid() {
    let temp_dir = TempDir::new().unwrap();
    let validator = ConfigValidator::new(temp_dir.path());

    let config = ProjectConfig {
        project_name: "Test Project".to_string(),
        issue_states: Some(ConfigurableField {
            values: vec![TaskStatus::Todo, TaskStatus::InProgress, TaskStatus::Done],
        }),
        issue_types: Some(ConfigurableField {
            values: vec![TaskType::Feature, TaskType::Bug],
        }),
        issue_priorities: Some(ConfigurableField {
            values: vec![Priority::Low, Priority::Medium, Priority::High],
        }),
        categories: None,
        tags: None,
        default_assignee: Some("user@example.com".to_string()),
        default_priority: Some(Priority::Medium),
        default_status: Some(TaskStatus::Todo),
        custom_fields: None,
    };

    let result = validator.validate_project_config(&config);
    assert!(!result.has_errors());
    assert!(!result.has_warnings());
}

#[test]
fn test_project_config_validation_empty_project_name() {
    let temp_dir = TempDir::new().unwrap();
    let validator = ConfigValidator::new(temp_dir.path());

    let config = ProjectConfig {
        project_name: "".to_string(), // Empty name should trigger error
        issue_states: None,
        issue_types: None,
        issue_priorities: None,
        categories: None,
        tags: None,
        default_assignee: None,
        default_priority: None,
        default_status: None,
        custom_fields: None,
    };

    let result = validator.validate_project_config(&config);
    assert!(result.has_errors());

    let error_messages: Vec<&str> = result.errors.iter().map(|e| e.message.as_str()).collect();
    assert!(
        error_messages
            .iter()
            .any(|msg| msg.contains("Project name cannot be empty"))
    );
}

#[test]
fn test_project_config_validation_long_project_name() {
    let temp_dir = TempDir::new().unwrap();
    let validator = ConfigValidator::new(temp_dir.path());

    let long_name = "a".repeat(150); // Very long name should trigger warning
    let config = ProjectConfig {
        project_name: long_name,
        issue_states: None,
        issue_types: None,
        issue_priorities: None,
        categories: None,
        tags: None,
        default_assignee: None,
        default_priority: None,
        default_status: None,
        custom_fields: None,
    };

    let result = validator.validate_project_config(&config);
    assert!(result.has_warnings());

    let warning_messages: Vec<&str> = result.warnings.iter().map(|e| e.message.as_str()).collect();
    assert!(
        warning_messages
            .iter()
            .any(|msg| msg.contains("Project name is very long"))
    );
}

#[test]
fn test_project_config_validation_invalid_defaults() {
    let temp_dir = TempDir::new().unwrap();
    let validator = ConfigValidator::new(temp_dir.path());

    let config = ProjectConfig {
        project_name: "Test Project".to_string(),
        issue_states: Some(ConfigurableField {
            values: vec![TaskStatus::Todo, TaskStatus::InProgress],
        }),
        issue_types: None,
        issue_priorities: Some(ConfigurableField {
            values: vec![Priority::Low, Priority::High],
        }),
        categories: None,
        tags: None,
        default_assignee: None,
        default_priority: Some(Priority::Medium), // Medium not in priorities list
        default_status: Some(TaskStatus::Done),   // Done not in states list
        custom_fields: None,
    };

    let result = validator.validate_project_config(&config);
    assert!(result.has_errors());

    let error_messages: Vec<&str> = result.errors.iter().map(|e| e.message.as_str()).collect();
    assert!(
        error_messages
            .iter()
            .any(|msg| msg.contains("Default priority 'MEDIUM' not found"))
    );
    assert!(
        error_messages
            .iter()
            .any(|msg| msg.contains("Default status 'DONE' not found"))
    );
}

#[test]
fn test_project_config_validation_invalid_email_format() {
    let temp_dir = TempDir::new().unwrap();
    let validator = ConfigValidator::new(temp_dir.path());

    let config = ProjectConfig {
        project_name: "Test Project".to_string(),
        issue_states: None,
        issue_types: None,
        issue_priorities: None,
        categories: None,
        tags: None,
        default_assignee: Some("invalid-email".to_string()), // Invalid email format
        default_priority: None,
        default_status: None,
        custom_fields: None,
    };

    let result = validator.validate_project_config(&config);
    assert!(result.has_warnings());

    let warning_messages: Vec<&str> = result.warnings.iter().map(|e| e.message.as_str()).collect();
    assert!(
        warning_messages
            .iter()
            .any(|msg| msg.contains("doesn't look like an email"))
    );
}

#[test]
fn test_prefix_format_validation() {
    let temp_dir = TempDir::new().unwrap();
    let validator = ConfigValidator::new(temp_dir.path());

    // Test valid prefix
    let result = validator.validate_prefix_format("ABC");
    assert!(!result.has_errors());
    assert!(!result.has_warnings());

    // Test empty prefix (should warn)
    let result = validator.validate_prefix_format("");
    assert!(result.has_warnings());

    // Test long prefix (should warn)
    let result = validator.validate_prefix_format("VERY_LONG_PREFIX");
    assert!(result.has_warnings());

    // Test invalid characters (should error)
    let result = validator.validate_prefix_format("ABC@123");
    assert!(result.has_errors());
}

#[test]
fn test_prefix_conflict_detection() {
    let temp_dir = TempDir::new().unwrap();

    // Create some existing project directories to test against
    std::fs::create_dir_all(temp_dir.path().join("EXISTING")).unwrap();
    std::fs::create_dir_all(temp_dir.path().join("TEST")).unwrap();

    let validator = ConfigValidator::new(temp_dir.path());

    // Test no conflict
    let result = validator.check_prefix_conflicts("NEWPREFIX");
    assert!(!result.has_errors());
    assert!(!result.has_warnings());

    // Test exact conflict
    let result = validator.check_prefix_conflicts("EXISTING");
    assert!(result.has_errors());

    let error_messages: Vec<&str> = result.errors.iter().map(|e| e.message.as_str()).collect();
    assert!(
        error_messages
            .iter()
            .any(|msg| msg.contains("already exists"))
    );
}

#[test]
fn test_validation_result_display() {
    let mut result = ValidationResult::new();

    // Add an error
    result.add_error(ValidationError::error(
        Some("test_field".to_string()),
        "Test error message".to_string(),
    ));

    // Add a warning
    result.add_error(
        ValidationError::warning(
            Some("other_field".to_string()),
            "Test warning message".to_string(),
        )
        .with_fix("Fix suggestion".to_string()),
    );

    // Test display formatting
    let display_string = format!("{result}");
    assert!(display_string.contains("❌")); // Error emoji
    assert!(display_string.contains("⚠️")); // Warning emoji
    assert!(display_string.contains("Test error message"));
    assert!(display_string.contains("Test warning message"));
    assert!(display_string.contains("Fix suggestion"));
}

#[test]
fn test_validation_result_merge() {
    use lotar::config::validation::errors::{ValidationError, ValidationResult};

    let mut result1 = ValidationResult::new();
    result1.add_error(ValidationError::error(
        Some("field1".to_string()),
        "Error 1".to_string(),
    ));

    let mut result2 = ValidationResult::new();
    result2.add_error(ValidationError::warning(
        Some("field2".to_string()),
        "Warning 1".to_string(),
    ));

    result1.merge(result2);

    assert_eq!(result1.errors.len(), 1);
    assert_eq!(result1.warnings.len(), 1);
    assert!(result1.has_errors());
    assert!(result1.has_warnings());
}

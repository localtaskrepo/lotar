use lotar::config::types::{
    ConfigurableField, GlobalConfig, ProjectConfig, ResolvedConfig, StringConfigField,
};
use lotar::config::validation::errors::{ValidationError, ValidationResult};
use lotar::config::validation::{ConfigValidator, ValidationSeverity};
use lotar::types::{Priority, TaskStatus, TaskType};
use tempfile::TempDir;
// No CLI invocations here; keep this module self-contained

fn base_global_config() -> GlobalConfig {
    GlobalConfig {
        default_prefix: "TEST".to_string(),
        ..GlobalConfig::default()
    }
}

fn base_project_config() -> ProjectConfig {
    let mut config = ProjectConfig::new("Test Project".to_string());
    config.issue_states = Some(ConfigurableField {
        values: vec![TaskStatus::from("Todo"), TaskStatus::from("InProgress")],
    });
    config.issue_types = Some(ConfigurableField {
        values: vec![TaskType::from("Feature"), TaskType::from("Bug")],
    });
    config.issue_priorities = Some(ConfigurableField {
        values: vec![Priority::from("Low"), Priority::from("High")],
    });
    config
}

#[test]
fn test_global_config_validation_valid() {
    let temp_dir = TempDir::new().unwrap();
    let validator = ConfigValidator::new(temp_dir.path());

    let config = GlobalConfig {
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
            values: vec![TaskType::from("Feature"), TaskType::from("Bug")],
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
        default_priority: Priority::from("Medium"),
        default_status: Some(TaskStatus::from("Todo")),
        custom_fields: StringConfigField::new_wildcard(),
        scan_signal_words: vec![
            "TODO".to_string(),
            "FIXME".to_string(),
            "HACK".to_string(),
            "BUG".to_string(),
            "NOTE".to_string(),
        ],
        scan_ticket_patterns: None,
        scan_enable_ticket_words: false,
        scan_enable_mentions: true,
        scan_strip_attributes: true,
        auto_identity: true,
        auto_identity_git: true,
        auto_codeowners_assign: true,
        auto_tags_from_path: true,
        auto_branch_infer_type: true,
        auto_branch_infer_status: true,
        auto_branch_infer_priority: true,
        branch_type_aliases: std::collections::HashMap::new(),
        branch_status_aliases: std::collections::HashMap::new(),
        branch_priority_aliases: std::collections::HashMap::new(),
        sprints: Default::default(),
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
            values: vec![TaskStatus::from("Todo")],
        },
        issue_types: ConfigurableField {
            values: vec![TaskType::from("Feature")],
        },
        issue_priorities: ConfigurableField {
            values: vec![Priority::from("Medium")],
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
        default_priority: Priority::from("Medium"),
        default_status: Some(TaskStatus::from("Todo")),
        custom_fields: StringConfigField::new_wildcard(),
        scan_signal_words: vec![
            "TODO".to_string(),
            "FIXME".to_string(),
            "HACK".to_string(),
            "BUG".to_string(),
            "NOTE".to_string(),
        ],
        scan_ticket_patterns: None,
        scan_enable_ticket_words: false,
        scan_enable_mentions: true,
        scan_strip_attributes: true,
        auto_identity: true,
        auto_identity_git: true,
        auto_codeowners_assign: true,
        auto_tags_from_path: true,
        auto_branch_infer_type: true,
        auto_branch_infer_status: true,
        auto_branch_infer_priority: true,
        branch_type_aliases: std::collections::HashMap::new(),
        branch_status_aliases: std::collections::HashMap::new(),
        branch_priority_aliases: std::collections::HashMap::new(),
        sprints: Default::default(),
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
        tags: StringConfigField::new_wildcard(),
        default_assignee: None,
        default_reporter: None,
        default_tags: vec![],
        members: vec![],
        strict_members: false,
        auto_populate_members: true,
        auto_set_reporter: true,
        auto_assign_on_status: true,
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
        scan_ticket_patterns: None,
        scan_enable_ticket_words: false,
        scan_enable_mentions: true,
        scan_strip_attributes: true,
        auto_identity: true,
        auto_identity_git: true,
        auto_codeowners_assign: true,
        auto_tags_from_path: true,
        auto_branch_infer_type: true,
        auto_branch_infer_status: true,
        auto_branch_infer_priority: true,
        branch_type_aliases: std::collections::HashMap::new(),
        branch_status_aliases: std::collections::HashMap::new(),
        branch_priority_aliases: std::collections::HashMap::new(),
        sprints: Default::default(),
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
            values: vec![TaskStatus::from("Todo"), TaskStatus::from("InProgress")],
        },
        issue_types: ConfigurableField {
            values: vec![TaskType::from("Feature")],
        },
        issue_priorities: ConfigurableField {
            values: vec![Priority::from("Low")], // Default priority is Medium but only Low is in list
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
        default_priority: Priority::from("Medium"), // This should cause an error
        default_status: Some(TaskStatus::from("Done")), // This should cause an error (Done not in states)
        custom_fields: StringConfigField::new_wildcard(),
        scan_signal_words: vec![
            "TODO".to_string(),
            "FIXME".to_string(),
            "HACK".to_string(),
            "BUG".to_string(),
            "NOTE".to_string(),
        ],
        scan_ticket_patterns: None,
        scan_enable_ticket_words: false,
        scan_enable_mentions: true,
        scan_strip_attributes: true,
        auto_identity: true,
        auto_identity_git: true,
        auto_codeowners_assign: true,
        auto_tags_from_path: true,
        auto_branch_infer_type: true,
        auto_branch_infer_status: true,
        auto_branch_infer_priority: true,
        branch_type_aliases: std::collections::HashMap::new(),
        branch_status_aliases: std::collections::HashMap::new(),
        branch_priority_aliases: std::collections::HashMap::new(),
        sprints: Default::default(),
    };

    let result = validator.validate_global_config(&config);
    assert!(!result.has_errors());
    assert!(result.has_warnings());

    let warning_messages: Vec<&str> = result.warnings.iter().map(|w| w.message.as_str()).collect();
    assert!(
        warning_messages
            .iter()
            .any(|msg| msg.contains("Default priority 'Medium' not found"))
    );
    assert!(
        warning_messages
            .iter()
            .any(|msg| msg.contains("Default status 'Done' not found"))
    );
}

#[test]
fn test_global_config_duplicate_entries_warning() {
    let temp_dir = TempDir::new().unwrap();
    let validator = ConfigValidator::new(temp_dir.path());

    let config = GlobalConfig {
        server_port: 8080,
        default_prefix: "TEST".to_string(),
        issue_states: ConfigurableField {
            values: vec![TaskStatus::from("Todo"), TaskStatus::from("todo")],
        },
        issue_types: ConfigurableField {
            values: vec![TaskType::from("Feature"), TaskType::from("feature")],
        },
        issue_priorities: ConfigurableField {
            values: vec![Priority::from("Low"), Priority::from("Medium")],
        },
        tags: StringConfigField::new_wildcard(),
        default_assignee: None,
        default_reporter: None,
        default_tags: vec!["tag".to_string(), "Tag".to_string()],
        members: vec![],
        strict_members: false,
        auto_populate_members: true,
        auto_set_reporter: true,
        auto_assign_on_status: true,
        default_priority: Priority::from("Medium"),
        default_status: Some(TaskStatus::from("Todo")),
        custom_fields: StringConfigField::new_wildcard(),
        scan_signal_words: vec!["TODO".to_string(), "todo".to_string()],
        scan_ticket_patterns: Some(vec!["PROJ-[0-9]+".to_string(), "proj-[0-9]+".to_string()]),
        scan_enable_ticket_words: false,
        scan_enable_mentions: true,
        scan_strip_attributes: true,
        auto_identity: true,
        auto_identity_git: true,
        auto_codeowners_assign: true,
        auto_tags_from_path: true,
        auto_branch_infer_type: true,
        auto_branch_infer_status: true,
        auto_branch_infer_priority: true,
        branch_type_aliases: std::collections::HashMap::new(),
        branch_status_aliases: std::collections::HashMap::new(),
        branch_priority_aliases: std::collections::HashMap::new(),
        sprints: Default::default(),
    };

    let result = validator.validate_global_config(&config);
    assert!(result.has_warnings());
    let warning_messages: Vec<&str> = result.warnings.iter().map(|w| w.message.as_str()).collect();
    assert!(
        warning_messages
            .iter()
            .any(|msg| msg.contains("Duplicate value 'todo'"))
    );
}

#[test]
fn test_global_config_rejects_default_tags_outside_allowed_list() {
    let temp_dir = TempDir::new().unwrap();
    let validator = ConfigValidator::new(temp_dir.path());

    let mut config = base_global_config();
    config.tags = StringConfigField {
        values: vec!["frontend".to_string(), "backend".to_string()],
    };
    config.default_tags = vec!["frontend".to_string(), "ops".to_string()];

    let result = validator.validate_global_config(&config);
    assert!(result.has_errors());
    assert!(result.errors.iter().any(|err| {
        err.message
            .contains("default_tags contains values not present")
    }));
}

#[test]
fn test_project_config_rejects_default_tags_outside_override() {
    let temp_dir = TempDir::new().unwrap();
    let validator = ConfigValidator::new(temp_dir.path());

    let mut config = base_project_config();
    config.tags = Some(StringConfigField {
        values: vec!["frontend".to_string()],
    });
    config.default_tags = Some(vec!["backend".to_string()]);

    let result = validator.validate_project_config(&config);
    assert!(result.has_errors());
    assert!(result.errors.iter().any(|err| {
        err.message
            .contains("default_tags contains values not present")
    }));
}

#[test]
fn test_resolved_config_rejects_default_tags_outside_allowed_list() {
    let temp_dir = TempDir::new().unwrap();
    let validator = ConfigValidator::new(temp_dir.path());

    let mut global = base_global_config();
    global.tags = StringConfigField {
        values: vec!["frontend".to_string()],
    };
    global.default_tags = vec!["frontend".to_string()];
    let mut resolved = ResolvedConfig::from_global(global);
    resolved.default_tags = vec!["ops".to_string()];

    let result = validator.validate_resolved_config(&resolved);
    assert!(result.has_errors());
    assert!(result.errors.iter().any(|err| {
        err.message
            .contains("default_tags contains values not present")
    }));
}

#[test]
fn test_global_config_requires_members_when_strict() {
    let temp_dir = TempDir::new().unwrap();
    let validator = ConfigValidator::new(temp_dir.path());

    let mut config = base_global_config();
    config.strict_members = true;
    config.members.clear();

    let result = validator.validate_global_config(&config);
    assert!(result.has_errors());
    assert!(
        result
            .errors
            .iter()
            .any(|err| err.message.contains("strict_members is enabled globally"))
    );
}

#[test]
fn test_project_config_requires_members_when_strict_and_empty() {
    let temp_dir = TempDir::new().unwrap();
    let validator = ConfigValidator::new(temp_dir.path());

    let mut config = base_project_config();
    config.strict_members = Some(true);
    config.members = Some(vec!["   ".to_string()]);

    let result = validator.validate_project_config(&config);
    assert!(result.has_errors());
    assert!(
        result
            .errors
            .iter()
            .any(|err| err.message.contains("strict_members is enabled"))
    );
}

#[test]
fn test_resolved_config_requires_members_when_strict() {
    let temp_dir = TempDir::new().unwrap();
    let validator = ConfigValidator::new(temp_dir.path());

    let mut resolved = ResolvedConfig::from_global(base_global_config());
    resolved.strict_members = true;
    resolved.members.clear();

    let result = validator.validate_resolved_config(&resolved);
    assert!(result.has_errors());
    assert!(result.errors.iter().any(|err| {
        err.message
            .contains("strict_members is enabled but no members remain")
    }));
}

#[test]
fn test_global_config_alias_targets_must_exist() {
    let temp_dir = TempDir::new().unwrap();
    let validator = ConfigValidator::new(temp_dir.path());

    let mut config = base_global_config();
    config.issue_states = ConfigurableField {
        values: vec![TaskStatus::from("Todo")],
    };
    let mut aliases = std::collections::HashMap::new();
    aliases.insert("wip".to_string(), TaskStatus::from("InProgress"));
    config.branch_status_aliases = aliases;

    let result = validator.validate_global_config(&config);
    assert!(result.has_errors());
    assert!(
        result
            .errors
            .iter()
            .any(|err| err.message.contains("Alias 'wip' maps to 'InProgress'"))
    );
}

#[test]
fn test_resolved_config_alias_targets_must_exist() {
    let temp_dir = TempDir::new().unwrap();
    let validator = ConfigValidator::new(temp_dir.path());

    let mut resolved = ResolvedConfig::from_global(base_global_config());
    resolved.issue_types = ConfigurableField {
        values: vec![TaskType::from("Feature")],
    };
    resolved
        .branch_type_aliases
        .insert("chore".to_string(), TaskType::from("Chore"));

    let result = validator.validate_resolved_config(&resolved);
    assert!(result.has_errors());
    assert!(
        result
            .errors
            .iter()
            .any(|err| err.message.contains("Alias 'chore' maps to 'Chore'"))
    );
}

#[test]
fn test_project_config_validation_valid() {
    let temp_dir = TempDir::new().unwrap();
    let validator = ConfigValidator::new(temp_dir.path());

    let mut config = base_project_config();
    config.issue_states = Some(ConfigurableField {
        values: vec![
            TaskStatus::from("Todo"),
            TaskStatus::from("InProgress"),
            TaskStatus::from("Done"),
        ],
    });
    config.issue_priorities = Some(ConfigurableField {
        values: vec![
            Priority::from("Low"),
            Priority::from("Medium"),
            Priority::from("High"),
        ],
    });
    config.default_assignee = Some("user@example.com".to_string());
    config.default_priority = Some(Priority::from("Medium"));
    config.default_status = Some(TaskStatus::from("Todo"));

    let result = validator.validate_project_config(&config);
    assert!(!result.has_errors());
    assert!(!result.has_warnings());
}

#[test]
fn test_project_config_duplicate_overrides_warning() {
    let temp_dir = TempDir::new().unwrap();
    let validator = ConfigValidator::new(temp_dir.path());

    let mut config = base_project_config();
    config.project_name = "Demo".to_string();
    config.issue_states = Some(ConfigurableField {
        values: vec![TaskStatus::from("Todo"), TaskStatus::from("todo")],
    });
    config.issue_types = Some(ConfigurableField {
        values: vec![TaskType::from("Feature"), TaskType::from("feature")],
    });
    config.issue_priorities = Some(ConfigurableField {
        values: vec![Priority::from("Low"), Priority::from("low")],
    });
    config.tags = Some(StringConfigField {
        values: vec!["UI".to_string(), "ui".to_string()],
    });
    config.default_tags = Some(vec!["tag".to_string(), "TAG".to_string()]);
    config.default_priority = Some(Priority::from("Low"));
    config.default_status = Some(TaskStatus::from("Todo"));
    config.custom_fields = Some(StringConfigField {
        values: vec!["field".to_string(), "Field".to_string()],
    });
    config.scan_signal_words = Some(vec!["TODO".to_string(), "todo".to_string()]);
    config.scan_ticket_patterns = Some(vec!["PROJ-[0-9]+".to_string(), "proj-[0-9]+".to_string()]);

    let result = validator.validate_project_config(&config);
    assert!(result.has_warnings());
    let warning_messages: Vec<&str> = result.warnings.iter().map(|w| w.message.as_str()).collect();
    assert!(
        warning_messages
            .iter()
            .any(|msg| msg.contains("Duplicate value 'todo'"))
    );
}

#[test]
fn test_project_config_empty_override_errors() {
    let temp_dir = TempDir::new().unwrap();
    let validator = ConfigValidator::new(temp_dir.path());

    let mut config = base_project_config();
    config.project_name = "Demo".to_string();
    config.issue_states = Some(ConfigurableField { values: vec![] });
    config.issue_types = Some(ConfigurableField { values: vec![] });
    config.issue_priorities = Some(ConfigurableField { values: vec![] });

    let result = validator.validate_project_config(&config);
    assert!(result.has_errors());
    let messages: Vec<&str> = result.errors.iter().map(|e| e.message.as_str()).collect();
    assert!(
        messages
            .iter()
            .any(|msg| msg.contains("Issue states override cannot be empty"))
    );
    assert!(
        messages
            .iter()
            .any(|msg| msg.contains("Issue types override cannot be empty"))
    );
    assert!(
        messages
            .iter()
            .any(|msg| msg.contains("Issue priorities override cannot be empty"))
    );
}

#[test]
fn test_project_config_validation_empty_project_name() {
    let temp_dir = TempDir::new().unwrap();
    let validator = ConfigValidator::new(temp_dir.path());

    let mut config = base_project_config();
    config.project_name = "".to_string(); // Empty name should trigger error

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
fn test_project_config_validation_whitespace_project_name() {
    let temp_dir = TempDir::new().unwrap();
    let validator = ConfigValidator::new(temp_dir.path());

    let mut config = base_project_config();
    config.project_name = "   ".to_string();

    let result = validator.validate_project_config(&config);
    assert!(result.has_errors());

    let messages: Vec<&str> = result.errors.iter().map(|e| e.message.as_str()).collect();
    assert!(
        messages
            .iter()
            .any(|msg| msg.contains("Project name cannot be empty"))
    );
}

#[test]
fn test_project_config_validation_long_project_name() {
    let temp_dir = TempDir::new().unwrap();
    let validator = ConfigValidator::new(temp_dir.path());

    let long_name = "a".repeat(150); // Very long name should trigger warning
    let mut config = base_project_config();
    config.project_name = long_name;

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

    let mut config = base_project_config();
    config.default_priority = Some(Priority::from("Medium")); // Medium not in priorities list
    config.default_status = Some(TaskStatus::from("Done")); // Done not in states list

    let result = validator.validate_project_config(&config);
    assert!(!result.has_errors());
    assert!(result.has_warnings());

    let warning_messages: Vec<&str> = result.warnings.iter().map(|w| w.message.as_str()).collect();
    assert!(
        warning_messages
            .iter()
            .any(|msg| msg.contains("Default priority 'Medium' not found"))
    );
    assert!(
        warning_messages
            .iter()
            .any(|msg| msg.contains("Default status 'Done' not found"))
    );
}

#[test]
fn test_project_config_validation_invalid_email_format() {
    let temp_dir = TempDir::new().unwrap();
    let validator = ConfigValidator::new(temp_dir.path());

    let mut config = base_project_config();
    config.default_assignee = Some("invalid-email".to_string()); // Invalid email format
    config.default_reporter = Some("invalid-reporter".to_string());
    config.issue_states = None;
    config.issue_types = None;
    config.issue_priorities = None;
    config.default_priority = None;
    config.default_status = None;

    let result = validator.validate_project_config(&config);
    assert!(result.has_warnings());

    let warning_messages: Vec<&str> = result.warnings.iter().map(|e| e.message.as_str()).collect();
    assert!(
        warning_messages
            .iter()
            .any(|msg| msg.contains("Assignee format doesn't look like an email"))
    );
    assert!(
        warning_messages
            .iter()
            .any(|msg| msg.contains("Reporter format doesn't look like an email"))
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

    // Test prefix exceeding max length (should error)
    let result = validator.validate_prefix_format("THIS_PREFIX_IS_MUCH_TOO_LONG");
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

#[test]
fn config_custom_fields_collision_rejected_global_and_project() {
    use lotar::config::operations::{save_global_config, save_project_config, update_config_field};
    let temp_dir = TempDir::new().unwrap();
    let tasks_dir = temp_dir.path();

    // Start with clean configs
    let global = GlobalConfig::default();
    save_global_config(tasks_dir, &global).unwrap();

    // Global: attempt to set custom_fields with a reserved name
    let err = update_config_field(tasks_dir, "custom_fields", "status,foo,bar", None)
        .expect_err("expected collision error");
    match err {
        lotar::config::types::ConfigError::ParseError(msg) => {
            assert!(
                msg.contains("collides with built-in"),
                "unexpected msg: {msg}"
            );
        }
        other => panic!("unexpected error type: {other:?}"),
    }

    // Project: attempt the same
    let project_prefix = "DEMO";
    let proj = ProjectConfig::new("Demo".into());
    save_project_config(tasks_dir, project_prefix, &proj).unwrap();
    let err = update_config_field(
        tasks_dir,
        "custom_fields",
        "Due_Date,baz",
        Some(project_prefix),
    )
    .expect_err("expected collision error");
    match err {
        lotar::config::types::ConfigError::ParseError(msg) => {
            assert!(
                msg.contains("collides with built-in"),
                "unexpected msg: {msg}"
            );
        }
        other => panic!("unexpected error type: {other:?}"),
    }
}

use lotar::cli::handlers::test_support as add_test_support;
mod common;
use common::env_mutex::EnvVarGuard;
use lotar::config::types::{ConfigurableField, ResolvedConfig, StringConfigField};
use lotar::types::{Priority, TaskStatus, TaskType};

fn quiet() -> EnvVarGuard {
    // Ensure warnings are silenced during these unit tests
    EnvVarGuard::set("LOTAR_TEST_SILENT", "1")
}

fn make_cfg(
    priorities: Vec<Priority>,
    default_priority: Priority,
    statuses: Vec<TaskStatus>,
    default_status: Option<TaskStatus>,
) -> ResolvedConfig {
    ResolvedConfig {
        server_port: 8080,
        default_project: "TEST".to_string(),
        issue_states: ConfigurableField { values: statuses },
        issue_types: ConfigurableField {
            values: vec![TaskType::from("Feature"), TaskType::from("Bug")],
        },
        issue_priorities: ConfigurableField { values: priorities },
        tags: StringConfigField {
            values: vec!["*".to_string()],
        },
        default_assignee: None,
        default_reporter: None,
        default_tags: vec![],
        members: vec![],
        strict_members: false,
        auto_populate_members: true,
        auto_set_reporter: true,
        auto_assign_on_status: true,
        auto_codeowners_assign: true,
        default_priority,
        default_status,
        custom_fields: StringConfigField {
            values: vec!["*".to_string()],
        },
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
fn smart_default_basic_string_cases() {
    let _silent = quiet();
    let project_values = vec!["Alpha".to_string(), "Beta".to_string(), "Gamma".to_string()];
    let global_default = "Beta".to_string();

    // 1: explicit exists
    let explicit = Some("Gamma".to_string());
    let got = add_test_support::smart_default_string(
        explicit.as_ref(),
        &global_default,
        &project_values,
        "test_field",
    )
    .unwrap();
    assert_eq!(got, "Gamma");

    // 2: no explicit, use global
    let got = add_test_support::smart_default_string(
        None,
        &global_default,
        &project_values,
        "test_field",
    )
    .unwrap();
    assert_eq!(got, "Beta");

    // 3: global not in values -> first
    let global2 = "Delta".to_string();
    let got = add_test_support::smart_default_string(None, &global2, &project_values, "test_field")
        .unwrap();
    assert_eq!(got, "Alpha");

    // 4: explicit invalid -> global
    let explicit2 = Some("Zeta".to_string());
    let got = add_test_support::smart_default_string(
        explicit2.as_ref(),
        &global_default,
        &project_values,
        "test_field",
    )
    .unwrap();
    assert_eq!(got, "Beta");

    // 5: empty values -> error
    let empty: Vec<String> = vec![];
    let err = add_test_support::smart_default_string(None, &global_default, &empty, "test_field")
        .unwrap_err();
    assert!(err.contains("configuration error"));
}

#[test]
fn default_priority_scenarios() {
    let _silent = quiet();
    // 1: global default present
    let cfg = make_cfg(
        vec![
            Priority::from("Low"),
            Priority::from("Medium"),
            Priority::from("High"),
        ],
        Priority::from("Medium"),
        vec![
            TaskStatus::from("Todo"),
            TaskStatus::from("InProgress"),
            TaskStatus::from("Done"),
        ],
        None,
    );
    assert_eq!(
        add_test_support::default_priority(&cfg),
        Priority::from("Medium")
    );

    // 2: global default not in project -> first
    let cfg = make_cfg(
        vec![Priority::from("Critical"), Priority::from("High")],
        Priority::from("Medium"),
        vec![TaskStatus::from("Todo"), TaskStatus::from("InProgress")],
        None,
    );
    assert_eq!(
        add_test_support::default_priority(&cfg),
        Priority::from("Critical")
    );

    // 3: global default Low exists -> Low
    let cfg = make_cfg(
        vec![
            Priority::from("High"),
            Priority::from("Medium"),
            Priority::from("Low"),
        ],
        Priority::from("Low"),
        vec![TaskStatus::from("Todo")],
        None,
    );
    assert_eq!(
        add_test_support::default_priority(&cfg),
        Priority::from("Low")
    );
}

#[test]
fn default_status_scenarios() {
    let _silent = quiet();
    // 1: explicit present
    let cfg = make_cfg(
        vec![Priority::from("Medium")],
        Priority::from("Medium"),
        vec![
            TaskStatus::from("Todo"),
            TaskStatus::from("InProgress"),
            TaskStatus::from("Done"),
        ],
        Some(TaskStatus::from("InProgress")),
    );
    assert_eq!(
        add_test_support::default_status(&cfg),
        TaskStatus::from("InProgress")
    );

    // 2: no explicit -> first
    let cfg = make_cfg(
        vec![Priority::from("Medium")],
        Priority::from("Medium"),
        vec![
            TaskStatus::from("Todo"),
            TaskStatus::from("InProgress"),
            TaskStatus::from("Done"),
        ],
        None,
    );
    assert_eq!(
        add_test_support::default_status(&cfg),
        TaskStatus::from("Todo")
    );

    // 3: explicit invalid -> first
    let cfg = make_cfg(
        vec![Priority::from("Medium")],
        Priority::from("Medium"),
        vec![TaskStatus::from("InProgress"), TaskStatus::from("Done")],
        Some(TaskStatus::from("Todo")),
    );
    assert_eq!(
        add_test_support::default_status(&cfg),
        TaskStatus::from("InProgress")
    );

    // 4: variety
    let cfg = make_cfg(
        vec![Priority::from("Medium")],
        Priority::from("Medium"),
        vec![TaskStatus::from("Verify"), TaskStatus::from("Blocked")],
        Some(TaskStatus::from("Verify")),
    );
    assert_eq!(
        add_test_support::default_status(&cfg),
        TaskStatus::from("Verify")
    );
}

#[test]
fn edge_case_single_values() {
    let _silent = quiet();
    let cfg = make_cfg(
        vec![Priority::from("Critical")],
        Priority::from("Medium"),
        vec![TaskStatus::from("Todo")],
        None,
    );

    assert_eq!(
        add_test_support::default_priority(&cfg),
        Priority::from("Critical")
    );
    assert_eq!(
        add_test_support::default_status(&cfg),
        TaskStatus::from("Todo")
    );
}

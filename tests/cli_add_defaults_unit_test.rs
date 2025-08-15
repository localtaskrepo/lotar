use lotar::cli::handlers::test_support as add_test_support;
use lotar::config::types::{ConfigurableField, ResolvedConfig, StringConfigField};
use lotar::types::{Priority, TaskStatus, TaskType};

fn quiet() {
    // Ensure warnings are silenced during these unit tests
    unsafe { std::env::set_var("LOTAR_TEST_SILENT", "1") };
}

fn make_cfg(
    priorities: Vec<Priority>,
    default_priority: Priority,
    statuses: Vec<TaskStatus>,
    default_status: Option<TaskStatus>,
) -> ResolvedConfig {
    ResolvedConfig {
        server_port: 8080,
        default_prefix: "TEST".to_string(),
        issue_states: ConfigurableField { values: statuses },
        issue_types: ConfigurableField {
            values: vec![TaskType::Feature, TaskType::Bug],
        },
        issue_priorities: ConfigurableField { values: priorities },
        categories: StringConfigField {
            values: vec!["*".to_string()],
        },
        tags: StringConfigField {
            values: vec!["*".to_string()],
        },
        default_assignee: None,
        default_reporter: None,
        default_category: None,
        default_tags: vec![],
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
        auto_identity: true,
        auto_identity_git: true,
        auto_tags_from_path: true,
    }
}

#[test]
fn smart_default_basic_string_cases() {
    quiet();
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
    quiet();
    // 1: global default present
    let cfg = make_cfg(
        vec![Priority::Low, Priority::Medium, Priority::High],
        Priority::Medium,
        vec![TaskStatus::Todo, TaskStatus::InProgress, TaskStatus::Done],
        None,
    );
    assert_eq!(add_test_support::default_priority(&cfg), Priority::Medium);

    // 2: global default not in project -> first
    let cfg = make_cfg(
        vec![Priority::Critical, Priority::High],
        Priority::Medium,
        vec![TaskStatus::Todo, TaskStatus::InProgress],
        None,
    );
    assert_eq!(add_test_support::default_priority(&cfg), Priority::Critical);

    // 3: global default Low exists -> Low
    let cfg = make_cfg(
        vec![Priority::High, Priority::Medium, Priority::Low],
        Priority::Low,
        vec![TaskStatus::Todo],
        None,
    );
    assert_eq!(add_test_support::default_priority(&cfg), Priority::Low);
}

#[test]
fn default_status_scenarios() {
    quiet();
    // 1: explicit present
    let cfg = make_cfg(
        vec![Priority::Medium],
        Priority::Medium,
        vec![TaskStatus::Todo, TaskStatus::InProgress, TaskStatus::Done],
        Some(TaskStatus::InProgress),
    );
    assert_eq!(
        add_test_support::default_status(&cfg),
        TaskStatus::InProgress
    );

    // 2: no explicit -> first
    let cfg = make_cfg(
        vec![Priority::Medium],
        Priority::Medium,
        vec![TaskStatus::Todo, TaskStatus::InProgress, TaskStatus::Done],
        None,
    );
    assert_eq!(add_test_support::default_status(&cfg), TaskStatus::Todo);

    // 3: explicit invalid -> first
    let cfg = make_cfg(
        vec![Priority::Medium],
        Priority::Medium,
        vec![TaskStatus::InProgress, TaskStatus::Done],
        Some(TaskStatus::Todo),
    );
    assert_eq!(
        add_test_support::default_status(&cfg),
        TaskStatus::InProgress
    );

    // 4: variety
    let cfg = make_cfg(
        vec![Priority::Medium],
        Priority::Medium,
        vec![TaskStatus::Verify, TaskStatus::Blocked],
        Some(TaskStatus::Verify),
    );
    assert_eq!(add_test_support::default_status(&cfg), TaskStatus::Verify);
}

#[test]
fn edge_case_single_values() {
    quiet();
    let cfg = make_cfg(
        vec![Priority::Critical],
        Priority::Medium,
        vec![TaskStatus::Todo],
        None,
    );

    assert_eq!(add_test_support::default_priority(&cfg), Priority::Critical);
    assert_eq!(add_test_support::default_status(&cfg), TaskStatus::Todo);
}

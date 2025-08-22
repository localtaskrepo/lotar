#[test]
fn task_yaml_parse_proper_case_succeeds() {
    let yml = r#"
title: Sample
status: Todo
priority: Medium
task_type: Feature
created: 2025-08-01T10:00:00Z
modified: 2025-08-01T10:00:00Z
"#;
    let t: Result<lotar::Task, _> = serde_yaml::from_str(yml);
    assert!(t.is_ok(), "Expected proper-cased YAML to parse into Task");
}

#[test]
fn task_yaml_parse_uppercase_status_fails() {
    // Uppercase enum strings are commonly present in legacy/manual files
    // Derive(Deserialize) expects the Rust variant names (e.g., Todo), so this should fail.
    let yml = r#"
title: Sample
status: TODO
priority: Medium
task_type: Feature
created: 2025-08-01T10:00:00Z
modified: 2025-08-01T10:00:00Z
"#;
    let t: Result<lotar::Task, _> = serde_yaml::from_str(yml);
    assert!(
        t.is_err(),
        "Uppercase status should not parse with derived enum deserializer"
    );
}

#[test]
fn task_yaml_parse_uppercase_priority_fails() {
    let yml = r#"
title: Sample
status: Todo
priority: MEDIUM
task_type: Feature
created: 2025-08-01T10:00:00Z
modified: 2025-08-01T10:00:00Z
"#;
    let t: Result<lotar::Task, _> = serde_yaml::from_str(yml);
    assert!(
        t.is_err(),
        "Uppercase priority should not parse with derived enum deserializer"
    );
}

#[test]
fn task_yaml_missing_created_or_modified_fails() {
    // created/modified are required String fields in Task
    let missing_created = r#"
title: Sample
status: Todo
priority: Medium
task_type: Feature
modified: 2025-08-01T10:00:00Z
"#;
    let t1: Result<lotar::Task, _> = serde_yaml::from_str(missing_created);
    assert!(t1.is_err(), "Missing 'created' should fail to deserialize");

    let missing_modified = r#"
title: Sample
status: Todo
priority: Medium
task_type: Feature
created: 2025-08-01T10:00:00Z
"#;
    let t2: Result<lotar::Task, _> = serde_yaml::from_str(missing_modified);
    assert!(t2.is_err(), "Missing 'modified' should fail to deserialize");
}

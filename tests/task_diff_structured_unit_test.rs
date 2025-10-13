#[test]
fn structured_diff_detects_title_change_when_yaml_parses() {
    // Simulate two valid Task snapshots as YAML and confirm serde can parse.
    let prev = r#"
title: Old Title
status: Todo
priority: Medium
task_type: Feature
created: 2025-08-01T10:00:00Z
modified: 2025-08-01T10:00:00Z
"#;
    let cur = r#"
title: New Title
status: Todo
priority: Medium
task_type: Feature
created: 2025-08-01T10:00:00Z
modified: 2025-08-02T10:00:00Z
"#;
    let prev_task: lotar::Task = serde_yaml::from_str(prev).expect("prev task should parse");
    let cur_task: lotar::Task = serde_yaml::from_str(cur).expect("cur task should parse");

    assert_ne!(prev_task.title, cur_task.title);
}

#[test]
fn structured_diff_handles_uppercase_enum_values() {
    // Uppercase enum values should still parse and allow structured diffs to run
    let prev = r#"
title: Keep Title
status: TODO
priority: Medium
task_type: Feature
created: 2025-08-01T10:00:00Z
modified: 2025-08-01T10:00:00Z
"#;
    let cur = r#"
title: Keep Title
status: TODO
priority: Medium
task_type: Feature
created: 2025-08-01T10:00:00Z
modified: 2025-08-02T10:00:00Z
"#;
    let prev_task: lotar::Task =
        serde_yaml::from_str(prev).expect("Uppercase enums should parse for previous task");
    let cur_task: lotar::Task =
        serde_yaml::from_str(cur).expect("Uppercase enums should parse for current task");

    assert_eq!(prev_task.status.as_str(), "TODO");
    assert_eq!(cur_task.status.as_str(), "TODO");
}

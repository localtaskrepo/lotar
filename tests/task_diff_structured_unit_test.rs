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
fn structured_diff_wont_detect_if_yaml_doesnt_parse() {
    // Uppercase enum prevents parsing → indicates need for fallback path for diffs
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
    let prev_task: Result<lotar::Task, _> = serde_yaml::from_str(prev);
    let cur_task: Result<lotar::Task, _> = serde_yaml::from_str(cur);

    assert!(
        prev_task.is_err() || cur_task.is_err(),
        "At least one snapshot fails to parse, so typed structured diff cannot run."
    );
}

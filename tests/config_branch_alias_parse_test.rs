use lotar::config::normalization::parse_global_from_yaml_str;

#[test]
fn parses_branch_alias_maps() {
    let cfg_yaml = r#"default.project: TEST
issue.states: [Todo, InProgress, Done]
issue.types: [Feature, Bug]
issue.priorities: [Low, Medium, High, Critical]
issue.tags: ['*']
branch:
  status_aliases: { wip: InProgress }
  priority_aliases: { hotfix: Critical }
  type_aliases: { feat: Feature }
"#;
    let cfg = parse_global_from_yaml_str(cfg_yaml).expect("parse");
    assert_eq!(
        cfg.branch_status_aliases.get("wip").unwrap().to_string(),
        "IN_PROGRESS"
    );
    assert_eq!(
        cfg.branch_priority_aliases
            .get("hotfix")
            .unwrap()
            .to_string(),
        "CRITICAL"
    );
    // TaskType Display renders lowercase variant names (e.g., "feature").
    assert_eq!(
        cfg.branch_type_aliases.get("feat").unwrap().to_string(),
        "feature"
    );
}

use lotar::config::{
    manager::ConfigManager,
    normalization::{
        parse_global_from_yaml_str, parse_project_from_yaml_str, to_canonical_global_yaml,
        to_canonical_project_yaml,
    },
    persistence,
    types::{GlobalConfig, ProjectConfig},
};
use lotar::workspace::{TasksDirectoryResolver, TasksDirectorySource};

const FLAT_CONFIG: &str = r#"
server_port: 8123
default_project: DEV
default_assignee: alice
default_reporter: bob
default_tags: [one]
default_priority: High
default_status: Doing
issue_states: [Ready, Doing, Finished]
issue_types: [Bug, Chore]
issue_priorities: [Low, High]
tags: [one, two]
custom_fields: [team]
members: [alice, bob]
strict_members: true
auto_populate_members: false
auto_set_reporter: false
auto_assign_on_status: false
auto_codeowners_assign: false
auto_tags_from_path: false
auto_branch_infer_type: false
auto_branch_infer_status: false
auto_branch_infer_priority: false
auto_identity: false
auto_identity_git: false
scan_signal_words: [FIXME]
scan_ticket_patterns: ['DEV-\d+']
scan_enable_ticket_words: true
scan_enable_mentions: false
scan_strip_attributes: false
branch_type_aliases: {fix: Bug}
branch_status_aliases: {wip: Doing}
branch_priority_aliases: {urgent: High}
attachments_dir: custom-attachments
attachments_max_upload_mb: 23
sync_reports_dir: custom-reports
sync_write_reports: false
agent_context_enabled: false
agent_context_extension: .notes
agent_logs_dir: debug-logs
agent_instructions: Review carefully
agents: {worker: codex}
agent_automation: {on_start: {set_status: Doing}}
agent_worktree: {enabled: true, dir: custom-worktrees}
web_ui_path: custom-ui
sprints:
  defaults: {capacity_points: 21, capacity_hours: 40, length: 2w, overdue_after: 1d}
  notifications: {enabled: false}
remotes:
  example: {provider: github, project: example/repo}
auth_profiles:
  example: {provider: github, method: token, token_env: FAKE_TEST_TOKEN}
"#;

#[test]
fn every_shipped_flat_field_survives_parse_and_canonical_roundtrip() {
    let expected: GlobalConfig = serde_yaml_ng::from_str(FLAT_CONFIG).unwrap();
    let parsed = parse_global_from_yaml_str(FLAT_CONFIG).unwrap();
    assert_eq!(
        serde_json::to_value(&parsed).unwrap(),
        serde_json::to_value(&expected).unwrap()
    );
    let reloaded = parse_global_from_yaml_str(&to_canonical_global_yaml(&parsed)).unwrap();
    assert_eq!(
        serde_json::to_value(&reloaded).unwrap(),
        serde_json::to_value(&expected).unwrap()
    );

    let project_yaml = format!("project_name: Full Project Name\n{FLAT_CONFIG}");
    let expected: ProjectConfig = serde_yaml_ng::from_str(&project_yaml).unwrap();
    let parsed = parse_project_from_yaml_str("DEV", &project_yaml).unwrap();
    assert_eq!(
        serde_json::to_value(&parsed).unwrap(),
        serde_json::to_value(&expected).unwrap()
    );
    let reloaded = parse_project_from_yaml_str("DEV", &to_canonical_project_yaml(&parsed)).unwrap();
    assert_eq!(
        serde_json::to_value(&reloaded).unwrap(),
        serde_json::to_value(&expected).unwrap()
    );
}

#[test]
fn canonical_values_win_over_flat_values_in_either_order() {
    let flat = "default_project: OLD\ndefault_assignee: alice\nserver_port: 8123\nscan_enable_mentions: false\nagent_logs_dir: old\n";
    let canonical = "default: {project: NEW, assignee: bob}\nserver.port: 8124\nscan: {enable_mentions: true}\nagent.logs_dir: new\n";
    for yaml in [format!("{flat}{canonical}"), format!("{canonical}{flat}")] {
        let cfg = parse_global_from_yaml_str(&yaml).unwrap();
        assert_eq!(cfg.default_project, "NEW");
        assert_eq!(cfg.default_assignee.as_deref(), Some("bob"));
        assert_eq!(cfg.server_port, 8124);
        assert!(cfg.scan_enable_mentions);
        assert_eq!(cfg.agent_logs_dir.as_deref(), Some("new"));
        let cfg = parse_project_from_yaml_str("DEV", &yaml).unwrap();
        assert_eq!(cfg.default_assignee.as_deref(), Some("bob"));
        assert_eq!(cfg.scan_enable_mentions, Some(true));
        assert_eq!(cfg.agent_logs_dir.as_deref(), Some("new"));
    }
}

#[test]
fn flat_defaults_are_effective_and_preserved_when_default_project_is_set() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join(".tasks");
    std::fs::create_dir_all(root.join("DEV")).unwrap();
    std::fs::write(
        root.join("DEV/config.yml"),
        "project: {name: Development}\n",
    )
    .unwrap();
    let path = root.join("config.yml");
    std::fs::write(&path, FLAT_CONFIG).unwrap();
    let resolver = TasksDirectoryResolver {
        path: root.clone(),
        source: TasksDirectorySource::CurrentDirectory,
    };
    assert_eq!(lotar::project::get_effective_project_name(&resolver), "DEV");
    std::fs::write(
        &path,
        FLAT_CONFIG.replace("default_project: DEV", "default_project: ''"),
    )
    .unwrap();
    let mut manager = ConfigManager::new_manager_with_tasks_dir_readonly(&root).unwrap();
    assert_eq!(manager.ensure_default_project(&root).unwrap(), "DEV");
    let cfg = persistence::load_global_config(Some(&root)).unwrap();
    let expected: GlobalConfig = serde_yaml_ng::from_str(FLAT_CONFIG).unwrap();
    assert_eq!(
        serde_json::to_value(cfg).unwrap(),
        serde_json::to_value(expected).unwrap()
    );
}

#[test]
fn invalid_recognized_types_and_nonmapping_roots_are_errors() {
    for yaml in [
        "[]",
        "false",
        "text",
        "null",
        "default: []",
        "default: {project: [DEV]}",
        "sprints: []",
        "agent_automation: []",
        "agent: {worktree: []}",
        "default_project: [DEV]",
        "default_assignee: [alice]",
        "default_assignee: 42",
        "default: {project: false}",
        "server_port: nope",
        "server: {port: -1}",
        "auto_identity: maybe",
        "auto: {identity: []}",
        "scan_signal_words: wrong",
        "scan: {signal_words: {bad: value}}",
        "issue: {states: [Todo, {}]}",
        "branch: {type_aliases: []}",
        "agent_context_extension: []",
        "agent: {logs_dir: []}",
        "web_ui_path: []",
        "agent: {worktree: {enabled: wrong}}",
        "agents: {worker: {env: []}}",
        "sync: {auth_profiles: []}",
        "sprints: {defaults: {capacity_points: wrong}}",
        "default_project: []\ndefault: {project: DEV}",
        "default: {project: []}\ndefault.project: DEV",
        "default.project: DEV\ndefault: {project: []}",
    ] {
        assert!(
            parse_global_from_yaml_str(yaml).is_err(),
            "accepted invalid config: {yaml}"
        );
    }
    for yaml in [
        "[]",
        "project: {name: []}",
        "default_assignee: []",
        "default: {assignee: []}",
        "agent: {logs_dir: []}",
        "scan: {enable_mentions: wrong}",
    ] {
        assert!(
            parse_project_from_yaml_str("DEV", yaml).is_err(),
            "accepted invalid project config: {yaml}"
        );
    }
    for yaml in ["", "# built-in defaults\n", "{}"] {
        assert!(parse_global_from_yaml_str(yaml).is_ok());
        assert!(parse_project_from_yaml_str("DEV", yaml).is_ok());
    }
}

#[test]
fn empty_project_overrides_roundtrip_without_becoming_inheritance() {
    let yaml = "project_name: Example\ndefault_tags: []\nagents: {}\nbranch_type_aliases: {}\nattachments_dir: ''\nsync_reports_dir: ''\n";
    let cfg = parse_project_from_yaml_str("EX", yaml).unwrap();
    let reloaded = parse_project_from_yaml_str("EX", &to_canonical_project_yaml(&cfg)).unwrap();
    assert_eq!(
        serde_json::to_value(cfg).unwrap(),
        serde_json::to_value(reloaded).unwrap()
    );
}

#[test]
fn literal_dotted_profile_names_and_environment_keys_roundtrip() {
    let yaml = "agents:\n  example.worker:\n    runner: codex\n    env: {FAKE.KEY: harmless-test-value}\nbranch_type_aliases: {fix.bug: Bug}\nauth_profiles:\n  example.profile: {provider: github, method: token}\n";
    let cfg = parse_global_from_yaml_str(yaml).unwrap();
    assert!(cfg.agents.contains_key("example.worker"));
    assert_eq!(
        cfg.agents["example.worker"].to_detail().env["FAKE.KEY"],
        "harmless-test-value"
    );
    assert!(cfg.branch_type_aliases.contains_key("fix.bug"));
    let reloaded = parse_global_from_yaml_str(&to_canonical_global_yaml(&cfg)).unwrap();
    assert_eq!(
        serde_json::to_value(cfg).unwrap(),
        serde_json::to_value(reloaded).unwrap()
    );
}

#[test]
fn canonical_placeholder_name_does_not_block_smart_prefix_reuse() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join(".tasks");
    std::fs::create_dir_all(root.join("MYAW")).unwrap();
    std::fs::write(root.join("MYAW/config.yml"), "project: {name: MYAW}\n").unwrap();
    assert_eq!(
        lotar::utils::project::generate_unique_project_prefix("myawesomeproject", &root).unwrap(),
        "MYAW"
    );
    std::fs::write(
        root.join("MYAW/config.yml"),
        "project: {name: Another Project}\n",
    )
    .unwrap();
    assert!(
        lotar::utils::project::generate_unique_project_prefix("myawesomeproject", &root).is_err()
    );
}

#[test]
fn dotted_and_nested_siblings_are_merged_without_order_dependent_loss() {
    for yaml in [
        "default.project: DEV\ndefault: {assignee: alice}\nsprints.defaults.capacity_points: 21",
        "default: {assignee: alice}\ndefault.project: DEV\nsprints.defaults.capacity_points: 21",
    ] {
        let cfg = parse_global_from_yaml_str(yaml).unwrap();
        assert_eq!(cfg.default_project, "DEV");
        assert_eq!(cfg.default_assignee.as_deref(), Some("alice"));
        assert_eq!(cfg.sprints.defaults.capacity_points, Some(21));
    }
}

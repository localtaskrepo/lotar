use lotar::config::candidate::{ConfigScope, ConfigSetRequest, apply_config_set};
use lotar::config::normalization::{
    parse_global_from_yaml_str, parse_project_from_yaml_str, to_canonical_project_yaml,
};
use lotar::config::operations::{
    apply_field_to_project_config, canonicalize_field_name, clear_project_override_field,
};
use lotar::config::resolution::preview_project_resolved;
use lotar::config::validation::ConfigValidator;
use lotar::config::{ConfigManager, GlobalConfig, ProjectConfig, ResolvedConfig};
use lotar::services::config_service::ConfigService;
use lotar::workspace::{TasksDirectoryResolver, TasksDirectorySource};
use std::fs;
use tempfile::TempDir;

#[test]
fn reviewers_parse_only_shipped_aliases_and_serialize_nested() {
    for yaml in [
        "review: {reviewers: [' Alice Smith ', 'alice smith', 'alice@example.com']}",
        "review.reviewers: [' Alice Smith ', 'alice smith', 'alice@example.com']",
        "review_reviewers: [' Alice Smith ', 'alice smith', 'alice@example.com']",
    ] {
        let config = parse_project_from_yaml_str("EX", yaml).unwrap();
        assert_eq!(
            config.review_reviewers.as_ref().unwrap(),
            &["Alice Smith", "alice@example.com"]
        );
        let canonical = to_canonical_project_yaml(&config);
        let value: serde_yaml_ng::Value = serde_yaml_ng::from_str(&canonical).unwrap();
        assert_eq!(
            value["review"]["reviewers"][0].as_str(),
            Some("Alice Smith")
        );
        assert!(value.get("review_reviewers").is_none());
        assert_eq!(
            parse_project_from_yaml_str("EX", &canonical).unwrap(),
            config
        );
    }
    assert_eq!(
        canonicalize_field_name("review.reviewers", false).unwrap(),
        "review_reviewers"
    );
    assert_eq!(
        canonicalize_field_name("review_reviewers", false).unwrap(),
        "review_reviewers"
    );
    for alias in ["reviewers", "default.reviewers", "review.review_reviewers"] {
        assert!(canonicalize_field_name(alias, false).is_err());
    }
}

#[test]
fn reviewer_authority_never_inherits_global_configuration() {
    for yaml in [
        "review: {reviewers: [alice]}",
        "review.reviewers: [alice]",
        "review_reviewers: [alice]",
    ] {
        let global = parse_global_from_yaml_str(yaml).unwrap();
        assert!(
            ResolvedConfig::from_global(global.clone())
                .review_reviewers
                .is_empty()
        );
        assert!(
            preview_project_resolved(&global, &ProjectConfig::new("EX".into()))
                .review_reviewers
                .is_empty()
        );
    }
    for alias in ["review.reviewers", "review_reviewers"] {
        assert!(canonicalize_field_name(alias, true).is_err());
    }
}

#[test]
fn invalid_reviewer_lists_fail_closed_even_when_hand_edited_or_forced() {
    let tmp = TempDir::new().unwrap();
    for invalid in ["", "   ", "@me", "@agent", "@alice", "*", "alice*"] {
        let yaml = format!(
            "review:\n  reviewers: [{}]\n",
            serde_json::to_string(invalid).unwrap()
        );
        assert!(parse_project_from_yaml_str("EX", &yaml).is_err());
        let mut config = ProjectConfig::new("EX".into());
        config.review_reviewers = Some(vec!["alice".into(), invalid.into()]);
        assert!(
            !ConfigValidator::new(tmp.path())
                .validate_project_config(&config)
                .errors
                .is_empty()
        );
        assert!(
            preview_project_resolved(&GlobalConfig::default(), &config)
                .review_reviewers
                .is_empty()
        );
    }
    for csv in [
        "@me",
        "*",
        "alice,",
        ",alice",
        "alice, ,bob",
        "[alice",
        "{alice: true}",
    ] {
        let error = apply_config_set(
            tmp.path(),
            &ConfigScope::Project("EX".into()),
            &ConfigSetRequest {
                entries: vec![("review.reviewers".into(), csv.into())],
                force: true,
                dry_run: false,
            },
        );
        assert!(
            error.is_err(),
            "force accepted invalid reviewer CSV {csv:?}"
        );
    }
    assert!(!tmp.path().join("EX").exists());
}

#[test]
fn reviewers_csv_empty_clear_and_explicit_empty_resolution() {
    let mut config = ProjectConfig::new("EX".into());
    apply_field_to_project_config(
        &mut config,
        "review_reviewers",
        " Alice Smith ,alice smith,bob@example.com",
    )
    .unwrap();
    assert_eq!(
        config.review_reviewers.as_ref().unwrap(),
        &["Alice Smith", "bob@example.com"]
    );
    let resolved = preview_project_resolved(&GlobalConfig::default(), &config);
    assert_eq!(
        resolved.review_reviewers,
        vec!["Alice Smith", "bob@example.com"]
    );
    apply_field_to_project_config(&mut config, "review_reviewers", "").unwrap();
    assert!(config.review_reviewers.is_none());
    config.review_reviewers = Some(vec!["alice".into()]);
    clear_project_override_field(&mut config, "review_reviewers").unwrap();
    assert!(config.review_reviewers.is_none());
    let explicit_empty = parse_project_from_yaml_str("EX", "review: {reviewers: []}").unwrap();
    assert_eq!(explicit_empty.review_reviewers, Some(Vec::new()));
    assert!(to_canonical_project_yaml(&explicit_empty).contains("reviewers: []"));
    apply_field_to_project_config(
        &mut config,
        "review_reviewers",
        r#"["Alice Smith", "bob@example.com"]"#,
    )
    .unwrap();
    assert_eq!(
        config.review_reviewers.as_deref().unwrap(),
        &["Alice Smith", "bob@example.com"]
    );
    apply_field_to_project_config(&mut config, "review_reviewers", "[]").unwrap();
    assert_eq!(config.review_reviewers, Some(Vec::new()));
}

#[test]
fn reviewers_candidate_dry_run_noop_and_inspect_preserve_contract() {
    let tmp = TempDir::new().unwrap();
    let tasks = tmp.path().join(".tasks");
    fs::create_dir(&tasks).unwrap();
    ConfigManager::save_project_config(&tasks, "EX", &ProjectConfig::new("EX".into())).unwrap();
    let path = tasks.join("EX/config.yml");
    let original = fs::read(&path).unwrap();
    let scope = ConfigScope::Project("EX".into());
    let mut request = ConfigSetRequest {
        entries: vec![(
            "review.reviewers".into(),
            " Alice Smith ,alice smith,bob@example.com".into(),
        )],
        force: false,
        dry_run: true,
    };
    assert!(!apply_config_set(&tasks, &scope, &request).unwrap().updated);
    assert_eq!(fs::read(&path).unwrap(), original);
    request.dry_run = false;
    assert!(apply_config_set(&tasks, &scope, &request).unwrap().updated);
    let bytes = fs::read(&path).unwrap();
    let modified = fs::metadata(&path).unwrap().modified().unwrap();
    assert!(!apply_config_set(&tasks, &scope, &request).unwrap().updated);
    assert_eq!(fs::read(&path).unwrap(), bytes);
    assert_eq!(fs::metadata(&path).unwrap().modified().unwrap(), modified);
    assert!(apply_config_set(&tasks, &ConfigScope::Global, &request).is_err());
    request.entries = vec![
        ("review.reviewers".into(), "charlie".into()),
        ("auto.identity".into(), "not-a-boolean".into()),
    ];
    assert!(apply_config_set(&tasks, &scope, &request).is_err());
    assert_eq!(fs::read(&path).unwrap(), bytes);

    let resolver = TasksDirectoryResolver {
        path: tasks,
        source: TasksDirectorySource::CurrentDirectory,
    };
    let show = ConfigService::show(&resolver, Some("EX")).unwrap();
    let inspect = ConfigService::inspect(&resolver, Some("EX")).unwrap();
    let expected = serde_json::json!(["Alice Smith", "bob@example.com"]);
    assert_eq!(show["review_reviewers"], expected);
    assert_eq!(inspect["effective"]["review_reviewers"], expected);
    assert_eq!(inspect["project_raw"]["review_reviewers"], expected);
    assert_eq!(
        inspect["global_effective"]["review_reviewers"],
        serde_json::json!([])
    );
    assert_eq!(inspect["sources"]["review_reviewers"], "project");
    assert!(inspect["global_raw"].get("review_reviewers").is_none());
}

#[test]
fn reviewer_sources_distinguish_absent_and_explicit_empty_and_invalid_disk_edits() {
    let tmp = TempDir::new().unwrap();
    let tasks = tmp.path().join(".tasks");
    fs::create_dir(&tasks).unwrap();
    let mut project = ProjectConfig::new("EX".into());
    ConfigManager::save_project_config(&tasks, "EX", &project).unwrap();
    let resolver = TasksDirectoryResolver {
        path: tasks.clone(),
        source: TasksDirectorySource::CurrentDirectory,
    };
    let inspect = ConfigService::inspect(&resolver, Some("EX")).unwrap();
    assert_eq!(inspect["sources"]["review_reviewers"], "built_in");
    project.review_reviewers = Some(Vec::new());
    ConfigManager::save_project_config(&tasks, "EX", &project).unwrap();
    let inspect = ConfigService::inspect(&resolver, Some("EX")).unwrap();
    assert_eq!(inspect["sources"]["review_reviewers"], "project");
    assert_eq!(
        inspect["effective"]["review_reviewers"],
        serde_json::json!([])
    );
    fs::write(
        tasks.join("EX/config.yml"),
        "review: {reviewers: [alice, '*']}\n",
    )
    .unwrap();
    let base = ResolvedConfig::from_global(GlobalConfig::default());
    assert!(lotar::config::resolution::get_project_config(&base, "EX", &tasks).is_err());
}

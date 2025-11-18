use std::collections::BTreeMap;

use lotar::config::persistence::load_project_config_from_dir;
use lotar::services::config_service::ConfigService;
use lotar::workspace::{TasksDirectoryResolver, TasksDirectorySource};
use tempfile::TempDir;

fn resolver_for(path: &std::path::Path) -> TasksDirectoryResolver {
    TasksDirectoryResolver {
        path: path.to_path_buf(),
        source: TasksDirectorySource::CurrentDirectory,
    }
}

fn ensure_tasks_dir(root: &std::path::Path) -> std::path::PathBuf {
    let tasks_dir = root.join(".tasks");
    std::fs::create_dir_all(&tasks_dir).unwrap();
    tasks_dir
}

#[test]
fn project_alias_identical_to_global_is_cleared() {
    let tmp = TempDir::new().unwrap();
    let tasks_dir = ensure_tasks_dir(tmp.path());
    let resolver = resolver_for(&tasks_dir);

    let mut base_values = BTreeMap::new();
    base_values.insert("default_prefix".to_string(), "TEST".to_string());
    base_values.insert(
        "issue_priorities".to_string(),
        "Low, Medium, High, Critical".to_string(),
    );
    ConfigService::set(&resolver, &base_values, true, None).expect("seed global config");

    let mut alias_values = BTreeMap::new();
    alias_values.insert(
        "branch_priority_aliases".to_string(),
        "{ feat: Critical }".to_string(),
    );
    ConfigService::set(&resolver, &alias_values, true, None).expect("set global alias");

    let mut project_values = BTreeMap::new();
    project_values.insert(
        "branch_priority_aliases".to_string(),
        r#"{"FEAT": "Critical"}"#.to_string(),
    );
    ConfigService::set(&resolver, &project_values, false, Some("TEST"))
        .expect("set project config");

    let stored = load_project_config_from_dir("TEST", &tasks_dir).expect("project config");
    assert!(stored.branch_priority_aliases.is_none());
}

#[test]
fn project_alias_difference_is_persisted() {
    let tmp = TempDir::new().unwrap();
    let tasks_dir = ensure_tasks_dir(tmp.path());
    let resolver = resolver_for(&tasks_dir);

    let mut base_values = BTreeMap::new();
    base_values.insert("default_prefix".to_string(), "APP".to_string());
    base_values.insert(
        "issue_priorities".to_string(),
        "Low, Medium, High, Critical".to_string(),
    );
    ConfigService::set(&resolver, &base_values, true, None).expect("seed global config");

    let mut alias_values = BTreeMap::new();
    alias_values.insert(
        "branch_priority_aliases".to_string(),
        "{ hotfix: Critical }".to_string(),
    );
    ConfigService::set(&resolver, &alias_values, true, None).expect("set global alias");

    let mut project_values = BTreeMap::new();
    project_values.insert(
        "branch_priority_aliases".to_string(),
        r#"{"hotfix": "High", "sev1": "Critical"}"#.to_string(),
    );
    ConfigService::set(&resolver, &project_values, false, Some("APP")).expect("set project config");

    let stored = load_project_config_from_dir("APP", &tasks_dir).expect("project config");
    let aliases = stored
        .branch_priority_aliases
        .expect("aliases should be stored");
    assert_eq!(aliases.len(), 2);
    assert_eq!(aliases.get("hotfix").unwrap().to_string(), "High");
    assert_eq!(aliases.get("sev1").unwrap().to_string(), "Critical");
}

#[test]
fn project_alias_can_be_cleared_with_empty_payload() {
    let tmp = TempDir::new().unwrap();
    let tasks_dir = ensure_tasks_dir(tmp.path());
    let resolver = resolver_for(&tasks_dir);

    let mut global_values = BTreeMap::new();
    global_values.insert("default_prefix".to_string(), "LIB".to_string());
    ConfigService::set(&resolver, &global_values, true, None).expect("set global prefix");

    let mut set_values = BTreeMap::new();
    set_values.insert(
        "branch_status_aliases".to_string(),
        r#"{"wip": "InProgress"}"#.to_string(),
    );
    ConfigService::set(&resolver, &set_values, false, Some("LIB")).expect("set project alias");

    let stored = load_project_config_from_dir("LIB", &tasks_dir).expect("project config");
    assert!(stored.branch_status_aliases.is_some());

    let mut clear_values = BTreeMap::new();
    clear_values.insert("branch_status_aliases".to_string(), String::new());
    ConfigService::set(&resolver, &clear_values, false, Some("LIB")).expect("clear project alias");

    let cleared = load_project_config_from_dir("LIB", &tasks_dir).expect("project config");
    assert!(cleared.branch_status_aliases.is_none());
}

#[test]
fn config_set_returns_warnings_when_validator_flags_issues() {
    let tmp = TempDir::new().unwrap();
    let tasks_dir = ensure_tasks_dir(tmp.path());
    let resolver = resolver_for(&tasks_dir);

    let mut values = BTreeMap::new();
    values.insert("server_port".to_string(), "80".to_string());

    let outcome = ConfigService::set(&resolver, &values, true, None).expect("set server port");

    assert!(outcome.updated, "expected update flag to be true");
    assert!(
        outcome.validation.has_warnings(),
        "expected validator to report warnings"
    );
    assert!(
        outcome
            .validation
            .warnings
            .iter()
            .any(|w| w.message.contains("may require elevated privileges")),
        "expected server_port warning message"
    );
}

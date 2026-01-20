use std::collections::BTreeMap;

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
fn inspect_reports_sources_for_global_scope() {
    let tmp = TempDir::new().unwrap();
    let tasks_dir = ensure_tasks_dir(tmp.path());
    let resolver = resolver_for(&tasks_dir);

    let mut global_values = BTreeMap::new();
    global_values.insert("default_project".to_string(), "ACME".to_string());
    ConfigService::set(&resolver, &global_values, true, None).expect("set global prefix");

    let payload = ConfigService::inspect(&resolver, None).expect("inspect global scope");
    let effective = payload["effective"].as_object().expect("effective config");
    let sources = payload["sources"].as_object().expect("sources object");

    assert_eq!(effective["default_project"].as_str(), Some("ACME"));
    assert!(effective.get("default_prefix").is_none());
    assert_eq!(sources["default_project"].as_str(), Some("global"));
    let tags_source = sources["tags"].as_str();
    assert!(
        matches!(tags_source, Some("built_in") | Some("global")),
        "unexpected tags source: {tags_source:?}"
    );
}

#[test]
fn inspect_reports_project_overrides_with_shared_helpers() {
    let tmp = TempDir::new().unwrap();
    let tasks_dir = ensure_tasks_dir(tmp.path());
    let resolver = resolver_for(&tasks_dir);

    let mut global_values = BTreeMap::new();
    global_values.insert("default_project".to_string(), "ACME".to_string());
    ConfigService::set(&resolver, &global_values, true, None).expect("set global prefix");

    ConfigService::create_project(&resolver, "Acme", Some("ACME"), None).expect("create project");

    let mut project_values = BTreeMap::new();
    project_values.insert("default_priority".to_string(), "High".to_string());
    ConfigService::set(&resolver, &project_values, false, Some("ACME"))
        .expect("set project override");

    let payload = ConfigService::inspect(&resolver, Some("ACME")).expect("inspect project scope");
    let sources = payload["sources"].as_object().expect("sources object");

    assert_eq!(sources["default_priority"].as_str(), Some("project"));
    assert_eq!(sources["default_project"].as_str(), Some("global"));
}

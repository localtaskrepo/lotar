//! DEV-485: `config show` lists the custom fields the web UI offers, per
//! `ui.custom_fields_from` (config, scanned or both; default both).

use std::collections::BTreeMap;

use lotar::services::config_service::ConfigService;
use lotar::workspace::{TasksDirectoryResolver, TasksDirectorySource};
use tempfile::TempDir;

fn setup(project_config: &str) -> (TempDir, TasksDirectoryResolver) {
    let tmp = TempDir::new().unwrap();
    let tasks_dir = tmp.path().join(".tasks");
    std::fs::create_dir_all(tasks_dir.join("INB")).unwrap();
    std::fs::write(tasks_dir.join("INB/config.yml"), format!("project:\n  name: INB\n{project_config}")).unwrap();
    let tasks = ["source: a\nsource_repo: r\nfinding: f\n", "source: b\nSource_Repo: r\n", "source: c\nsource_repo: r\nempty: ''\n"];
    for (n, fields) in tasks.iter().enumerate() {
        let custom: String = fields.lines().map(|line| format!("  {line}\n")).collect();
        let body = format!("title: Ticket {}\nstatus: Todo\npriority: Medium\ntype: Task\ncustom_fields:\n{custom}", n + 1);
        std::fs::write(tasks_dir.join(format!("INB/{}.yml", n + 1)), body).unwrap();
    }
    let resolver = TasksDirectoryResolver { path: tasks_dir, source: TasksDirectorySource::CurrentDirectory };
    (tmp, resolver)
}

fn offered(resolver: &TasksDirectoryResolver) -> Vec<String> {
    let show = ConfigService::show(resolver, Some("INB")).unwrap();
    show["custom_fields_offered"].as_array().expect("custom_fields_offered").iter().map(|v| v.as_str().unwrap().to_string()).collect()
}

#[test]
fn wildcard_project_offers_used_fields_by_default() {
    let (_tmp, resolver) = setup("");
    // Most used first; case variants merge under the most used spelling;
    // empty values don't count.
    assert_eq!(offered(&resolver), ["source", "source_repo", "finding"]);
    assert_eq!(ConfigService::show(&resolver, Some("INB")).unwrap()["ui_custom_fields_from"], "both");
}

#[test]
fn both_lists_declared_fields_first_in_their_spelling() {
    let (_tmp, resolver) = setup("custom:\n  fields: [estimate, Finding]\n");
    assert_eq!(offered(&resolver), ["estimate", "Finding", "source", "source_repo"]);
}

#[test]
fn config_and_scanned_offer_one_side_only() {
    let (_tmp, resolver) = setup("custom:\n  fields: [estimate]\nui:\n  custom_fields_from: config\n");
    assert_eq!(offered(&resolver), ["estimate"]);

    let (_tmp, resolver) = setup("custom:\n  fields: [estimate]\nui:\n  custom_fields_from: scanned\n");
    assert_eq!(offered(&resolver), ["source", "source_repo", "finding"]);
}

#[test]
fn config_set_validates_and_clears_the_setting() {
    let (_tmp, resolver) = setup("");
    let set = |value: &str| {
        let map: BTreeMap<String, String> = [("ui.custom_fields_from".to_string(), value.to_string())].into();
        ConfigService::set(&resolver, &map, false, Some("INB")).map_err(|e| e.to_string())
    };
    let err = set("tasks").unwrap_err();
    assert!(err.contains("config, scanned, both"), "{err}");

    set("Config").unwrap();
    assert!(offered(&resolver).is_empty());
    let raw = std::fs::read_to_string(resolver.path.join("INB/config.yml")).unwrap();
    assert!(raw.contains("custom_fields_from: config"), "{raw}");

    set("inherit").unwrap();
    assert_eq!(offered(&resolver), ["source", "source_repo", "finding"]);
}

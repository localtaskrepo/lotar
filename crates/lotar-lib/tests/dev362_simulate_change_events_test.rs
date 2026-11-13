//! DEV-362: `lotar automation simulate` judges change events (updated,
//! assigned, created, sprint_changed) the way a real update does: a field
//! condition only matches when that field changes in the same update.

use lotar::api_types::TaskUpdate;
use lotar::services::automation_service::{AutomationEvent, AutomationService};
use lotar::services::task_service::TaskService;
use lotar::storage::manager::Storage;
use std::path::PathBuf;

use crate::common;
use common::env_mutex::EnvVarGuard;

struct Workspace {
    _tmp: tempfile::TempDir,
    tasks: PathBuf,
}

fn workspace(rules: &str) -> Workspace {
    let tmp = tempfile::tempdir().unwrap();
    let tasks = tmp.path().join(".tasks");
    std::fs::create_dir_all(tasks.join("SC")).unwrap();
    std::fs::write(tasks.join("config.yml"), "default:\n  project: SC\nissue:\n  states: [Todo, InProgress, Done]\n").unwrap();
    std::fs::write(tasks.join("SC/1.yml"), "title: Tagged\nstatus: Todo\npriority: Medium\ntype: Feature\ntags: [t]\ncreated: 2026-07-01T00:00:00Z\n").unwrap();
    std::fs::write(tasks.join("automation.yml"), rules).unwrap();
    Workspace { _tmp: tmp, tasks }
}

fn comments(ws: &Workspace) -> usize {
    let storage = Storage::new(&ws.tasks);
    TaskService::get(&storage, "SC-1", None).unwrap().comments.len()
}

const BOTH: &str = r#"automation:
  rules:
    - name: Done and tagged
      when:
        all:
          - status: Done
          - tags: { any: [t] }
      on:
        updated:
          comment: fired
"#;

#[test]
fn simulate_does_not_match_a_rule_that_needs_two_fields_to_change_together() {
    let ws = workspace(BOTH);
    let _env = [EnvVarGuard::set("LOTAR_IGNORE_HOME_CONFIG", "1"), EnvVarGuard::clear("LOTAR_PROJECT")];
    let mut storage = Storage::new(&ws.tasks);
    TaskService::update(&mut storage, "SC-1", TaskUpdate { status: Some("Done".into()), ..TaskUpdate::default() }).unwrap();
    assert_eq!(comments(&ws), 0, "a status change alone does not fire the rule");

    let result = AutomationService::simulate(&ws.tasks, "SC-1", AutomationEvent::Updated).unwrap();
    assert!(!result.matched, "simulate must agree with the real update");
    assert_eq!(
        result.warnings,
        vec!["Done and tagged: matches only when status and tags change in the same update; a change of one of them alone does not fire it.".to_string()]
    );
}

#[test]
fn simulate_says_which_field_has_to_change() {
    let ws = workspace(
        r#"automation:
  rules:
    - name: Tagged work is done
      when:
        all:
          - status: Done
          - tags: { any: [t] }
          - status_changed: { within: 3650d }
      on:
        updated:
          comment: fired
    - name: Done
      when: { status: Done }
      on:
        updated:
          comment: done
"#,
    );
    let _env = [EnvVarGuard::set("LOTAR_IGNORE_HOME_CONFIG", "1"), EnvVarGuard::clear("LOTAR_PROJECT")];
    let mut storage = Storage::new(&ws.tasks);
    TaskService::update(&mut storage, "SC-1", TaskUpdate { status: Some("Done".into()), ..TaskUpdate::default() }).unwrap();
    assert_eq!(comments(&ws), 1, "only the Done rule fires");

    let result = AutomationService::simulate(&ws.tasks, "SC-1", AutomationEvent::Updated).unwrap();
    assert!(result.matched);
    assert_eq!(result.rule_name.as_deref(), Some("Done"));
    assert_eq!(result.warnings, vec!["Matches only when status changes in the same update.".to_string()]);
}

#[test]
fn simulate_created_matches_like_a_new_task() {
    let ws = workspace(
        r#"automation:
  rules:
    - name: New tagged work
      when:
        all:
          - status: Todo
          - tags: { any: [t] }
      on:
        created:
          comment: new
"#,
    );
    let _env = [EnvVarGuard::set("LOTAR_IGNORE_HOME_CONFIG", "1"), EnvVarGuard::clear("LOTAR_PROJECT")];
    let result = AutomationService::simulate(&ws.tasks, "SC-1", AutomationEvent::Created).unwrap();
    assert!(result.matched);
    assert!(result.warnings.is_empty(), "{:?}", result.warnings);
}

//! DEV-365: conditions written with the aliases `labels` and `task_type`
//! match a real update, and `lotar automation simulate`, the same way as
//! `tags` and `type`.

use lotar::api_types::TaskUpdate;
use lotar::services::automation_service::{AutomationEvent, AutomationService};
use lotar::services::task_service::TaskService;
use lotar::storage::manager::Storage;

use crate::common;
use common::env_mutex::EnvVarGuard;

/// Comments a real update adds, and whether simulate matches the task after it.
fn fire(when: &str, update: TaskUpdate) -> (usize, bool) {
    let tmp = tempfile::tempdir().unwrap();
    let tasks = tmp.path().join(".tasks");
    std::fs::create_dir_all(tasks.join("SC")).unwrap();
    std::fs::write(tasks.join("config.yml"), "default:\n  project: SC\nissue:\n  states: [Todo, Done]\n  types: [Feature, Bug]\n").unwrap();
    std::fs::write(tasks.join("SC/1.yml"), "title: One\nstatus: Todo\npriority: Medium\ntype: Feature\ncreated: 2026-07-01T00:00:00Z\n").unwrap();
    std::fs::write(
        tasks.join("automation.yml"),
        format!("automation:\n  rules:\n    - name: Rule\n      when: {when}\n      on:\n        updated:\n          comment: fired\n"),
    )
    .unwrap();
    let _env = [EnvVarGuard::set("LOTAR_IGNORE_HOME_CONFIG", "1"), EnvVarGuard::clear("LOTAR_PROJECT")];
    let mut storage = Storage::new(&tasks);
    TaskService::update(&mut storage, "SC-1", update).unwrap();
    let comments = TaskService::get(&storage, "SC-1", None).unwrap().comments.len();
    let simulated = AutomationService::simulate(&tasks, "SC-1", AutomationEvent::Updated).unwrap();
    (comments, simulated.matched)
}

fn tag() -> TaskUpdate {
    TaskUpdate { tags: Some(vec!["t".into()]), ..TaskUpdate::default() }
}

fn bug() -> TaskUpdate {
    TaskUpdate { task_type: Some("Bug".into()), ..TaskUpdate::default() }
}

#[test]
fn labels_matches_like_tags() {
    assert_eq!(fire("{ tags: { any: [t] } }", tag()), (1, true));
    assert_eq!(fire("{ labels: { any: [t] } }", tag()), (1, true));
    assert_eq!(fire("{ changes: { labels: { to: { any: [t] } } } }", tag()), (1, true));
}

#[test]
fn task_type_matches_like_type() {
    assert_eq!(fire("{ type: Bug }", bug()), (1, true));
    assert_eq!(fire("{ task_type: Bug }", bug()), (1, true));
    assert_eq!(fire("{ changes: { task_type: { to: Bug } } }", bug()), (1, true));
}

//! DEV-298: `current` conditions look at the task as it is now, so a rule can
//! fire on a status change of a ticket whose type did not change.

use lotar::api_types::TaskUpdate;
use lotar::services::automation_service::{AutomationEvent, AutomationService};
use lotar::services::task_service::TaskService;
use lotar::storage::manager::Storage;

use crate::common;
use common::env_mutex::EnvVarGuard;

/// Comments a real update of a Chore adds, and the simulation of the task after it.
fn fire(when: &str, update: TaskUpdate) -> (usize, bool, Vec<String>) {
    let tmp = tempfile::tempdir().unwrap();
    let tasks = tmp.path().join(".tasks");
    std::fs::create_dir_all(tasks.join("SC")).unwrap();
    std::fs::write(tasks.join("config.yml"), "default:\n  project: SC\nissue:\n  states: [Todo, InProgress, Done]\n  types: [Feature, Chore]\n").unwrap();
    std::fs::write(tasks.join("SC/1.yml"), "title: One\nstatus: Todo\npriority: Medium\ntype: Chore\ncreated: 2026-07-01T00:00:00Z\n").unwrap();
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
    (comments, simulated.matched, simulated.warnings)
}

fn start() -> TaskUpdate {
    TaskUpdate { status: Some("InProgress".into()), ..TaskUpdate::default() }
}

fn retitle() -> TaskUpdate {
    TaskUpdate { title: Some("Two".into()), ..TaskUpdate::default() }
}

#[test]
fn short_form_still_needs_the_field_to_change() {
    assert_eq!(fire("{ all: [ {type: Chore}, {status: InProgress} ] }", start()).0, 0);
}

#[test]
fn current_condition_fires_on_a_change_of_another_field() {
    let (comments, matched, warnings) = fire("{ all: [ {current: {type: Chore}}, {status: InProgress} ] }", start());
    assert_eq!((comments, matched), (1, true));
    assert!(warnings.iter().any(|w| w == "Matches only when status changes in the same update."), "{warnings:?}");
    assert!(warnings.iter().any(|w| w == "Checked on the ticket as it is now: type."), "{warnings:?}");
    assert_eq!(fire("{ current: {type: Chore}, status: InProgress }", start()).0, 1);
}

#[test]
fn current_condition_alone_fires_on_any_update() {
    assert_eq!(fire("{ current: {type: Chore} }", retitle()).0, 1);
    assert_eq!(fire("{ current: {type: Feature} }", retitle()).0, 0);
    assert_eq!(fire("{ all: [ {current: {type: Feature}}, {status: InProgress} ] }", start()).0, 0);
}

#[test]
fn current_condition_works_inside_any_and_not() {
    assert_eq!(fire("{ status: InProgress, not: {current: {tags: {any: [skip]}}} }", start()).0, 1);
    assert_eq!(fire("{ status: InProgress, not: {current: {type: Chore}} }", start()).0, 0);
    assert_eq!(fire("{ any: [ {current: {type: Feature}}, {current: {status: InProgress}} ] }", start()).0, 1);
}

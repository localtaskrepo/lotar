//! DEV-373: automation conditions on version fields, with `version: <range>`.

use lotar::api_types::TaskUpdate;
use lotar::services::automation_service::{AutomationEvent, AutomationService};
use lotar::services::task_service::TaskService;
use lotar::storage::manager::Storage;

use crate::common;
use common::env_mutex::EnvVarGuard;

/// Comments a real update of a task with `affects: [0.13.2]` adds, and
/// whether the simulation of the task after it matches.
fn fire(when: &str, update: TaskUpdate) -> (usize, bool) {
    let tmp = tempfile::tempdir().unwrap();
    let tasks = tmp.path().join(".tasks");
    std::fs::create_dir_all(tasks.join("SC")).unwrap();
    std::fs::write(
        tasks.join("config.yml"),
        "default:\n  project: SC\nissue:\n  states: [Todo, InProgress, Done]\ncustom:\n  fields: [{ name: affects, type: version }, product]\n",
    )
    .unwrap();
    std::fs::write(
        tasks.join("SC/1.yml"),
        "title: One\nstatus: Todo\npriority: Medium\ntype: Bug\ncreated: 2026-07-01T00:00:00Z\ncustom_fields:\n  affects: ['0.13.2']\n  product: web\n",
    )
    .unwrap();
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

fn start() -> TaskUpdate {
    TaskUpdate { status: Some("InProgress".into()), ..TaskUpdate::default() }
}

fn set_affects(value: &str) -> TaskUpdate {
    let mut fields = lotar::types::CustomFields::new();
    fields.insert("affects".into(), lotar::types::custom_value_string(value));
    fields.insert("product".into(), lotar::types::custom_value_string("web"));
    TaskUpdate { custom_fields: Some(fields), ..TaskUpdate::default() }
}

#[test]
fn current_version_ranges_match_the_task_as_it_is_now() {
    assert_eq!(fire("{ current: { affects: { version: '>=0.13 <0.14' } }, status: InProgress }", start()), (1, true));
    assert_eq!(fire("{ current: { custom_fields: { affects: { version: '^0.13' } } }, status: InProgress }", start()).0, 1);
    assert_eq!(fire("{ current: { affects: { version: '>=0.14' } }, status: InProgress }", start()).0, 0);
}

#[test]
fn short_form_version_conditions_need_the_field_to_change() {
    assert_eq!(fire("{ affects: { version: '>=0.13' } }", start()).0, 0);
    assert_eq!(fire("{ affects: { version: '>=0.14' } }", set_affects("0.14.1")), (1, true));
    assert_eq!(fire("{ affects: { version: '>=0.14' } }", set_affects("0.13.9")).0, 0);
    assert_eq!(fire("{ custom_fields: { affects: { version: '0.14' } } }", set_affects("v0.14.0")).0, 1);
}

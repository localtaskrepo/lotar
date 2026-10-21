//! DEV-118 characterization: apply and preview (simulate) share one parsed
//! `set` plan. Apply covers every supported field and alias; preview keeps
//! its historical subset until preview parity is defined separately.

use lotar::api_types::{TaskCreate, TaskDTO, TaskUpdate};
use lotar::services::automation_service::{AutomationEvent, AutomationService};
use lotar::services::task_service::TaskService;
use lotar::storage::manager::Storage;
use lotar::utils::paths;
use std::path::{Path, PathBuf};

fn workspace() -> (tempfile::TempDir, PathBuf) {
    let tmp = tempfile::tempdir().unwrap();
    let tasks_dir = tmp.path().join(".tasks");
    std::fs::create_dir_all(&tasks_dir).unwrap();
    std::fs::write(
        paths::global_config_path(&tasks_dir),
        "default.project: TEST\nissue.states: [Todo, InProgress, Done]\nissue.types: [Feature, Bug, Chore]\nissue.priorities: [Low, Medium, High]\nauto.set_reporter: false\nauto.assign_on_status: false\n",
    )
    .unwrap();
    (tmp, tasks_dir)
}

fn set_automation(tasks_dir: &Path, rules: &str) {
    let yaml = format!("automation:\n  rules:\n{rules}");
    AutomationService::set(tasks_dir, Some("TEST"), &yaml).expect("set automation");
}

fn create_task(storage: &mut Storage, title: &str) -> TaskDTO {
    TaskService::create(
        storage,
        TaskCreate {
            title: title.to_string(),
            project: Some("TEST".to_string()),
            tags: vec!["seed".to_string()],
            ..TaskCreate::default()
        },
    )
    .expect("create task")
}

fn start_in_progress(storage: &mut Storage, id: &str) {
    TaskService::update(
        storage,
        id,
        TaskUpdate {
            status: Some("InProgress".to_string()),
            ..TaskUpdate::default()
        },
    )
    .expect("update task");
}

const ALL_FIELDS_RULE: &str = "    - name: set-everything\n      when: {status: InProgress}\n      on:\n        updated:\n          set:\n            status: Done\n            priority: high\n            type: bug\n            title: \"Renamed ${{ticket.title}}\"\n            description: \"was ${{ticket.status}}\"\n            due_date: \"2026-12-01\"\n            effort: 3h\n            assignee: alice\n            reporter: bob\n            labels: [Alpha, beta]\n            custom_fields:\n              team: core\n";

#[test]
fn apply_sets_every_supported_field_with_templates_and_labels_alias() {
    let (_tmp, tasks_dir) = workspace();
    let mut storage = Storage::new(&tasks_dir);
    let task = create_task(&mut storage, "Plan");
    set_automation(&tasks_dir, ALL_FIELDS_RULE);

    start_in_progress(&mut storage, &task.id);

    let after = TaskService::get(&storage, &task.id, None).unwrap();
    assert_eq!(after.status.to_string(), "Done");
    assert_eq!(after.priority.to_string(), "High");
    assert_eq!(after.task_type.to_string(), "Bug");
    assert_eq!(after.title, "Renamed Plan");
    assert_eq!(after.description.as_deref(), Some("was InProgress"));
    assert!(
        after
            .due_date
            .as_deref()
            .unwrap_or("")
            .starts_with("2026-12-01")
    );
    assert_eq!(after.effort.as_deref(), Some("3.00h"));
    assert_eq!(after.assignee.as_deref(), Some("alice"));
    assert_eq!(after.reporter.as_deref(), Some("bob"));
    assert_eq!(after.tags, vec!["Alpha".to_string(), "beta".to_string()]);
    assert_eq!(
        after
            .custom_fields
            .get("team")
            .map(lotar::types::custom_value_to_string),
        Some("core".to_string())
    );
}

#[test]
fn apply_prefers_tags_over_labels_and_skips_unknown_enums() {
    let (_tmp, tasks_dir) = workspace();
    let mut storage = Storage::new(&tasks_dir);
    let task = create_task(&mut storage, "Precedence");
    set_automation(
        &tasks_dir,
        "    - name: mixed\n      when: {status: InProgress}\n      on:\n        updated:\n          set:\n            priority: urgent\n            type: epic\n            tags: [from-tags]\n            labels: [from-labels]\n",
    );

    start_in_progress(&mut storage, &task.id);

    let after = TaskService::get(&storage, &task.id, None).unwrap();
    assert_eq!(after.tags, vec!["from-tags".to_string()]);
    assert_eq!(after.priority.to_string(), task.priority.to_string());
    assert_eq!(after.task_type.to_string(), task.task_type.to_string());
    assert_eq!(after.status.to_string(), "InProgress");
}

#[test]
fn preview_keeps_its_current_subset_of_set_fields() {
    let (_tmp, tasks_dir) = workspace();
    let mut storage = Storage::new(&tasks_dir);
    let task = create_task(&mut storage, "Plan");
    start_in_progress(&mut storage, &task.id);
    set_automation(&tasks_dir, ALL_FIELDS_RULE);

    let result = AutomationService::simulate(&tasks_dir, &task.id, AutomationEvent::Updated)
        .expect("simulate");
    assert!(result.matched);
    let preview = result.task_after.expect("preview task");
    // Applied in preview: parsed enums and raw text values.
    assert_eq!(preview.status.to_string(), "Done");
    assert_eq!(preview.priority.to_string(), "High");
    assert_eq!(preview.task_type.to_string(), "Bug");
    assert_eq!(preview.title, "Renamed ${{ticket.title}}");
    assert_eq!(
        preview.description.as_deref(),
        Some("was ${{ticket.status}}")
    );
    assert_eq!(preview.assignee.as_deref(), Some("alice"));
    assert_eq!(preview.reporter.as_deref(), Some("bob"));
    // Not yet previewed (DEV-118 parity decision): due date, effort, the
    // `labels` alias and custom fields keep their current values.
    assert_eq!(preview.due_date, None);
    assert_eq!(preview.effort, None);
    assert_eq!(preview.tags, vec!["seed".to_string()]);
    assert!(!preview.custom_fields.contains_key("team"));

    // Preview never writes.
    let stored = TaskService::get(&storage, &task.id, None).unwrap();
    assert_eq!(stored.status.to_string(), "InProgress");
    assert_eq!(stored.title, "Plan");
}

#[test]
fn validation_warns_for_each_unknown_set_enum() {
    let (_tmp, tasks_dir) = workspace();
    let yaml = "automation:\n  rules:\n    - name: bad\n      when: {status: Todo}\n      on:\n        updated:\n          set: {status: Nope, priority: urgent, type: epic}\n";
    let warnings: Vec<String> = AutomationService::set(&tasks_dir, Some("TEST"), yaml)
        .expect("unknown enums are warnings, not errors")
        .validation
        .warnings
        .into_iter()
        .map(|w| w.message)
        .collect();
    for expected in [
        "Automation action sets unknown status 'Nope'.",
        "Automation action sets unknown priority 'urgent'.",
        "Automation action sets unknown type 'epic'.",
    ] {
        assert!(
            warnings.iter().any(|w| w.contains(expected)),
            "missing warning {expected:?} in {warnings:?}"
        );
    }
}

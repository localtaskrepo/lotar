//! DEV-118 characterization: apply and preview (simulate) share one parsed
//! `set` plan. Apply covers every supported field and alias; preview keeps
//! the same field coverage and reports what only a real run can decide.

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
fn preview_matches_apply_for_every_set_field() {
    // Preview on one workspace, real apply on an identical one.
    let (_preview_tmp, preview_dir) = workspace();
    let mut preview_storage = Storage::new(&preview_dir);
    let task = create_task(&mut preview_storage, "Plan");
    start_in_progress(&mut preview_storage, &task.id);
    set_automation(&preview_dir, ALL_FIELDS_RULE);
    let result = AutomationService::simulate(&preview_dir, &task.id, AutomationEvent::Updated)
        .expect("simulate");
    assert!(result.matched);
    assert!(result.warnings.is_empty(), "{:?}", result.warnings);
    let preview = result.task_after.expect("preview task");

    let (_apply_tmp, apply_dir) = workspace();
    let mut apply_storage = Storage::new(&apply_dir);
    let applied_task = create_task(&mut apply_storage, "Plan");
    set_automation(&apply_dir, ALL_FIELDS_RULE);
    start_in_progress(&mut apply_storage, &applied_task.id);
    let applied = TaskService::get(&apply_storage, &applied_task.id, None).unwrap();

    assert_eq!(preview.status, applied.status);
    assert_eq!(preview.priority, applied.priority);
    assert_eq!(preview.task_type, applied.task_type);
    assert_eq!(preview.title, applied.title);
    assert_eq!(preview.title, "Renamed Plan");
    assert_eq!(preview.description, applied.description);
    assert_eq!(preview.due_date, applied.due_date);
    assert_eq!(preview.effort, applied.effort);
    assert_eq!(preview.assignee, applied.assignee);
    assert_eq!(preview.reporter, applied.reporter);
    assert_eq!(preview.tags, applied.tags);
    assert_eq!(preview.custom_fields, applied.custom_fields);

    let described: Vec<&str> = result.actions.iter().map(|a| a.action.as_str()).collect();
    for expected in [
        "set_status",
        "set_priority",
        "set_type",
        "set_assignee",
        "set_reporter",
        "set_title",
        "set_description",
        "set_due_date",
        "set_effort",
        "set_tags",
        "set_custom_field",
    ] {
        assert!(
            described.contains(&expected),
            "missing {expected} in {described:?}"
        );
    }
    assert!(
        result
            .actions
            .iter()
            .any(|a| a.description == "Set title to 'Renamed Plan'")
    );

    // Preview never writes.
    let stored = TaskService::get(&preview_storage, &task.id, None).unwrap();
    assert_eq!(stored.status.to_string(), "InProgress");
    assert_eq!(stored.title, "Plan");
}

#[test]
fn preview_warns_about_values_only_known_at_run_time() {
    let (_tmp, tasks_dir) = workspace();
    std::fs::write(
        paths::global_config_path(&tasks_dir),
        "default.project: TEST\nissue.states: [Todo, InProgress, Done]\nissue.types: [Feature, Bug, Chore]\nissue.priorities: [Low, Medium, High]\nauto.set_reporter: false\nauto.assign_on_status: false\nmembers: [alice, bob]\n",
    )
    .unwrap();
    let mut storage = Storage::new(&tasks_dir);
    let task = create_task(&mut storage, "Runtime");
    start_in_progress(&mut storage, &task.id);
    set_automation(
        &tasks_dir,
        "    - name: runtime\n      when: {status: InProgress}\n      on:\n        updated:\n          set:\n            assignee: \"@round_robin\"\n            priority: urgent\n            description: \"from ${{previous.status}} for ${{ticket.id}}\"\n          add:\n            tags: [Seed, Extra]\n            sprint: active\n          comment: \"moved by ${{comment.text}}\"\n",
    );

    let first = AutomationService::simulate(&tasks_dir, &task.id, AutomationEvent::Updated)
        .expect("simulate");
    let second = AutomationService::simulate(&tasks_dir, &task.id, AutomationEvent::Updated)
        .expect("simulate again");
    let preview = first.task_after.clone().expect("preview");

    // Runtime assignee stays a token and previewing twice is stable (no rotation).
    assert_eq!(preview.assignee.as_deref(), Some("@round_robin"));
    assert_eq!(
        second.task_after.unwrap().assignee.as_deref(),
        Some("@round_robin")
    );
    // Ticket values expand; event-only values stay literal.
    assert_eq!(
        preview.description.as_deref(),
        Some(format!("from ${{{{previous.status}}}} for {}", task.id).as_str())
    );
    // Case-insensitive add keeps the existing `seed` tag once, like apply.
    assert_eq!(preview.tags, vec!["seed".to_string(), "Extra".to_string()]);
    assert_eq!(preview.priority, task.priority);

    let warnings = first.warnings.join("\n");
    for expected in [
        "Assignee '@round_robin' is chosen when the rule runs",
        "Unknown priority 'urgent' would be skipped",
        "Sprint membership changes are listed in actions but not reflected in task_after",
        "'${{previous.status}}' comes from the triggering event",
        "'${{comment.text}}' comes from the triggering event",
    ] {
        assert!(
            warnings.contains(expected),
            "missing {expected:?} in {warnings}"
        );
    }
    assert!(
        first.actions.iter().any(|a| a.action == "add_comment"
            && a.description == "Add comment: moved by ${{comment.text}}")
    );
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

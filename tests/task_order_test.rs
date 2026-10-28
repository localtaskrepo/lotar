use lotar::api_server::{ApiServer, HttpRequest};
use lotar::api_types::TaskOrderRequest;
use lotar::services::sprint_assignment::{SprintBacklogOptions, fetch_backlog};
use lotar::services::sprint_service::SprintService;
use lotar::services::task_order::{get_order, reorder};
use lotar::storage::manager::Storage;
use lotar::storage::sprint::{BacklogOrder, Sprint};
use lotar::utils::paths;
use serde_json::{Value, json};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

mod common;

fn workspace() -> (tempfile::TempDir, Storage) {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join(".tasks");
    std::fs::create_dir_all(&root).unwrap();
    std::fs::write(paths::global_config_path(&root), "default.project: TEST\nissue.states: [Todo, InProgress, Done]\nissue.types: [Feature, Bug, Chore]\nissue.priorities: [Low, Medium, High]\n").unwrap();
    (temp, Storage { root_path: root })
}

fn task(root: &Path, project: &str, number: u32, extra: &str) -> PathBuf {
    let path = root.join(project).join(format!("{number}.yml"));
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(&path, format!("# User-owned task bytes\ntitle: Task {number}\nstatus: Todo\npriority: Medium\ntype: Feature\ncreated: '2026-01-01T00:00:00Z'\nmodified: '2026-01-02T00:00:00Z'\nhistory: [{{at: '2026-01-02', changes: [{{field: title, old: Before, new: After}}]}}]\nx_keep: {{nested: [one, 2]}}\n{extra}")).unwrap();
    path
}

fn sprint(root: &Path, id: u32, raw: &str) -> PathBuf {
    let path = root.join("@sprints").join(format!("{id}.yml"));
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(&path, raw).unwrap();
    path
}

fn metadata(root: &Path, raw: &str) -> PathBuf {
    let path = BacklogOrder::path(root);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(&path, raw).unwrap();
    path
}

fn state(path: &Path) -> (Vec<u8>, SystemTime) {
    (
        std::fs::read(path).unwrap(),
        std::fs::metadata(path).unwrap().modified().unwrap(),
    )
}

fn before(task: &str, anchor: &str, sprint: Option<u32>) -> TaskOrderRequest {
    TaskOrderRequest {
        task: task.into(),
        sprint,
        before: Some(anchor.into()),
        after: None,
        allow_closed: false,
    }
}

fn seed_four(storage: &Storage) -> Vec<PathBuf> {
    (1..=4)
        .map(|id| task(&storage.root_path, "TEST", id, ""))
        .collect()
}

#[test]
fn missing_order_reads_do_not_create_metadata_or_directories() {
    let (temp, storage) = workspace();
    seed_four(&storage);
    assert_eq!(
        get_order(&storage, None).unwrap().tasks,
        ["TEST-1", "TEST-2", "TEST-3", "TEST-4"]
    );
    assert!(!storage.root_path.join("@sprints").exists());
    let missing = Storage {
        root_path: temp.path().join("missing").join(".tasks"),
    };
    assert!(get_order(&missing, None).unwrap().tasks.is_empty());
    assert!(!missing.root_path.exists());
    let response = reorder(&storage, &before("TEST-001", "TEST-2", None)).unwrap();
    assert!(!response.changed);
    assert!(!BacklogOrder::path(&storage.root_path).exists());
}

#[test]
fn relative_backlog_move_preserves_every_task_and_config_and_hidden_positions() {
    let (_temp, storage) = workspace();
    let files = seed_four(&storage);
    let other = task(&storage.root_path, "OTHER", 1, "");
    let path = metadata(
        &storage.root_path,
        "# Preserve if no-op\ntasks: [TEST-1, OTHER-1, TEST-2, TEST-3, TEST-4]\nmodified: old\nhistory: []\nx_keep: {nested: [one, 2]}\n",
    );
    let task_states: Vec<_> = files
        .iter()
        .chain([&other])
        .map(|path| (path, state(path)))
        .collect();
    let config = paths::global_config_path(&storage.root_path);
    let config_state = state(&config);
    let response = reorder(&storage, &before("TEST-003", "TEST-001", None)).unwrap();
    assert!(response.changed);
    assert_eq!(
        response.tasks,
        ["TEST-3", "TEST-1", "OTHER-1", "TEST-2", "TEST-4"]
    );
    let persisted = BacklogOrder::load(&storage.root_path).unwrap();
    assert_eq!(persisted.tasks, response.tasks);
    assert_eq!(persisted.history.len(), 1);
    assert_eq!(
        persisted.extra_fields["x_keep"]["nested"][0].as_str(),
        Some("one")
    );
    assert_ne!(persisted.modified.as_deref(), Some("old"));
    for (path, before) in task_states {
        assert_eq!(state(path), before);
    }
    assert_eq!(state(&config), config_state);
    let unchanged = state(&path);
    assert!(
        !reorder(&storage, &before("TEST-3", "TEST-1", None))
            .unwrap()
            .changed
    );
    assert!(
        !reorder(&storage, &before("TEST-003", "TEST-3", None))
            .unwrap()
            .changed
    );
    let after = TaskOrderRequest {
        task: "OTHER-1".into(),
        sprint: None,
        before: None,
        after: Some("TEST-1".into()),
        allow_closed: false,
    };
    assert!(!reorder(&storage, &after).unwrap().changed);
    assert_eq!(state(&path), unchanged);
}

#[test]
fn inactive_stored_ids_return_to_their_positions_and_unlisted_ids_append() {
    let (_temp, storage) = workspace();
    seed_four(&storage);
    let deleted = task(
        &storage.root_path,
        "TEST",
        9,
        "deleted_at: '2026-01-03T00:00:00Z'\n",
    );
    let assigned = sprint(&storage.root_path, 1, "tasks: [TEST-4]\n");
    metadata(
        &storage.root_path,
        "tasks: [TEST-1, TEST-9, TEST-4, TEST-2]\n",
    );
    assert_eq!(
        get_order(&storage, None).unwrap().tasks,
        ["TEST-1", "TEST-2", "TEST-3"]
    );
    assert!(
        reorder(&storage, &before("TEST-3", "TEST-1", None))
            .unwrap()
            .changed
    );
    assert_eq!(
        BacklogOrder::load(&storage.root_path).unwrap().tasks,
        ["TEST-3", "TEST-1", "TEST-9", "TEST-4", "TEST-2"]
    );
    std::fs::write(&assigned, "tasks: []\n").unwrap();
    task(&storage.root_path, "TEST", 9, "");
    assert!(deleted.exists());
    assert_eq!(
        get_order(&storage, None).unwrap().tasks,
        ["TEST-3", "TEST-1", "TEST-9", "TEST-4", "TEST-2"]
    );
    task(&storage.root_path, "TEST", 10, "");
    assert_eq!(
        get_order(&storage, None).unwrap().tasks.last().unwrap(),
        "TEST-10"
    );
}

#[test]
fn backlog_filters_and_limits_project_the_central_order_without_replacing_it() {
    let (_temp, storage) = workspace();
    task(&storage.root_path, "TEST", 1, "tags: [keep]\n");
    task(&storage.root_path, "TEST", 2, "tags: [hidden]\n");
    task(
        &storage.root_path,
        "TEST",
        3,
        "tags: [keep]\nassignee: alice\n",
    );
    task(&storage.root_path, "OTHER", 1, "tags: [keep]\n");
    metadata(
        &storage.root_path,
        "tasks: [TEST-3, OTHER-1, TEST-2, TEST-1]\n",
    );
    // The backlog runs the shared task query (DEV-236).
    let backlog = |search: &str| {
        let mut input = lotar::services::task_list::TaskQueryInput {
            project: Some("TEST".into()),
            ..Default::default()
        };
        input
            .add_search(&lotar::utils::search_syntax::parse(search))
            .unwrap();
        let config =
            lotar::config::resolution::load_and_merge_configs(Some(&storage.root_path)).unwrap();
        SprintBacklogOptions {
            query: input.resolve(&config, &storage.root_path).unwrap(),
            manual_order: true,
            limit: 1,
        }
    };
    let options = backlog("tag:keep");
    let result = fetch_backlog(&storage, &options).unwrap();
    assert_eq!(result.entries[0].id, "TEST-3");
    assert!(result.truncated);
    let response = reorder(&storage, &before("TEST-1", "TEST-3", None)).unwrap();
    assert_eq!(response.tasks, ["TEST-1", "TEST-3", "OTHER-1", "TEST-2"]);
    assert_eq!(
        fetch_backlog(&storage, &options).unwrap().entries[0].id,
        "TEST-1"
    );
    let assigned = backlog("tag:keep assignee:alice");
    assert_eq!(
        fetch_backlog(&storage, &assigned).unwrap().entries[0].id,
        "TEST-3"
    );
}

#[test]
fn sprint_moves_change_only_sequence_and_noop_keeps_legacy_bytes() {
    let (_temp, storage) = workspace();
    let files = seed_four(&storage);
    let states: Vec<_> = files.iter().map(|path| state(path)).collect();
    let path = sprint(
        &storage.root_path,
        1,
        "# Preserve on no-op\ntasks: [TEST-001, TEST-2, TEST-3]\nx_keep: {nested: true}\n",
    );
    let unrelated = sprint(&storage.root_path, 2, "tasks: [TEST-4]\n");
    let original = state(&path);
    let other = state(&unrelated);
    assert!(
        !reorder(&storage, &before("TEST-001", "TEST-2", Some(1)))
            .unwrap()
            .changed
    );
    assert_eq!(state(&path), original);
    assert!(
        reorder(&storage, &before("TEST-3", "TEST-1", Some(1)))
            .unwrap()
            .changed
    );
    assert_eq!(
        get_order(&storage, Some(1)).unwrap().tasks,
        ["TEST-3", "TEST-1", "TEST-2"]
    );
    assert!(reorder(&storage, &before("TEST-4", "TEST-1", Some(1))).is_err());
    assert_eq!(state(&unrelated), other);
    let updated: Sprint = serde_yaml_ng::from_slice(&std::fs::read(&path).unwrap()).unwrap();
    assert_eq!(
        updated.extra_fields["x_keep"]["nested"].as_bool(),
        Some(true)
    );
    for (path, original) in files.iter().zip(states) {
        assert_eq!(state(path), original);
    }
    assert!(!BacklogOrder::path(&storage.root_path).exists());
}

#[test]
fn sprint_reorder_preserves_all_unrelated_nested_yaml_values() {
    let (_temp, storage) = workspace();
    let files = seed_four(&storage);
    let original_tasks: Vec<_> = files.iter().map(|path| state(path)).collect();
    let path = sprint(
        &storage.root_path,
        1,
        r#"modified: old
plan:
  label: Nested metadata
  capacity: {points: 3, x_units: {nested: [one, 2]}}
  x_plan: {keep: [one, {two: true}]}
actual:
  started_at: '2026-01-01T00:00:00Z'
  x_actual: {nested: [one, 2]}
tasks:
  - id: TEST-001
    order: 2
    x_note: {keep: [one, 2]}
  - TEST-2
  - id: TEST-3
    order: 1
    x_note: {keep: {deep: true}}
history:
  - at: '2026-01-01T00:00:00Z'
    actor: Historic actor
    x_entry: {keep: [one, 2]}
    changes:
      - field: plan.label
        old: Old
        new: New
        x_change: {keep: {deep: true}}
x_root: {nested: [one, 2]}
"#,
    );
    let original = state(&path);
    let raw_before: serde_yaml_ng::Value = serde_yaml_ng::from_slice(&original.0).unwrap();
    assert!(
        !reorder(&storage, &before("TEST-1", "TEST-2", Some(1)))
            .unwrap()
            .changed
    );
    assert_eq!(state(&path), original);
    let response = reorder(&storage, &before("TEST-3", "TEST-1", Some(1))).unwrap();
    assert!(response.changed);
    assert_eq!(response.tasks, ["TEST-3", "TEST-1", "TEST-2"]);
    let raw_after: serde_yaml_ng::Value =
        serde_yaml_ng::from_slice(&std::fs::read(&path).unwrap()).unwrap();
    let mut original_unrelated = raw_before.as_mapping().unwrap().clone();
    let mut updated_unrelated = raw_after.as_mapping().unwrap().clone();
    for key in ["tasks", "modified", "history"] {
        let key = serde_yaml_ng::Value::String(key.into());
        original_unrelated.remove(&key);
        updated_unrelated.remove(&key);
    }
    assert_eq!(updated_unrelated, original_unrelated);
    assert_ne!(raw_after["modified"], raw_before["modified"]);
    assert_eq!(raw_after["tasks"][0], raw_before["tasks"][2]);
    assert_eq!(raw_after["tasks"][1], raw_before["tasks"][0]);
    assert_eq!(raw_after["tasks"][2], raw_before["tasks"][1]);
    assert_eq!(raw_after["history"][0], raw_before["history"][0]);
    assert_eq!(raw_after["history"].as_sequence().unwrap().len(), 2);
    assert_eq!(
        raw_after["history"][1]["changes"][0]["field"].as_str(),
        Some("tasks.TEST-3")
    );
    let reloaded = Storage {
        root_path: storage.root_path.clone(),
    };
    assert_eq!(get_order(&reloaded, Some(1)).unwrap().tasks, response.tasks);
    let mut original_members: Vec<_> = vec!["TEST-1", "TEST-2", "TEST-3"];
    let mut updated_members = response.tasks.clone();
    original_members.sort();
    updated_members.sort();
    assert_eq!(updated_members, original_members);
    for (index, id) in response.tasks.iter().enumerate() {
        let dto = lotar::services::task_service::TaskService::get(&reloaded, id, None).unwrap();
        assert_eq!(dto.sprint_order[&1], index as u32 + 1);
    }
    let persisted = state(&path);
    assert!(
        !reorder(&reloaded, &before("TEST-3", "TEST-1", Some(1)))
            .unwrap()
            .changed
    );
    assert_eq!(state(&path), persisted);
    for (path, original) in files.iter().zip(original_tasks) {
        assert_eq!(state(path), original);
    }
}

#[test]
fn sprint_reorder_rejects_invalid_typed_metadata_without_writes() {
    let (_temp, storage) = workspace();
    let files = seed_four(&storage);
    let original_tasks: Vec<_> = files.iter().map(|path| state(path)).collect();
    for invalid in [
        "plan: {capacity: {points: nope}}\n",
        "actual: {started_at: []}\n",
        "history: [{at: old, changes: broken}]\n",
    ] {
        let path = sprint(
            &storage.root_path,
            1,
            &format!("tasks: [TEST-1, TEST-2]\nx_keep: {{nested: true}}\n{invalid}"),
        );
        let original = state(&path);
        assert!(reorder(&storage, &before("TEST-2", "TEST-1", Some(1))).is_err());
        assert_eq!(state(&path), original);
    }
    for (path, original) in files.iter().zip(original_tasks) {
        assert_eq!(state(path), original);
    }
}

#[test]
fn legacy_sprint_vector_order_matches_actual_dtos_before_and_after_reorder() {
    let (_temp, storage) = workspace();
    let files = seed_four(&storage);
    let original_tasks: Vec<_> = files.iter().map(|path| state(path)).collect();
    let path = sprint(
        &storage.root_path,
        1,
        "# Legacy order decorations do not override the vector\ntasks: [{id: TEST-001, order: 2}, {id: TEST-2, order: 1}]\nx_keep: {nested: true}\n",
    );
    assert_eq!(
        get_order(&storage, Some(1)).unwrap().tasks,
        ["TEST-1", "TEST-2"]
    );
    let first = lotar::services::task_service::TaskService::get(&storage, "TEST-1", None).unwrap();
    let second = lotar::services::task_service::TaskService::get(&storage, "TEST-2", None).unwrap();
    assert_eq!(first.sprint_order[&1], 1);
    assert_eq!(second.sprint_order[&1], 2);
    let original = state(&path);
    assert!(
        !reorder(&storage, &before("TEST-001", "TEST-2", Some(1)))
            .unwrap()
            .changed
    );
    assert!(
        !reorder(&storage, &before("TEST-001", "TEST-1", Some(1)))
            .unwrap()
            .changed
    );
    assert_eq!(state(&path), original);

    let response = reorder(&storage, &before("TEST-2", "TEST-001", Some(1))).unwrap();
    assert!(response.changed);
    assert_eq!(response.tasks, ["TEST-2", "TEST-1"]);
    let reloaded = Storage {
        root_path: storage.root_path.clone(),
    };
    assert_eq!(get_order(&reloaded, Some(1)).unwrap().tasks, response.tasks);
    let stored: Sprint = serde_yaml_ng::from_slice(&std::fs::read(&path).unwrap()).unwrap();
    assert_eq!(
        stored
            .tasks
            .iter()
            .map(|entry| entry.id.as_str())
            .collect::<Vec<_>>(),
        ["TEST-2", "TEST-001"]
    );
    assert_eq!(stored.tasks[0].order, Some(1));
    assert_eq!(stored.tasks[1].order, Some(2));
    assert_eq!(
        stored.extra_fields["x_keep"]["nested"].as_bool(),
        Some(true)
    );
    assert_eq!(
        stored.history.last().unwrap().changes[0].field,
        "tasks.TEST-2"
    );
    assert_eq!(
        stored.history.last().unwrap().changes[0].old.as_deref(),
        Some("position 2")
    );
    assert_eq!(
        stored.history.last().unwrap().changes[0].new.as_deref(),
        Some("position 1; before TEST-1")
    );
    let first = lotar::services::task_service::TaskService::get(&reloaded, "TEST-1", None).unwrap();
    let second =
        lotar::services::task_service::TaskService::get(&reloaded, "TEST-2", None).unwrap();
    assert_eq!(first.sprint_order[&1], 2);
    assert_eq!(second.sprint_order[&1], 1);
    let canonical = state(&path);
    assert!(
        !reorder(&reloaded, &before("TEST-2", "TEST-1", Some(1)))
            .unwrap()
            .changed
    );
    assert_eq!(state(&path), canonical);
    for (path, original) in files.iter().zip(original_tasks) {
        assert_eq!(state(path), original);
    }
}

#[test]
fn mixed_legacy_decorations_preserve_vector_order_and_actual_dto_projection() {
    let (_temp, storage) = workspace();
    seed_four(&storage);
    let path = sprint(
        &storage.root_path,
        1,
        "tasks: [{id: TEST-3, order: 1}, TEST-2, {id: TEST-001, order: 2}, TEST-4]\n",
    );
    assert_eq!(
        get_order(&storage, Some(1)).unwrap().tasks,
        ["TEST-3", "TEST-2", "TEST-1", "TEST-4"]
    );
    for (index, id) in get_order(&storage, Some(1))
        .unwrap()
        .tasks
        .iter()
        .enumerate()
    {
        let dto = lotar::services::task_service::TaskService::get(&storage, id, None).unwrap();
        assert_eq!(dto.sprint_order[&1], index as u32 + 1);
    }
    let original = state(&path);
    assert!(
        !reorder(&storage, &before("TEST-3", "TEST-2", Some(1)))
            .unwrap()
            .changed
    );
    assert!(
        !reorder(&storage, &before("TEST-1", "TEST-4", Some(1)))
            .unwrap()
            .changed
    );
    assert_eq!(state(&path), original);
    assert!(
        reorder(&storage, &before("TEST-4", "TEST-3", Some(1)))
            .unwrap()
            .changed
    );
    let reloaded = Storage {
        root_path: storage.root_path.clone(),
    };
    let order = get_order(&reloaded, Some(1)).unwrap().tasks;
    assert_eq!(order, ["TEST-4", "TEST-3", "TEST-2", "TEST-1"]);
    for (index, id) in order.iter().enumerate() {
        let dto = lotar::services::task_service::TaskService::get(&reloaded, id, None).unwrap();
        assert_eq!(dto.sprint_order[&1], index as u32 + 1);
    }
}

#[test]
fn order_history_is_constant_size_with_resolved_actor_and_no_task_edits() {
    let (_temp, storage) = workspace();
    let files: Vec<_> = (1..=80)
        .map(|number| task(&storage.root_path, "TEST", number, ""))
        .collect();
    let original_tasks: Vec<_> = files.iter().map(|path| state(path)).collect();
    let config = paths::project_config_path(&storage.root_path, "TEST");
    std::fs::write(&config, "default.reporter: Order Actor\n").unwrap();
    let original_config = state(&config);
    let ids: Vec<_> = (1..=80).map(|number| format!("TEST-{number}")).collect();
    metadata(
        &storage.root_path,
        &format!("tasks: [{}]\n", ids.join(", ")),
    );
    assert!(
        reorder(&storage, &before("TEST-80", "TEST-1", None))
            .unwrap()
            .changed
    );
    let stored = BacklogOrder::load(&storage.root_path).unwrap();
    assert_eq!(stored.history.len(), 1);
    let history = &stored.history[0];
    assert_eq!(history.actor.as_deref(), Some("Order Actor"));
    assert_eq!(history.changes.len(), 1);
    assert_eq!(history.changes[0].field, "tasks.TEST-80");
    assert_eq!(history.changes[0].old.as_deref(), Some("position 80"));
    assert_eq!(
        history.changes[0].new.as_deref(),
        Some("position 1; before TEST-1")
    );
    assert!(serde_yaml_ng::to_string(history).unwrap().len() < 256);
    for (path, original) in files.iter().zip(original_tasks) {
        assert_eq!(state(path), original);
    }
    assert_eq!(state(&config), original_config);
}

#[test]
fn closed_invalid_stale_and_deleted_requests_write_nothing() {
    let (_temp, storage) = workspace();
    seed_four(&storage);
    task(
        &storage.root_path,
        "TEST",
        9,
        "deleted_at: '2026-01-03T00:00:00Z'\n",
    );
    let closed = sprint(
        &storage.root_path,
        1,
        "actual: {closed_at: '2026-01-03T00:00:00Z'}\ntasks: [TEST-1, TEST-2]\n",
    );
    let original = state(&closed);
    assert!(reorder(&storage, &before("TEST-2", "TEST-1", Some(1))).is_err());
    assert_eq!(state(&closed), original);
    for request in [
        before("TEST-1", "TEST-3", None),
        before("TEST-3", "TEST-1", None),
        before("TEST-9", "TEST-3", None),
        before("TEST-3", "TEST-9", None),
        before("TEST-99", "TEST-3", None),
        before("../TEST-3", "TEST-4", None),
        before("TEST-3", "TEST-4", Some(0)),
        TaskOrderRequest {
            after: Some("TEST-4".into()),
            ..before("TEST-3", "TEST-4", None)
        },
        TaskOrderRequest {
            before: None,
            ..before("TEST-3", "TEST-4", None)
        },
    ] {
        assert!(reorder(&storage, &request).is_err());
    }
    assert!(!BacklogOrder::path(&storage.root_path).exists());
    let mut allowed = before("TEST-2", "TEST-1", Some(1));
    allowed.allow_closed = true;
    assert!(reorder(&storage, &allowed).unwrap().changed);
}

#[test]
fn corrupt_order_files_fail_closed_and_backlog_is_not_a_sprint() {
    let (_temp, mut storage) = workspace();
    seed_four(&storage);
    for raw in [
        "tasks: broken\nx_keep: true\n",
        "tasks: [TEST-1, {id: TEST-2}]\n",
        "tasks: [TEST-1, TEST-001]\n",
        "tasks: ['../evil-1']\n",
    ] {
        let path = metadata(&storage.root_path, raw);
        let original = state(&path);
        assert!(get_order(&storage, None).is_err());
        assert!(reorder(&storage, &before("TEST-3", "TEST-1", None)).is_err());
        assert_eq!(state(&path), original);
    }
    let valid = sprint(&storage.root_path, 1, "tasks: [TEST-1, TEST-2]\n");
    assert_eq!(SprintService::list(&storage).unwrap().len(), 1);
    assert_eq!(
        SprintService::create(&mut storage, Sprint::default(), None)
            .unwrap()
            .record
            .id,
        2
    );
    std::fs::write(&valid, "tasks: broken\nx_keep: true\n").unwrap();
    let original = state(&valid);
    assert!(reorder(&storage, &before("TEST-2", "TEST-1", Some(1))).is_err());
    assert_eq!(state(&valid), original);
    assert!(reorder(&storage, &before("TEST-4", "TEST-3", None)).is_err());
}

#[test]
fn foreign_root_tasks_are_rejected_even_when_visible() {
    let (_temp, storage) = workspace();
    seed_four(&storage);
    let nested = storage
        .root_path
        .parent()
        .unwrap()
        .join("nested")
        .join(".tasks");
    let foreign = task(&nested, "OTHER", 1, "");
    let original = state(&foreign);
    assert!(
        get_order(&storage, None)
            .unwrap()
            .tasks
            .contains(&"OTHER-1".into())
    );
    assert!(reorder(&storage, &before("OTHER-1", "TEST-1", None)).is_err());
    assert!(reorder(&storage, &before("TEST-1", "OTHER-1", None)).is_err());
    assert_eq!(state(&foreign), original);
    assert!(!BacklogOrder::path(&storage.root_path).exists());
    task(&nested, "TEST", 1, "");
    assert!(reorder(&storage, &before("TEST-1", "TEST-2", None)).is_err());
    assert!(!BacklogOrder::path(&storage.root_path).exists());
}

#[test]
fn concurrent_relative_moves_preserve_both_edits_and_latest_membership_is_validated() {
    let (_temp, storage) = workspace();
    seed_four(&storage);
    std::thread::scope(|scope| {
        let first = scope.spawn(|| reorder(&storage, &before("TEST-2", "TEST-1", None)).unwrap());
        let second = scope.spawn(|| reorder(&storage, &before("TEST-4", "TEST-3", None)).unwrap());
        assert!(first.join().unwrap().changed);
        assert!(second.join().unwrap().changed);
    });
    assert_eq!(
        get_order(&storage, None).unwrap().tasks,
        ["TEST-2", "TEST-1", "TEST-4", "TEST-3"]
    );
    let metadata_path = BacklogOrder::path(&storage.root_path);
    let original = state(&metadata_path);
    let dir = storage.root_path.join("@sprints");
    let guard = lotar::storage::safety::acquire_storage_lock(
        &dir,
        lotar::storage::safety::sprint_lock_name(),
    )
    .unwrap();
    std::thread::scope(|scope| {
        let worker = scope.spawn(|| reorder(&storage, &before("TEST-3", "TEST-1", None)));
        sprint(&storage.root_path, 1, "tasks: [TEST-3]\n");
        drop(guard);
        assert!(worker.join().unwrap().is_err());
    });
    assert_eq!(state(&metadata_path), original);
}

fn request(method: &str, path: &str, body: Value) -> HttpRequest {
    HttpRequest {
        method: method.into(),
        path: path.into(),
        query: HashMap::new(),
        headers: HashMap::new(),
        body: serde_json::to_vec(&body).unwrap(),
    }
}

#[test]
fn rest_mcp_contracts_and_changed_only_events() {
    let (_temp, storage) = workspace();
    let _guard = common::env_mutex::EnvVarGuard::set(
        "LOTAR_TASKS_DIR",
        &storage.root_path.to_string_lossy(),
    );
    for number in 1..=3 {
        task(&storage.root_path, "ORD", number, "");
    }
    let mut api = ApiServer::new();
    lotar::routes::initialize(&mut api);
    let initial = api.handle_request(&request("GET", "/api/sprints/backlog/order", Value::Null));
    assert_eq!(initial.status, 200);
    let initial: Value = serde_json::from_slice(&initial.body).unwrap();
    assert_eq!(
        initial["data"],
        json!({"status":"ok","tasks":["ORD-1","ORD-2","ORD-3"],"changed":false})
    );
    let events = lotar::api_events::subscribe();
    let moved = api.handle_request(&request(
        "POST",
        "/api/sprints/order",
        json!({"task":"ORD-003","before":"ORD-1","sprint":null}),
    ));
    assert_eq!(moved.status, 200);
    let moved: Value = serde_json::from_slice(&moved.body).unwrap();
    assert_eq!(
        moved["data"],
        json!({"status":"ok","tasks":["ORD-3","ORD-1","ORD-2"],"changed":true})
    );
    let noop = api.handle_request(&request(
        "POST",
        "/api/sprints/order",
        json!({"task":"ORD-3","before":"ORD-1"}),
    ));
    assert_eq!(noop.status, 200);
    let matching: Vec<_> = events
        .try_iter()
        .filter(|event| {
            event.kind == "task_order_changed"
                && event.data["tasks"]
                    .as_array()
                    .is_some_and(|tasks| tasks.contains(&json!("ORD-1")))
        })
        .collect();
    assert_eq!(matching.len(), 1);
    assert_eq!(matching[0].data["sprint_id"], Value::Null);
    for body in [
        json!({"task":"ORD-1"}),
        json!({"task":"ORD-1","before":"ORD-2","after":"ORD-3"}),
        json!({"task":"ORD-1","before":"ORD-2","sprint":0}),
        json!({"task":"ORD-1","before":"ORD-2","sprint":"1"}),
        json!({"task":"ORD-1","before":"ORD-2","tasks":["ORD-1"]}),
    ] {
        assert_eq!(
            api.handle_request(&request("POST", "/api/sprints/order", body))
                .status,
            400
        );
    }
    let line = json!({"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"sprint_reorder","arguments":{"task":"ORD-2","after":"ORD-3"}}}).to_string();
    let response: Value =
        serde_json::from_str(&lotar::mcp::server::handle_json_line(&line)).unwrap();
    assert!(response.get("error").is_none(), "{response}");
    let payload: Value =
        serde_json::from_str(response["result"]["content"][0]["text"].as_str().unwrap()).unwrap();
    assert_eq!(payload["tasks"], json!(["ORD-3", "ORD-2", "ORD-1"]));
}

#[test]
fn cli_relative_order_supports_backlog_sprint_noop_and_flag_validation() {
    let (temp, storage) = workspace();
    seed_four(&storage);
    let command = || {
        let mut command = common::lotar_cmd().unwrap();
        command
            .current_dir(temp.path())
            .env("LOTAR_TASKS_DIR", &storage.root_path);
        command
    };
    let output = command()
        .args([
            "sprint", "order", "TEST-3", "--before", "TEST-1", "--format", "json",
        ])
        .assert()
        .success();
    let payload: Value = serde_json::from_slice(&output.get_output().stdout).unwrap();
    assert_eq!(
        payload,
        json!({"status":"ok","tasks":["TEST-3","TEST-1","TEST-2","TEST-4"],"changed":true})
    );
    let path = BacklogOrder::path(&storage.root_path);
    let original = state(&path);
    command()
        .args(["sprint", "order", "TEST-3", "--before", "TEST-1"])
        .assert()
        .success();
    assert_eq!(state(&path), original);
    for args in [
        vec!["sprint", "order", "TEST-1"],
        vec![
            "sprint", "order", "TEST-1", "--before", "TEST-2", "--after", "TEST-3",
        ],
        vec![
            "sprint", "order", "TEST-1", "--before", "TEST-2", "--sprint", "0",
        ],
    ] {
        command().args(args).assert().failure();
    }
    sprint(&storage.root_path, 1, "tasks: [TEST-1, TEST-2]\n");
    command()
        .args([
            "sprint", "order", "TEST-2", "--before", "TEST-1", "--sprint", "#1",
        ])
        .assert()
        .success();
    assert_eq!(
        get_order(&storage, Some(1)).unwrap().tasks,
        ["TEST-2", "TEST-1"]
    );
    assert_eq!(state(&path), original);
}

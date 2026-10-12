//! DEV-102 regression coverage: the shared task update boundary treats
//! effective no-ops as true no-ops. Unchanged, same-value, and
//! normalized-equal patches must preserve the original task YAML bytes
//! (comments, key order, user extensions) and file mtime, leave
//! `modified`/history, project-config bytes, and sprint bytes untouched,
//! and emit no mutation SSE events or automation hooks — while genuine
//! changes keep writing, bumping timestamps, recording history, and firing
//! side effects exactly as before. Validation, pending-journal recovery,
//! and comment lifecycle semantics stay intact.

use lotar::api_events;
use lotar::api_types::TaskUpdate;
use lotar::services::automation_service::AutomationService;
use lotar::services::task_service::{TaskService, TaskUpdateContext, TaskUpdateReceipt};
use lotar::storage::manager::Storage;
use lotar::storage::task::parse_task_yaml_tolerant;
use lotar::types::ReferenceEntry;
use lotar::utils::paths;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

fn workspace(extra_config: &str) -> (tempfile::TempDir, PathBuf) {
    let tmp = tempfile::tempdir().unwrap();
    let tasks_dir = tmp.path().join(".tasks");
    let content = format!(
        "default.project: TEST\nissue.states: [Todo, InProgress, Done]\nissue.types: [Feature, Bug, Chore]\nissue.priorities: [Low, Medium, High]\n{extra_config}"
    );
    std::fs::create_dir_all(&tasks_dir).unwrap();
    std::fs::write(paths::global_config_path(&tasks_dir), content).unwrap();
    (tmp, tasks_dir)
}

/// Raw task YAML with user-owned comments and a YAML extension; byte
/// preservation across no-op updates proves the file is never
/// reserialized.
const RAW_TASK: &str = "# User-owned header comment kept byte-for-byte.\n\
title: Original title\n\
status: Todo\n\
priority: Medium\n\
type: Feature\n\
reporter: alice\n\
assignee: alice\n\
created: '2026-01-01T00:00:00Z'\n\
modified: '2026-01-02T00:00:00Z'\n\
due_date: '2026-06-01'\n\
effort: 1d\n\
description: keep me\n\
tags:\n  - backend\n\
acceptance_criteria:\n  - first\n  - second\n\
x_extension:\n  nested:\n    keep: true\n";

/// Sparse legacy task: no status/priority/type/tags — a no-op update must
/// not "repair" it by persisting DTO-projection defaults.
const RAW_LEGACY_TASK: &str = "title: Legacy sparse\ncreated: '2026-01-01T00:00:00Z'\n";

/// Task introducing `bob`, who is absent from the configured members, so
/// auto-population plans a project-config repair.
const RAW_BOB_TASK: &str = "title: Bob task\nstatus: Todo\npriority: Medium\nreporter: alice\n\
assignee: bob\ncreated: '2026-01-01T00:00:00Z'\n";

/// Minimal task with a configurable assignee spelling, for agent/human
/// identity comparisons.
fn raw_task_with_assignee(assignee: &str) -> String {
    format!(
        "title: Identity\nstatus: Todo\npriority: Medium\nreporter: alice\nassignee: {assignee}\n\
created: '2026-01-01T00:00:00Z'\n"
    )
}

/// Task under review owned by a reporter no test actor can collide with,
/// assigned to the registered agent directive form.
const RAW_REVIEW_AGENT_TASK: &str = "title: Under review\nstatus: NeedsReview\npriority: Medium\n\
reporter: review-owner-42\nassignee: '@copilot'\ncreated: '2026-01-01T00:00:00Z'\n";

/// Task whose description starts with a four-space code indent and ends
/// with a newline — Markdown layout that must survive as content.
const RAW_MARKDOWN_TASK: &str = "title: Markdown\nstatus: Todo\npriority: Medium\nreporter: alice\n\
created: '2026-01-01T00:00:00Z'\ndescription: \"    sort(items)\\nthen prose\\n\"\n";

fn seed_task(tasks_dir: &Path, file: &str, raw: &str) -> PathBuf {
    let path = tasks_dir.join("TEST").join(file);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(&path, raw).unwrap();
    path
}

fn seed_project_config(tasks_dir: &Path, raw: &str) -> PathBuf {
    let path = tasks_dir.join("TEST").join("config.yml");
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(&path, raw).unwrap();
    path
}

fn seed_padded_sprint_member(tasks_dir: &Path) -> PathBuf {
    let path = tasks_dir.join("@sprints").join("1.yml");
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(&path, "tasks:\n  - id: TEST-001\n    order: 1\n").unwrap();
    path
}

fn mtime(path: &Path) -> SystemTime {
    std::fs::metadata(path).unwrap().modified().unwrap()
}

fn updated_events(rx: &std::sync::mpsc::Receiver<api_events::ApiEvent>) -> usize {
    let mut count = 0;
    while let Ok(event) = rx.try_recv() {
        if event.kind == "task_updated" {
            count += 1;
        }
    }
    count
}

fn emitting() -> TaskUpdateContext {
    TaskUpdateContext {
        emit_api_event: true,
        ..TaskUpdateContext::default()
    }
}

fn set_automation(tasks_dir: &Path, rules: &str) {
    let yaml = format!("automation:\n  rules:\n{rules}");
    AutomationService::set(tasks_dir, Some("TEST"), &yaml).expect("set automation");
}

fn noop_update(storage: &mut Storage, id: &str, patch: TaskUpdate) -> TaskUpdateReceipt {
    let receipt =
        TaskService::update_with_receipt(storage, id, patch, TaskUpdateContext::default())
            .expect("update must succeed");
    assert!(!receipt.changed, "expected an effective no-op");
    receipt
}

#[test]
fn empty_patch_is_an_effective_noop() {
    let (_tmp, tasks_dir) = workspace("");
    let task_path = seed_task(&tasks_dir, "1.yml", RAW_TASK);
    let config_path = seed_project_config(&tasks_dir, "project_name: TEST\nmembers:\n  - alice\n");
    let sprint_path = seed_padded_sprint_member(&tasks_dir);
    let (task_before, config_before, sprint_before) = (
        std::fs::read(&task_path).unwrap(),
        std::fs::read(&config_path).unwrap(),
        std::fs::read(&sprint_path).unwrap(),
    );
    let task_mtime = mtime(&task_path);

    let rx = api_events::subscribe();
    let receipt = noop_update(
        &mut Storage::new(&tasks_dir),
        "TEST-1",
        TaskUpdate::default(),
    );

    assert_eq!(std::fs::read(&task_path).unwrap(), task_before);
    assert_eq!(std::fs::read(&config_path).unwrap(), config_before);
    assert_eq!(std::fs::read(&sprint_path).unwrap(), sprint_before);
    assert_eq!(mtime(&task_path), task_mtime);
    assert_eq!(updated_events(&rx), 0);
    assert_eq!(receipt.dto.title, "Original title");
    assert_eq!(receipt.dto.modified, "2026-01-02T00:00:00Z");
    assert!(receipt.dto.history.is_empty());
    assert_eq!(receipt.dto.sprints, vec![1]);
}

#[test]
fn same_value_patch_is_an_effective_noop() {
    let (_tmp, tasks_dir) = workspace("");
    let task_path = seed_task(&tasks_dir, "1.yml", RAW_TASK);
    let (task_before, task_mtime) = (std::fs::read(&task_path).unwrap(), mtime(&task_path));

    let rx = api_events::subscribe();
    let receipt = noop_update(
        &mut Storage::new(&tasks_dir),
        "TEST-1",
        TaskUpdate {
            title: Some("Original title".to_string()),
            status: Some("Todo".to_string()),
            priority: Some("Medium".to_string()),
            task_type: Some("Feature".to_string()),
            reporter: Some("alice".to_string()),
            assignee: Some("alice".to_string()),
            due_date: Some("2026-06-01".to_string()),
            effort: Some("1d".to_string()),
            description: Some("keep me".to_string()),
            tags: Some(vec!["backend".to_string()]),
            acceptance_criteria: Some(vec!["first".to_string(), "second".to_string()]),
            ..TaskUpdate::default()
        },
    );

    assert_eq!(std::fs::read(&task_path).unwrap(), task_before);
    assert_eq!(mtime(&task_path), task_mtime);
    assert_eq!(updated_events(&rx), 0);
    assert_eq!(receipt.dto.effort.as_deref(), Some("1d"));
}

#[test]
fn normalized_aliases_scalars_and_collections_are_effective_noops() {
    let (_tmp, tasks_dir) = workspace("");
    let task_path = seed_task(&tasks_dir, "1.yml", RAW_TASK);
    let sprint_path = seed_padded_sprint_member(&tasks_dir);
    let (task_before, sprint_before) = (
        std::fs::read(&task_path).unwrap(),
        std::fs::read(&sprint_path).unwrap(),
    );
    let task_mtime = mtime(&task_path);

    // Status spelling normalizes to the configured value, @-prefixed and
    // differently-cased members compare equal, tag whitespace trims away,
    // the effort canonical form matches the stored token, and a padded
    // sprint-member alias equals the canonical membership. The padded
    // task-id alias must also resolve to the canonical identity.
    let receipt = noop_update(
        &mut Storage::new(&tasks_dir),
        "TEST-001",
        TaskUpdate {
            status: Some("TODO".to_string()),
            assignee: Some("@Alice".to_string()),
            tags: Some(vec![" backend ".to_string()]),
            effort: Some("1d".to_string()),
            sprints: Some(vec![1]),
            ..TaskUpdate::default()
        },
    );

    assert_eq!(std::fs::read(&task_path).unwrap(), task_before);
    assert_eq!(std::fs::read(&sprint_path).unwrap(), sprint_before);
    assert_eq!(mtime(&task_path), task_mtime);
    assert_eq!(receipt.dto.id, "TEST-1");
    assert_eq!(receipt.dto.status.as_str(), "Todo");
    assert_eq!(receipt.dto.sprints, vec![1]);
}

#[test]
fn reordered_and_changed_collections_are_genuine_changes() {
    let (_tmp, tasks_dir) = workspace("");
    let task_path = seed_task(&tasks_dir, "1.yml", RAW_TASK);
    let task_before = std::fs::read(&task_path).unwrap();

    // Reordering acceptance criteria is a real change: ordered collections
    // are compared in order, never sorted into equality.
    let mut storage = Storage::new(&tasks_dir);
    let receipt = TaskService::update_with_receipt(
        &mut storage,
        "TEST-1",
        TaskUpdate {
            acceptance_criteria: Some(vec!["second".to_string(), "first".to_string()]),
            ..TaskUpdate::default()
        },
        TaskUpdateContext::default(),
    )
    .expect("update must succeed");
    assert!(receipt.changed);
    assert_ne!(std::fs::read(&task_path).unwrap(), task_before);
    assert_eq!(receipt.dto.acceptance_criteria, vec!["second", "first"]);
}

#[test]
fn legacy_sparse_task_is_not_repaired_and_not_auto_assigned_by_noops() {
    let (_tmp, tasks_dir) =
        workspace("default.tags: [auto-tag]\nauto.assign_on_status: true\ndefault.status: Todo\n");
    let task_path = seed_task(&tasks_dir, "1.yml", RAW_LEGACY_TASK);
    let (task_before, task_mtime) = (std::fs::read(&task_path).unwrap(), mtime(&task_path));

    let mut storage = Storage::new(&tasks_dir);

    // Empty patch: no default-tag repair, no enum repair.
    let receipt = noop_update(&mut storage, "TEST-1", TaskUpdate::default());
    assert_eq!(std::fs::read(&task_path).unwrap(), task_before);
    assert_eq!(mtime(&task_path), task_mtime);
    assert!(receipt.dto.tags.is_empty());
    assert_eq!(receipt.dto.status.as_str(), "Todo", "projected default");

    // Setting the DTO-projection default status explicitly is still a
    // no-op: no status persisted, no auto-assign side effect gained.
    let receipt = noop_update(
        &mut storage,
        "TEST-1",
        TaskUpdate {
            status: Some("Todo".to_string()),
            ..TaskUpdate::default()
        },
    );
    assert_eq!(std::fs::read(&task_path).unwrap(), task_before);
    assert_eq!(mtime(&task_path), task_mtime);
    assert!(receipt.dto.assignee.is_none());
    assert_eq!(receipt.dto.status.as_str(), "Todo");

    // A genuine effective transition DOES auto-assign (side effects are
    // preserved for real changes).
    let receipt = TaskService::update_with_receipt(
        &mut storage,
        "TEST-1",
        TaskUpdate {
            status: Some("InProgress".to_string()),
            ..TaskUpdate::default()
        },
        TaskUpdateContext::default(),
    )
    .expect("update must succeed");
    assert!(receipt.changed);
    assert!(receipt.dto.assignee.is_some(), "auto-assign must fire");
}

#[test]
fn clearing_absent_fields_is_a_noop_and_clearing_set_fields_is_genuine() {
    let (_tmp, tasks_dir) = workspace("");
    let legacy_path = seed_task(&tasks_dir, "1.yml", RAW_LEGACY_TASK);
    let legacy_before = std::fs::read(&legacy_path).unwrap();

    noop_update(
        &mut Storage::new(&tasks_dir),
        "TEST-1",
        TaskUpdate {
            description: Some(String::new()),
            due_date: Some(String::new()),
            effort: Some(String::new()),
            tags: Some(Vec::new()),
            acceptance_criteria: Some(Vec::new()),
            relationships: Some(Default::default()),
            custom_fields: Some(Default::default()),
            sprints: Some(Vec::new()),
            ..TaskUpdate::default()
        },
    );
    assert_eq!(std::fs::read(&legacy_path).unwrap(), legacy_before);

    let task_path = seed_task(&tasks_dir, "2.yml", RAW_TASK);
    let mut storage = Storage::new(&tasks_dir);
    let receipt = TaskService::update_with_receipt(
        &mut storage,
        "TEST-2",
        TaskUpdate {
            due_date: Some(String::new()),
            ..TaskUpdate::default()
        },
        TaskUpdateContext::default(),
    )
    .expect("update must succeed");
    assert!(receipt.changed);
    assert!(receipt.dto.due_date.is_none());
    let raw = std::fs::read_to_string(&task_path).unwrap();
    assert!(!raw.contains("due_date:"));
}

#[test]
fn genuine_change_writes_history_and_emits_one_event() {
    let (_tmp, tasks_dir) = workspace("");
    let task_path = seed_task(&tasks_dir, "1.yml", RAW_TASK);
    // Members already cover the task's reporter/assignee so the genuine
    // update stages no config repair here.
    let config_path = seed_project_config(&tasks_dir, "project_name: TEST\nmembers:\n  - alice\n");
    let (task_before, config_before) = (
        std::fs::read(&task_path).unwrap(),
        std::fs::read(&config_path).unwrap(),
    );
    let task_mtime = mtime(&task_path);

    let rx = api_events::subscribe();
    let receipt = TaskService::update_with_receipt(
        &mut Storage::new(&tasks_dir),
        "TEST-1",
        TaskUpdate {
            title: Some("Renamed".to_string()),
            ..TaskUpdate::default()
        },
        emitting(),
    )
    .expect("update must succeed");

    assert!(receipt.changed);
    assert_eq!(receipt.dto.title, "Renamed");
    assert_ne!(receipt.dto.modified, "2026-01-02T00:00:00Z");
    assert_eq!(receipt.dto.history.len(), 1);
    assert_eq!(receipt.dto.history[0].changes[0].field, "title");
    assert_eq!(
        receipt.dto.history[0].changes[0].old.as_deref(),
        Some("Original title")
    );
    assert_eq!(
        receipt.dto.history[0].changes[0].new.as_deref(),
        Some("Renamed")
    );
    assert_ne!(std::fs::read(&task_path).unwrap(), task_before);
    assert_ne!(mtime(&task_path), task_mtime);
    assert_eq!(std::fs::read(&config_path).unwrap(), config_before);
    assert_eq!(updated_events(&rx), 1);
}

#[test]
fn noop_update_fires_no_automation_and_genuine_update_fires_it() {
    let (_tmp, tasks_dir) = workspace("");
    seed_task(&tasks_dir, "1.yml", RAW_TASK);
    seed_project_config(&tasks_dir, "project_name: TEST\nmembers:\n  - alice\n");
    set_automation(
        &tasks_dir,
        "    - name: stamp\n      on: {updated: {add: {tags: [stamped]}}}\n",
    );

    let mut storage = Storage::new(&tasks_dir);
    noop_update(&mut storage, "TEST-1", TaskUpdate::default());
    let after = TaskService::get(&storage, "TEST-1", None).expect("get");
    assert!(!after.tags.contains(&"stamped".to_string()));

    TaskService::update(
        &mut storage,
        "TEST-1",
        TaskUpdate {
            title: Some("Renamed".to_string()),
            ..TaskUpdate::default()
        },
    )
    .expect("genuine update");
    let after = TaskService::get(&storage, "TEST-1", None).expect("get");
    assert!(after.tags.contains(&"stamped".to_string()));
}

#[test]
fn invalid_patches_fail_closed_without_writing() {
    let (_tmp, tasks_dir) = workspace("");
    let task_path = seed_task(&tasks_dir, "1.yml", RAW_TASK);
    let (task_before, task_mtime) = (std::fs::read(&task_path).unwrap(), mtime(&task_path));

    for patch in [
        TaskUpdate {
            status: Some("Bogus".to_string()),
            ..TaskUpdate::default()
        },
        TaskUpdate {
            priority: Some("Nope".to_string()),
            ..TaskUpdate::default()
        },
        TaskUpdate {
            title: Some("   ".to_string()),
            ..TaskUpdate::default()
        },
    ] {
        let err = TaskService::update_with_receipt(
            &mut Storage::new(&tasks_dir),
            "TEST-1",
            patch,
            TaskUpdateContext::default(),
        )
        .expect_err("invalid patch must fail");
        assert!(!err.to_string().is_empty());
        assert_eq!(std::fs::read(&task_path).unwrap(), task_before);
        assert_eq!(mtime(&task_path), task_mtime);
    }
}

#[test]
fn pending_journal_refusal_survives_noop_detection() {
    let (_tmp, tasks_dir) = workspace("");
    let task_path = seed_task(&tasks_dir, "1.yml", RAW_TASK);
    let task_before = std::fs::read(&task_path).unwrap();
    let journal_path = tasks_dir.join(".txn-pending.json");
    std::fs::write(&journal_path, "{not json").unwrap();

    let err = TaskService::update_with_receipt(
        &mut Storage::new(&tasks_dir),
        "TEST-1",
        TaskUpdate::default(),
        TaskUpdateContext::default(),
    )
    .expect_err("pending journal must fail closed");
    assert!(
        err.to_string()
            .contains("Corrupt pending transaction journal"),
        "unexpected error: {err}"
    );
    assert_eq!(std::fs::read(&task_path).unwrap(), task_before);
    assert!(journal_path.exists());
}

#[test]
fn comment_append_is_genuine_and_same_text_comment_edit_is_a_noop() {
    let (_tmp, tasks_dir) = workspace("");
    let task_path = seed_task(&tasks_dir, "1.yml", RAW_TASK);
    let mut storage = Storage::new(&tasks_dir);

    // Appending the same text twice is intentional and genuine both times.
    TaskService::add_comment(&mut storage, "TEST-1", "note").expect("first comment");
    let dto = TaskService::add_comment(&mut storage, "TEST-1", "note").expect("second comment");
    assert_eq!(dto.comments.len(), 2);
    assert_eq!(dto.comments.iter().filter(|c| c.text == "note").count(), 2);

    // Editing a comment to its existing text writes nothing.
    let (task_before, task_mtime) = (std::fs::read(&task_path).unwrap(), mtime(&task_path));
    let dto = TaskService::update_comment(&mut storage, "TEST-1", 0, "note").expect("edit");
    assert_eq!(std::fs::read(&task_path).unwrap(), task_before);
    assert_eq!(mtime(&task_path), task_mtime);
    assert_eq!(dto.comments[0].text, "note");
}

#[test]
fn storage_edit_keeps_original_bytes_for_semantically_equal_tasks() {
    let (_tmp, tasks_dir) = workspace("");
    let task_path = seed_task(&tasks_dir, "1.yml", RAW_TASK);
    let (task_before, task_mtime) = (std::fs::read(&task_path).unwrap(), mtime(&task_path));

    // A parsed round-trip of the same data is formatting-only: the guard
    // keeps the original bytes including comments and extension layout.
    let parsed = parse_task_yaml_tolerant(RAW_TASK).expect("parse raw task");
    Storage::new(&tasks_dir)
        .edit("TEST-1", &parsed)
        .expect("edit must succeed");
    assert_eq!(std::fs::read(&task_path).unwrap(), task_before);
    assert_eq!(mtime(&task_path), task_mtime);
    assert!(task_before.starts_with(b"# User-owned header comment"));
}

#[test]
fn storage_edit_still_writes_changed_references() {
    let (_tmp, tasks_dir) = workspace("");
    let task_path = seed_task(&tasks_dir, "1.yml", RAW_TASK);
    let task_mtime = mtime(&task_path);

    let mut parsed = parse_task_yaml_tolerant(RAW_TASK).expect("parse raw task");
    parsed.references.push(ReferenceEntry {
        link: Some("https://example.com/changed".to_string()),
        ..ReferenceEntry::default()
    });
    Storage::new(&tasks_dir)
        .edit("TEST-1", &parsed)
        .expect("edit must succeed");

    let raw = std::fs::read_to_string(&task_path).unwrap();
    assert!(raw.contains("https://example.com/changed"));
    assert_ne!(mtime(&task_path), task_mtime);
}

#[test]
fn comment_edit_receipt_reports_genuine_and_idempotent_changes() {
    let (_tmp, tasks_dir) = workspace("");
    let task_path = seed_task(&tasks_dir, "1.yml", RAW_TASK);
    let mut storage = Storage::new(&tasks_dir);
    TaskService::add_comment(&mut storage, "TEST-1", "note").expect("comment");
    let (bytes_before, mtime_before) = (std::fs::read(&task_path).unwrap(), mtime(&task_path));

    // Same-text edit: idempotent no-op, receipt reports changed == false,
    // and nothing is written.
    let receipt =
        TaskService::update_comment_with_receipt(&mut storage, "TEST-1", 0, "note").expect("edit");
    assert!(!receipt.changed);
    assert_eq!(receipt.dto.comments[0].text, "note");
    assert_eq!(std::fs::read(&task_path).unwrap(), bytes_before);
    assert_eq!(mtime(&task_path), mtime_before);

    // Genuine edit: changed == true, comment#1 history recorded, file
    // rewritten.
    let receipt = TaskService::update_comment_with_receipt(&mut storage, "TEST-1", 0, "renamed")
        .expect("edit");
    assert!(receipt.changed);
    assert_eq!(receipt.dto.comments[0].text, "renamed");
    assert!(
        receipt
            .dto
            .history
            .iter()
            .any(|entry| entry.changes.iter().any(|c| c.field == "comment#1"))
    );
    assert_ne!(std::fs::read(&task_path).unwrap(), bytes_before);
    assert_ne!(mtime(&task_path), mtime_before);
}

#[test]
fn soft_delete_receipt_distinguishes_genuine_from_idempotent() {
    let (_tmp, tasks_dir) = workspace("");
    let task_path = seed_task(&tasks_dir, "1.yml", RAW_TASK);
    let other_path = seed_task(&tasks_dir, "2.yml", RAW_TASK);
    let mut storage = Storage::new(&tasks_dir);

    // Missing targets keep failing closed through the receipt variant.
    for hard in [false, true] {
        let err = TaskService::delete_with_receipt(&mut storage, "TEST-404", None, hard)
            .expect_err("missing task must fail");
        assert!(matches!(err, lotar::LoTaRError::TaskNotFound(_)));
    }

    // First soft delete is a genuine lifecycle change: tombstone written,
    // `deleted` history appended, `modified` deliberately preserved.
    let (response, changed) =
        TaskService::delete_with_receipt(&mut storage, "TEST-1", None, false).expect("delete");
    assert!(response.deleted);
    assert!(!response.hard);
    assert!(changed);
    let (bytes_after_delete, mtime_after_delete) =
        (std::fs::read(&task_path).unwrap(), mtime(&task_path));
    assert!(
        std::fs::read_to_string(&task_path)
            .unwrap()
            .contains("deleted_at:")
    );
    let deleted = TaskService::get_including_deleted(&storage, "TEST-1", None).expect("dto");
    assert!(deleted.deleted_at.is_some());
    assert_eq!(deleted.modified, "2026-01-02T00:00:00Z");
    assert_eq!(
        deleted
            .history
            .iter()
            .filter(|entry| entry.changes.iter().any(|c| c.field == "deleted"))
            .count(),
        1
    );

    // Repeated soft delete keeps the state-after body (`deleted: true`)
    // but reports changed == false without writing.
    let (response, changed) =
        TaskService::delete_with_receipt(&mut storage, "TEST-1", None, false).expect("delete");
    assert!(response.deleted);
    assert!(!changed);
    assert_eq!(std::fs::read(&task_path).unwrap(), bytes_after_delete);
    assert_eq!(mtime(&task_path), mtime_after_delete);

    // Tombstones stay read-only for regular updates.
    let err = TaskService::update_with_receipt(
        &mut storage,
        "TEST-1",
        TaskUpdate::default(),
        TaskUpdateContext::default(),
    )
    .expect_err("tombstone must refuse updates");
    assert!(err.to_string().contains("restore it before modifying it"));

    // Hard deletion is always genuine: file removed, changed == true.
    let (response, changed) =
        TaskService::delete_with_receipt(&mut storage, "TEST-2", None, true).expect("hard delete");
    assert!(response.deleted && response.hard);
    assert!(changed);
    assert!(!other_path.exists());
}

#[test]
fn restore_receipt_distinguishes_active_noop_from_genuine_restore() {
    let (_tmp, tasks_dir) = workspace("");
    let task_path = seed_task(&tasks_dir, "1.yml", RAW_TASK);
    let mut storage = Storage::new(&tasks_dir);
    let (bytes_before, mtime_before) = (std::fs::read(&task_path).unwrap(), mtime(&task_path));

    // Restoring an active task is an idempotent no-op: DTO returned,
    // changed == false, nothing written.
    let receipt = TaskService::restore_with_receipt(&mut storage, "TEST-1", None).expect("restore");
    assert!(!receipt.changed);
    assert_eq!(receipt.dto.id, "TEST-1");
    assert!(receipt.dto.deleted_at.is_none());
    assert_eq!(std::fs::read(&task_path).unwrap(), bytes_before);
    assert_eq!(mtime(&task_path), mtime_before);

    // Genuine restore after a soft delete: tombstone cleared, `restored`
    // history appended, `modified` still untouched.
    TaskService::delete(&mut storage, "TEST-1", None).expect("soft delete");
    let receipt = TaskService::restore_with_receipt(&mut storage, "TEST-1", None).expect("restore");
    assert!(receipt.changed);
    assert!(receipt.dto.deleted_at.is_none());
    assert_eq!(receipt.dto.modified, "2026-01-02T00:00:00Z");
    assert!(
        receipt
            .dto
            .history
            .iter()
            .any(|entry| entry.changes.iter().any(|c| c.field == "restored"))
    );
    assert!(
        !std::fs::read_to_string(&task_path)
            .unwrap()
            .contains("deleted_at:")
    );
}

#[test]
fn member_config_repair_is_not_persisted_by_a_noop_but_fires_on_genuine_changes() {
    let (_tmp, tasks_dir) = workspace("members: [alice]\nauto.populate_members: true\n");
    let task_path = seed_task(&tasks_dir, "1.yml", RAW_BOB_TASK);
    let config_path = seed_project_config(&tasks_dir, "project_name: TEST\nmembers:\n  - alice\n");
    let (task_before, config_before) = (
        std::fs::read(&task_path).unwrap(),
        std::fs::read(&config_path).unwrap(),
    );
    let task_mtime = mtime(&task_path);

    // The task's assignee `bob` is missing from the members, so the update
    // plans a config repair in memory — but an effective no-op must never
    // persist it (nor rewrite the task).
    let mut storage = Storage::new(&tasks_dir);
    noop_update(&mut storage, "TEST-1", TaskUpdate::default());
    assert_eq!(std::fs::read(&task_path).unwrap(), task_before);
    assert_eq!(std::fs::read(&config_path).unwrap(), config_before);
    assert_eq!(mtime(&task_path), task_mtime);

    // A genuine change keeps the existing member side effect: the config
    // repair is staged and published alongside it.
    TaskService::update(
        &mut storage,
        "TEST-1",
        TaskUpdate {
            title: Some("Renamed".to_string()),
            ..TaskUpdate::default()
        },
    )
    .expect("genuine update");
    let config_after = std::fs::read_to_string(&config_path).unwrap();
    assert!(
        config_after.contains("bob"),
        "config repair expected: {config_after}"
    );
    assert!(
        std::fs::read_to_string(&task_path)
            .unwrap()
            .contains("Renamed")
    );
}

#[test]
fn agent_directive_assignment_changes_are_genuine() {
    let (_tmp, tasks_dir) = workspace("agents:\n  copilot: command\n");
    let task_path = seed_task(&tasks_dir, "1.yml", &raw_task_with_assignee("copilot"));
    let mut storage = Storage::new(&tasks_dir);

    // Human spelling -> registered agent directive is a GENUINE change:
    // the assignment-aware comparison keeps "@copilot" and "copilot"
    // distinct, so the switch is written, not suppressed as a no-op, and
    // the public event surface observes exactly one task_updated.
    let rx = api_events::subscribe();
    let receipt = TaskService::update_with_receipt(
        &mut storage,
        "TEST-1",
        TaskUpdate {
            assignee: Some("@copilot".to_string()),
            ..TaskUpdate::default()
        },
        emitting(),
    )
    .expect("agent assignment");
    assert!(receipt.changed);
    assert_eq!(receipt.dto.assignee.as_deref(), Some("@copilot"));
    assert_eq!(updated_events(&rx), 1);
    assert!(
        std::fs::read_to_string(&task_path)
            .unwrap()
            .contains("assignee: '@copilot'")
    );

    // Same-value agent assignment is a byte-exact no-op.
    let (bytes, modified) = (std::fs::read(&task_path).unwrap(), mtime(&task_path));
    let receipt = TaskService::update_with_receipt(
        &mut storage,
        "TEST-1",
        TaskUpdate {
            assignee: Some("@copilot".to_string()),
            ..TaskUpdate::default()
        },
        TaskUpdateContext::default(),
    )
    .expect("same agent assignment");
    assert!(!receipt.changed);
    assert_eq!(std::fs::read(&task_path).unwrap(), bytes);
    assert_eq!(mtime(&task_path), modified);

    // Agent directive -> bare human spelling is genuine in the reverse
    // direction too.
    let receipt = TaskService::update_with_receipt(
        &mut storage,
        "TEST-1",
        TaskUpdate {
            assignee: Some("copilot".to_string()),
            ..TaskUpdate::default()
        },
        TaskUpdateContext::default(),
    )
    .expect("human assignment");
    assert!(receipt.changed);
    assert_eq!(receipt.dto.assignee.as_deref(), Some("copilot"));
    let raw = std::fs::read_to_string(&task_path).unwrap();
    assert!(raw.contains("assignee: copilot\n") || raw.contains("assignee: copilot'"));

    // Bare human same-value assignment stays a no-op.
    let (bytes, modified) = (std::fs::read(&task_path).unwrap(), mtime(&task_path));
    let receipt = TaskService::update_with_receipt(
        &mut storage,
        "TEST-1",
        TaskUpdate {
            assignee: Some("copilot".to_string()),
            ..TaskUpdate::default()
        },
        TaskUpdateContext::default(),
    )
    .expect("same human assignment");
    assert!(!receipt.changed);
    assert_eq!(std::fs::read(&task_path).unwrap(), bytes);
    assert_eq!(mtime(&task_path), modified);
}

#[test]
fn human_member_aliases_and_me_resolution_remain_noops() {
    let (_tmp, tasks_dir) = workspace("agents:\n  copilot: command\n");
    let task_path = seed_task(&tasks_dir, "1.yml", &raw_task_with_assignee("alice"));
    let (bytes, modified) = (std::fs::read(&task_path).unwrap(), mtime(&task_path));
    let mut storage = Storage::new(&tasks_dir);

    // @-prefixed and case-varied HUMAN spellings are still the same member.
    for alias in ["@Alice", "ALICE", "  alice  "] {
        let receipt = TaskService::update_with_receipt(
            &mut storage,
            "TEST-1",
            TaskUpdate {
                assignee: Some(alias.to_string()),
                ..TaskUpdate::default()
            },
            TaskUpdateContext::default(),
        )
        .expect("alias assignment");
        assert!(!receipt.changed, "'{alias}' must stay a no-op");
        assert_eq!(std::fs::read(&task_path).unwrap(), bytes);
        assert_eq!(mtime(&task_path), modified);
    }

    // "@me" resolving to the already-assigned current user is a no-op.
    let me = lotar::utils::identity::resolve_me_alias("@me", Some(&tasks_dir))
        .expect("identity resolution");
    let me_path = seed_task(&tasks_dir, "2.yml", &raw_task_with_assignee(&me));
    let (bytes, modified) = (std::fs::read(&me_path).unwrap(), mtime(&me_path));
    let receipt = TaskService::update_with_receipt(
        &mut storage,
        "TEST-2",
        TaskUpdate {
            assignee: Some("@me".to_string()),
            ..TaskUpdate::default()
        },
        TaskUpdateContext::default(),
    )
    .expect("me assignment");
    assert!(!receipt.changed);
    assert_eq!(std::fs::read(&me_path).unwrap(), bytes);
    assert_eq!(mtime(&me_path), modified);
}

#[test]
fn review_owner_guard_blocks_agent_identity_switches() {
    let (_tmp, tasks_dir) = workspace("agents:\n  copilot: command\n");
    seed_task(&tasks_dir, "1.yml", RAW_REVIEW_AGENT_TASK);
    let mut storage = Storage::new(&tasks_dir);

    // Switching "@copilot" -> "copilot" under review is a real assignee
    // change: only the reporter may authorize it. (The generic @-stripping
    // comparison used to treat this as "same" and let it through.)
    let err = TaskService::update_with_receipt(
        &mut storage,
        "TEST-1",
        TaskUpdate {
            assignee: Some("copilot".to_string()),
            ..TaskUpdate::default()
        },
        TaskUpdateContext::default(),
    )
    .expect_err("identity switch must be review-guarded");
    assert!(err.to_string().contains("awaiting review"), "{err}");

    // The identical assignment stays permitted (and is a no-op write-wise)
    // for a non-reporter: the guard only gates genuine changes.
    let receipt = TaskService::update_with_receipt(
        &mut storage,
        "TEST-1",
        TaskUpdate {
            assignee: Some("@copilot".to_string()),
            ..TaskUpdate::default()
        },
        TaskUpdateContext::default(),
    )
    .expect("same assignment under review");
    assert!(!receipt.changed);
    assert_eq!(receipt.dto.assignee.as_deref(), Some("@copilot"));
}

#[test]
fn markdown_description_whitespace_changes_are_genuine() {
    let (_tmp, tasks_dir) = workspace("");
    let task_path = seed_task(&tasks_dir, "1.yml", RAW_MARKDOWN_TASK);
    let mut storage = Storage::new(&tasks_dir);
    let original = "    sort(items)\nthen prose\n";

    // Byte-exact echo (indent + trailing newline) is a no-op.
    let (bytes, modified) = (std::fs::read(&task_path).unwrap(), mtime(&task_path));
    let receipt = TaskService::update_with_receipt(
        &mut storage,
        "TEST-1",
        TaskUpdate {
            description: Some(original.to_string()),
            ..TaskUpdate::default()
        },
        TaskUpdateContext::default(),
    )
    .expect("same description");
    assert!(!receipt.changed);
    assert_eq!(receipt.dto.description.as_deref(), Some(original));
    assert_eq!(std::fs::read(&task_path).unwrap(), bytes);
    assert_eq!(mtime(&task_path), modified);

    // Removing the leading four-space code indent is a genuine change
    // (trimming would have swallowed it).
    let receipt = TaskService::update_with_receipt(
        &mut storage,
        "TEST-1",
        TaskUpdate {
            description: Some("sort(items)\nthen prose\n".to_string()),
            ..TaskUpdate::default()
        },
        TaskUpdateContext::default(),
    )
    .expect("dedented description");
    assert!(receipt.changed);
    assert_eq!(
        receipt.dto.description.as_deref(),
        Some("sort(items)\nthen prose\n")
    );

    // Adding/removing trailing newlines is genuine, not trimmed away.
    let receipt = TaskService::update_with_receipt(
        &mut storage,
        "TEST-1",
        TaskUpdate {
            description: Some("sort(items)\nthen prose".to_string()),
            ..TaskUpdate::default()
        },
        TaskUpdateContext::default(),
    )
    .expect("trailing-newline change");
    assert!(receipt.changed);
    assert_eq!(
        receipt.dto.description.as_deref(),
        Some("sort(items)\nthen prose")
    );

    // An exact empty string still clears (and equals an absent field).
    let receipt = TaskService::update_with_receipt(
        &mut storage,
        "TEST-1",
        TaskUpdate {
            description: Some(String::new()),
            ..TaskUpdate::default()
        },
        TaskUpdateContext::default(),
    )
    .expect("clear description");
    assert!(receipt.changed);
    assert!(receipt.dto.description.is_none());
}

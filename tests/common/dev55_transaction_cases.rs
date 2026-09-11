//! DEV-55 fault-injection cases exercising the public service boundaries
//! (TaskService::create/update, sprint assignment) against deterministic
//! mid-publish failures injected under the transaction primitive.
//! Included from `task_service.rs` via `#[path]` so `#[cfg(test)]`-only
//! hooks stay unreachable from production code paths.

use super::*;
use crate::api_types::TaskUpdate;
use crate::services::sprint_assignment;
use crate::services::sprint_service::SprintService;
use crate::storage::sprint::{Sprint, SprintPlan};
use crate::storage::transaction::fault;
use crate::utils::paths;

fn write_global_config(tasks_dir: &std::path::Path, extra: &str) {
    let content = format!(
        "default.project: TEST\nissue.states: [Todo, InProgress, Done]\nissue.types: [Feature, Bug, Chore]\nissue.priorities: [Low, Medium, High]\n{extra}"
    );
    std::fs::create_dir_all(tasks_dir).unwrap();
    std::fs::write(paths::global_config_path(tasks_dir), content).unwrap();
}

fn labeled_sprint(label: &str) -> Sprint {
    Sprint {
        plan: Some(SprintPlan {
            label: Some(label.to_string()),
            ..SprintPlan::default()
        }),
        ..Sprint::default()
    }
}

fn read_bytes(path: &std::path::Path) -> Option<Vec<u8>> {
    std::fs::read(path).ok()
}

fn sprint_path(tasks_dir: &std::path::Path, id: u32) -> std::path::PathBuf {
    tasks_dir.join("@sprints").join(format!("{id}.yml"))
}

fn task_path(tasks_dir: &std::path::Path, id: &str) -> std::path::PathBuf {
    let project = id.split('-').next().unwrap();
    let numeric = id.split('-').nth(1).unwrap();
    tasks_dir.join(project).join(format!("{numeric}.yml"))
}

#[test]
fn create_midpublish_failure_rolls_back_task_sprint_and_config() {
    let tmp = tempfile::tempdir().unwrap();
    let tasks_dir = tmp.path().join(".tasks");
    write_global_config(&tasks_dir, "auto.populate_members: true\n");

    let mut storage = Storage::new(&tasks_dir);
    SprintService::create(&mut storage, labeled_sprint("Alpha"), None).unwrap();

    let sprint_before = read_bytes(&sprint_path(&tasks_dir, 1)).unwrap();
    let config_before = read_bytes(&tasks_dir.join("TEST").join("config.yml"));

    // Stage order is sprints -> config -> task file; fail at the task file so
    // the rollback has to restore an already published sprint and config.
    fault::fail_publish_at(2);
    let err = TaskService::create(
        &mut storage,
        TaskCreate {
            title: "Rollback me".to_string(),
            project: Some("TEST".to_string()),
            assignee: Some("brand-new-member".to_string()),
            sprints: vec![1],
            ..TaskCreate::default()
        },
    )
    .unwrap_err();
    assert!(
        err.to_string().contains("injected publish failure"),
        "{err}"
    );

    assert!(!task_path(&tasks_dir, "TEST-1").exists());
    assert_eq!(
        read_bytes(&sprint_path(&tasks_dir, 1)).unwrap(),
        sprint_before
    );
    assert_eq!(
        read_bytes(&tasks_dir.join("TEST").join("config.yml")),
        config_before
    );
    assert!(
        !tasks_dir
            .join(crate::storage::transaction::JOURNAL_FILE_NAME)
            .exists()
    );
}

#[test]
fn update_task_write_failure_rolls_back_published_sprint_bytes() {
    let tmp = tempfile::tempdir().unwrap();
    let tasks_dir = tmp.path().join(".tasks");
    write_global_config(&tasks_dir, "");

    let mut storage = Storage::new(&tasks_dir);
    SprintService::create(&mut storage, labeled_sprint("Alpha"), None).unwrap();
    let task = TaskService::create(
        &mut storage,
        TaskCreate {
            title: "Keep me".to_string(),
            project: Some("TEST".to_string()),
            description: Some("original description".to_string()),
            ..TaskCreate::default()
        },
    )
    .unwrap();

    let task_before = read_bytes(&task_path(&tasks_dir, &task.id)).unwrap();
    let sprint_before = read_bytes(&sprint_path(&tasks_dir, 1)).unwrap();

    // Stage order is sprints -> task file; fail at the task write so the
    // already published sprint membership must be rolled back exactly.
    fault::fail_publish_at(1);
    let err = TaskService::update(
        &mut storage,
        &task.id,
        TaskUpdate {
            description: Some("changed description".to_string()),
            sprints: Some(vec![1]),
            ..TaskUpdate::default()
        },
    )
    .unwrap_err();
    assert!(
        err.to_string().contains("injected publish failure"),
        "{err}"
    );

    assert_eq!(
        read_bytes(&task_path(&tasks_dir, &task.id)).unwrap(),
        task_before
    );
    assert_eq!(
        read_bytes(&sprint_path(&tasks_dir, 1)).unwrap(),
        sprint_before
    );
    assert!(
        !tasks_dir
            .join(crate::storage::transaction::JOURNAL_FILE_NAME)
            .exists()
    );

    // Retry without the fault lands both writes consistently.
    TaskService::update(
        &mut storage,
        &task.id,
        TaskUpdate {
            description: Some("changed description".to_string()),
            sprints: Some(vec![1]),
            ..TaskUpdate::default()
        },
    )
    .unwrap();
    let dto = TaskService::get(&storage, &task.id, None).unwrap();
    assert_eq!(dto.sprints, vec![1]);
    assert_eq!(dto.description.as_deref(), Some("changed description"));
}

#[test]
fn force_single_assignment_midbatch_failure_keeps_every_membership() {
    let tmp = tempfile::tempdir().unwrap();
    let tasks_dir = tmp.path().join(".tasks");
    write_global_config(&tasks_dir, "");

    let mut storage = Storage::new(&tasks_dir);
    SprintService::create(&mut storage, labeled_sprint("Alpha"), None).unwrap();
    SprintService::create(&mut storage, labeled_sprint("Beta"), None).unwrap();

    let task = TaskService::create(
        &mut storage,
        TaskCreate {
            title: "Movable".to_string(),
            project: Some("TEST".to_string()),
            sprints: vec![1],
            ..TaskCreate::default()
        },
    )
    .unwrap();

    let sprint_one_before = read_bytes(&sprint_path(&tasks_dir, 1)).unwrap();
    let sprint_two_before = read_bytes(&sprint_path(&tasks_dir, 2)).unwrap();

    let mut records = SprintService::list(&storage).unwrap();
    // Two sprint files change (remove from #1, add to #2); fail the second.
    fault::fail_publish_at(1);
    let err = sprint_assignment::assign_tasks(
        &mut storage,
        &mut records,
        std::slice::from_ref(&task.id),
        Some("2"),
        false,
        true,
    )
    .unwrap_err();
    assert!(err.contains("injected publish failure"), "{err}");

    assert_eq!(
        read_bytes(&sprint_path(&tasks_dir, 1)).unwrap(),
        sprint_one_before
    );
    assert_eq!(
        read_bytes(&sprint_path(&tasks_dir, 2)).unwrap(),
        sprint_two_before
    );
    assert!(
        !tasks_dir
            .join(crate::storage::transaction::JOURNAL_FILE_NAME)
            .exists()
    );

    // Retry succeeds and the membership moved exactly once.
    let mut records = SprintService::list(&storage).unwrap();
    sprint_assignment::assign_tasks(
        &mut storage,
        &mut records,
        std::slice::from_ref(&task.id),
        Some("2"),
        false,
        true,
    )
    .unwrap();
    let dto = TaskService::get(&storage, &task.id, None).unwrap();
    assert_eq!(dto.sprints, vec![2]);

    let sprint_one: Sprint =
        serde_yaml_ng::from_str(&std::fs::read_to_string(sprint_path(&tasks_dir, 1)).unwrap())
            .unwrap();
    assert!(sprint_one.tasks.iter().all(|entry| entry.id != task.id));
    let sprint_two: Sprint =
        serde_yaml_ng::from_str(&std::fs::read_to_string(sprint_path(&tasks_dir, 2)).unwrap())
            .unwrap();
    assert_eq!(
        sprint_two
            .tasks
            .iter()
            .filter(|entry| entry.id == task.id)
            .count(),
        1
    );
}

#[test]
fn nonparticipating_writers_refuse_while_a_journal_is_pending() {
    let tmp = tempfile::tempdir().unwrap();
    let tasks_dir = tmp.path().join(".tasks");
    write_global_config(&tasks_dir, "");

    let mut storage = Storage::new(&tasks_dir);
    let task = TaskService::create(
        &mut storage,
        TaskCreate {
            title: "Pending".to_string(),
            project: Some("TEST".to_string()),
            ..TaskCreate::default()
        },
    )
    .unwrap();

    // Leave a realistic pending journal: the update published, its journal
    // unlink failed twice (publish path and post-rollback retry), so the
    // task file is already restored but the valid journal is retained.
    fault::fail_journal_removal_times(2);
    let err = TaskService::update(
        &mut storage,
        &task.id,
        TaskUpdate {
            description: Some("pending change".to_string()),
            ..TaskUpdate::default()
        },
    )
    .unwrap_err();
    assert!(
        err.to_string().contains("injected journal removal failure"),
        "{err}"
    );
    assert!(
        tasks_dir
            .join(crate::storage::transaction::JOURNAL_FILE_NAME)
            .exists()
    );
    let pending_task = std::fs::read_to_string(task_path(&tasks_dir, &task.id)).unwrap();
    assert!(
        !pending_task.contains("pending change"),
        "error must imply an already-unchanged task file (R1): {pending_task}"
    );

    // A nonparticipating lock-taking writer (raw task edit, e.g. comments)
    // must refuse under the pending journal and leave the file untouched.
    let err = storage.edit(&task.id, &{
        let mut t = storage.get(&task.id, "TEST").unwrap();
        t.description = Some("sneaky edit".to_string());
        t
    });
    let refusal = err.err().map(|err| err.to_string());
    assert!(
        refusal
            .as_deref()
            .is_some_and(|text| text.contains("pending task/sprint transaction journal")),
        "raw edit must refuse under a pending journal, got: {refusal:?}"
    );
    assert_eq!(
        std::fs::read_to_string(task_path(&tasks_dir, &task.id)).unwrap(),
        pending_task
    );

    // The next participating mutation recovers (rolls the unacknowledged
    // change back) and then applies itself normally.
    TaskService::update(
        &mut storage,
        &task.id,
        TaskUpdate {
            description: Some("after recovery".to_string()),
            ..TaskUpdate::default()
        },
    )
    .unwrap();
    let recovered = std::fs::read_to_string(task_path(&tasks_dir, &task.id)).unwrap();
    assert!(recovered.contains("after recovery"));
    assert!(!recovered.contains("pending change"));
    assert!(!recovered.contains("sneaky edit"));
    assert!(
        !tasks_dir
            .join(crate::storage::transaction::JOURNAL_FILE_NAME)
            .exists()
    );
}

#[test]
fn cleanup_failure_leaves_caller_records_and_files_unchanged() {
    use crate::services::sprint_integrity;

    let tmp = tempfile::tempdir().unwrap();
    let tasks_dir = tmp.path().join(".tasks");
    write_global_config(&tasks_dir, "");

    let mut storage = Storage::new(&tasks_dir);
    // Task references sprint 7, which does not exist: cleanup would rewrite
    // the task file and no sprint files.
    let mut task = crate::storage::task::Task::new(
        tasks_dir.clone(),
        "Orphaned".to_string(),
        crate::types::Priority::from("Medium"),
    );
    task.sprints = vec![7];
    let id = storage.add(&task, "TEST", None).unwrap();
    let task_file = task_path(&tasks_dir, &id);
    let task_before = std::fs::read(&task_file).unwrap();

    let mut records = SprintService::list(&storage).unwrap();
    let records_before = records.clone();

    // Stage order is task file first here (no sprint files change); fail at
    // publish so rollback must restore the task bytes.
    fault::fail_publish_at(0);
    let err = sprint_integrity::cleanup_missing_sprint_refs(&mut storage, &mut records, None)
        .unwrap_err();
    assert!(
        err.to_string().contains("injected publish failure"),
        "{err}"
    );
    assert_eq!(
        records, records_before,
        "caller records must not change before commit"
    );
    assert_eq!(std::fs::read(&task_file).unwrap(), task_before);
    assert!(
        !tasks_dir
            .join(crate::storage::transaction::JOURNAL_FILE_NAME)
            .exists()
    );

    // Retry without the fault completes the cleanup.
    let outcome =
        sprint_integrity::cleanup_missing_sprint_refs(&mut storage, &mut records, None).unwrap();
    assert_eq!(outcome.updated_tasks, 1);
    assert_eq!(outcome.removed_references, 1);
    assert!(
        !std::fs::read_to_string(&task_file)
            .unwrap()
            .contains("sprints:")
    );
}

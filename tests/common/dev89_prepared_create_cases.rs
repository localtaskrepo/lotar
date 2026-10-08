//! DEV-89 fault-injection cases for `TaskService::commit_prepared_create`,
//! the prepared-create commit path the CLI `add` / `task add` handlers use.
//! Included from `task_service.rs` via `#[path]` so `#[cfg(test)]`-only
//! hooks stay unreachable from production code paths.

use super::*;
use crate::storage::transaction::fault;
use crate::utils::paths;

fn write_global_config(tasks_dir: &std::path::Path, extra: &str) {
    let content = format!(
        "default.project: TEST\nissue.states: [Todo, InProgress, Done]\nissue.types: [Feature, Bug, Chore]\nissue.priorities: [Low, Medium, High]\n{extra}"
    );
    std::fs::create_dir_all(tasks_dir).unwrap();
    std::fs::write(paths::global_config_path(tasks_dir), content).unwrap();
}

fn prepared_task(tasks_dir: &std::path::Path, reporter: &str) -> Task {
    let mut task = Task::new(
        tasks_dir.to_path_buf(),
        "Prepared".to_string(),
        Priority::from("Medium"),
    );
    task.reporter = Some(reporter.to_string());
    task.status = TaskStatus::from("Todo");
    task
}

#[test]
fn prepared_create_midpublish_failure_rolls_back_config_bytes_and_created_task() {
    let tmp = tempfile::tempdir().unwrap();
    let tasks_dir = tmp.path().join(".tasks");
    write_global_config(
        &tasks_dir,
        "members: [alice]\nauto.populate_members: true\n",
    );

    let storage = Storage::new(&tasks_dir);
    let config_path = tasks_dir.join("TEST").join("config.yml");
    std::fs::create_dir_all(config_path.parent().unwrap()).unwrap();
    let original_config = "project_name: TEST\nmembers:\n  - alice\n";
    std::fs::write(&config_path, original_config).unwrap();
    let config_before = std::fs::read(&config_path).unwrap();

    let task = prepared_task(&tasks_dir, "bob");
    let validation_config = TaskService::resolve_config_for_project(&tasks_dir, "TEST");
    let base_members = vec!["alice".to_string()];
    let pending = vec!["bob".to_string()];

    // Stage order is config then the task file; fail at the task file so the
    // rollback has to restore the already published member mutation.
    fault::fail_publish_at(1);
    let err = TaskService::commit_prepared_create(
        &storage,
        PreparedTaskCreate {
            task: &task,
            project: "TEST",
            original_project_name: None,
            validation_config: &validation_config,
            base_members: &base_members,
            pending_members: &pending,
        },
    )
    .unwrap_err();
    assert!(
        err.to_string().contains("injected publish failure"),
        "{err}"
    );

    assert_eq!(std::fs::read(&config_path).unwrap(), config_before);
    assert_eq!(
        String::from_utf8_lossy(&std::fs::read(&config_path).unwrap()),
        original_config
    );
    assert!(!tasks_dir.join("TEST").join("1.yml").exists());
    assert!(
        !tasks_dir
            .join(crate::storage::transaction::JOURNAL_FILE_NAME)
            .exists()
    );
}

#[test]
fn prepared_create_midpublish_failure_removes_config_file_it_created() {
    let tmp = tempfile::tempdir().unwrap();
    let tasks_dir = tmp.path().join(".tasks");
    write_global_config(
        &tasks_dir,
        "members: [alice]\nauto.populate_members: true\n",
    );

    let storage = Storage::new(&tasks_dir);
    let config_path = tasks_dir.join("FRESH").join("config.yml");

    let task = prepared_task(&tasks_dir, "bob");
    let validation_config = TaskService::resolve_config_for_project(&tasks_dir, "FRESH");
    let base_members = vec!["alice".to_string()];
    let pending = vec!["bob".to_string()];

    // The project config does not exist yet: the naming rule plus the member
    // merge stage its creation, and the mid-publish rollback must remove it
    // again together with the task file.
    fault::fail_publish_at(1);
    let err = TaskService::commit_prepared_create(
        &storage,
        PreparedTaskCreate {
            task: &task,
            project: "FRESH",
            original_project_name: Some("Alpha Tools"),
            validation_config: &validation_config,
            base_members: &base_members,
            pending_members: &pending,
        },
    )
    .unwrap_err();
    assert!(
        err.to_string().contains("injected publish failure"),
        "{err}"
    );

    assert!(
        !config_path.exists(),
        "config file the transaction created must be removed on rollback"
    );
    assert!(!tasks_dir.join("FRESH").join("1.yml").exists());
    assert!(
        !tasks_dir
            .join(crate::storage::transaction::JOURNAL_FILE_NAME)
            .exists()
    );
}

#[test]
fn prepared_create_commits_config_members_and_task_together() {
    let tmp = tempfile::tempdir().unwrap();
    let tasks_dir = tmp.path().join(".tasks");
    write_global_config(
        &tasks_dir,
        "members: [alice]\nauto.populate_members: true\n",
    );

    let storage = Storage::new(&tasks_dir);
    let config_path = tasks_dir.join("TEST").join("config.yml");
    std::fs::create_dir_all(config_path.parent().unwrap()).unwrap();
    std::fs::write(&config_path, "project_name: TEST\nmembers:\n  - alice\n").unwrap();

    let task = prepared_task(&tasks_dir, "bob");
    let validation_config = TaskService::resolve_config_for_project(&tasks_dir, "TEST");
    let base_members = vec!["alice".to_string()];
    let pending = vec!["bob".to_string()];

    let id = TaskService::commit_prepared_create(
        &storage,
        PreparedTaskCreate {
            task: &task,
            project: "TEST",
            original_project_name: None,
            validation_config: &validation_config,
            base_members: &base_members,
            pending_members: &pending,
        },
    )
    .unwrap();

    assert_eq!(id, "TEST-1");
    let committed = std::fs::read_to_string(&config_path).unwrap();
    assert!(
        committed.contains("bob"),
        "member merge must be persisted: {committed}"
    );
    assert!(committed.contains("alice"), "{committed}");
    let task_yaml = serde_yaml_ng::from_str::<serde_yaml_ng::Value>(
        &std::fs::read_to_string(tasks_dir.join("TEST").join("1.yml")).unwrap(),
    )
    .unwrap();
    assert_eq!(
        task_yaml.get("reporter").and_then(|v| v.as_str()),
        Some("bob")
    );
}

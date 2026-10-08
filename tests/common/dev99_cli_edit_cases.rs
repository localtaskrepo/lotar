//! DEV-99 fault-injection case for the CLI `task edit` handler: a mid-publish
//! failure of the coordinated edit transaction must leave the auto-populated
//! project config and the task file byte-identical and surface an error.
//! Included from src/cli/handlers/task/edit.rs via `#[path]` so the
//! `#[cfg(test)]`-only transaction fault hooks stay unreachable from
//! production code paths.

use super::*;
use crate::output::{LogLevel, OutputFormat};
use crate::storage::transaction::fault;

fn write_global_config(tasks_dir: &std::path::Path, extra: &str) {
    let content = format!(
        "default.project: TEST\nissue.states: [Todo, InProgress, Done]\nissue.types: [Feature, Bug, Chore]\nissue.priorities: [Low, Medium, High]\n{extra}"
    );
    std::fs::create_dir_all(tasks_dir).unwrap();
    std::fs::write(crate::utils::paths::global_config_path(tasks_dir), content).unwrap();
}

fn edit_args(id: &str, title: Option<&str>, reporter: Option<&str>) -> TaskEditArgs {
    TaskEditArgs {
        id: id.to_string(),
        title: title.map(str::to_string),
        task_type: None,
        priority: None,
        reporter: reporter.map(str::to_string),
        assignee: None,
        effort: None,
        due: None,
        description: None,
        tags: Vec::new(),
        fields: Vec::new(),
        dry_run: false,
    }
}

/// The staged order of a member-populating edit is the project config (index
/// 0) followed by the task file (index 1); failing the task publish must roll
/// the already published member mutation back and leave the task untouched
/// (DEV-99).
#[test]
fn edit_midpublish_failure_rolls_back_config_and_task_bytes() {
    let tmp = tempfile::tempdir().unwrap();
    let tasks_dir = tmp.path().join(".tasks");
    write_global_config(
        &tasks_dir,
        "members: [alice]\nauto.populate_members: true\n",
    );

    let project_dir = tasks_dir.join("TEST");
    std::fs::create_dir_all(&project_dir).unwrap();
    std::fs::write(
        project_dir.join("1.yml"),
        "title: Seed\nstatus: Todo\npriority: Medium\ntype: Feature\nreporter: alice\ncreated: 2026-01-01T00:00:00Z\n",
    )
    .unwrap();
    std::fs::write(
        project_dir.join("config.yml"),
        "project_name: TEST\nmembers:\n  - alice\n",
    )
    .unwrap();
    let config_path = project_dir.join("config.yml");
    let task_path = project_dir.join("1.yml");
    let config_before = std::fs::read(&config_path).unwrap();
    let task_before = std::fs::read(&task_path).unwrap();

    let resolver =
        TasksDirectoryResolver::resolve(Some(tasks_dir.to_str().unwrap()), None).unwrap();
    let renderer = crate::output::OutputRenderer::new(OutputFormat::Text, LogLevel::Error);

    fault::fail_publish_at(1);
    let err = EditHandler::execute(
        edit_args("TEST-1", Some("Renamed"), Some("bob")),
        Some("TEST"),
        &resolver,
        &renderer,
    )
    .expect_err("mid-publish failure must fail the edit");

    assert!(
        err.contains("injected publish failure"),
        "unexpected error: {err}"
    );
    assert_eq!(
        std::fs::read(&config_path).unwrap(),
        config_before,
        "config bytes must be unchanged after the failed edit"
    );
    assert_eq!(
        std::fs::read(&task_path).unwrap(),
        task_before,
        "task bytes must be unchanged after the failed edit"
    );
    assert!(
        !tasks_dir
            .join(crate::storage::transaction::JOURNAL_FILE_NAME)
            .exists()
    );
}

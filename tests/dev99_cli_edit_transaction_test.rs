//! DEV-99 regressions: the CLI `task edit` handler must run member
//! auto-population, task changes, and history through the coordinated update
//! transaction, so service validation failures, staging failures, and journal
//! refusals leave the project configuration and task files untouched. All
//! cases run against the real `lotar` binary at the public CLI boundary.

mod common;

use common::lotar_cmd;
use std::path::{Path, PathBuf};
use std::process::Output;

fn workspace() -> (tempfile::TempDir, PathBuf) {
    let tmp = tempfile::tempdir().unwrap();
    let tasks_dir = tmp.path().join(".tasks");
    std::fs::create_dir_all(&tasks_dir).unwrap();
    (tmp, tasks_dir)
}

fn write_global_config(tasks_dir: &Path, extra: &str) {
    let content = format!(
        "default.project: TEST\nissue.states: [Todo, InProgress, Done]\nissue.types: [Feature, Bug, Chore]\nissue.priorities: [Low, Medium, High]\n{extra}"
    );
    std::fs::write(tasks_dir.join("config.yml"), content).unwrap();
}

fn seed_project(tasks_dir: &Path, prefix: &str, task_no: &str) -> PathBuf {
    let project_dir = tasks_dir.join(prefix);
    std::fs::create_dir_all(&project_dir).unwrap();
    std::fs::write(
        project_dir.join(format!("{task_no}.yml")),
        "title: Seed\nstatus: Todo\npriority: Medium\ntype: Feature\nreporter: alice\ncreated: 2026-01-01T00:00:00Z\n",
    )
    .unwrap();
    std::fs::write(
        project_dir.join("config.yml"),
        format!("project_name: {prefix}\nmembers:\n  - alice\n"),
    )
    .unwrap();
    project_dir
}

fn run(dir: &Path, args: &[&str]) -> Output {
    let mut cmd = lotar_cmd().expect("binary 'lotar' not found");
    cmd.env("LOTAR_TEST_SILENT", "1")
        .args(args)
        .current_dir(dir)
        .output()
        .expect("failed to run lotar")
}

fn stdout(output: &Output) -> String {
    String::from_utf8_lossy(&output.stdout).to_string()
}

fn stderr(output: &Output) -> String {
    String::from_utf8_lossy(&output.stderr).to_string()
}

fn parse_yaml(path: &Path) -> serde_yaml_ng::Value {
    serde_yaml_ng::from_str(&std::fs::read_to_string(path).unwrap()).unwrap()
}

fn yaml_str(value: &serde_yaml_ng::Value, key: &str) -> String {
    value
        .get(key)
        .and_then(|v| v.as_str())
        .unwrap_or_default()
        .to_string()
}

fn members_of(value: &serde_yaml_ng::Value) -> Vec<String> {
    value
        .get("members")
        .and_then(|v| v.as_sequence())
        .map(|seq| {
            seq.iter()
                .filter_map(|v| v.as_str().map(str::to_string))
                .collect()
        })
        .unwrap_or_default()
}

/// Every (field, old, new) change recorded in the task's history.
fn history_changes(task: &serde_yaml_ng::Value) -> Vec<(String, String, String)> {
    task.get("history")
        .and_then(|v| v.as_sequence())
        .map(|entries| {
            entries
                .iter()
                .flat_map(|entry| {
                    entry
                        .get("changes")
                        .and_then(|v| v.as_sequence())
                        .map(|changes| {
                            changes.iter().map(|change| {
                                (
                                    yaml_str(change, "field"),
                                    yaml_str(change, "old"),
                                    yaml_str(change, "new"),
                                )
                            })
                        })
                })
                .flatten()
                .collect()
        })
        .unwrap_or_default()
}

fn journal_exists(tasks_dir: &Path) -> bool {
    tasks_dir.join(".txn-pending.json").exists()
}

fn snapshot(tasks_dir: &Path) -> Vec<(PathBuf, Option<Vec<u8>>)> {
    fn walk(dir: &Path, out: &mut Vec<(PathBuf, Option<Vec<u8>>)>) {
        let mut entries: Vec<PathBuf> = std::fs::read_dir(dir)
            .expect("readable")
            .flatten()
            .map(|entry| entry.path())
            .collect();
        entries.sort();
        for path in entries {
            if path.is_dir() {
                walk(&path, out);
            } else {
                out.push((path.clone(), std::fs::read(&path).ok()));
            }
        }
    }
    let mut files = Vec::new();
    walk(tasks_dir, &mut files);
    files
}

/// A service-level validation failure (`--title ""` is only rejected inside
/// `TaskService::update`) must not leave the auto-populated member mutation
/// behind: historically the CLI persisted the project config members before
/// the service validated the patch (DEV-99 legacy side effect).
#[test]
fn cli_edit_failed_title_validation_rolls_back_auto_populated_members() {
    let (tmp, tasks_dir) = workspace();
    write_global_config(
        &tasks_dir,
        "members: [alice]\nauto.populate_members: true\n",
    );
    let project_dir = seed_project(&tasks_dir, "TEST", "1");
    let config_path = project_dir.join("config.yml");
    let task_path = project_dir.join("1.yml");
    let config_before = std::fs::read(&config_path).unwrap();
    let task_before = std::fs::read(&task_path).unwrap();

    let output = run(
        tmp.path(),
        &[
            "--project",
            "TEST",
            "task",
            "edit",
            "TEST-1",
            "--title",
            "",
            "--reporter",
            "bob",
        ],
    );
    assert!(
        !output.status.success(),
        "empty title must fail: {}{}",
        stdout(&output),
        stderr(&output)
    );
    assert!(
        stderr(&output).contains("Title cannot be empty"),
        "unexpected error: {}{}",
        stdout(&output),
        stderr(&output)
    );

    assert_eq!(
        std::fs::read(&config_path).unwrap(),
        config_before,
        "failed validation must not persist the auto-populated member"
    );
    assert_eq!(std::fs::read(&task_path).unwrap(), task_before);
    assert!(!journal_exists(&tasks_dir));
}

/// A readable symlink at the task file target is rejected by the coordinated
/// transaction's staging rules; the rejection must surface without the member
/// config side effect (DEV-99; Unix-only staging discipline).
#[test]
#[cfg(unix)]
fn cli_edit_symlinked_task_target_rejects_stage_without_config_side_effect() {
    let (tmp, tasks_dir) = workspace();
    write_global_config(
        &tasks_dir,
        "members: [alice]\nauto.populate_members: true\n",
    );
    let project_dir = seed_project(&tasks_dir, "TEST", "1");
    let config_path = project_dir.join("config.yml");
    let task_path = project_dir.join("1.yml");
    let config_before = std::fs::read(&config_path).unwrap();
    let task_before = std::fs::read(&task_path).unwrap();

    // Keep the task readable (storage reads through the symlink) while the
    // coordinated stage must refuse the symlinked target itself.
    let real = tmp.path().join("1.real.yml");
    std::fs::rename(&task_path, &real).unwrap();
    std::os::unix::fs::symlink(&real, &task_path).unwrap();

    let output = run(
        tmp.path(),
        &[
            "--project",
            "TEST",
            "task",
            "edit",
            "TEST-1",
            "--title",
            "Renamed",
            "--reporter",
            "bob",
        ],
    );
    assert!(
        !output.status.success(),
        "symlinked task target must be refused: {}{}",
        stdout(&output),
        stderr(&output)
    );
    assert!(
        stderr(&output).to_lowercase().contains("symlink"),
        "unexpected error: {}{}",
        stdout(&output),
        stderr(&output)
    );

    assert_eq!(
        std::fs::read(&config_path).unwrap(),
        config_before,
        "stage rejection must not persist the auto-populated member"
    );
    assert!(
        std::fs::symlink_metadata(&task_path)
            .expect("task path present")
            .is_symlink(),
        "task target must remain the untouched symlink"
    );
    assert_eq!(std::fs::read(&real).unwrap(), task_before);
    assert!(!journal_exists(&tasks_dir));
}

/// A corrupt pending-recovery journal makes every participating mutation fail
/// closed; the CLI edit must reach that refusal without writing the member
/// config first (the legacy pre-write bypassed the journal check entirely).
#[test]
fn cli_edit_corrupt_pending_journal_refused_without_config_side_effect() {
    let (tmp, tasks_dir) = workspace();
    write_global_config(
        &tasks_dir,
        "members: [alice]\nauto.populate_members: true\n",
    );
    let project_dir = seed_project(&tasks_dir, "TEST", "1");
    let config_path = project_dir.join("config.yml");
    let task_path = project_dir.join("1.yml");
    let config_before = std::fs::read(&config_path).unwrap();
    let task_before = std::fs::read(&task_path).unwrap();

    std::fs::write(tasks_dir.join(".txn-pending.json"), "not a journal").unwrap();

    let output = run(
        tmp.path(),
        &[
            "--project",
            "TEST",
            "task",
            "edit",
            "TEST-1",
            "--title",
            "Renamed",
            "--reporter",
            "bob",
        ],
    );
    assert!(
        !output.status.success(),
        "pending journal must fail the edit closed: {}{}",
        stdout(&output),
        stderr(&output)
    );
    assert!(
        stderr(&output)
            .to_lowercase()
            .contains("pending transaction"),
        "unexpected error: {}{}",
        stdout(&output),
        stderr(&output)
    );

    assert_eq!(
        std::fs::read(&config_path).unwrap(),
        config_before,
        "journal refusal must not persist the auto-populated member"
    );
    assert_eq!(std::fs::read(&task_path).unwrap(), task_before);
    assert!(
        journal_exists(&tasks_dir),
        "the corrupt journal must remain for manual reconciliation"
    );
}

/// Dry runs preview member-populating edits (including unknown members) in
/// memory only; no file anywhere in the tasks tree may change.
#[test]
fn cli_edit_dry_run_unknown_members_writes_no_files() {
    let (tmp, tasks_dir) = workspace();
    write_global_config(
        &tasks_dir,
        "members: [alice]\nauto.populate_members: true\nstrict_members: true\n",
    );
    seed_project(&tasks_dir, "TEST", "1");
    let before = snapshot(&tasks_dir);

    let output = run(
        tmp.path(),
        &[
            "--project",
            "TEST",
            "task",
            "edit",
            "TEST-1",
            "-n",
            "--title",
            "Preview",
            "--reporter",
            "newbie",
            "--assignee",
            "newtwo",
        ],
    );
    assert!(
        output.status.success(),
        "dry run must succeed: {}{}",
        stdout(&output),
        stderr(&output)
    );
    assert!(stdout(&output).contains("DRY RUN"), "{}", stdout(&output));

    assert_eq!(snapshot(&tasks_dir), before, "dry run must write nothing");
    assert!(!tasks_dir.join("@sprints").exists());

    let output = run(
        tmp.path(),
        &[
            "--format",
            "json",
            "task",
            "edit",
            "TEST-1",
            "-n",
            "--reporter",
            "newbie",
            "--assignee",
            "newtwo",
        ],
    );
    assert!(output.status.success(), "{}", stderr(&output));
    let preview: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(preview["status"], "preview");
    assert_eq!(preview["action"], "edit");
    assert_eq!(preview["task_id"], "TEST-1");
    assert_eq!(preview["assignee"], "newtwo");
    assert_eq!(snapshot(&tasks_dir), before);
}

/// A successful member-populating edit persists the task fields, the history
/// entry, and the merged project members together, leaving no journal.
#[test]
fn cli_edit_commits_task_members_and_history_together() {
    let (tmp, tasks_dir) = workspace();
    write_global_config(
        &tasks_dir,
        "members: [alice]\nauto.populate_members: true\nstrict_members: true\n",
    );
    let project_dir = seed_project(&tasks_dir, "TEST", "1");

    let output = run(
        tmp.path(),
        &[
            "--project",
            "TEST",
            "task",
            "edit",
            "TEST-1",
            "--title",
            "Renamed",
            "--reporter",
            "bob",
            "--assignee",
            "carol",
        ],
    );
    assert!(
        output.status.success(),
        "{}{}",
        stdout(&output),
        stderr(&output)
    );

    let task = parse_yaml(&project_dir.join("1.yml"));
    assert_eq!(yaml_str(&task, "title"), "Renamed");
    assert_eq!(yaml_str(&task, "reporter"), "bob");
    assert_eq!(yaml_str(&task, "assignee"), "carol");
    let changes = history_changes(&task);
    assert!(
        changes.contains(&(
            "title".to_string(),
            "Seed".to_string(),
            "Renamed".to_string()
        )),
        "history must record the title change: {changes:?}"
    );
    assert!(
        changes.contains(&(
            "reporter".to_string(),
            "alice".to_string(),
            "bob".to_string()
        )),
        "history must record the reporter change: {changes:?}"
    );

    let config = parse_yaml(&project_dir.join("config.yml"));
    assert_eq!(
        members_of(&config),
        vec!["alice".to_string(), "bob".to_string(), "carol".to_string()],
        "merged members must be committed with the task"
    );
    assert!(!journal_exists(&tasks_dir));
}

/// With auto-population disabled, an edit never mutates the project config:
/// under `strict_members` an unknown member is rejected outright, and without
/// it the unknown member is accepted onto the task while the config bytes
/// stay untouched (member enforcement is gated by `strict_members`, not by
/// `auto.populate_members`).
#[test]
fn cli_edit_auto_populate_disabled_keeps_project_config_untouched() {
    // strict_members: unknown members are rejected before any write.
    let (tmp, tasks_dir) = workspace();
    write_global_config(
        &tasks_dir,
        "members: [alice]\nauto.populate_members: false\nstrict_members: true\n",
    );
    let project_dir = seed_project(&tasks_dir, "TEST", "1");
    let config_path = project_dir.join("config.yml");
    let task_path = project_dir.join("1.yml");
    let config_before = std::fs::read(&config_path).unwrap();
    let task_before = std::fs::read(&task_path).unwrap();

    let output = run(
        tmp.path(),
        &[
            "--project",
            "TEST",
            "task",
            "edit",
            "TEST-1",
            "--title",
            "Nope",
            "--reporter",
            "bob",
        ],
    );
    assert!(
        !output.status.success(),
        "unknown member must be rejected: {}{}",
        stdout(&output),
        stderr(&output)
    );
    assert!(
        stderr(&output).contains("'bob' is not in configured members"),
        "unexpected error: {}{}",
        stdout(&output),
        stderr(&output)
    );
    assert_eq!(std::fs::read(&config_path).unwrap(), config_before);
    assert_eq!(std::fs::read(&task_path).unwrap(), task_before);
    assert!(!journal_exists(&tasks_dir));

    // Without strict_members the unknown member lands on the task, but the
    // project config must not gain it.
    let (tmp, tasks_dir) = workspace();
    write_global_config(
        &tasks_dir,
        "members: [alice]\nauto.populate_members: false\n",
    );
    let project_dir = seed_project(&tasks_dir, "TEST", "1");
    let config_path = project_dir.join("config.yml");
    let config_before = std::fs::read(&config_path).unwrap();

    let output = run(
        tmp.path(),
        &[
            "--project",
            "TEST",
            "task",
            "edit",
            "TEST-1",
            "--title",
            "Renamed",
            "--reporter",
            "bob",
        ],
    );
    assert!(
        output.status.success(),
        "{}{}",
        stdout(&output),
        stderr(&output)
    );
    assert_eq!(
        std::fs::read(&config_path).unwrap(),
        config_before,
        "auto-population disabled must leave the config bytes untouched"
    );
    let task = parse_yaml(&project_dir.join("1.yml"));
    assert_eq!(yaml_str(&task, "title"), "Renamed");
    assert_eq!(yaml_str(&task, "reporter"), "bob");
    assert!(!journal_exists(&tasks_dir));
}

/// Member auto-population is scoped to the edited task's project: editing
/// OTHER-2 through its numeric alias must not touch the TEST project config.
#[test]
fn cli_edit_scopes_member_population_to_edited_task_project() {
    let (tmp, tasks_dir) = workspace();
    write_global_config(
        &tasks_dir,
        "members: [alice]\nauto.populate_members: true\n",
    );
    let test_dir = seed_project(&tasks_dir, "TEST", "1");
    let other_dir = seed_project(&tasks_dir, "OTHER", "2");
    let test_config_before = std::fs::read(test_dir.join("config.yml")).unwrap();

    let output = run(
        tmp.path(),
        &[
            "--project",
            "OTHER",
            "task",
            "edit",
            "2",
            "--title",
            "Renamed",
            "--reporter",
            "bob",
        ],
    );
    assert!(
        output.status.success(),
        "{}{}",
        stdout(&output),
        stderr(&output)
    );

    let task = parse_yaml(&other_dir.join("2.yml"));
    assert_eq!(yaml_str(&task, "title"), "Renamed");
    assert_eq!(yaml_str(&task, "reporter"), "bob");
    assert_eq!(
        members_of(&parse_yaml(&other_dir.join("config.yml"))),
        vec!["alice".to_string(), "bob".to_string()]
    );
    assert_eq!(
        std::fs::read(test_dir.join("config.yml")).unwrap(),
        test_config_before,
        "unrelated project config must stay untouched"
    );
}

/// Reporter/assignee spellings that case-insensitively match configured
/// members are deduplicated: the edit succeeds without rewriting the project
/// config or duplicating member entries.
#[test]
fn cli_edit_existing_member_case_variant_writes_no_config() {
    let (tmp, tasks_dir) = workspace();
    write_global_config(
        &tasks_dir,
        "members: [alice]\nauto.populate_members: true\n",
    );
    let project_dir = seed_project(&tasks_dir, "TEST", "1");
    let config_path = project_dir.join("config.yml");
    std::fs::write(
        &config_path,
        "project_name: TEST\nmembers:\n  - alice\n  - bob\n",
    )
    .unwrap();
    let config_before = std::fs::read(&config_path).unwrap();

    let output = run(
        tmp.path(),
        &[
            "--project",
            "TEST",
            "task",
            "edit",
            "TEST-1",
            "--reporter",
            "BOB",
            "--assignee",
            "Bob",
        ],
    );
    assert!(
        output.status.success(),
        "{}{}",
        stdout(&output),
        stderr(&output)
    );

    assert_eq!(
        std::fs::read(&config_path).unwrap(),
        config_before,
        "case-insensitive member match must not rewrite the config"
    );
    let task = parse_yaml(&project_dir.join("1.yml"));
    assert_eq!(yaml_str(&task, "reporter"), "BOB");
    assert_eq!(yaml_str(&task, "assignee"), "Bob");
}

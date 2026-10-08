//! DEV-89 regressions: the CLI `add` / `task add` handlers must create the
//! task and its auto-populated project configuration through the coordinated
//! transaction, so validation failures, staging failures, and publish
//! failures leave the configuration and task files untouched. All cases run
//! against the real `lotar` binary at the public CLI boundary.

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

fn seed_project(tasks_dir: &Path, prefix: &str) -> PathBuf {
    let project_dir = tasks_dir.join(prefix);
    std::fs::create_dir_all(&project_dir).unwrap();
    std::fs::write(
        project_dir.join("1.yml"),
        "title: Seed\nstatus: Todo\npriority: Medium\ntype: Feature\ncreated: 2026-01-01T00:00:00Z\n",
    )
    .unwrap();
    std::fs::write(
        project_dir.join("config.yml"),
        "project_name: TEST\nmembers:\n  - alice\n",
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

fn task_files(project_dir: &Path) -> Vec<String> {
    let mut names: Vec<String> = std::fs::read_dir(project_dir)
        .expect("project dir readable")
        .flatten()
        .filter(|entry| entry.file_type().is_ok_and(|kind| kind.is_file()))
        .map(|entry| entry.file_name().to_string_lossy().to_string())
        .filter(|name| name.ends_with(".yml") && name != "config.yml")
        .collect();
    names.sort();
    names
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

/// Canonical project configs nest the name under `project.name`; hand-seeded
/// fixtures may use the root-level `project_name` spelling.
fn project_name_of(value: &serde_yaml_ng::Value) -> String {
    value
        .get("project")
        .and_then(|p| p.get("name"))
        .and_then(|v| v.as_str())
        .or_else(|| value.get("project_name").and_then(|v| v.as_str()))
        .unwrap_or_default()
        .to_string()
}

/// Failing task creation must not leave the auto-populated member mutation
/// behind, even when the failure only surfaces at the write itself (the
/// next task file target is unwritable: a directory in its place).
#[test]
fn cli_add_write_failure_rolls_back_auto_populated_members() {
    let (tmp, tasks_dir) = workspace();
    write_global_config(
        &tasks_dir,
        "members: [alice]\nauto.populate_members: true\n",
    );
    let project_dir = seed_project(&tasks_dir, "TEST");
    let config_path = project_dir.join("config.yml");
    let config_before = std::fs::read(&config_path).unwrap();

    // The next numeric id is 2; a directory at that target makes the task
    // file write fail while the member merge is already staged.
    std::fs::create_dir_all(project_dir.join("2.yml")).unwrap();

    let output = run(
        tmp.path(),
        &["--project", "TEST", "add", "Boom", "--reporter", "bob"],
    );
    assert!(
        !output.status.success(),
        "write must fail: {}",
        stdout(&output)
    );
    assert!(
        stderr(&output).contains("Storage error while creating task in TEST"),
        "unexpected error: {}{}",
        stdout(&output),
        stderr(&output)
    );

    assert_eq!(
        std::fs::read(&config_path).unwrap(),
        config_before,
        "config bytes must be unchanged after the failed create"
    );
    assert_eq!(
        task_files(&project_dir),
        vec!["1.yml".to_string()],
        "no task file may appear"
    );
    assert!(project_dir.join("2.yml").is_dir());
    assert!(!journal_exists(&tasks_dir));
}

/// Same invariant through the `task add` subcommand boundary.
#[test]
fn cli_task_add_write_failure_rolls_back_auto_populated_members() {
    let (tmp, tasks_dir) = workspace();
    write_global_config(
        &tasks_dir,
        "members: [alice]\nauto.populate_members: true\n",
    );
    let project_dir = seed_project(&tasks_dir, "TEST");
    let config_path = project_dir.join("config.yml");
    let config_before = std::fs::read(&config_path).unwrap();

    std::fs::create_dir_all(project_dir.join("2.yml")).unwrap();

    let output = run(
        tmp.path(),
        &[
            "--project",
            "TEST",
            "task",
            "add",
            "Boom",
            "--reporter",
            "dave",
        ],
    );
    assert!(
        !output.status.success(),
        "write must fail: {}",
        stdout(&output)
    );
    assert_eq!(std::fs::read(&config_path).unwrap(), config_before);
    assert_eq!(task_files(&project_dir), vec!["1.yml".to_string()]);
    assert!(!journal_exists(&tasks_dir));
}

/// An invalid explicit --status must be rejected before any configuration
/// side effect; historically the auto-populated members were persisted
/// before status validation ran.
#[test]
fn cli_add_invalid_status_rejects_before_config_mutation() {
    let (tmp, tasks_dir) = workspace();
    write_global_config(
        &tasks_dir,
        "members: [alice]\nauto.populate_members: true\n",
    );
    let project_dir = seed_project(&tasks_dir, "TEST");
    let config_path = project_dir.join("config.yml");
    let config_before = std::fs::read(&config_path).unwrap();

    let output = run(
        tmp.path(),
        &[
            "--project",
            "TEST",
            "add",
            "Bad status",
            "--status",
            "Bogus",
            "--reporter",
            "bob",
        ],
    );
    assert!(!output.status.success());
    assert!(
        stderr(&output).contains("Status validation failed"),
        "unexpected error: {}{}",
        stdout(&output),
        stderr(&output)
    );

    assert_eq!(std::fs::read(&config_path).unwrap(), config_before);
    assert_eq!(task_files(&project_dir), vec!["1.yml".to_string()]);
    assert!(!journal_exists(&tasks_dir));
}

/// Dry runs preview without touching any file, including the in-memory
/// member merge and invalid input cases that fail validation.
#[test]
fn cli_add_dry_run_writes_no_files_or_config() {
    let (tmp, tasks_dir) = workspace();
    write_global_config(
        &tasks_dir,
        "members: [alice]\nauto.populate_members: true\n",
    );
    seed_project(&tasks_dir, "TEST");
    let before = snapshot(&tasks_dir);

    let output = run(
        tmp.path(),
        &[
            "--project",
            "TEST",
            "add",
            "-n",
            "Preview",
            "--reporter",
            "bob",
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

    // Invalid input under dry run still writes nothing.
    let output = run(
        tmp.path(),
        &["--project", "TEST", "add", "-n", "Bad", "--status", "Bogus"],
    );
    assert!(!output.status.success());
    assert_eq!(snapshot(&tasks_dir), before);
}

/// A project name (not a prefix) generates the storage prefix and records
/// the original name in the created project config; a short explicit name
/// is used as the prefix directly and still creates the config file, exactly
/// like the legacy creation path did.
#[test]
fn cli_add_records_original_project_name_and_custom_prefix() {
    let (tmp, tasks_dir) = workspace();
    write_global_config(&tasks_dir, "");

    let output = run(tmp.path(), &["--project", "Alpha Tools", "add", "Named"]);
    assert!(
        output.status.success(),
        "{}{}",
        stdout(&output),
        stderr(&output)
    );
    assert!(stdout(&output).contains("AT-1"), "{}", stdout(&output));
    let config = parse_yaml(&tasks_dir.join("AT").join("config.yml"));
    assert_eq!(project_name_of(&config), "Alpha Tools");
    assert!(tasks_dir.join("AT").join("1.yml").is_file());

    let output = run(
        tmp.path(),
        &["--project", "Web Marketplace", "add", "Second"],
    );
    assert!(output.status.success());
    assert!(stdout(&output).contains("WM-1"), "{}", stdout(&output));
    assert!(tasks_dir.join("WM").join("1.yml").is_file());

    // An explicit short name doubles as the prefix; the config file is still
    // created with that name (legacy creation parity).
    let output = run(tmp.path(), &["--project", "CORE", "add", "Third"]);
    assert!(output.status.success());
    let config = parse_yaml(&tasks_dir.join("CORE").join("config.yml"));
    assert_eq!(project_name_of(&config), "CORE");
    assert!(tasks_dir.join("CORE").join("1.yml").is_file());
}

/// `task add` keeps persisting the CLI-configured defaults (reporter actor,
/// default tags, type, status, priority) and auto-populates the configured
/// member into the project config in the same commit.
#[test]
fn cli_task_add_preserves_actor_tag_type_status_and_priority_defaults() {
    let (tmp, tasks_dir) = workspace();
    write_global_config(
        &tasks_dir,
        "default.reporter: carol\ndefault.tags: [backend]\nauto.populate_members: true\nauto.tags_from_path: false\n",
    );

    let output = run(tmp.path(), &["task", "add", "Defaults task"]);
    assert!(
        output.status.success(),
        "{}{}",
        stdout(&output),
        stderr(&output)
    );
    assert!(stdout(&output).contains("TEST-1"), "{}", stdout(&output));

    let task = parse_yaml(&tasks_dir.join("TEST").join("1.yml"));
    assert_eq!(yaml_str(&task, "reporter"), "carol");
    assert_eq!(yaml_str(&task, "status"), "Todo");
    // The CLI smart default prefers the built-in Medium when no configured
    // default exists and Medium is an allowed priority (preserved behavior).
    assert_eq!(yaml_str(&task, "priority"), "Medium");
    assert_eq!(yaml_str(&task, "type"), "Feature");
    let tags: Vec<&str> = task
        .get("tags")
        .and_then(|v| v.as_sequence())
        .map(|seq| seq.iter().filter_map(|v| v.as_str()).collect())
        .unwrap_or_default();
    assert_eq!(tags, vec!["backend"]);

    let config = parse_yaml(&tasks_dir.join("TEST").join("config.yml"));
    let members: Vec<String> = config
        .get("members")
        .and_then(|v| v.as_sequence())
        .map(|seq| {
            seq.iter()
                .filter_map(|v| v.as_str().map(str::to_string))
                .collect()
        })
        .unwrap_or_default();
    assert_eq!(members, vec!["carol".to_string()]);
}

/// Soft-deleted tasks keep their tombstone files, so the numeric id stays
/// reserved and the next creation does not reuse it.
#[test]
fn cli_add_does_not_reuse_id_reserved_by_tombstone() {
    let (tmp, tasks_dir) = workspace();
    write_global_config(&tasks_dir, "");

    let output = run(tmp.path(), &["--project", "TEST", "add", "First"]);
    assert!(
        output.status.success(),
        "{}{}",
        stdout(&output),
        stderr(&output)
    );
    assert!(stdout(&output).contains("TEST-1"), "{}", stdout(&output));

    let output = run(tmp.path(), &["task", "delete", "-y", "TEST-1"]);
    assert!(
        output.status.success(),
        "{}{}",
        stdout(&output),
        stderr(&output)
    );
    let tombstone = parse_yaml(&tasks_dir.join("TEST").join("1.yml"));
    assert!(
        tombstone.get("deleted_at").is_some(),
        "seed task must be soft deleted in place"
    );

    let output = run(tmp.path(), &["--project", "TEST", "add", "Second"]);
    assert!(output.status.success());
    assert!(
        stdout(&output).contains("TEST-2"),
        "tombstone must reserve its id: {}",
        stdout(&output)
    );
    assert!(tasks_dir.join("TEST").join("2.yml").is_file());
}

/// `--field=sprint=N` stores a custom field and never creates sprint
/// membership (documented custom-data semantics, not sprint assignment).
#[test]
fn cli_add_sprint_field_stays_custom_data_without_membership() {
    let (tmp, tasks_dir) = workspace();
    write_global_config(&tasks_dir, "");

    let output = run(
        tmp.path(),
        &[
            "--project",
            "TEST",
            "add",
            "Custom sprint",
            "--field",
            "sprint=3",
        ],
    );
    assert!(
        output.status.success(),
        "{}{}",
        stdout(&output),
        stderr(&output)
    );

    let task = parse_yaml(&tasks_dir.join("TEST").join("1.yml"));
    assert_eq!(
        task.get("custom_fields")
            .and_then(|v| v.get("sprint"))
            .and_then(|v| v.as_str()),
        Some("3")
    );
    assert!(
        !tasks_dir.join("@sprints").join("3.yml").exists(),
        "no sprint file may be written"
    );
}

/// A successful create through either boundary persists the task and the
/// merged member list together, leaving no journal behind.
#[test]
fn cli_add_commits_task_and_members_together() {
    let (tmp, tasks_dir) = workspace();
    write_global_config(
        &tasks_dir,
        "members: [alice]\nauto.populate_members: true\n",
    );
    let project_dir = seed_project(&tasks_dir, "TEST");

    let output = run(
        tmp.path(),
        &[
            "--project",
            "TEST",
            "add",
            "New member",
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
        task_files(&project_dir),
        vec!["1.yml".to_string(), "2.yml".to_string()]
    );
    let config = parse_yaml(&project_dir.join("config.yml"));
    let members: Vec<String> = config
        .get("members")
        .and_then(|v| v.as_sequence())
        .map(|seq| {
            seq.iter()
                .filter_map(|v| v.as_str().map(str::to_string))
                .collect()
        })
        .unwrap_or_default();
    assert_eq!(
        members,
        vec!["alice".to_string(), "bob".to_string()],
        "merged members must be committed with the task"
    );
    assert!(!journal_exists(&tasks_dir));
}

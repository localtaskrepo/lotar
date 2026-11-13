//! DEV-407: automatic writes to a hand-written project config (members added
//! by `auto.populate_members`, the detected project name) keep its comments
//! and leave values nobody asked to change alone.

use crate::common;
use common::lotar_cmd;
use std::path::Path;

const PROJECT_CONFIG: &str =
    "# Why the tickets aren't tracked\nproject:\n  name: inbox # lower case on purpose\n# Triage has no worktree\nauto:\n  populate_members: true\n";

fn workspace() -> (tempfile::TempDir, std::path::PathBuf) {
    let tmp = tempfile::tempdir().unwrap();
    let tasks_dir = tmp.path().join(".tasks");
    std::fs::create_dir_all(tasks_dir.join("INBOX")).unwrap();
    std::fs::write(
        tasks_dir.join("config.yml"),
        "default.project: INBOX\nissue.states: [Todo, InProgress, Done]\nissue.types: [Feature, Bug, Chore]\nissue.priorities: [Low, Medium, High]\n",
    )
    .unwrap();
    std::fs::write(tasks_dir.join("INBOX").join("config.yml"), PROJECT_CONFIG).unwrap();
    (tmp, tasks_dir)
}

fn run(dir: &Path, args: &[&str]) {
    let output =
        lotar_cmd().expect("binary 'lotar' not found").env("LOTAR_TEST_SILENT", "1").args(args).current_dir(dir).output().expect("failed to run lotar");
    assert!(output.status.success(), "lotar {args:?} failed: {}{}", String::from_utf8_lossy(&output.stdout), String::from_utf8_lossy(&output.stderr));
}

#[test]
fn auto_populated_members_keep_the_project_config_comments() {
    let (tmp, tasks_dir) = workspace();
    let config_path = tasks_dir.join("INBOX").join("config.yml");

    run(tmp.path(), &["--project", "INBOX", "add", "First", "--reporter", "mallox"]);
    assert_eq!(std::fs::read_to_string(&config_path).unwrap(), format!("{PROJECT_CONFIG}members:\n- mallox\n"), "only the members block is added");

    run(tmp.path(), &["--project", "INBOX", "add", "Second", "--reporter", "claude"]);
    assert_eq!(
        std::fs::read_to_string(&config_path).unwrap(),
        format!("{PROJECT_CONFIG}members:\n- claude\n- mallox\n"),
        "a new member joins the existing block"
    );
}

#[test]
fn auto_populated_members_replace_a_hand_written_block_in_place() {
    let (tmp, tasks_dir) = workspace();
    let config_path = tasks_dir.join("INBOX").join("config.yml");
    std::fs::write(&config_path, "members: # who may work here\n  - mallox\n# the rest\nauto:\n  populate_members: true\n").unwrap();

    run(tmp.path(), &["--project", "INBOX", "add", "First", "--reporter", "claude"]);
    assert_eq!(
        std::fs::read_to_string(&config_path).unwrap(),
        "members: # who may work here\n  - claude\n  - mallox\n# the rest\nauto:\n  populate_members: true\n"
    );
}

#[test]
fn a_detected_name_replaces_only_the_name_line() {
    let existing = "# kept\nproject:\n  name: PRJ\nauto:\n  populate_members: true\n";
    let mut cfg = lotar::config::normalization::parse_project_from_yaml_str("PRJ", existing).unwrap();
    cfg.project_name = "My Project".to_string();
    assert_eq!(
        lotar::config::normalization::to_project_yaml_preserving("PRJ", Some(existing), &cfg),
        "# kept\nproject:\n  name: My Project\nauto:\n  populate_members: true\n"
    );
}

#[test]
fn a_change_the_patch_cannot_express_falls_back_to_canonical_yaml() {
    let existing = "# dropped\nproject:\n  name: PRJ\n";
    let mut cfg = lotar::config::normalization::parse_project_from_yaml_str("PRJ", existing).unwrap();
    cfg.default_assignee = Some("alice".to_string());
    assert_eq!(
        lotar::config::normalization::to_project_yaml_preserving("PRJ", Some(existing), &cfg),
        lotar::config::normalization::to_canonical_project_yaml(&cfg)
    );
}

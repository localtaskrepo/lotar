//! DEV-811: `config set` rewrites only the key it sets; comments and the
//! formatting of every other key in the project and global config stay.

use crate::common;
use common::lotar_cmd;
use std::path::Path;

const PROJECT_CONFIG: &str = "project:
  name: Local Task Repo
issue:
  types:
  - Feature
  - Bug
agent:
  worktree:
    # At most 3 jobs at a time (owner 2026-11-15).
    max_parallel_jobs: 3
    # Free the build output of a waiting ticket (owner 2026-11-18).
    waiting_run: [cargo, clean, --workspace]
";

fn run(dir: &Path, args: &[&str]) {
    let output =
        lotar_cmd().expect("binary 'lotar' not found").env("LOTAR_TEST_SILENT", "1").args(args).current_dir(dir).output().expect("failed to run lotar");
    assert!(output.status.success(), "lotar {args:?} failed: {}{}", String::from_utf8_lossy(&output.stdout), String::from_utf8_lossy(&output.stderr));
}

#[test]
fn project_config_set_changes_only_that_key() {
    let tmp = tempfile::tempdir().unwrap();
    let tasks_dir = tmp.path().join(".tasks");
    std::fs::create_dir_all(tasks_dir.join("DEV")).unwrap();
    std::fs::write(tasks_dir.join("config.yml"), "default:\n  project: DEV\n").unwrap();
    let config_path = tasks_dir.join("DEV").join("config.yml");
    std::fs::write(&config_path, PROJECT_CONFIG).unwrap();

    run(tmp.path(), &["--project", "DEV", "config", "set", "issue.types", "Feature,Bug,Chore"]);
    assert_eq!(std::fs::read_to_string(&config_path).unwrap(), PROJECT_CONFIG.replace("  - Bug\n", "  - Bug\n  - Chore\n"));

    run(tmp.path(), &["--project", "DEV", "config", "set", "default.assignee", "mallox"]);
    assert_eq!(
        std::fs::read_to_string(&config_path).unwrap(),
        format!("{}default:\n  assignee: mallox\n", PROJECT_CONFIG.replace("  - Bug\n", "  - Bug\n  - Chore\n")),
        "a new key is appended, the rest stays"
    );
}

#[test]
fn global_config_set_changes_only_that_key() {
    let tmp = tempfile::tempdir().unwrap();
    let tasks_dir = tmp.path().join(".tasks");
    std::fs::create_dir_all(&tasks_dir).unwrap();
    let config_path = tasks_dir.join("config.yml");
    let global = "# Shared defaults\ndefault:\n  project: DEV # the backlog\n  priority: Low\nissue.states: [Todo, InProgress, Done]\n";
    std::fs::write(&config_path, global).unwrap();

    run(tmp.path(), &["config", "set", "default.priority", "High", "--global"]);
    assert_eq!(std::fs::read_to_string(&config_path).unwrap(), global.replace("priority: Low", "priority: High"));
}

#[test]
fn comments_between_list_items_and_at_column_zero_stay() {
    let tmp = tempfile::tempdir().unwrap();
    let tasks_dir = tmp.path().join(".tasks");
    std::fs::create_dir_all(tasks_dir.join("DEV")).unwrap();
    std::fs::write(tasks_dir.join("config.yml"), "default:\n  project: DEV\n").unwrap();
    let config_path = tasks_dir.join("DEV").join("config.yml");
    let config = "# top\nissue:\n  types:\n  - Bug\n  # chore next\n  - Chore\nagent:\n  worktree:\n# owner: 3\n    max_parallel_jobs: 3 # owner\n";
    std::fs::write(&config_path, config).unwrap();

    run(tmp.path(), &["--project", "DEV", "config", "set", "issue.types", "Bug,Chore,Feature"]);
    let config = config.replace("  - Chore\n", "  - Chore\n  - Feature\n");
    assert_eq!(std::fs::read_to_string(&config_path).unwrap(), config);

    run(tmp.path(), &["--project", "DEV", "config", "set", "agent_worktree_max_parallel_jobs", "4"]);
    assert_eq!(std::fs::read_to_string(&config_path).unwrap(), config.replace("max_parallel_jobs: 3", "max_parallel_jobs: 4"));
}

#[test]
fn config_init_force_overwrites_the_project_config() {
    let tmp = tempfile::tempdir().unwrap();
    let tasks_dir = tmp.path().join(".tasks");
    std::fs::create_dir_all(tasks_dir.join("PRJ")).unwrap();
    let config_path = tasks_dir.join("PRJ").join("config.yml");
    std::fs::write(&config_path, "# old comment: max 3 (owner)\nproject:\n  name: PRJ\nfoo_unknown: 1\n").unwrap();

    run(tmp.path(), &["config", "init", "--project=PRJ", "--force"]);
    let written = std::fs::read_to_string(&config_path).unwrap();
    assert!(!written.contains("old comment") && !written.contains("foo_unknown"), "--force writes a fresh config:\n{written}");
}

#[test]
fn config_init_force_overwrites_the_global_config() {
    let tmp = tempfile::tempdir().unwrap();
    let tasks_dir = tmp.path().join(".tasks");
    std::fs::create_dir_all(&tasks_dir).unwrap();
    let config_path = tasks_dir.join("config.yml");
    std::fs::write(&config_path, "# old comment: port 9000 (owner)\ndefault:\n  priority: Low\nfoo_unknown: 1\n").unwrap();

    run(tmp.path(), &["config", "init", "--global", "--force"]);
    let written = std::fs::read_to_string(&config_path).unwrap();
    assert!(!written.contains("old comment") && !written.contains("foo_unknown"), "--force writes a fresh config:\n{written}");
}

//! DEV-153: the agent job limit and worktree cleanup settings can be changed
//! with `config set`, globally and per project, like the other worktree
//! settings.

use crate::common;
fn run(dir: &std::path::Path, args: &[&str]) -> (bool, String) {
    let mut cmd = common::cargo_bin_silent();
    cmd.env("LOTAR_IGNORE_HOME_CONFIG", "1");
    cmd.current_dir(dir);
    let out = cmd.args(args).output().unwrap();
    (out.status.success(), format!("{}{}", String::from_utf8_lossy(&out.stdout), String::from_utf8_lossy(&out.stderr)))
}

fn read(dir: &std::path::Path, rel: &str) -> String {
    std::fs::read_to_string(dir.join(".tasks").join(rel)).unwrap_or_default()
}

#[test]
fn job_limit_and_cleanup_settings_are_settable() {
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path();
    assert!(run(dir, &["add", "Seed", "-p", "WT"]).0);

    for (field, value) in [
        ("agent_worktree_max_parallel_jobs", "3"),
        ("agent_worktree_cleanup_on_done", "true"),
        ("agent_worktree_cleanup_on_failure", "true"),
        ("agent_worktree_cleanup_on_cancel", "true"),
        ("agent_worktree_cleanup_delete_branches", "true"),
        ("agent_worktree_cleanup_ignored_files", "false"),
    ] {
        let (ok, out) = run(dir, &["config", "set", field, value, "--global"]);
        assert!(ok, "{field}: {out}");
    }
    let global = read(dir, "config.yml");
    for expected in [
        "max_parallel_jobs: 3",
        "cleanup_on_done: true",
        "cleanup_on_failure: true",
        "cleanup_on_cancel: true",
        "cleanup_delete_branches: true",
        "cleanup_ignored_files: false",
    ] {
        assert!(global.contains(expected), "missing {expected}:\n{global}");
    }

    // 0 pauses new jobs; anything that is not a whole number is refused.
    assert!(run(dir, &["config", "set", "agent_worktree_max_parallel_jobs", "0", "--global"]).0);
    assert!(read(dir, "config.yml").contains("max_parallel_jobs: 0"));
    let (ok, out) = run(dir, &["config", "set", "agent_worktree_max_parallel_jobs", "two", "--global"]);
    assert!(!ok && out.contains("whole number"), "{out}");
    // Empty removes the limit.
    assert!(run(dir, &["config", "set", "agent_worktree_max_parallel_jobs", "", "--global"]).0);
    assert!(!read(dir, "config.yml").contains("max_parallel_jobs"));

    // Project overrides set and clear one setting at a time.
    assert!(run(dir, &["config", "set", "agent_worktree_cleanup_on_done", "false", "-p", "WT"]).0);
    assert!(run(dir, &["config", "set", "agent_worktree_max_parallel_jobs", "1", "-p", "WT"]).0);
    let project = read(dir, "WT/config.yml");
    assert!(project.contains("cleanup_on_done: false"), "{project}");
    assert!(project.contains("max_parallel_jobs: 1"), "{project}");
    assert!(run(dir, &["config", "set", "agent_worktree_cleanup_on_done", "", "-p", "WT"]).0);
    let project = read(dir, "WT/config.yml");
    assert!(!project.contains("cleanup_on_done"), "{project}");
    assert!(project.contains("max_parallel_jobs: 1"), "{project}");
}

/// DEV-364: build output deleted at job end, as a comma-separated list;
/// paths outside the worktree are refused.
#[test]
fn cleanup_paths_are_settable() {
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path();
    assert!(run(dir, &["add", "Seed", "-p", "WT"]).0);

    let (ok, out) = run(dir, &["config", "set", "agent_worktree_cleanup_paths", "target, view/dist", "--global"]);
    assert!(ok, "{out}");
    let global = read(dir, "config.yml");
    assert!(global.contains("cleanup_paths:\n    - target\n    - view/dist"), "{global}");
    for bad in ["../target", "/tmp/target", ".git"] {
        let (ok, out) = run(dir, &["config", "set", "agent_worktree_cleanup_paths", bad, "--global"]);
        assert!(!ok && out.contains("inside the worktree"), "{bad}: {out}");
    }

    assert!(run(dir, &["config", "set", "agent_worktree_cleanup_paths", "build", "-p", "WT"]).0);
    assert!(read(dir, "WT/config.yml").contains("cleanup_paths:\n    - build"), "{}", read(dir, "WT/config.yml"));
    assert!(run(dir, &["config", "set", "agent_worktree_cleanup_paths", "", "-p", "WT"]).0);
    assert!(!read(dir, "WT/config.yml").contains("cleanup_paths"));
    assert!(run(dir, &["config", "set", "agent_worktree_cleanup_paths", "", "--global"]).0);
    assert!(!read(dir, "config.yml").contains("cleanup_paths"));
}

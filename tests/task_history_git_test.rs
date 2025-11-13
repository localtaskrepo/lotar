use serde_json::Value;
use std::process::Command as ProcCommand;
use tempfile::TempDir;

mod common;

fn run_git(repo: &std::path::Path, args: &[&str], envs: &[(&str, &str)]) {
    let mut cmd = ProcCommand::new("git");
    cmd.current_dir(repo).args(args);
    for (k, v) in envs {
        cmd.env(k, v);
    }
    let out = cmd.output().expect("failed to run git");
    if !out.status.success() {
        panic!(
            "git {:?} failed: {}",
            args,
            String::from_utf8_lossy(&out.stderr)
        );
    }
}

fn init_repo(temp: &TempDir) {
    let root = temp.path();
    run_git(root, &["init"], &[]);
    run_git(root, &["config", "user.name", "Test User"], &[]);
    run_git(root, &["config", "user.email", "test@example.com"], &[]);
    run_git(root, &["config", "commit.gpgsign", "false"], &[]);
}

fn write_file(root: &std::path::Path, rel: &str, content: &str) {
    let path = root.join(rel);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(&path, content).unwrap();
}

fn add_and_commit(
    repo: &std::path::Path,
    rel: &str,
    author: (&str, &str),
    date_rfc3339: &str,
    msg: &str,
) {
    run_git(repo, &["add", rel], &[]);
    let envs = [
        ("GIT_AUTHOR_NAME", author.0),
        ("GIT_AUTHOR_EMAIL", author.1),
        ("GIT_COMMITTER_NAME", author.0),
        ("GIT_COMMITTER_EMAIL", author.1),
        ("GIT_AUTHOR_DATE", date_rfc3339),
        ("GIT_COMMITTER_DATE", date_rfc3339),
    ];
    run_git(repo, &["commit", "-m", msg], &envs);
}

#[test]
fn task_history_diff_at() {
    let temp = crate::common::temp_dir();
    let root = temp.path();
    init_repo(&temp);

    write_file(root, ".tasks/TEST/config.yml", "project_name: TEST\n");
    write_file(
        root,
        ".tasks/TEST/1.yml",
        "title: One\nstatus: TODO\nmodified: 2025-08-01T10:00:00Z\n",
    );
    add_and_commit(
        root,
        ".tasks/TEST/1.yml",
        ("A", "a@example.com"),
        "2025-08-01T10:00:00Z",
        "add 1",
    );
    write_file(
        root,
        ".tasks/TEST/1.yml",
        "title: One2\nstatus: IN_PROGRESS\nmodified: 2025-08-02T09:00:00Z\n",
    );
    add_and_commit(
        root,
        ".tasks/TEST/1.yml",
        ("B", "b@example.com"),
        "2025-08-02T09:00:00Z",
        "edit 1",
    );

    // History JSON
    let out = crate::common::lotar_cmd()
        .unwrap()
        .current_dir(root)
        .args(["--format", "json", "task", "history", "TEST-1", "-L", "5"])
        .output()
        .unwrap();
    assert!(out.status.success());
    let v: Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(v["status"], "ok");
    let items = v["items"].as_array().unwrap();
    assert!(items.len() >= 2);

    // Diff JSON (latest)
    let out = crate::common::lotar_cmd()
        .unwrap()
        .current_dir(root)
        .args(["--format", "json", "task", "diff", "TEST-1"])
        .output()
        .unwrap();
    assert!(out.status.success());
    let v: Value = serde_json::from_slice(&out.stdout).unwrap();
    assert!(v["patch"].as_str().unwrap().contains("diff"));

    // At JSON (first commit)
    // Extract last item commit (oldest) from history
    let first_commit = items.last().unwrap()["commit"].as_str().unwrap();
    let out = crate::common::lotar_cmd()
        .unwrap()
        .current_dir(root)
        .args(["--format", "json", "task", "at", "TEST-1", first_commit])
        .output()
        .unwrap();
    assert!(out.status.success());
    let v: Value = serde_json::from_slice(&out.stdout).unwrap();
    assert!(v["content"].as_str().unwrap().contains("title: One"));
}

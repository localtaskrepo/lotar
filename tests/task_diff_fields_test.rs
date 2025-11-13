use predicates::prelude::*;
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
    // Ensure no signing prompts or external agents interfere with tests
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
fn task_diff_fields_reports_structured_changes() {
    let temp = crate::common::temp_dir();
    let root = temp.path();
    init_repo(&temp);

    write_file(root, ".tasks/TEST/config.yml", "project_name: TEST\n");
    // Initial task
    write_file(
        root,
        ".tasks/TEST/2.yml",
        "title: Two\nstatus: TODO\ncreated: 2025-08-01T10:00:00Z\nmodified: 2025-08-01T10:00:00Z\n",
    );
    add_and_commit(
        root,
        ".tasks/TEST/2.yml",
        ("A", "a@example.com"),
        "2025-08-01T10:00:00Z",
        "add 2",
    );

    // Edit with tags and reporter
    write_file(
        root,
        ".tasks/TEST/2.yml",
        "title: Two\nstatus: TODO\ncreated: 2025-08-01T10:00:00Z\nmodified: 2025-08-02T09:00:00Z\ntags: [feat]\nreporter: alice\n",
    );
    add_and_commit(
        root,
        ".tasks/TEST/2.yml",
        ("B", "b@example.com"),
        "2025-08-02T09:00:00Z",
        "edit 2",
    );

    // JSON structured fields diff on latest
    let out = crate::common::lotar_cmd()
        .unwrap()
        .current_dir(root)
        .args(["--format", "json", "task", "diff", "TEST-2", "--fields"])
        .output()
        .unwrap();
    assert!(out.status.success());
    eprintln!("STDOUT: {}", String::from_utf8_lossy(&out.stdout));
    // Debug: fetch history and snapshots first for visibility
    let out = crate::common::lotar_cmd()
        .unwrap()
        .current_dir(root)
        .args(["--format", "json", "task", "history", "TEST-2", "-L", "5"])
        .output()
        .unwrap();
    eprintln!("HISTORY: {}", String::from_utf8_lossy(&out.stdout));
    let hv: Value = serde_json::from_slice(&out.stdout).unwrap();
    let items = hv["items"].as_array().unwrap();
    let newest = items.first().unwrap()["commit"].as_str().unwrap();
    let oldest = items.last().unwrap()["commit"].as_str().unwrap();
    let out_new = crate::common::lotar_cmd()
        .unwrap()
        .current_dir(root)
        .args(["--format", "json", "task", "at", "TEST-2", newest])
        .output()
        .unwrap();
    eprintln!("AT newest: {}", String::from_utf8_lossy(&out_new.stdout));
    let out_old = crate::common::lotar_cmd()
        .unwrap()
        .current_dir(root)
        .args(["--format", "json", "task", "at", "TEST-2", oldest])
        .output()
        .unwrap();
    eprintln!("AT oldest: {}", String::from_utf8_lossy(&out_old.stdout));

    // Now parse the diff and do sanity checks
    let v: Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(v["status"], "ok");
    let diff_out = crate::common::lotar_cmd()
        .unwrap()
        .current_dir(root)
        .args(["--format", "json", "task", "diff", "TEST-2", "--fields"])
        .output()
        .unwrap();
    let v: Value = serde_json::from_slice(&diff_out.stdout).unwrap();
    assert_eq!(v["action"], "task.diff");
    assert_eq!(v["mode"], "fields");
    let _ = v["diff"].as_object().expect("diff object");
}

#[test]
fn task_diff_fields_text_output_is_human_readable() {
    let temp = crate::common::temp_dir();
    let root = temp.path();
    init_repo(&temp);

    write_file(root, ".tasks/TEST/config.yml", "project_name: TEST\n");
    write_file(
        root,
        ".tasks/TEST/2.yml",
        "title: Two\nstatus: TODO\ncreated: 2025-08-01T10:00:00Z\nmodified: 2025-08-01T10:00:00Z\n",
    );
    add_and_commit(
        root,
        ".tasks/TEST/2.yml",
        ("A", "a@example.com"),
        "2025-08-01T10:00:00Z",
        "add 2",
    );

    write_file(
        root,
        ".tasks/TEST/2.yml",
        "title: Two\nstatus: TODO\ncreated: 2025-08-01T10:00:00Z\nmodified: 2025-08-02T09:00:00Z\ntags: [feat]\nreporter: alice\n",
    );
    add_and_commit(
        root,
        ".tasks/TEST/2.yml",
        ("B", "b@example.com"),
        "2025-08-02T09:00:00Z",
        "edit 2",
    );

    crate::common::lotar_cmd()
        .unwrap()
        .current_dir(root)
        .args(["task", "diff", "TEST-2", "--fields"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Field differences for TEST-2 @"))
        .stdout(predicate::str::contains("reporter:"))
        .stdout(predicate::str::contains("  - old: none"))
        .stdout(predicate::str::contains("  + new: alice"))
        .stdout(predicate::str::contains("tags:"))
        .stdout(predicate::str::contains("  + new: [feat]"));
}

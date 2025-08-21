use assert_cmd::Command;
use serde_json::Value;
use std::process::Command as ProcCommand;
use tempfile::TempDir;

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
    // Basic identity
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
    message: &str,
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
    run_git(repo, &["commit", "-m", message], &envs);
}

#[test]
fn stats_effort_with_transitions_window_filters_tasks() {
    let temp = TempDir::new().unwrap();
    let root = temp.path();
    init_repo(&temp);

    // Project config
    write_file(root, ".tasks/TEST/config.yml", "project_name: TEST\n");

    // TEST-1: transitions to IN_PROGRESS within window (2025-08-12)
    write_file(
        root,
        ".tasks/TEST/1.yml",
        "title: One\nassignee: Alice\nstatus: TODO\npriority: MEDIUM\neffort: 4h\ncreated: 2025-08-10T10:00:00Z\nmodified: 2025-08-10T10:00:00Z\n",
    );
    add_and_commit(
        root,
        ".tasks/TEST/1.yml",
        ("Alice", "alice@example.com"),
        "2025-08-10T10:00:00Z",
        "add 1",
    );
    // Change status to IN_PROGRESS inside window; keep effort
    write_file(
        root,
        ".tasks/TEST/1.yml",
        "title: One\nassignee: Alice\nstatus: IN_PROGRESS\npriority: MEDIUM\neffort: 4h\ncreated: 2025-08-10T10:00:00Z\nmodified: 2025-08-12T09:00:00Z\n",
    );
    add_and_commit(
        root,
        ".tasks/TEST/1.yml",
        ("Bob", "bob@example.com"),
        "2025-08-12T09:00:00Z",
        "move to in_progress",
    );
    // Later change to DONE (outside the query until)
    write_file(
        root,
        ".tasks/TEST/1.yml",
        "title: One\nassignee: Alice\nstatus: DONE\npriority: MEDIUM\neffort: 4h\ncreated: 2025-08-10T10:00:00Z\nmodified: 2025-08-16T12:00:00Z\n",
    );
    add_and_commit(
        root,
        ".tasks/TEST/1.yml",
        ("Alice", "alice@example.com"),
        "2025-08-16T12:00:00Z",
        "done 1",
    );

    // TEST-2: transitions to IN_PROGRESS before window (should be excluded)
    write_file(
        root,
        ".tasks/TEST/2.yml",
        "title: Two\nassignee: Bob\nstatus: TODO\npriority: LOW\neffort: 2h\ncreated: 2025-08-05T11:00:00Z\nmodified: 2025-08-05T11:00:00Z\n",
    );
    add_and_commit(
        root,
        ".tasks/TEST/2.yml",
        ("Bob", "bob@example.com"),
        "2025-08-05T11:00:00Z",
        "add 2",
    );
    write_file(
        root,
        ".tasks/TEST/2.yml",
        "title: Two\nassignee: Bob\nstatus: IN_PROGRESS\npriority: LOW\neffort: 2h\ncreated: 2025-08-05T11:00:00Z\nmodified: 2025-08-09T08:00:00Z\n",
    );
    add_and_commit(
        root,
        ".tasks/TEST/2.yml",
        ("Bob", "bob@example.com"),
        "2025-08-09T08:00:00Z",
        "move 2 to in_progress (before window)",
    );

    // TEST-3: transitions to DONE inside window (filter asks for IN_PROGRESS, should be excluded)
    write_file(
        root,
        ".tasks/TEST/3.yml",
        "title: Three\nassignee: Carol\nstatus: TODO\npriority: LOW\neffort: 1h\ncreated: 2025-08-11T10:00:00Z\nmodified: 2025-08-11T10:00:00Z\n",
    );
    add_and_commit(
        root,
        ".tasks/TEST/3.yml",
        ("Carol", "carol@example.com"),
        "2025-08-11T10:00:00Z",
        "add 3",
    );
    write_file(
        root,
        ".tasks/TEST/3.yml",
        "title: Three\nassignee: Carol\nstatus: DONE\npriority: LOW\neffort: 1h\ncreated: 2025-08-11T10:00:00Z\nmodified: 2025-08-12T10:00:00Z\n",
    );
    add_and_commit(
        root,
        ".tasks/TEST/3.yml",
        ("Carol", "carol@example.com"),
        "2025-08-12T10:00:00Z",
        "move 3 to done (inside window)",
    );

    // Query: window that includes 2025-08-12 only, transitions to IN_PROGRESS
    let output = Command::cargo_bin("lotar")
        .unwrap()
        .current_dir(root)
        .args([
            "--format",
            "json",
            "stats",
            "effort",
            "--by",
            "assignee",
            "--unit",
            "hours",
            "--since",
            "2025-08-11T00:00:00Z",
            "--until",
            "2025-08-13T00:00:00Z",
            "--transitions",
            "IN_PROGRESS",
            "--global",
        ])
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "stats effort with transitions failed"
    );
    let v: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(v["status"], "ok");
    let items = v["items"].as_array().unwrap();
    // Expect exactly one group (only TEST-1 matched), regardless of grouping key normalization
    assert_eq!(
        items.len(),
        1,
        "expected exactly one matching group, got {items:?}"
    );
    let hours = items[0]["hours"].as_f64().unwrap_or(0.0);
    assert!((3.99..=4.01).contains(&hours), "unexpected hours: {hours}");
}

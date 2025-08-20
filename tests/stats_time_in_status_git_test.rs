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
fn stats_time_in_status_basic_window() {
    let temp = TempDir::new().unwrap();
    let root = temp.path();
    init_repo(&temp);

    // Setup project and task
    write_file(root, ".tasks/TEST/config.yml", "project_name: TEST\n");
    // Initial commit at 2025-08-01T10:00Z - status TODO
    write_file(
        root,
        ".tasks/TEST/1.yml",
        "title: One\nstatus: Todo\npriority: Medium\ncreated: 2025-08-01T10:00:00Z\nmodified: 2025-08-01T10:00:00Z\n",
    );
    add_and_commit(
        root,
        ".tasks/TEST/1.yml",
        ("Alice", "alice@example.com"),
        "2025-08-01T10:00:00Z",
        "add 1",
    );

    // Move to IN_PROGRESS at 2025-08-10T09:00Z
    write_file(
        root,
        ".tasks/TEST/1.yml",
        "title: One\nstatus: InProgress\npriority: Medium\ncreated: 2025-08-01T10:00:00Z\nmodified: 2025-08-10T09:00:00Z\n",
    );
    add_and_commit(
        root,
        ".tasks/TEST/1.yml",
        ("Bob", "bob@example.com"),
        "2025-08-10T09:00:00Z",
        "progress",
    );

    // Move to DONE at 2025-08-17T12:00Z
    write_file(
        root,
        ".tasks/TEST/1.yml",
        "title: One\nstatus: Done\npriority: Medium\ncreated: 2025-08-01T10:00:00Z\nmodified: 2025-08-17T12:00:00Z\n",
    );
    add_and_commit(
        root,
        ".tasks/TEST/1.yml",
        ("Alice", "alice@example.com"),
        "2025-08-17T12:00:00Z",
        "done",
    );

    // Query time-in-status in a window up to 2025-08-18T00:00Z (global scope)
    let output = Command::cargo_bin("lotar")
        .unwrap()
        .current_dir(root)
        .args([
            "--format",
            "json",
            "stats",
            "time-in-status",
            "--since",
            "2025-08-01T00:00:00Z",
            "--until",
            "2025-08-18T00:00:00Z",
            "--global",
        ])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "stats time-in-status failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let v: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(v["status"], "ok");
    let items = v["items"].as_array().unwrap();
    assert!(!items.is_empty());
    // Find TEST-1
    let my = items
        .iter()
        .find(|it| it["id"].as_str() == Some("TEST-1"))
        .expect("missing TEST-1 in results");

    let rows = my["items"].as_array().unwrap();
    let get_secs = |name: &str| -> i64 {
        rows.iter()
            .find(|r| r["status"].as_str() == Some(name))
            .and_then(|r| r["seconds"].as_i64())
            .unwrap_or(0)
    };

    // Expected seconds:
    // TODO: 2025-08-01T10:00 -> 2025-08-10T09:00 = 8d 23h = 774_000s
    // IN_PROGRESS: 2025-08-10T09:00 -> 2025-08-17T12:00 = 7d 3h = 615_600s
    // DONE: 2025-08-17T12:00 -> 2025-08-18T00:00 = 12h = 43_200s
    assert_eq!(get_secs("TODO"), 774_000);
    assert_eq!(get_secs("IN_PROGRESS"), 615_600);
    assert_eq!(get_secs("DONE"), 43_200);
}

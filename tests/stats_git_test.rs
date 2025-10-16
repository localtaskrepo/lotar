use assert_cmd::Command;
use serde_json::Value;
use std::process::Command as ProcCommand;
use tempfile::TempDir;

mod common;

// --- Shared helper functions for stats git-related tests ---
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

fn normalize_status_key(value: &str) -> String {
    value.to_ascii_lowercase().replace(['_', '-', ' '], "")
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

// --- Merged from stats_time_in_status_git_test.rs ---
#[test]
fn stats_time_in_status_basic_window() {
    let temp = crate::common::temp_dir();
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
        let expect_key = normalize_status_key(name);
        rows.iter()
            .find(|r| {
                r["status"]
                    .as_str()
                    .map(|status| normalize_status_key(status) == expect_key)
                    .unwrap_or(false)
            })
            .and_then(|r| r["seconds"].as_i64())
            .unwrap_or(0)
    };

    // Expected seconds
    assert_eq!(get_secs("TODO"), 774_000);
    assert_eq!(get_secs("IN_PROGRESS"), 615_600);
    assert_eq!(get_secs("DONE"), 43_200);
}

// --- Merged from stats_time_in_status_single_task_test.rs ---
#[test]
fn stats_time_in_status_single_task() {
    let temp = crate::common::temp_dir();
    let root = temp.path();
    init_repo(&temp);

    // Setup project and task
    write_file(root, ".tasks/TEST/config.yml", "project_name: TEST\n");
    // Initial commit at 2025-08-01T10:00Z - status Todo (PascalCase)
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

    // Query per-ticket time-in-status
    let output = Command::cargo_bin("lotar")
        .unwrap()
        .current_dir(root)
        .args([
            "--format",
            "json",
            "stats",
            "status",
            "TEST-1",
            "--time-in-status",
            "--since",
            "2025-08-01T00:00:00Z",
            "--until",
            "2025-08-18T00:00:00Z",
        ])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "stats status --time-in-status failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let v: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(v["status"], "ok");
    let items = v["items"].as_array().unwrap();
    assert!(!items.is_empty());
    let row = &items[0];
    assert_eq!(row["id"].as_str(), Some("TEST-1"));
    let rows = row["items"].as_array().unwrap();
    let get_secs = |name: &str| -> i64 {
        let expect_key = normalize_status_key(name);
        rows.iter()
            .find(|r| {
                r["status"]
                    .as_str()
                    .map(|status| normalize_status_key(status) == expect_key)
                    .unwrap_or(false)
            })
            .and_then(|r| r["seconds"].as_i64())
            .unwrap_or(0)
    };
    assert_eq!(get_secs("TODO"), 774_000);
    assert_eq!(get_secs("IN_PROGRESS"), 615_600);
    assert_eq!(get_secs("DONE"), 43_200);
}

// --- Merged from stats_effort_transitions_window_test.rs ---
#[test]
fn stats_effort_with_transitions_window_filters_tasks() {
    let temp = crate::common::temp_dir();
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
    assert_eq!(
        items.len(),
        1,
        "expected exactly one matching group, got {items:?}"
    );
    let hours = items[0]["hours"].as_f64().unwrap_or(0.0);
    assert!((3.99..=4.01).contains(&hours), "unexpected hours: {hours}");
}

#[test]
fn stats_changed_and_churn_and_authors() {
    let temp = crate::common::temp_dir();
    let root = temp.path();
    init_repo(&temp);

    // tasks directory and sample tasks
    write_file(root, ".tasks/TEST/config.yml", "project_name: TEST\n");
    write_file(
        root,
        ".tasks/TEST/1.yml",
        "title: One\nstatus: TODO\npriority: MEDIUM\ncreated: 2025-08-01T10:00:00Z\nmodified: 2025-08-01T10:00:00Z\n",
    );
    add_and_commit(
        root,
        ".tasks/TEST/1.yml",
        ("Alice", "alice@example.com"),
        "2025-08-01T10:00:00Z",
        "add 1",
    );

    // Modify 1
    write_file(
        root,
        ".tasks/TEST/1.yml",
        "title: One edited\nstatus: IN_PROGRESS\npriority: MEDIUM\ncreated: 2025-08-01T10:00:00Z\nmodified: 2025-08-10T09:00:00Z\n",
    );
    add_and_commit(
        root,
        ".tasks/TEST/1.yml",
        ("Bob", "bob@example.com"),
        "2025-08-10T09:00:00Z",
        "edit 1",
    );

    // Add 2
    write_file(
        root,
        ".tasks/TEST/2.yml",
        "title: Two\nstatus: TODO\npriority: LOW\ncreated: 2025-08-15T11:00:00Z\nmodified: 2025-08-15T11:00:00Z\n",
    );
    add_and_commit(
        root,
        ".tasks/TEST/2.yml",
        ("Alice", "alice@example.com"),
        "2025-08-15T11:00:00Z",
        "add 2",
    );

    // Another churn for 1
    write_file(
        root,
        ".tasks/TEST/1.yml",
        "title: One edited again\nstatus: IN_PROGRESS\npriority: MEDIUM\ncreated: 2025-08-01T10:00:00Z\nmodified: 2025-08-16T12:00:00Z\n",
    );
    add_and_commit(
        root,
        ".tasks/TEST/1.yml",
        ("Alice", "alice@example.com"),
        "2025-08-16T12:00:00Z",
        "edit 1 again",
    );

    // stats changed (global to avoid project scope mismatch)
    let output = Command::cargo_bin("lotar")
        .unwrap()
        .current_dir(root)
        .args([
            "--format", "json", "stats", "changed", "--since", "120d", "--global",
        ])
        .output()
        .unwrap();
    assert!(output.status.success(), "stats changed failed");
    let v: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(v["status"], "ok");
    let items = v["items"].as_array().unwrap();
    // Should include TEST-1 and TEST-2
    let ids: Vec<String> = items
        .iter()
        .map(|i| i["id"].as_str().unwrap().to_string())
        .collect();
    assert!(ids.contains(&"TEST-1".to_string()));
    assert!(ids.contains(&"TEST-2".to_string()));

    // stats churn: TEST-1 should have the most commits (>=2 commits)
    // Use a 90-day window to ensure both commits land inside the range regardless of test execution time
    let output = Command::cargo_bin("lotar")
        .unwrap()
        .current_dir(root)
        .args([
            "--format", "json", "stats", "churn", "--since", "90d", "--global",
        ])
        .output()
        .unwrap();
    assert!(output.status.success(), "stats churn failed");
    let v: Value = serde_json::from_slice(&output.stdout).unwrap();
    let items = v["items"].as_array().unwrap();
    assert!(!items.is_empty());
    assert_eq!(items[0]["id"], "TEST-1");
    assert!(items[0]["commits"].as_u64().unwrap() >= 2);

    // stats authors: both Alice and Bob present; Alice has >= 2 commits
    let output = Command::cargo_bin("lotar")
        .unwrap()
        .current_dir(root)
        .args([
            "--format", "json", "stats", "authors", "--since", "90d", "--global",
        ])
        .output()
        .unwrap();
    assert!(output.status.success(), "stats authors failed");
    let v: Value = serde_json::from_slice(&output.stdout).unwrap();
    let items = v["items"].as_array().unwrap();
    let names: Vec<String> = items
        .iter()
        .map(|i| i["author"].as_str().unwrap().to_string())
        .collect();
    assert!(names.contains(&"Alice".to_string()));
    assert!(names.contains(&"Bob".to_string()));

    // stats activity by day
    let output = Command::cargo_bin("lotar")
        .unwrap()
        .current_dir(root)
        .args([
            "--format",
            "json",
            "stats",
            "activity",
            "--group-by",
            "day",
            "--since",
            "90d",
            "--global",
        ])
        .output()
        .unwrap();
    assert!(output.status.success(), "stats activity day failed");
    let v: Value = serde_json::from_slice(&output.stdout).unwrap();
    let items = v["items"].as_array().unwrap();
    assert!(!items.is_empty());
    // Ensure keys look like YYYY-MM-DD
    assert!(items[0]["key"].as_str().unwrap().contains('-'));

    // stats activity by project
    let output = Command::cargo_bin("lotar")
        .unwrap()
        .current_dir(root)
        .args([
            "--format",
            "json",
            "stats",
            "activity",
            "--group-by",
            "project",
            "--since",
            "90d",
            "--global",
        ])
        .output()
        .unwrap();
    assert!(output.status.success(), "stats activity project failed");
    let v: Value = serde_json::from_slice(&output.stdout).unwrap();
    let items = v["items"].as_array().unwrap();
    let keys: Vec<String> = items
        .iter()
        .map(|i| i["key"].as_str().unwrap().to_string())
        .collect();
    assert!(keys.contains(&"TEST".to_string()));
}

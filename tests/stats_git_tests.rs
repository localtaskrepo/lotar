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
fn stats_changed_and_churn_and_authors() {
    let temp = TempDir::new().unwrap();
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
            "--format", "json", "stats", "changed", "--since", "60d", "--global",
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
    let output = Command::cargo_bin("lotar")
        .unwrap()
        .current_dir(root)
        .args([
            "--format", "json", "stats", "churn", "--since", "60d", "--global",
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

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
fn stats_stale_threshold() {
    let temp = TempDir::new().unwrap();
    let root = temp.path();
    init_repo(&temp);

    // Create two tasks: one very old, one recent
    write_file(root, ".tasks/TEST/config.yml", "project_name: TEST\n");
    write_file(
        root,
        ".tasks/TEST/1.yml",
        "title: Old\nstatus: TODO\nmodified: 2000-01-01T00:00:00Z\n",
    );
    add_and_commit(
        root,
        ".tasks/TEST/1.yml",
        ("Old", "old@example.com"),
        "2000-01-01T00:00:00Z",
        "old",
    );

    write_file(
        root,
        ".tasks/TEST/2.yml",
        "title: New\nstatus: TODO\nmodified: 2025-08-18T00:00:00Z\n",
    );
    add_and_commit(
        root,
        ".tasks/TEST/2.yml",
        ("New", "new@example.com"),
        "2025-08-18T00:00:00Z",
        "new",
    );

    // Threshold 7000d should include the old task but not the new
    let out = Command::cargo_bin("lotar")
        .unwrap()
        .current_dir(root)
        .env("LOTAR_IGNORE_HOME_CONFIG", "1")
        .env("LOTAR_IGNORE_ENV_TASKS_DIR", "1")
        .env("LOTAR_TEST_MODE", "1")
        .args([
            "--format",
            "json",
            "stats",
            "stale",
            "--threshold",
            "7000d",
            "--global",
        ])
        .output()
        .unwrap();
    assert!(out.status.success());
    let v: Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(v["status"], "ok");
    let items = v["items"].as_array().unwrap();
    // Expect at least one item and that it contains TEST-1 and not TEST-2
    assert!(items.iter().any(|it| it["id"] == "TEST-1"));
    assert!(!items.iter().any(|it| it["id"] == "TEST-2"));
}

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

#[test]
fn audit_list_last_change_per_task_smoke() {
    let temp = TempDir::new().unwrap();
    let root = temp.path();
    init_repo(&temp);

    std::fs::create_dir_all(root.join(".tasks/ABC")).unwrap();
    std::fs::write(root.join(".tasks/ABC/1.yml"), "title: A\n").unwrap();
    run_git(root, &["add", ".tasks/ABC/1.yml"], &[]);
    run_git(root, &["commit", "-m", "add 1"], &[]);

    std::fs::write(root.join(".tasks/ABC/2.yml"), "title: B\n").unwrap();
    run_git(root, &["add", ".tasks/ABC/2.yml"], &[]);
    run_git(root, &["commit", "-m", "add 2"], &[]);

    // Call via stats stale over 0d threshold to include all, asserting both IDs appear
    let out = Command::cargo_bin("lotar")
        .unwrap()
        .current_dir(root)
        .args([
            "--format",
            "json",
            "stats",
            "stale",
            "--threshold",
            "0d",
            "--global",
            "--limit",
            "10",
        ])
        .output()
        .unwrap();
    assert!(out.status.success());
    let v: Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(v["status"], "ok");
    let items = v["items"].as_array().unwrap();
    let ids: Vec<String> = items
        .iter()
        .map(|i| i["id"].as_str().unwrap().to_string())
        .collect();
    assert!(ids.contains(&"ABC-1".to_string()));
    assert!(ids.contains(&"ABC-2".to_string()));
}

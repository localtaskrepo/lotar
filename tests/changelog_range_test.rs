use assert_cmd::prelude::*;
use std::fs;
use std::path::Path;
use std::process::Command;
use tempfile::TempDir;

fn write_task(dir: &Path, proj: &str, id: &str, title: &str) {
    let tasks = dir.join(".tasks").join(proj);
    fs::create_dir_all(&tasks).unwrap();
    let path = tasks.join(format!("{id}.yml"));
    let content = format!(
        "title: {title}\nstatus: Todo\npriority: Medium\ntask_type: Feature\ncreated: 2024-01-01T00:00:00Z\nmodified: 2024-01-01T00:00:00Z\n"
    );
    fs::write(path, content).unwrap();
}

#[test]
fn changelog_with_ref_range_runs() {
    let temp = TempDir::new().unwrap();
    let root = temp.path();

    // init git
    Command::new("git")
        .current_dir(root)
        .args(["init"])
        .assert()
        .success();
    // configure identity for commits
    Command::new("git")
        .current_dir(root)
        .args(["config", "user.email", "dev@example.com"])
        .assert()
        .success();
    Command::new("git")
        .current_dir(root)
        .args(["config", "user.name", "Dev"])
        .assert()
        .success();

    // initial commit
    write_task(root, "TEST", "1", "first");
    Command::new("git")
        .current_dir(root)
        .args(["add", "."])
        .assert()
        .success();
    Command::new("git")
        .current_dir(root)
        .args(["commit", "-m", "init"])
        .assert()
        .success();

    // second change
    write_task(root, "TEST", "1", "second");
    Command::new("git")
        .current_dir(root)
        .args(["add", "."])
        .assert()
        .success();
    Command::new("git")
        .current_dir(root)
        .args(["commit", "-m", "second"])
        .assert()
        .success();

    // run changelog for HEAD~1..HEAD
    Command::cargo_bin("lotar")
        .unwrap()
        .current_dir(root)
        .args(["changelog", "HEAD~1", "--global"]) // since ref
        .assert()
        .success();
}

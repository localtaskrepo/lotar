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
fn changelog_range_mode_json() {
    let temp = TempDir::new().unwrap();
    let root = temp.path();

    Command::new("git")
        .current_dir(root)
        .args(["init"])
        .assert()
        .success();
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

    write_task(root, "PRJ", "1", "v1");
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

    write_task(root, "PRJ", "1", "v2");
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

    let out = Command::cargo_bin("lotar")
        .unwrap()
        .current_dir(root)
        .args(["--format", "json", "changelog", "HEAD~1", "--global"]) // since
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let text = String::from_utf8_lossy(&out);
    let json: serde_json::Value = serde_json::from_str(&text).expect("valid json");
    assert_eq!(json["status"], "ok");
    assert_eq!(json["action"], "changelog");
    assert_eq!(json["mode"], "range");
    let items = json["items"].as_array().unwrap();
    assert!(items.iter().any(|it| it["id"] == "PRJ-1"));
}

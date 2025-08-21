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

fn init_repo(root: &Path) {
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
    Command::new("git")
        .current_dir(root)
        .args(["config", "commit.gpgsign", "false"])
        .assert()
        .success();
}

#[test]
fn changelog_working_tree_modified_json() {
    let temp = TempDir::new().unwrap();
    let root = temp.path();
    init_repo(root);

    // commit initial task
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

    // modify working tree (not committed)
    write_task(root, "TEST", "1", "changed");

    // run changelog default (working)
    let output = Command::cargo_bin("lotar")
        .unwrap()
        .current_dir(root)
        .args(["--format", "json", "changelog", "--global"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let text = String::from_utf8_lossy(&output);
    let json: serde_json::Value = serde_json::from_str(&text).expect("valid json");
    assert_eq!(json["status"], "ok");
    assert_eq!(json["action"], "changelog");
    assert_eq!(json["mode"], "working");

    // find TEST-1
    let items = json["items"].as_array().unwrap();
    let found = items
        .iter()
        .find(|it| it["id"] == "TEST-1")
        .expect("TEST-1 present");
    let changes = found["changes"].as_array().unwrap();
    // expect a title change old:first -> new:changed
    assert!(
        changes
            .iter()
            .any(|c| c["field"] == "title" && c["old"] == "first" && c["new"] == "changed")
    );
}

#[test]
fn changelog_working_tree_created_and_deleted() {
    let temp = TempDir::new().unwrap();
    let root = temp.path();
    init_repo(root);

    // commit initial task 1
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

    // create new task 2 (uncommitted) and delete 1 from working tree
    write_task(root, "TEST", "2", "new");
    let path1 = root.join(".tasks/TEST/1.yml");
    fs::remove_file(&path1).unwrap();

    let output = Command::cargo_bin("lotar")
        .unwrap()
        .current_dir(root)
        .args(["--format", "json", "changelog", "--global"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let text = String::from_utf8_lossy(&output);
    let json: serde_json::Value = serde_json::from_str(&text).expect("valid json");

    let items = json["items"].as_array().unwrap();

    // TEST-2 should have a "created" change
    let item2 = items
        .iter()
        .find(|it| it["id"] == "TEST-2")
        .expect("TEST-2 present");
    let changes2 = item2["changes"].as_array().unwrap();
    assert!(changes2.iter().any(|c| c["field"] == "created"));

    // TEST-1 should have a "deleted" change
    let item1 = items
        .iter()
        .find(|it| it["id"] == "TEST-1")
        .expect("TEST-1 present");
    let changes1 = item1["changes"].as_array().unwrap();
    assert!(changes1.iter().any(|c| c["field"] == "deleted"));
}

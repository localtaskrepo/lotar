use assert_cmd::Command;
use tempfile::TempDir;

mod common;

fn run(cmd: &mut Command, temp_dir: &TempDir, args: &[&str]) -> assert_cmd::assert::Assert {
    cmd.current_dir(temp_dir.path())
        .args(["--format", "json"]) // stable json output
        .args(args)
        .assert()
}

#[test]
fn comment_shortcut_adds_comment() {
    let temp = crate::common::temp_dir();
    // Create a task
    run(
        &mut crate::common::lotar_cmd().unwrap(),
        &temp,
        &["task", "add", "A"],
    )
    .success();

    // Resolve created ID by listing
    let out = crate::common::lotar_cmd()
        .unwrap()
        .current_dir(temp.path())
        .args(["--format", "json", "list"]) // get first id
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let s = String::from_utf8_lossy(&out);
    let v: serde_json::Value = serde_json::from_str(&s).unwrap();
    let id = v["tasks"][0]["id"].as_str().unwrap().to_string();

    // Add a comment
    let out = crate::common::lotar_cmd()
        .unwrap()
        .current_dir(temp.path())
        .args(["--format", "json", "comment", &id, "First comment"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let s = String::from_utf8_lossy(&out);
    let v: serde_json::Value = serde_json::from_str(&s).unwrap();
    assert_eq!(v["action"], "task.comment");
    assert_eq!(v["task_id"], id);
    assert_eq!(v["comments"], 1);
}

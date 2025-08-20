use assert_cmd::Command;
use predicates::prelude::*;

mod common;
use common::TestFixtures;

#[test]
fn task_comment_parity_list_on_empty() {
    let tf = TestFixtures::new();
    // create a task
    Command::cargo_bin("lotar")
        .unwrap()
        .current_dir(tf.get_temp_path())
        .args(["add", "Task for task comment parity"])
        .assert()
        .success();

    // get id via JSON list
    let out = Command::cargo_bin("lotar")
        .unwrap()
        .current_dir(tf.get_temp_path())
        .args(["list", "--format", "json"]) // json
        .output()
        .unwrap();
    let body = String::from_utf8_lossy(&out.stdout);
    let id = regex::Regex::new(r#"id"\s*:\s*"([A-Z0-9]+-\d+)"#)
        .unwrap()
        .captures(&body)
        .and_then(|c| c.get(1))
        .map(|m| m.as_str().to_string())
        .expect("Expected an ID in list JSON output");

    // lotar task comment with no text should list existing comments
    Command::cargo_bin("lotar")
        .unwrap()
        .current_dir(tf.get_temp_path())
        .args(["--format", "json", "task", "comment", &id])
        .assert()
        .success()
        .stdout(predicate::str::contains("\"action\":\"task.comment.list\""))
        .stdout(predicate::str::contains("\"comments\":0"));
}

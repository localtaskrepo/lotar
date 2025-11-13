use predicates::prelude::*;

mod common;
use common::TestFixtures;

#[test]
fn comment_positional_text_adds_comment() {
    let tf = TestFixtures::new();
    // create a task
    let mut cmd = crate::common::lotar_cmd().unwrap();
    cmd.current_dir(tf.get_temp_path())
        .args(["add", "Test task for comments"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Created task:"));

    // list to get ID
    let output = crate::common::lotar_cmd()
        .unwrap()
        .current_dir(tf.get_temp_path())
        .args(["list"]) // default text output
        .output()
        .unwrap();
    assert!(output.status.success());
    let body = String::from_utf8_lossy(&output.stdout);
    let id = regex::Regex::new(r"([A-Z0-9]+-\d+)")
        .unwrap()
        .captures(&body)
        .and_then(|c| c.get(1))
        .map(|m| m.as_str().to_string())
        .expect("Expected an ID in list output");

    // add comment
    let mut cmd = crate::common::lotar_cmd().unwrap();
    cmd.current_dir(tf.get_temp_path())
        .args(["comment", &id, "hello world"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Comment added to"));

    // verify via JSON second run
    let out = crate::common::lotar_cmd()
        .unwrap()
        .current_dir(tf.get_temp_path())
        .args(["-f", "json", "comment", &id, "again"]) // ensure json output
        .output()
        .unwrap();
    assert!(out.status.success());
    let s = String::from_utf8_lossy(&out.stdout);
    assert!(s.contains("\"action\":\"task.comment\""));
    assert!(s.contains("\"added_comment\""));
}

#[test]
fn comment_message_flag_adds_comment() {
    let tf = TestFixtures::new();
    // create a task
    crate::common::lotar_cmd()
        .unwrap()
        .current_dir(tf.get_temp_path())
        .args(["add", "Task for -m"])
        .assert()
        .success();

    // get id (use JSON for stability)
    let out = crate::common::lotar_cmd()
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

    // add comment with -m
    crate::common::lotar_cmd()
        .unwrap()
        .current_dir(tf.get_temp_path())
        .args(["comment", &id, "-m", "via flag message"])
        .assert()
        .success();
}

#[test]
fn comment_requires_text() {
    let tf = TestFixtures::new();
    // create a task
    crate::common::lotar_cmd()
        .unwrap()
        .current_dir(tf.get_temp_path())
        .args(["add", "Task for empty check"])
        .assert()
        .success();

    // get id via JSON list
    let out = crate::common::lotar_cmd()
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

    // run with no text
    // With no text, it should list existing comments (success) and show 0 initially
    crate::common::lotar_cmd()
        .unwrap()
        .current_dir(tf.get_temp_path())
        .args(["--format", "json", "comment", &id])
        .assert()
        .success()
        .stdout(predicate::str::contains("\"action\":\"task.comment.list\""))
        .stdout(predicate::str::contains("\"comments\":0"));
}

#[test]
fn comment_dry_run_preview_includes_explain() {
    let tf = TestFixtures::new();
    crate::common::lotar_cmd()
        .unwrap()
        .current_dir(tf.get_temp_path())
        .args(["add", "Task for dry-run comment"])
        .assert()
        .success();

    let id_output = crate::common::lotar_cmd()
        .unwrap()
        .current_dir(tf.get_temp_path())
        .args(["list", "--format", "json"])
        .output()
        .unwrap();
    let body = String::from_utf8_lossy(&id_output.stdout);
    let id = regex::Regex::new(r#"id"\s*:\s*"([A-Z0-9]+-\d+)"#)
        .unwrap()
        .captures(&body)
        .and_then(|c| c.get(1))
        .map(|m| m.as_str().to_string())
        .expect("Expected an ID in list JSON output");

    crate::common::lotar_cmd()
        .unwrap()
        .current_dir(tf.get_temp_path())
        .args([
            "--format",
            "json",
            "comment",
            &id,
            "--dry-run",
            "--explain",
            "preview from test",
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("\"status\":\"preview\""))
        .stdout(predicate::str::contains("\"explain\":"))
        .stdout(predicate::str::contains("\"comments\":1"));
}

use predicates::prelude::*;

mod common;
use common::TestFixtures;

#[test]
fn task_comment_parity_list_on_empty() {
    let tf = TestFixtures::new();
    // create a task
    crate::common::lotar_cmd()
        .unwrap()
        .current_dir(tf.get_temp_path())
        .args(["add", "Task for task comment parity"])
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

    // lotar task comment with no text should list existing comments
    crate::common::lotar_cmd()
        .unwrap()
        .current_dir(tf.get_temp_path())
        .args(["--format", "json", "task", "comment", &id])
        .assert()
        .success()
        .stdout(predicate::str::contains("\"action\":\"task.comment.list\""))
        .stdout(predicate::str::contains("\"comments\":0"));
}

#[test]
fn task_comment_does_not_duplicate_body_into_history() {
    let tf = TestFixtures::new();
    crate::common::lotar_cmd()
        .unwrap()
        .current_dir(tf.get_temp_path())
        .args(["add", "Task for changelog separation"])
        .assert()
        .success();

    let out = crate::common::lotar_cmd()
        .unwrap()
        .current_dir(tf.get_temp_path())
        .args(["list", "--format", "json"])
        .output()
        .unwrap();
    let body = String::from_utf8_lossy(&out.stdout);
    let id = regex::Regex::new(r#"id"\s*:\s*"([A-Z0-9]+-\d+)"#)
        .unwrap()
        .captures(&body)
        .and_then(|c| c.get(1))
        .map(|m| m.as_str().to_string())
        .expect("Expected an ID in list JSON output");

    let (project_prefix, numeric_id) = id.split_once('-').expect("id has prefix and numeric part");
    let task_file = tf
        .tasks_root
        .join(project_prefix)
        .join(format!("{numeric_id}.yml"));

    let unique = format!("CANARY-{:x}-UNIQUE-MARKER", std::process::id());
    crate::common::lotar_cmd()
        .unwrap()
        .current_dir(tf.get_temp_path())
        .args(["--format", "json", "task", "comment", &id, "-m", &unique])
        .assert()
        .success();

    let yaml = std::fs::read_to_string(&task_file).expect("task file should exist");
    let history_section = yaml
        .split_once("history:")
        .map(|(_, rest)| rest)
        .unwrap_or("");
    assert!(
        history_section.contains("comment_added"),
        "history should mark a comment_added event, got: {history_section}"
    );
    assert!(
        !history_section.contains(&unique),
        "comment body should not appear in the history changelog"
    );
    let mut had_legacy_comment_field = false;
    for line in history_section.lines() {
        if line.trim() == "field: comment" {
            had_legacy_comment_field = true;
            break;
        }
    }
    assert!(
        !had_legacy_comment_field,
        "no 'field: comment' change entries should be recorded in history"
    );
}

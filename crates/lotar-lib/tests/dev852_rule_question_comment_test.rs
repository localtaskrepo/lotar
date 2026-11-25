//! DEV-852: a rule's `comment` asks a question with options, which people
//! answer like one asked with `lotar comment --question`.
use serde_json::{Value, json};
use std::path::Path;

use crate::common;
use common::TestFixtures;

fn lotar(dir: &Path, args: &[&str]) -> std::process::Output {
    common::lotar_cmd().unwrap().current_dir(dir).env("LOTAR_DEFAULT_REPORTER", "mallox").env("LOTAR_IGNORE_HOME_CONFIG", "1").args(args).output().unwrap()
}

fn run(dir: &Path, args: &[&str]) -> String {
    let out = lotar(dir, args);
    assert!(out.status.success(), "{args:?}: {}", String::from_utf8_lossy(&out.stderr));
    String::from_utf8_lossy(&out.stdout).into_owned()
}

fn workspace(rules: &str) -> TestFixtures {
    let tf = TestFixtures::new();
    std::fs::write(tf.tasks_root.join("config.yml"), "default:\n  project: QA\nissue:\n  states: [Todo, InProgress, Done]\n").unwrap();
    std::fs::write(tf.tasks_root.join("automation.yml"), rules).unwrap();
    tf
}

const ASKS: &str = r#"automation:
  rules:
    - name: Ask what next
      on:
        created:
          comment:
            text: "What should happen with ${{ticket.title}}?"
            question:
              options:
                - { key: close, label: "Close ${{ticket.id}}", set: { status: Done } }
                - { key: keep, label: Keep it open }
              recommended: close
              other: false
"#;

#[test]
fn a_rule_asks_and_the_answer_applies_the_option_set() {
    let tf = workspace(ASKS);
    let dir = tf.get_temp_path();
    let created: Value = serde_json::from_str(&run(dir, &["--format", "json", "add", "Old idea"])).unwrap();
    let id = created["task"]["id"].as_str().unwrap().to_string();

    assert!(run(dir, &["list", "--has", "question"]).contains(&id), "the rule's question is open");
    let task: Value = serde_json::from_str(&run(dir, &["--format", "json", "show", &id])).unwrap();
    let task = task.get("task").unwrap_or(&task);
    let comment = task["comments"].as_array().unwrap().last().unwrap();
    assert_eq!(comment["author"], "automation", "{comment}");
    assert_eq!(comment["question"]["recommended"], "close");
    assert_eq!(comment["question"]["other"], false);
    assert_eq!(comment["question"]["options"][0]["label"], format!("Close {id}"), "labels take templates");
    assert_eq!(comment["question"]["options"][0]["set"], json!({ "status": "Done" }));
    let text = comment["text"].as_str().unwrap();
    assert!(text.starts_with("What should happen with Old idea?"), "{text}");
    assert!(text.contains(&format!("- close: Close {id} (recommended)")), "{text}");

    let simulated = run(dir, &["automation", "simulate", "--ticket", &id, "--event", "created"]);
    assert!(simulated.contains("Ask: What should happen with Old idea?"), "{simulated}");
    assert!(simulated.contains("- keep: Keep it open"), "{simulated}");

    run(dir, &["comment", &id, "--answer", "close"]);
    let task: Value = serde_json::from_str(&run(dir, &["--format", "json", "show", &id])).unwrap();
    let task = task.get("task").unwrap_or(&task);
    assert_eq!(task["status"], "Done", "{task}");
    assert!(!run(dir, &["list", "--has", "question"]).contains(&id), "answered");
}

#[test]
fn the_map_form_without_a_question_writes_a_plain_comment() {
    let tf = workspace("automation:\n  rules:\n    - on:\n        created:\n          comment: { text: 'Hello ${{ticket.id}}' }\n");
    let dir = tf.get_temp_path();
    let created: Value = serde_json::from_str(&run(dir, &["--format", "json", "add", "Plain"])).unwrap();
    let id = created["task"]["id"].as_str().unwrap().to_string();
    let task: Value = serde_json::from_str(&run(dir, &["--format", "json", "show", &id])).unwrap();
    let task = task.get("task").unwrap_or(&task);
    let comment = task["comments"].as_array().unwrap().last().unwrap();
    assert_eq!(comment["text"], format!("Hello {id}"));
    assert!(comment.get("question").is_none(), "{comment}");
}

#[test]
fn config_validate_names_the_rule_of_an_invalid_question() {
    let tf = workspace(
        r#"automation:
  rules:
    - name: Bad question
      on:
        commented:
          comment:
            text: Pick
            question:
              options:
                - { key: a, label: A, set: { status: Gone } }
                - { key: a, label: Again }
              recommended: z
"#,
    );
    let out = lotar(tf.get_temp_path(), &["config", "validate"]);
    let shown = format!("{}{}", String::from_utf8_lossy(&out.stdout), String::from_utf8_lossy(&out.stderr));
    assert!(!out.status.success(), "{shown}");
    assert!(shown.contains("Rule 'Bad question' (commented): comment question:"), "{shown}");
}

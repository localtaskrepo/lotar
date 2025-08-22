use assert_cmd::Command;
use predicates::prelude::*;
use tempfile::TempDir;

mod common;
use common::env_mutex::EnvVarGuard;
use lotar::utils::paths;

fn write_minimal_config(tasks_dir: &std::path::Path, extra: &str) {
    // Provide minimal, valid lists so CLI validation passes where needed
    let base = r#"default.project: TEST
issue.states: [Todo, InProgress, Done]
issue.types: [Feature, Bug, Chore]
issue.priorities: [Low, Medium, High]
"#;
    let mut content = String::from(base);
    if !extra.is_empty() {
        content.push_str(extra);
        if !extra.ends_with('\n') {
            content.push('\n');
        }
    }
    std::fs::write(paths::global_config_path(tasks_dir), content).unwrap();
}

#[test]
fn add_with_assignee_me_resolves_identity() {
    let temp = TempDir::new().unwrap();
    let tasks_dir = temp.path().join(".tasks");
    std::fs::create_dir_all(&tasks_dir).unwrap();

    // Configure a deterministic reporter identity
    write_minimal_config(&tasks_dir, "default.reporter: alice@example.com\n");

    let _silent = EnvVarGuard::set("LOTAR_TEST_SILENT", "1");
    let _tasks = EnvVarGuard::set("LOTAR_TASKS_DIR", &tasks_dir.to_string_lossy());

    // Create a task with @me
    let mut cmd = Command::cargo_bin("lotar").unwrap();
    cmd.current_dir(temp.path())
        .args([
            "add",
            "ME Alias Task",
            "--assignee=@me",
            "--project=TEST",
            "--format=json",
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "\"assignee\":\"alice@example.com\"",
        ));

    // Ensure list --assignee=@me filters to the same task
    let out = Command::cargo_bin("lotar")
        .unwrap()
        .current_dir(temp.path())
        .args(["list", "--assignee=@me", "--format=json"])
        .output()
        .unwrap();
    assert!(out.status.success());
    let json: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    let tasks = json
        .get("tasks")
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();
    assert!(
        tasks
            .iter()
            .any(|t| t.get("title").and_then(|v| v.as_str()) == Some("ME Alias Task"))
    );

    // restored by guards
}

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
fn explicit_project_creation_and_me_filter_share_identity() {
    let temp = TempDir::new().unwrap();
    let tasks = temp.path().join("workspace/.tasks");
    for (project, reporter) in [("TEST", "alice"), ("OTHER", "bob")] {
        std::fs::create_dir_all(tasks.join(project)).unwrap();
        std::fs::write(
            tasks.join(project).join("config.yml"),
            format!("default.reporter: {reporter}\n"),
        )
        .unwrap();
    }
    write_minimal_config(&tasks, "");
    let mut create = common::lotar_cmd().unwrap();
    create
        .current_dir(temp.path())
        .env("HOME", temp.path())
        .env_remove("LOTAR_DEFAULT_REPORTER")
        .args([
            "--tasks-dir",
            tasks.to_str().unwrap(),
            "--project",
            "OTHER",
            "--format",
            "json",
            "add",
            "Other project task",
            "--assignee",
            "@me",
        ]);
    let output = create.assert().success().get_output().stdout.clone();
    let created: serde_json::Value = serde_json::from_slice(&output).unwrap();
    assert_eq!(created["task"]["assignee"], "bob");
    assert_eq!(created["task"]["reporter"], "bob");
    let mut list = common::lotar_cmd().unwrap();
    list.current_dir(temp.path())
        .env("HOME", temp.path())
        .env_remove("LOTAR_DEFAULT_REPORTER")
        .args([
            "--tasks-dir",
            tasks.to_str().unwrap(),
            "--project",
            "OTHER",
            "--format",
            "json",
            "list",
            "--assignee",
            "@me",
        ]);
    let output = list.assert().success().get_output().stdout.clone();
    let listed: serde_json::Value = serde_json::from_slice(&output).unwrap();
    let rows = listed["tasks"].as_array().unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0]["title"], "Other project task");
    assert_eq!(rows[0]["assignee"], "bob");
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
    let mut cmd = crate::common::lotar_cmd().unwrap();
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
    let out = crate::common::lotar_cmd()
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

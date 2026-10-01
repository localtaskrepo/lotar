mod common;

use common::TestFixtures;
use predicates::prelude::*;
use std::process::Output;

// Output format consistency: every documented format for list, add,
// status, config show, and scan is asserted with its exact observable
// contract, and an unknown format value fails with an argument error.

fn run(temp_dir: &std::path::Path, args: &[&str]) -> Output {
    let mut cmd = crate::common::lotar_cmd().unwrap();
    let output = cmd
        .current_dir(temp_dir)
        .env("LOTAR_TEST_SILENT", "1")
        .args(args)
        .output()
        .expect("run lotar");
    assert!(
        output.status.success(),
        "lotar {:?} failed: {}",
        args,
        String::from_utf8_lossy(&output.stderr)
    );
    output
}

fn stdout_of(output: &Output) -> String {
    String::from_utf8_lossy(&output.stdout).to_string()
}

#[test]
fn list_formats_report_the_same_tasks() {
    let fixtures = TestFixtures::new();
    let temp_dir = fixtures.temp_dir.path();
    run(
        temp_dir,
        &["add", "Format task 1", "--type=feature", "--priority=high"],
    );
    run(temp_dir, &["add", "Format task 2", "--type=bug"]);

    let json = stdout_of(&run(temp_dir, &["list", "--format=json"]));
    let payload: serde_json::Value =
        serde_json::from_str(&json).unwrap_or_else(|e| panic!("invalid JSON: {e}: {json}"));
    let mut titles: Vec<String> = payload["tasks"]
        .as_array()
        .expect("tasks array")
        .iter()
        .map(|task| task["title"].as_str().expect("title").to_string())
        .collect();
    titles.sort();
    assert_eq!(titles, vec!["Format task 1", "Format task 2"]);

    for format in ["text", "table", "markdown"] {
        let output = stdout_of(&run(temp_dir, &["list", &format!("--format={format}")]));
        assert!(
            output.contains("Found 2 task(s)"),
            "{format} output must report the task count: {output}"
        );
        assert!(
            output.contains("Format task 1") && output.contains("Format task 2"),
            "{format} output must list every task title: {output}"
        );
    }
}

#[test]
fn add_formats_report_the_created_task() {
    let fixtures = TestFixtures::new();
    let temp_dir = fixtures.temp_dir.path();

    for format in ["text", "table", "markdown"] {
        let output = stdout_of(&run(
            temp_dir,
            &[
                "add",
                &format!("Add {format} task"),
                &format!("--format={format}"),
            ],
        ));
        assert!(
            output.contains("Created task:"),
            "{format} output must announce the created task: {output}"
        );
        assert!(
            output.contains(&format!("Add {format} task")),
            "{format} output must echo the title: {output}"
        );
    }

    let json = stdout_of(&run(temp_dir, &["add", "Add json task", "--format=json"]));
    let payload: serde_json::Value =
        serde_json::from_str(&json).unwrap_or_else(|e| panic!("invalid JSON: {e}: {json}"));
    assert_eq!(payload["status"], "success");
    assert_eq!(payload["task"]["title"], "Add json task");
    let id = payload["task"]["id"].as_str().expect("task id").to_string();
    assert!(
        !id.is_empty(),
        "created task JSON must carry the canonical id: {payload}"
    );
}

#[test]
fn status_formats_report_the_exact_transition() {
    let fixtures = TestFixtures::new();
    let temp_dir = fixtures.temp_dir.path();
    run(temp_dir, &["add", "Status format task"]);

    let listing = stdout_of(&run(temp_dir, &["list", "--format=json"]));
    let payload: serde_json::Value = serde_json::from_str(&listing).expect("valid list JSON");
    let id = payload["tasks"][0]["id"].as_str().expect("id").to_string();

    for (format, target, expected) in [
        ("text", "in_progress", "from Todo to InProgress"),
        ("table", "todo", "from InProgress to Todo"),
        ("markdown", "in_progress", "from Todo to InProgress"),
    ] {
        let output = stdout_of(&run(
            temp_dir,
            &["status", &id, target, &format!("--format={format}")],
        ));
        assert!(
            output.contains(&format!("Task {id} status changed {expected}")),
            "{format} output must state the exact transition: {output}"
        );
    }

    let json = stdout_of(&run(temp_dir, &["status", &id, "done", "--format=json"]));
    let payload: serde_json::Value =
        serde_json::from_str(&json).unwrap_or_else(|e| panic!("invalid JSON: {e}: {json}"));
    assert_eq!(payload["status"], "success");
    assert_eq!(payload["task_id"], id);
    assert_eq!(payload["old_status"], "InProgress");
    assert_eq!(payload["new_status"], "Done");
}

#[test]
fn config_show_formats_expose_resolved_configuration() {
    let fixtures = TestFixtures::new();
    let temp_dir = fixtures.temp_dir.path();
    run(temp_dir, &["config", "init", "--template=default"]);
    run(
        temp_dir,
        &["config", "set", "default_project", "DEMO", "--global"],
    );

    let json = stdout_of(&run(temp_dir, &["config", "show", "--format=json"]));
    let payload: serde_json::Value =
        serde_json::from_str(&json).unwrap_or_else(|e| panic!("invalid JSON: {e}: {json}"));
    assert_eq!(payload["config"]["default_project"], "DEMO", "{payload}");

    for format in ["text", "table", "markdown"] {
        let output = stdout_of(&run(
            temp_dir,
            &["config", "show", &format!("--format={format}")],
        ));
        assert!(
            output.contains("Tasks directory"),
            "{format} output must show the resolved tasks directory: {output}"
        );
        assert!(
            output.contains("DEMO"),
            "{format} output must show the resolved default project: {output}"
        );
    }
}

#[test]
fn scan_formats_report_discovered_todo_comments() {
    let fixtures = TestFixtures::new();
    let temp_dir = fixtures.temp_dir.path();
    std::fs::write(
        temp_dir.join("sample.rs"),
        "// TODO: format probe marker\nfn main() {}\n",
    )
    .expect("write sample source");

    let output = stdout_of(&run(temp_dir, &["scan", ".", "--format=text"]));
    assert!(
        output.contains("Found 1 TODO comment(s)"),
        "text scan output must report the finding count: {output}"
    );
    assert!(
        output.contains("sample.rs:1"),
        "text scan output must name the file and line: {output}"
    );

    let json = stdout_of(&run(temp_dir, &["scan", ".", "--format=json"]));
    let findings: Vec<serde_json::Value> =
        serde_json::from_str(&json).unwrap_or_else(|e| panic!("invalid JSON: {e}: {json}"));
    assert_eq!(findings.len(), 1, "json scan must list the finding: {json}");
    assert_eq!(findings[0]["file"], "./sample.rs");
    assert_eq!(findings[0]["line"], 1);
    assert_eq!(findings[0]["title"], "format probe marker");
}

#[test]
fn format_flag_is_equivalent_globally_and_per_command() {
    let fixtures = TestFixtures::new();
    let temp_dir = fixtures.temp_dir.path();
    run(temp_dir, &["add", "Flag placement task"]);

    let global = stdout_of(&run(temp_dir, &["--format=json", "list"]));
    let per_command = stdout_of(&run(temp_dir, &["list", "--format=json"]));

    let global_json: serde_json::Value = serde_json::from_str(&global)
        .unwrap_or_else(|e| panic!("global --format=json must emit valid JSON: {e}: {global}"));
    let per_command_json: serde_json::Value =
        serde_json::from_str(&per_command).unwrap_or_else(|e| {
            panic!("command --format=json must emit valid JSON: {e}: {per_command}")
        });
    assert_eq!(
        global_json["tasks"].as_array().map(Vec::len),
        per_command_json["tasks"].as_array().map(Vec::len),
        "both placements must return the same task count: {global_json} vs {per_command_json}"
    );
    assert!(
        global_json["tasks"].as_array().is_some_and(|tasks| tasks
            .iter()
            .any(|task| task["title"] == "Flag placement task")),
        "global placement must include the seeded task: {global_json}"
    );
}

#[test]
fn unknown_format_value_fails_with_argument_error() {
    let fixtures = TestFixtures::new();
    let temp_dir = fixtures.temp_dir.path();

    let mut cmd = crate::common::lotar_cmd().unwrap();
    cmd.current_dir(temp_dir)
        .env("LOTAR_TEST_SILENT", "1")
        .args(["list", "--format=invalid_format"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("error"));
}

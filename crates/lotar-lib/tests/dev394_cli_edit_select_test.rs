//! DEV-394: `lotar edit --select '<query>'` edits every matching task, and
//! `--field key=` drops a field while the task's other fields stay.

use serde_json::Value;
use std::path::PathBuf;

use crate::common;

struct Workspace {
    _tmp: tempfile::TempDir,
    tasks: PathBuf,
}

fn workspace() -> Workspace {
    let tmp = tempfile::tempdir().unwrap();
    let tasks = tmp.path().join(".tasks");
    std::fs::create_dir_all(tasks.join("RS")).unwrap();
    std::fs::write(
        tasks.join("config.yml"),
        "default:\n  project: RS\nissue:\n  states: [Todo, Done]\ncustom:\n  fields:\n    - product\n    - name: fixed_in\n      type: version\n",
    )
    .unwrap();
    for (num, tag) in [(1, "web"), (2, "web"), (3, "cli")] {
        std::fs::write(
            tasks.join("RS").join(format!("{num}.yml")),
            format!(
                "title: Task {num}\nstatus: Todo\npriority: Medium\ntype: Feature\ntags: [{tag}]\ncustom_fields:\n  product: {tag}\ncreated: 2026-01-01T00:00:00Z\nmodified: 2026-01-01T00:00:00Z\n"
            ),
        )
        .unwrap();
    }
    Workspace { _tmp: tmp, tasks }
}

fn run(ws: &Workspace, args: &[&str]) -> std::process::Output {
    let mut cmd = common::cargo_bin_silent();
    cmd.env("LOTAR_TASKS_DIR", &ws.tasks);
    cmd.current_dir(ws.tasks.parent().unwrap());
    cmd.args(args).output().unwrap()
}

fn fields(ws: &Workspace, num: u32) -> serde_yaml_ng::Value {
    let text = std::fs::read_to_string(ws.tasks.join("RS").join(format!("{num}.yml"))).unwrap();
    let task: serde_yaml_ng::Value = serde_yaml_ng::from_str(&text).unwrap();
    task.get("custom_fields").cloned().unwrap_or(serde_yaml_ng::Value::Null)
}

fn yaml(text: &str) -> serde_yaml_ng::Value {
    serde_yaml_ng::from_str(text).unwrap()
}

#[test]
fn select_sets_a_field_on_the_matching_tasks_only() {
    let ws = workspace();
    let out = run(&ws, &["--format", "json", "edit", "--select", "tag:web", "-F", "fixed_in=v0.15.0"]);
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
    let report: Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(report["updated"], serde_json::json!(["RS-1", "RS-2"]));
    assert_eq!(fields(&ws, 1), yaml("{product: web, fixed_in: 0.15.0}"));
    assert_eq!(fields(&ws, 2), yaml("{product: web, fixed_in: 0.15.0}"));
    assert_eq!(fields(&ws, 3), yaml("{product: cli}"));
}

#[test]
fn an_empty_value_drops_the_field_and_keeps_the_others() {
    let ws = workspace();
    assert!(run(&ws, &["edit", "RS-1", "-F", "fixed_in=0.15"]).status.success());
    let out = run(&ws, &["edit", "RS-1", "-F", "PRODUCT="]);
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
    assert_eq!(fields(&ws, 1), yaml("{fixed_in: '0.15'}"));
}

#[test]
fn select_reports_tasks_it_could_not_edit() {
    let ws = workspace();
    let out = run(&ws, &["edit", "--select", "tag:web", "-F", "fixed_in=soon"]);
    assert!(!out.status.success());
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("RS-1") && stderr.contains("2 task(s) could not be edited"), "{stderr}");
    assert_eq!(fields(&ws, 1), yaml("{product: web}"));
}

#[test]
fn an_empty_select_and_no_id_are_refused() {
    let ws = workspace();
    let out = run(&ws, &["edit", "--select", " ", "-F", "product=x"]);
    assert!(String::from_utf8_lossy(&out.stderr).contains("--select is empty"));
    let out = run(&ws, &["edit", "-F", "product=x"]);
    assert!(!out.status.success());
    for num in 1..=3 {
        assert_ne!(fields(&ws, num).get("product").and_then(|v| v.as_str()), Some("x"));
    }
}

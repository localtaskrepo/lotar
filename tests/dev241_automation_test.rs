//! DEV-241: automation rules can be switched off, test moments in a task's
//! past, and run on purpose (`on.manual`, `lotar automation run`, REST),
//! which is how they run on a schedule from cron.

use lotar::api_server::{ApiServer, HttpRequest};
use serde_json::{Value, json};
use std::collections::HashMap;
use std::path::PathBuf;

mod common;
use common::env_mutex::EnvVarGuard;

struct Workspace {
    _tmp: tempfile::TempDir,
    tasks: PathBuf,
}

fn workspace(rules: &str) -> Workspace {
    let tmp = tempfile::tempdir().unwrap();
    let tasks = tmp.path().join(".tasks");
    std::fs::create_dir_all(tasks.join("AM")).unwrap();
    std::fs::write(
        tasks.join("config.yml"),
        "default:\n  project: AM\nissue:\n  states: [Todo, InProgress, Done]\n",
    )
    .unwrap();
    // AM-1 has been Done since August, AM-2 just became Done, AM-3 is open.
    let task = |title: &str, status: &str, changed: &str| {
        format!(
            "title: {title}\nstatus: {status}\npriority: Medium\ntype: Feature\ncreated: 2026-07-01T00:00:00Z\nmodified: {changed}\nhistory:\n- at: {changed}\n  actor: me\n  changes:\n  - field: status\n    old: Todo\n    new: {status}\n"
        )
    };
    let now = chrono::Utc::now().to_rfc3339();
    std::fs::write(
        tasks.join("AM/1.yml"),
        task("Done long ago", "Done", "2026-08-01T00:00:00Z"),
    )
    .unwrap();
    std::fs::write(tasks.join("AM/2.yml"), task("Done today", "Done", &now)).unwrap();
    std::fs::write(tasks.join("AM/3.yml"), task("Still open", "Todo", &now)).unwrap();
    std::fs::write(tasks.join("automation.yml"), rules).unwrap();
    Workspace { _tmp: tmp, tasks }
}

const RULES: &str = r#"automation:
  rules:
    - name: Archive finished work
      when:
        all:
          - status: Done
          - status_changed: { older_than: 30d }
      on:
        manual:
          comment: "Done for a month; archiving."
          archive: true
    - name: Tag new work
      enabled: false
      on:
        created:
          add: { tags: [never] }
        manual:
          add: { tags: [never] }
    - name: Flag recent work
      when:
        created: { within: 3650d }
      on:
        manual:
          add: { tags: [recent] }
"#;

fn run(ws: &Workspace, args: &[&str]) -> (bool, String) {
    let mut cmd = common::cargo_bin_silent();
    cmd.env("LOTAR_TASKS_DIR", &ws.tasks);
    cmd.env("LOTAR_IGNORE_HOME_CONFIG", "1");
    cmd.current_dir(ws.tasks.parent().unwrap());
    let out = cmd.args(args).output().unwrap();
    (
        out.status.success(),
        format!(
            "{}{}",
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr)
        ),
    )
}

fn file(ws: &Workspace, rel: &str) -> String {
    std::fs::read_to_string(ws.tasks.join(rel)).unwrap_or_default()
}

#[test]
fn manual_rules_run_from_the_command_line_on_matching_tasks() {
    let ws = workspace(RULES);
    let (ok, out) = run(
        &ws,
        &["automation", "run", "Archive finished work", "--dry-run"],
    );
    assert!(ok, "{out}");
    assert!(
        out.contains("Would run 'Archive finished work' on AM-1"),
        "{out}"
    );
    assert!(!out.contains("AM-2") && !out.contains("AM-3"), "{out}");
    assert!(
        ws.tasks.join("AM/1.yml").exists(),
        "a dry run changes nothing"
    );

    let (ok, out) = run(&ws, &["automation", "run", "Archive finished work"]);
    assert!(ok, "{out}");
    let archived = file(&ws, "AM/@archive/1.yml");
    assert!(
        archived.contains("Done for a month; archiving."),
        "{archived}"
    );

    // Without names every enabled rule with a manual action runs; the
    // switched-off rule does not, and --select narrows the tasks.
    let (ok, out) = run(&ws, &["automation", "run", "--select", "status:todo"]);
    assert!(ok, "{out}");
    assert!(out.contains("'Flag recent work' on AM-3"), "{out}");
    assert!(!out.contains("AM-2"), "{out}");
    assert!(file(&ws, "AM/3.yml").contains("recent"));
    assert!(!file(&ws, "AM/3.yml").contains("never"));
    assert!(!file(&ws, "AM/2.yml").contains("recent"));

    let (ok, out) = run(&ws, &["automation", "run", "No such rule"]);
    assert!(!ok);
    assert!(out.contains("No rule named 'no such rule'"), "{out}");
}

#[test]
fn switched_off_rules_skip_events_too() {
    let ws = workspace(RULES);
    let (ok, out) = run(&ws, &["add", "Fresh", "-p", "AM"]);
    assert!(ok, "{out}");
    assert!(!file(&ws, "AM/4.yml").contains("never"));
}

#[test]
fn time_conditions_are_validated_when_saving() {
    let ws = workspace(RULES);
    let bad = r#"automation:
  rules:
    - name: Bad span
      when:
        modified: { older_than: soon }
      on:
        manual:
          comment: x
    - name: No operator
      when:
        created: "2026-01-01"
      on:
        manual:
          comment: x
"#;
    let path = ws.tasks.parent().unwrap().join("bad.yml");
    std::fs::write(&path, bad).unwrap();
    let _env = [
        EnvVarGuard::set("LOTAR_TASKS_DIR", ws.tasks.to_str().unwrap()),
        EnvVarGuard::set("LOTAR_IGNORE_HOME_CONFIG", "1"),
    ];
    let api = server();
    let (status, body) = request(&api, "/api/automation/set", json!({"yaml": bad}));
    let text = body.to_string();
    assert!(status == 400 || text.contains("errors"), "{status}: {text}");
    assert!(text.contains("unreadable older_than 'soon'"), "{text}");
    assert!(
        text.contains("needs older_than, within or before"),
        "{text}"
    );
}

fn server() -> ApiServer {
    let mut api = ApiServer::new();
    lotar::routes::initialize(&mut api);
    api
}

fn request(api: &ApiServer, path: &str, body: Value) -> (u16, Value) {
    let response = api.handle_request(&HttpRequest {
        method: "POST".into(),
        path: path.into(),
        query: HashMap::new(),
        headers: HashMap::new(),
        body: serde_json::to_vec(&body).unwrap(),
    });
    (
        response.status,
        serde_json::from_slice(&response.body).unwrap_or(Value::Null),
    )
}

#[test]
fn rest_runs_manual_rules_and_simulates_unsaved_ones() {
    let ws = workspace(RULES);
    let _env = [
        EnvVarGuard::set("LOTAR_TASKS_DIR", ws.tasks.to_str().unwrap()),
        EnvVarGuard::set("LOTAR_IGNORE_HOME_CONFIG", "1"),
        EnvVarGuard::clear("LOTAR_PROJECT"),
    ];
    let api = server();

    let (status, body) = request(
        &api,
        "/api/automation/run",
        json!({"rules": ["Archive finished work"], "dry_run": true}),
    );
    assert_eq!(status, 200, "{body}");
    assert_eq!(body["data"]["dry_run"], true);
    assert_eq!(body["data"]["entries"][0]["task"], "AM-1");
    assert!(ws.tasks.join("AM/1.yml").exists());

    let (status, body) = request(&api, "/api/automation/run", json!({"query": "status:done"}));
    assert_eq!(status, 200, "{body}");
    let tasks: Vec<&str> = body["data"]["entries"]
        .as_array()
        .unwrap()
        .iter()
        .map(|entry| entry["task"].as_str().unwrap())
        .collect();
    assert!(tasks.contains(&"AM-1") && tasks.contains(&"AM-2"), "{body}");
    assert!(ws.tasks.join("AM/@archive/1.yml").exists());

    // A rule that is not saved yet, tried on a task while editing it.
    let draft = json!({
        "name": "Draft",
        "when": {"status": "Todo"},
        "on": {"updated": {"set": {"status": "InProgress"}, "comment": "Picked up ${{ticket.title}}"}}
    });
    let (status, body) = request(
        &api,
        "/api/automation/simulate",
        json!({"ticket_id": "AM-3", "event": "updated", "rule": draft}),
    );
    assert_eq!(status, 200, "{body}");
    assert_eq!(body["data"]["matched"], true, "{body}");
    assert_eq!(body["data"]["task_after"]["status"], "InProgress");
    assert!(body.to_string().contains("Picked up Still open"), "{body}");
    let (status, body) = request(
        &api,
        "/api/automation/simulate",
        json!({"ticket_id": "AM-2", "event": "updated", "rule": draft}),
    );
    assert_eq!(status, 200);
    assert_eq!(body["data"]["matched"], false, "{body}");
}

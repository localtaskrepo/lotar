//! DEV-183: moving a task to another project. The task gets the next free
//! number there and its old file becomes a stub (a soft-deleted task with
//! `moved_to`) that keeps the number reserved and redirects the old ID.

use lotar::api_server::{ApiServer, HttpRequest};
use serde_json::{Value, json};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

mod common;
use common::env_mutex::EnvVarGuard;

struct Workspace {
    _tmp: tempfile::TempDir,
    tasks: PathBuf,
}

/// DEV uses the default workflow; WEB has its own states, a default status
/// and only the `team` custom field.
fn workspace() -> Workspace {
    let tmp = tempfile::tempdir().unwrap();
    let tasks = tmp.path().join(".tasks");
    std::fs::create_dir_all(&tasks).unwrap();
    std::fs::write(
        tasks.join("config.yml"),
        "default:\n  project: DEV\nissue:\n  states: [Todo, InProgress, Done]\n  types: [Feature, Bug]\n  priorities: [Low, Medium, High]\n",
    )
    .unwrap();
    std::fs::create_dir_all(tasks.join("WEB")).unwrap();
    std::fs::write(
        tasks.join("WEB/config.yml"),
        "issue:\n  states: [Backlog, Doing, Shipped]\ndefault:\n  status: Backlog\ncustom:\n  fields: [team]\n",
    )
    .unwrap();
    std::fs::create_dir_all(tasks.join("OPS")).unwrap();
    for num in 1..=3 {
        write_task(
            &tasks.join(format!("DEV/{num}.yml")),
            &format!("Task {num}"),
            "InProgress",
        );
    }
    write_task(&tasks.join("WEB/1.yml"), "Web task", "Backlog");
    Workspace { _tmp: tmp, tasks }
}

fn write_task(path: &Path, title: &str, status: &str) {
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(
        path,
        format!(
            "title: {title}\nstatus: {status}\npriority: High\ntype: Bug\ncreated: 2026-01-01T00:00:00Z\nmodified: 2026-01-01T00:00:00Z\n"
        ),
    )
    .unwrap();
}

fn run(ws: &Workspace, args: &[&str]) -> std::process::Output {
    let mut cmd = common::cargo_bin_silent();
    cmd.env("LOTAR_TASKS_DIR", &ws.tasks);
    cmd.env("LOTAR_IGNORE_HOME_CONFIG", "1");
    cmd.current_dir(ws.tasks.parent().unwrap());
    cmd.args(args).output().unwrap()
}

fn ok(ws: &Workspace, args: &[&str]) -> String {
    let out = run(ws, args);
    let text = format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(out.status.success(), "{args:?}: {text}");
    text
}

fn fails(ws: &Workspace, args: &[&str]) -> String {
    let out = run(ws, args);
    let text = format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(!out.status.success(), "{args:?} should fail: {text}");
    text
}

fn json_ok(ws: &Workspace, args: &[&str]) -> Value {
    let mut all = vec!["--format", "json"];
    all.extend_from_slice(args);
    let out = ok(ws, &all);
    serde_json::from_str(out.lines().find(|line| line.starts_with('{')).unwrap()).unwrap()
}

fn yaml(ws: &Workspace, rel: &str) -> serde_yaml_ng::Value {
    serde_yaml_ng::from_str(&std::fs::read_to_string(ws.tasks.join(rel)).unwrap()).unwrap()
}

fn listed(ws: &Workspace) -> Vec<String> {
    let mut ids: Vec<String> = json_ok(ws, &["list"])["tasks"]
        .as_array()
        .unwrap()
        .iter()
        .map(|t| t["id"].as_str().unwrap().to_string())
        .collect();
    ids.sort();
    ids
}

#[test]
fn move_leaves_a_stub_and_fits_values_to_the_target() {
    let ws = workspace();
    let preview = json_ok(&ws, &["task", "move", "DEV-3", "--to", "WEB", "--dry-run"]);
    assert_eq!(preview["moved"][0]["to"], "WEB-2");
    assert!(ws.tasks.join("DEV/3.yml").exists());
    assert!(
        !ws.tasks.join("WEB/2.yml").exists(),
        "a dry run writes nothing"
    );

    let out = ok(
        &ws,
        &[
            "task", "move", "DEV-3", "--to", "WEB", "--field", "team=web",
        ],
    );
    assert!(out.contains("Moved DEV-3 to WEB-2"), "{out}");
    // InProgress does not exist in WEB, so it falls back to WEB's default.
    assert!(out.contains("status: InProgress -> Backlog"), "{out}");

    let moved = yaml(&ws, "WEB/2.yml");
    assert_eq!(moved["title"], "Task 3");
    assert_eq!(moved["status"], "Backlog");
    assert_eq!(moved["priority"], "High");
    assert_eq!(moved["custom_fields"]["team"], "web");
    assert_eq!(moved["moved_from"][0], "DEV-3");
    let history = moved["history"].as_sequence().unwrap();
    let last = &history.last().unwrap()["changes"];
    assert_eq!(last[0]["field"], "moved");
    assert_eq!(last[0]["old"], "DEV-3");
    assert_eq!(last[0]["new"], "WEB-2");

    let stub = yaml(&ws, "DEV/3.yml");
    assert_eq!(stub["title"], "Task 3");
    assert_eq!(stub["moved_to"], "WEB-2");
    assert!(stub["deleted_at"].is_string());
    assert!(stub.get("status").is_none());

    // The stub is hidden, and keeps its number taken.
    assert_eq!(listed(&ws), ["DEV-1", "DEV-2", "WEB-1", "WEB-2"]);
    let added = ok(&ws, &["add", "New after move", "-p", "DEV"]);
    assert!(added.contains("DEV-4"), "{added}");

    // The old ID works everywhere a task ID is typed.
    let out = ok(&ws, &["status", "DEV-3", "Shipped"]);
    assert!(out.contains("WEB-2"), "{out}");
    ok(&ws, &["comment", "DEV-3", "-m", "via the old id"]);
    let moved = yaml(&ws, "WEB/2.yml");
    assert_eq!(moved["status"], "Shipped");
    assert_eq!(moved["comments"][0]["text"], "via the old id");
    let stub_after = yaml(&ws, "DEV/3.yml");
    assert_eq!(stub_after, stub, "the stub never changes");
}

#[test]
fn moves_are_refused_when_they_cannot_be_done_safely() {
    let ws = workspace();
    let out = fails(&ws, &["task", "move", "DEV-1", "--to", "DEV"]);
    assert!(out.contains("already in project 'DEV'"), "{out}");
    let out = fails(&ws, &["task", "move", "DEV-1", "--to", "NOPE"]);
    assert!(out.contains("does not exist"), "{out}");
    let out = fails(
        &ws,
        &[
            "task", "move", "DEV-1", "--to", "WEB", "--field", "owner=me",
        ],
    );
    assert!(out.contains("Custom field 'owner' is not allowed"), "{out}");
    assert!(out.contains("--field name="), "{out}");
    let out = fails(
        &ws,
        &["task", "move", "DEV-1", "--to", "WEB", "--status", "Todo"],
    );
    assert!(out.to_lowercase().contains("status"), "{out}");

    ok(&ws, &["task", "delete", "DEV-2", "--yes"]);
    let out = fails(&ws, &["task", "move", "DEV-2", "--to", "WEB"]);
    assert!(out.contains("deleted"), "{out}");

    ok(&ws, &["task", "archive", "DEV-1"]);
    let out = fails(&ws, &["task", "move", "DEV-1", "--to", "WEB"]);
    assert!(out.contains("archived and read-only"), "{out}");
    ok(
        &ws,
        &["--force-archived", "task", "move", "DEV-1", "--to", "WEB"],
    );
    assert!(ws.tasks.join("WEB/@archive/2.yml").exists());
    assert_eq!(yaml(&ws, "DEV/@archive/1.yml")["moved_to"], "WEB-2");
    // Nothing was half-written by the refused attempts.
    assert!(!ws.tasks.join("WEB/3.yml").exists());
}

#[test]
fn chains_resolve_and_moving_back_takes_the_old_number() {
    let ws = workspace();
    ok(&ws, &["task", "move", "DEV-2", "--to", "WEB"]);
    ok(&ws, &["task", "move", "WEB-2", "--to", "OPS"]);
    assert_eq!(yaml(&ws, "DEV/2.yml")["moved_to"], "WEB-2");
    assert_eq!(yaml(&ws, "WEB/2.yml")["moved_to"], "OPS-1");
    let ops = yaml(&ws, "OPS/1.yml");
    assert_eq!(ops["moved_from"][0], "DEV-2");
    assert_eq!(ops["moved_from"][1], "WEB-2");

    // The oldest ID still reaches the task.
    let shown = json_ok(&ws, &["task", "move", "DEV-2", "--to", "WEB", "--dry-run"]);
    assert_eq!(shown["moved"][0]["from"], "OPS-1");
    // Back to WEB: its own stub's number is reclaimed instead of a new one.
    assert_eq!(shown["moved"][0]["to"], "WEB-2");

    ok(&ws, &["task", "move", "OPS-1", "--to", "DEV"]);
    let back = yaml(&ws, "DEV/2.yml");
    assert!(
        back.get("moved_to").is_none(),
        "the stub became the task again"
    );
    assert!(back.get("deleted_at").is_none());
    assert_eq!(back["moved_from"].as_sequence().unwrap().len(), 2);
    assert_eq!(yaml(&ws, "OPS/1.yml")["moved_to"], "DEV-2");
    assert_eq!(yaml(&ws, "WEB/2.yml")["moved_to"], "OPS-1");
    // WEB-2 -> OPS-1 -> DEV-2, all ending at the same task.
    let out = ok(&ws, &["status", "WEB-2", "Done"]);
    assert!(out.contains("DEV-2"), "{out}");
}

#[test]
fn broken_chains_and_loops_are_reported() {
    let ws = workspace();
    ok(&ws, &["task", "move", "DEV-1", "--to", "WEB"]);
    ok(&ws, &["task", "delete", "WEB-2", "--hard", "--yes"]);
    let out = fails(&ws, &["status", "DEV-1", "Done"]);
    assert!(out.contains("DEV-1"), "{out}");

    std::fs::write(
        ws.tasks.join("DEV/2.yml"),
        "title: A\ncreated: 2026-01-01T00:00:00Z\ndeleted_at: 2026-01-01T00:00:00Z\nmoved_to: OPS-9\n",
    )
    .unwrap();
    std::fs::write(
        ws.tasks.join("OPS/9.yml"),
        "title: B\ncreated: 2026-01-01T00:00:00Z\ndeleted_at: 2026-01-01T00:00:00Z\nmoved_to: DEV-2\n",
    )
    .unwrap();
    let out = fails(&ws, &["task", "move", "DEV-2", "--to", "WEB"]);
    assert!(out.contains("loop"), "{out}");
}

fn request(api: &ApiServer, path: &str, body: Value) -> (u16, Value) {
    let response = api.handle_request(&HttpRequest {
        method: if body.is_null() { "GET" } else { "POST" }.into(),
        path: path.split('?').next().unwrap().into(),
        query: path
            .split_once('?')
            .map(|(_, q)| {
                q.split('&')
                    .filter_map(|pair| pair.split_once('='))
                    .map(|(k, v)| (k.to_string(), v.to_string()))
                    .collect()
            })
            .unwrap_or_default(),
        headers: HashMap::new(),
        body: if body.is_null() {
            Vec::new()
        } else {
            serde_json::to_vec(&body).unwrap()
        },
    });
    (
        response.status,
        serde_json::from_slice(&response.body).unwrap_or(Value::Null),
    )
}

#[test]
fn rest_and_mcp_move_and_redirect() {
    let ws = workspace();
    let _env = [
        EnvVarGuard::set("LOTAR_TASKS_DIR", ws.tasks.to_str().unwrap()),
        EnvVarGuard::set("LOTAR_IGNORE_HOME_CONFIG", "1"),
        EnvVarGuard::clear("LOTAR_PROJECT"),
    ];
    let mut api = ApiServer::new();
    lotar::routes::initialize(&mut api);

    let (status, body) = request(
        &api,
        "/api/tasks/move",
        json!({"id": "DEV-1", "project": "WEB", "status": "Doing", "dry_run": true}),
    );
    assert_eq!(status, 200, "{body}");
    assert_eq!(body["data"]["to"], "WEB-2");
    assert_eq!(body["data"]["dry_run"], true);
    assert_eq!(body["data"]["task"]["status"], "Doing");
    assert!(!ws.tasks.join("WEB/2.yml").exists());

    let (status, body) = request(
        &api,
        "/api/tasks/move",
        json!({"id": "DEV-1", "project": "WEB", "status": "Doing"}),
    );
    assert_eq!(status, 200, "{body}");
    assert_eq!(body["data"]["task"]["id"], "WEB-2");
    assert_eq!(body["data"]["task"]["moved_from"][0], "DEV-1");

    // GET by the old ID answers with the task under its new ID.
    let (status, body) = request(&api, "/api/tasks/get?id=DEV-1", Value::Null);
    assert_eq!(status, 200, "{body}");
    assert_eq!(body["data"]["id"], "WEB-2");
    let (status, body) = request(
        &api,
        "/api/tasks/update",
        json!({"id": "DEV-1", "title": "Renamed through the old ID"}),
    );
    assert_eq!(status, 200, "{body}");
    assert_eq!(body["data"]["id"], "WEB-2");
    assert_eq!(
        yaml(&ws, "WEB/2.yml")["title"],
        "Renamed through the old ID"
    );

    let (status, body) = request(
        &api,
        "/api/tasks/move",
        json!({"id": "DEV-2", "project": "WEB", "custom_fields": {"owner": "me"}}),
    );
    assert_eq!(status, 400, "{body}");

    let call = |name: &str, arguments: Value| -> Value {
        let line = json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "tools/call",
            "params": {"name": name, "arguments": arguments}
        })
        .to_string();
        serde_json::from_str(&lotar::mcp::server::handle_json_line(&line)).unwrap()
    };
    let resp = call("task_move", json!({"id": "DEV-2", "project": "OPS"}));
    assert!(resp.get("error").is_none(), "{resp}");
    assert!(resp.to_string().contains("OPS-1"), "{resp}");
    assert_eq!(yaml(&ws, "DEV/2.yml")["moved_to"], "OPS-1");
    let resp = call("task_get", json!({"id": "DEV-2"}));
    assert!(resp.to_string().contains("OPS-1"), "{resp}");
}

fn git(ws: &Workspace, args: &[&str]) {
    let out = std::process::Command::new("git")
        .current_dir(ws.tasks.parent().unwrap())
        .args(["-c", "user.name=Tester", "-c", "user.email=t@example.com"])
        .args([
            "-c",
            "commit.gpgsign=false",
            "-c",
            "core.hooksPath=/dev/null",
        ])
        .args(args)
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "git {args:?}: {}",
        String::from_utf8_lossy(&out.stderr)
    );
}

#[test]
fn sprints_relationships_and_search_follow_moved_tasks() {
    let ws = workspace();
    ok(&ws, &["sprint", "create", "--label", "Now"]);
    ok(&ws, &["sprint", "add", "--sprint", "1", "DEV-1"]);
    ok(
        &ws,
        &[
            "task",
            "relationships",
            "add",
            "DEV-2",
            "DEV-1",
            "--kind",
            "depends-on",
        ],
    );
    ok(&ws, &["task", "move", "DEV-1", "--to", "WEB"]);

    // The sprint file still says DEV-1, but the task is a member as WEB-2.
    let sprint = std::fs::read_to_string(ws.tasks.join("@sprints/1.yml")).unwrap();
    assert!(sprint.contains("DEV-1"), "{sprint}");
    let in_sprint = json_ok(&ws, &["list", "sprint:1"]);
    let ids: Vec<&str> = in_sprint["tasks"]
        .as_array()
        .unwrap()
        .iter()
        .map(|t| t["id"].as_str().unwrap())
        .collect();
    assert_eq!(ids, ["WEB-2"], "{in_sprint}");
    // Adding it again by its new ID does not list it twice.
    let out = ok(&ws, &["sprint", "add", "--sprint", "1", "WEB-2"]);
    let sprint = std::fs::read_to_string(ws.tasks.join("@sprints/1.yml")).unwrap();
    assert!(!sprint.contains("WEB-2"), "{out}\n{sprint}");
    // Removing it by its new ID removes the old entry.
    ok(&ws, &["sprint", "remove", "--sprint", "1", "WEB-2"]);
    let sprint = std::fs::read_to_string(ws.tasks.join("@sprints/1.yml")).unwrap();
    assert!(!sprint.contains("DEV-1"), "{sprint}");

    // DEV-2 still stores DEV-1; the graph treats it as WEB-2 and sees no problem.
    assert_eq!(
        yaml(&ws, "DEV/2.yml")["relationships"]["depends_on"][0],
        "DEV-1"
    );
    let check = ok(&ws, &["task", "relationships", "check"]);
    assert!(
        !check.contains("missing") && !check.contains("deleted"),
        "{check}"
    );
    let deps = ok(&ws, &["task", "relationships", "DEV-2"]);
    assert!(deps.contains("WEB-2") || deps.contains("DEV-1"), "{deps}");

    // Searching the old ID finds the moved task, not the stub.
    let found = json_ok(&ws, &["list", "DEV-1"]);
    let ids: Vec<&str> = found["tasks"]
        .as_array()
        .unwrap()
        .iter()
        .map(|t| t["id"].as_str().unwrap())
        .collect();
    assert_eq!(ids.first(), Some(&"WEB-2"), "{found}");
    assert!(!ids.contains(&"DEV-1"));
    // The trash does not show the stub either.
    let trash = json_ok(&ws, &["list", "--deleted"]);
    assert_eq!(trash["tasks"].as_array().unwrap().len(), 0, "{trash}");
}

#[test]
fn history_spans_the_move_and_hand_moves_can_be_recorded() {
    let ws = workspace();
    git(&ws, &["init", "-q"]);
    git(&ws, &["add", "-A"]);
    git(&ws, &["commit", "-q", "-m", "DEV-1 initial"]);
    ok(&ws, &["status", "DEV-1", "Done"]);
    git(&ws, &["commit", "-q", "-am", "DEV-1 done"]);
    ok(&ws, &["task", "move", "DEV-1", "--to", "WEB"]);
    git(&ws, &["add", "-A"]);
    git(&ws, &["commit", "-q", "-m", "Move DEV-1 to WEB"]);

    let history = json_ok(&ws, &["task", "history", "WEB-2"]);
    let text = history.to_string();
    for message in ["DEV-1 initial", "DEV-1 done", "Move DEV-1 to WEB"] {
        assert!(text.contains(message), "{message}: {text}");
    }
    // The old ID shows the same history.
    assert_eq!(
        json_ok(&ws, &["task", "history", "DEV-1"])["items"],
        history["items"]
    );

    // A move made by hand: the file is renamed outside LoTaR.
    std::fs::rename(ws.tasks.join("DEV/3.yml"), ws.tasks.join("OPS/7.yml")).unwrap();
    let out = fails(&ws, &["task", "move", "DEV-2", "--record", "OPS-7"]);
    assert!(out.contains("still exists"), "{out}");
    let out = ok(&ws, &["task", "move", "DEV-3", "--record", "OPS-7"]);
    assert!(out.contains("Recorded the move of DEV-3 to OPS-7"), "{out}");
    assert_eq!(yaml(&ws, "DEV/3.yml")["moved_to"], "OPS-7");
    assert_eq!(yaml(&ws, "OPS/7.yml")["moved_from"][0], "DEV-3");
    let again = ok(&ws, &["task", "move", "DEV-3", "--record", "OPS-7"]);
    assert!(again.contains("already redirects"), "{again}");
    let out = ok(&ws, &["status", "DEV-3", "Done"]);
    assert!(out.contains("OPS-7"), "{out}");
}

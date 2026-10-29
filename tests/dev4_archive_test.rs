//! DEV-4: tasks may sit in any subfolder of their project folder, and
//! `<project>/@archive` holds archived, read-only tasks.

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

fn workspace() -> Workspace {
    let tmp = tempfile::tempdir().unwrap();
    let tasks = tmp.path().join(".tasks");
    std::fs::create_dir_all(tasks.join("RS")).unwrap();
    std::fs::write(
        tasks.join("config.yml"),
        "default:\n  project: RS\nissue:\n  states: [Todo, InProgress, Done]\n",
    )
    .unwrap();
    for (num, status) in [(1, "Done"), (2, "Todo"), (3, "Todo")] {
        write_task(&tasks.join("RS").join(format!("{num}.yml")), num, status);
    }
    Workspace { _tmp: tmp, tasks }
}

fn write_task(path: &Path, num: u64, status: &str) {
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(
        path,
        format!(
            "title: Task {num}\nstatus: {status}\npriority: Medium\ntype: Feature\ncreated: 2026-01-01T00:00:00Z\nmodified: 2026-01-01T00:00:00Z\n"
        ),
    )
    .unwrap();
}

fn run(ws: &Workspace, args: &[&str]) -> std::process::Output {
    let mut cmd = common::cargo_bin_silent();
    cmd.env("LOTAR_TASKS_DIR", &ws.tasks);
    cmd.current_dir(ws.tasks.parent().unwrap());
    cmd.args(args).output().unwrap()
}

fn json_ok(ws: &Workspace, args: &[&str]) -> Value {
    let mut all = vec!["--format", "json"];
    all.extend_from_slice(args);
    let out = run(ws, &all);
    assert!(
        out.status.success(),
        "{args:?}: {}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    serde_json::from_slice(&out.stdout).unwrap()
}

fn listed(ws: &Workspace, query: &[&str]) -> Vec<String> {
    let mut args = vec!["list"];
    args.extend_from_slice(query);
    json_ok(ws, &args)["tasks"]
        .as_array()
        .unwrap()
        .iter()
        .map(|t| t["id"].as_str().unwrap().to_string())
        .collect::<std::collections::BTreeSet<_>>()
        .into_iter()
        .collect()
}

fn files(ws: &Workspace) -> Vec<String> {
    let mut found: Vec<String> = walk(&ws.tasks.join("RS"))
        .into_iter()
        .filter(|p| p.extension().is_some_and(|e| e == "yml") && !p.ends_with("config.yml"))
        .map(|p| {
            p.strip_prefix(ws.tasks.join("RS"))
                .unwrap()
                .to_string_lossy()
                .into_owned()
        })
        .collect();
    found.sort();
    found
}

fn walk(dir: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    for entry in std::fs::read_dir(dir).unwrap().flatten() {
        let path = entry.path();
        if path.is_dir() {
            out.extend(walk(&path));
        } else {
            out.push(path);
        }
    }
    out
}

#[test]
fn nested_folders_are_found_and_numbers_are_never_reused() {
    let ws = workspace();
    let rs = ws.tasks.join("RS");
    std::fs::create_dir_all(rs.join("web/deep")).unwrap();
    std::fs::rename(rs.join("2.yml"), rs.join("web/deep/2.yml")).unwrap();
    // The highest number lives in the archive; it must still be reserved.
    write_task(&rs.join("@archive/2025/9.yml"), 9, "Done");
    // Ignored folders.
    write_task(&rs.join("@attachments/50.yml"), 50, "Todo");
    write_task(&rs.join(".hidden/51.yml"), 51, "Todo");

    assert_eq!(listed(&ws, &[]), ["RS-1", "RS-2", "RS-3"]);
    assert_eq!(listed(&ws, &["archived=yes"]), ["RS-9"]);

    // Lookups by ID find nested tasks, and edits write back in place.
    json_ok(&ws, &["status", "RS-2", "Done"]);
    let nested = std::fs::read_to_string(rs.join("web/deep/2.yml")).unwrap();
    assert!(nested.contains("status: Done"), "{nested}");
    assert!(!rs.join("2.yml").exists());

    let added = json_ok(&ws, &["add", "Four", "-p", "RS"]);
    let added_id = added["task"]["id"]
        .as_str()
        .or_else(|| added["id"].as_str())
        .unwrap()
        .to_string();
    assert_eq!(added_id, "RS-10");
    assert!(rs.join("10.yml").exists());
}

#[test]
fn a_number_stored_twice_is_listed_once_and_refused() {
    let ws = workspace();
    let rs = ws.tasks.join("RS");
    write_task(&rs.join("copy/3.yml"), 3, "Todo");

    let mut cmd = common::lotar_cmd().unwrap();
    cmd.env("LOTAR_TASKS_DIR", &ws.tasks);
    cmd.env_remove("LOTAR_TEST_SILENT");
    cmd.current_dir(ws.tasks.parent().unwrap());
    let out = cmd.args(["--format", "json", "list"]).output().unwrap();
    assert!(out.status.success());
    let ids: Vec<String> = serde_json::from_slice::<Value>(&out.stdout).unwrap()["tasks"]
        .as_array()
        .unwrap()
        .iter()
        .map(|t| t["id"].as_str().unwrap().to_string())
        .collect();
    assert_eq!(ids.iter().filter(|id| *id == "RS-3").count(), 1, "{ids:?}");
    assert!(
        String::from_utf8_lossy(&out.stderr).contains("RS-3 is stored in more than one file"),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );

    let out = run(&ws, &["status", "RS-3", "Done"]);
    assert!(!out.status.success());
    let message = format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(message.contains("copy"), "{message}");
    assert!(
        !std::fs::read_to_string(rs.join("3.yml"))
            .unwrap()
            .contains("Done")
    );
}

#[test]
fn archive_and_unarchive_from_the_cli() {
    let ws = workspace();
    let rs = ws.tasks.join("RS");
    std::fs::create_dir_all(rs.join("web")).unwrap();
    std::fs::rename(rs.join("3.yml"), rs.join("web/3.yml")).unwrap();

    // Dry run moves nothing.
    let preview = json_ok(&ws, &["task", "archive", "--select", "status:done", "-n"]);
    assert_eq!(preview["changed"], json!(["RS-1"]));
    assert_eq!(files(&ws), ["1.yml", "2.yml", "web/3.yml"]);

    json_ok(&ws, &["task", "archive", "--select", "status:done"]);
    let by_id = json_ok(&ws, &["task", "archive", "RS-3", "RS-1"]);
    assert_eq!(by_id["changed"], json!(["RS-3"]));
    assert_eq!(by_id["unchanged"], json!(["RS-1"]));
    assert_eq!(
        files(&ws),
        ["2.yml", "@archive/1.yml", "@archive/web/3.yml"]
    );

    assert_eq!(listed(&ws, &[]), ["RS-2"]);
    assert_eq!(listed(&ws, &["archived=yes"]), ["RS-1", "RS-3"]);
    let all = json_ok(&ws, &["list", "archived=all"]);
    let flags: HashMap<String, bool> = all["tasks"]
        .as_array()
        .unwrap()
        .iter()
        .map(|t| {
            (
                t["id"].as_str().unwrap().to_string(),
                t["archived"].as_bool().unwrap_or(false),
            )
        })
        .collect();
    assert!(flags["RS-1"]);
    assert!(!flags["RS-2"]);

    // Read-only until forced.
    let out = run(&ws, &["status", "RS-1", "Todo"]);
    assert!(!out.status.success());
    assert!(String::from_utf8_lossy(&out.stderr).contains("archived and read-only"));
    let out = run(&ws, &["task", "delete", "RS-1", "-y"]);
    assert!(!out.status.success());
    json_ok(
        &ws,
        &["--force-archived", "comment", "RS-1", "-m", "forced note"],
    );
    let stored = std::fs::read_to_string(rs.join("@archive/1.yml")).unwrap();
    assert!(stored.contains("forced note"), "{stored}");
    assert!(stored.contains("field: archived"), "{stored}");
    let modified_line = |text: &str| {
        text.lines()
            .find(|line| line.starts_with("modified:"))
            .unwrap()
            .to_string()
    };
    let before_unarchive = modified_line(&stored);

    // Unarchive brings files back to where they came from.
    json_ok(&ws, &["task", "unarchive", "--select", "status:todo"]);
    json_ok(&ws, &["task", "unarchive", "RS-1"]);
    assert_eq!(files(&ws), ["1.yml", "2.yml", "web/3.yml"]);
    let stored = std::fs::read_to_string(rs.join("1.yml")).unwrap();
    assert!(stored.contains("field: unarchived"), "{stored}");
    assert_eq!(
        modified_line(&stored),
        before_unarchive,
        "unarchive must not change modified"
    );
    let untouched = std::fs::read_to_string(rs.join("web/3.yml")).unwrap();
    assert!(
        untouched.contains("modified: 2026-01-01"),
        "archive must not change modified: {untouched}"
    );
    json_ok(&ws, &["status", "RS-1", "Todo"]);

    let out = run(&ws, &["task", "archive"]);
    assert!(!out.status.success());
    assert!(String::from_utf8_lossy(&out.stderr).contains("--select"));
}

#[test]
fn unarchive_checks_the_current_config() {
    let ws = workspace();
    json_ok(&ws, &["task", "archive", "RS-1"]);
    // Retiring "Done" is not blocked by the archived task...
    std::fs::write(
        ws.tasks.join("config.yml"),
        "default:\n  project: RS\nissue:\n  states: [Todo, InProgress, Closed]\n",
    )
    .unwrap();
    // ...but bringing it back with a retired status is refused.
    let out = run(&ws, &["task", "unarchive", "RS-1"]);
    assert!(!out.status.success());
    let stdout = String::from_utf8_lossy(&out.stdout);
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        format!("{stdout}{stderr}").contains("cannot unarchive"),
        "{stdout}{stderr}"
    );
    assert!(ws.tasks.join("RS/@archive/1.yml").exists());
}

#[test]
fn automation_action_archives_the_task() {
    let ws = workspace();
    std::fs::write(
        ws.tasks.join("automation.yml"),
        "automation:\n  rules:\n    - name: Archive done work\n      when:\n        status: Done\n      on:\n        updated:\n          comment: archiving\n          archive: true\n",
    )
    .unwrap();
    json_ok(&ws, &["status", "RS-2", "Done"]);
    let archived = ws.tasks.join("RS/@archive/2.yml");
    assert!(archived.exists(), "{:?}", files(&ws));
    assert!(
        std::fs::read_to_string(archived)
            .unwrap()
            .contains("archiving")
    );
}

fn request(
    api: &ApiServer,
    method: &str,
    path: &str,
    query: &[(&str, &str)],
    body: Value,
) -> (u16, Value) {
    let response = api.handle_request(&HttpRequest {
        method: method.into(),
        path: path.into(),
        query: query
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect(),
        headers: HashMap::new(),
        body: serde_json::to_vec(&body).unwrap(),
    });
    let value = serde_json::from_slice(&response.body).unwrap_or(Value::Null);
    (response.status, value)
}

#[test]
fn rest_and_mcp_archive_with_force() {
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
        "POST",
        "/api/tasks/archive",
        &[],
        json!({"id": "RS-2"}),
    );
    assert_eq!(status, 200, "{body}");
    assert_eq!(body["data"]["archived"], true);
    assert!(ws.tasks.join("RS/@archive/2.yml").exists());

    let ids = |query: &[(&str, &str)]| {
        let (status, body) = request(&api, "GET", "/api/tasks/list", query, Value::Null);
        assert_eq!(status, 200, "{body}");
        let mut ids: Vec<String> = body["data"]
            .as_array()
            .or_else(|| body["data"]["tasks"].as_array())
            .unwrap()
            .iter()
            .map(|t| t["id"].as_str().unwrap().to_string())
            .collect();
        ids.sort();
        ids
    };
    assert_eq!(ids(&[]), ["RS-1", "RS-3"]);
    assert_eq!(ids(&[("archived", "yes")]), ["RS-2"]);
    assert_eq!(ids(&[("query", "archived=all")]), ["RS-1", "RS-2", "RS-3"]);

    let (status, body) = request(
        &api,
        "GET",
        "/api/tasks/get",
        &[("id", "RS-2")],
        Value::Null,
    );
    assert_eq!(status, 200, "{body}");
    assert_eq!(body["data"]["archived"], true);

    let update = json!({"id": "RS-2", "title": "Renamed"});
    let (status, body) = request(&api, "POST", "/api/tasks/update", &[], update.clone());
    assert_eq!(status, 400, "{body}");
    assert!(
        body.to_string().contains("archived and read-only"),
        "{body}"
    );
    let (status, body) = request(
        &api,
        "POST",
        "/api/tasks/update",
        &[("force_archived", "true")],
        update,
    );
    assert_eq!(status, 200, "{body}");
    assert_eq!(body["data"]["title"], "Renamed");

    // MCP: the same rules through tools/call.
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
    let is_error = |resp: &Value| {
        resp.get("error").is_some() || resp["result"]["isError"].as_bool() == Some(true)
    };
    let resp = call("task_archive", json!({"id": "RS-3"}));
    assert!(!is_error(&resp), "{resp}");
    assert!(ws.tasks.join("RS/@archive/3.yml").exists());

    let patch = json!({"id": "RS-3", "patch": {"title": "Via MCP"}});
    let resp = call("task_update", patch.clone());
    assert!(is_error(&resp), "{resp}");
    assert!(
        resp.to_string().contains("archived and read-only"),
        "{resp}"
    );
    let mut forced = patch;
    forced["force_archived"] = json!(true);
    let resp = call("task_update", forced);
    assert!(!is_error(&resp), "{resp}");

    let resp = call("task_list", json!({"archived": "yes"}));
    let text = resp["result"]["content"][0]["text"]
        .as_str()
        .unwrap_or_default();
    assert!(text.contains("RS-2") && text.contains("RS-3"), "{resp}");
    assert!(!text.contains("\"RS-1\""), "{resp}");

    let resp = call("task_unarchive", json!({"id": "RS-3"}));
    assert!(!is_error(&resp), "{resp}");
    assert!(ws.tasks.join("RS/3.yml").exists());
    let (status, _) = request(
        &api,
        "POST",
        "/api/tasks/unarchive",
        &[],
        json!({"id": "RS-2"}),
    );
    assert_eq!(status, 200);
    assert!(ws.tasks.join("RS/2.yml").exists());
}

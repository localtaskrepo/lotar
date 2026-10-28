//! DEV-236: the sprint backlog, sprint add/remove over REST and MCP, and the
//! dependency graph CLI select tasks with the shared task query.

use lotar::api_server::{ApiServer, HttpRequest};
use serde_json::{Value, json};
use std::collections::HashMap;

mod common;
use crate::common::env_mutex::EnvVarGuard;

struct Workspace {
    _tmp: tempfile::TempDir,
    tasks_dir: std::path::PathBuf,
}

fn workspace() -> Workspace {
    let tmp = tempfile::tempdir().unwrap();
    let tasks_dir = tmp.path().join(".tasks");
    std::fs::create_dir_all(tasks_dir.join("BQ")).unwrap();
    std::fs::write(
        tasks_dir.join("config.yml"),
        "default:\n  project: BQ\nissue:\n  priorities: [Low, Medium, High]\n  custom_fields: [area]\n",
    )
    .unwrap();
    for (num, priority, extra) in [
        (1, "High", "tags: [ui]\ncustom_fields:\n  area: billing\n"),
        (2, "High", "tags: [api]\n"),
        (3, "Low", "tags: [ui]\n"),
        (4, "High", "tags: [ui]\n"),
    ] {
        std::fs::write(
            tasks_dir.join("BQ").join(format!("{num}.yml")),
            format!(
                "title: Task {num}\nstatus: Todo\npriority: {priority}\ntype: Feature\ncreated: 2026-01-01T00:00:00Z\nmodified: 2026-01-01T00:00:0{num}Z\n{extra}"
            ),
        )
        .unwrap();
    }
    // BQ-4 is already in sprint 1, so it is not in the backlog.
    let sprints = tasks_dir.join("@sprints");
    std::fs::create_dir_all(&sprints).unwrap();
    std::fs::write(
        sprints.join("1.yml"),
        "plan:\n  label: One\ntasks:\n  - id: BQ-4\n",
    )
    .unwrap();
    std::fs::write(sprints.join("2.yml"), "plan:\n  label: Two\n").unwrap();
    Workspace {
        _tmp: tmp,
        tasks_dir,
    }
}

fn cli(ws: &Workspace, args: &[&str]) -> Result<Value, String> {
    let mut cmd = common::cargo_bin_silent();
    cmd.env("LOTAR_TASKS_DIR", &ws.tasks_dir);
    cmd.current_dir(ws.tasks_dir.parent().unwrap());
    cmd.args(["--format", "json"]).args(args);
    let out = cmd.output().map_err(|e| e.to_string())?;
    if !out.status.success() {
        return Err(format!(
            "{}{}",
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr)
        ));
    }
    serde_json::from_slice(&out.stdout).map_err(|e| e.to_string())
}

fn ids(value: &Value) -> Vec<String> {
    value
        .as_array()
        .unwrap()
        .iter()
        .map(|t| t["id"].as_str().unwrap().to_string())
        .collect()
}

fn api() -> ApiServer {
    let mut api = ApiServer::new();
    lotar::routes::initialize(&mut api);
    api
}

fn rest(
    api: &ApiServer,
    method: &str,
    path: &str,
    query: &[(&str, &str)],
    body: Value,
) -> (u16, Value) {
    let resp = api.handle_request(&HttpRequest {
        method: method.into(),
        path: path.into(),
        query: query
            .iter()
            .map(|(k, v)| ((*k).to_string(), (*v).to_string()))
            .collect::<HashMap<_, _>>(),
        headers: HashMap::new(),
        body: if body.is_null() {
            Vec::new()
        } else {
            body.to_string().into_bytes()
        },
    });
    (resp.status, serde_json::from_slice(&resp.body).unwrap())
}

fn mcp(method: &str, params: Value) -> Value {
    let line = json!({"jsonrpc": "2.0", "id": 1, "method": method, "params": params});
    let resp: Value =
        serde_json::from_str(&lotar::mcp::server::handle_json_line(&line.to_string())).unwrap();
    if resp.get("error").is_some() {
        return resp;
    }
    serde_json::from_str(resp["result"]["content"][0]["text"].as_str().unwrap()).unwrap()
}

fn sprint_tasks(ws: &Workspace, id: u32) -> String {
    std::fs::read_to_string(ws.tasks_dir.join("@sprints").join(format!("{id}.yml"))).unwrap()
}

#[test]
fn backlog_takes_the_search_syntax_on_every_interface() {
    let ws = workspace();
    let _guard = EnvVarGuard::set("LOTAR_TASKS_DIR", ws.tasks_dir.to_string_lossy().as_ref());
    let api = api();

    // High-priority ui work in no sprint: BQ-1 (BQ-4 is in sprint 1).
    let cli_backlog = cli(&ws, &["sprint", "backlog", "prio:high", "tag:ui"]).unwrap();
    assert_eq!(ids(&cli_backlog["tasks"]), ["BQ-1"]);
    let (status, body) = rest(
        &api,
        "GET",
        "/api/sprints/backlog",
        &[("query", "prio:high tag:ui")],
        Value::Null,
    );
    assert_eq!(status, 200, "{body}");
    assert_eq!(ids(&body["data"]["tasks"]), ["BQ-1"]);
    let payload = mcp("sprint/backlog", json!({"query": "prio:high tag:ui"}));
    assert_eq!(ids(&payload["tasks"]), ["BQ-1"]);

    // Every list filter works, e.g. a custom field; `tag` stays an alias.
    let (_, body) = rest(
        &api,
        "GET",
        "/api/sprints/backlog",
        &[("area", "billing")],
        Value::Null,
    );
    assert_eq!(ids(&body["data"]["tasks"]), ["BQ-1"]);
    let (_, body) = rest(
        &api,
        "GET",
        "/api/sprints/backlog",
        &[("tag", "ui")],
        Value::Null,
    );
    assert_eq!(ids(&body["data"]["tasks"]).len(), 2);

    // Invalid values are errors, not an empty backlog.
    let (status, _) = rest(
        &api,
        "GET",
        "/api/sprints/backlog",
        &[("status", "Bogus")],
        Value::Null,
    );
    assert_eq!(status, 400);
    assert!(cli(&ws, &["sprint", "backlog", "status:Bogus"]).is_err());
    assert_eq!(
        mcp("sprint/backlog", json!({"status": "Bogus"}))["error"]["code"],
        -32602
    );
}

#[test]
fn rest_and_mcp_sprint_add_and_remove_select_by_query() {
    let ws = workspace();
    let _guard = EnvVarGuard::set("LOTAR_TASKS_DIR", ws.tasks_dir.to_string_lossy().as_ref());
    let api = api();

    // REST: add the backlog's ui tasks to sprint 2 without listing IDs.
    let (status, body) = rest(
        &api,
        "POST",
        "/api/sprints/add",
        &[],
        json!({"sprint": 2, "query": "tag:ui prio:low"}),
    );
    assert_eq!(status, 200, "{body}");
    assert!(sprint_tasks(&ws, 2).contains("BQ-3"));
    assert!(!sprint_tasks(&ws, 2).contains("BQ-1"));

    // MCP: same with tasks omitted, then remove by query.
    let payload = mcp("sprint/add", json!({"sprint": 2, "query": "area=billing"}));
    assert!(payload.get("error").is_none(), "{payload}");
    assert!(sprint_tasks(&ws, 2).contains("BQ-1"));
    let payload = mcp("sprint/remove", json!({"sprint": 2, "query": "prio:low"}));
    assert!(payload.get("error").is_none(), "{payload}");
    assert!(!sprint_tasks(&ws, 2).contains("BQ-3"));

    // A malformed query is rejected without changing anything.
    let before = sprint_tasks(&ws, 2);
    let (status, _) = rest(
        &api,
        "POST",
        "/api/sprints/add",
        &[],
        json!({"sprint": 2, "query": "owner="}),
    );
    assert_eq!(status, 400);
    assert_eq!(sprint_tasks(&ws, 2), before);
}

#[test]
fn graph_cli_filters_with_the_search_syntax() {
    let ws = workspace();
    let all = cli(
        &ws,
        &[
            "task",
            "relationships",
            "graph",
            "--include-isolated",
            "true",
        ],
    );
    let filtered = cli(
        &ws,
        &[
            "task",
            "relationships",
            "graph",
            "--include-isolated",
            "true",
            "--filter",
            "tag:api",
        ],
    );
    let (all, filtered) = (all.unwrap(), filtered.unwrap());
    let matching = |graph: &Value| -> Vec<String> {
        let nodes = graph
            .pointer("/data/nodes")
            .or_else(|| graph.get("nodes"))
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        nodes
            .iter()
            .filter(|n| n["matches_filter"].as_bool().unwrap_or(true))
            .filter_map(|n| n["id"].as_str().map(str::to_string))
            .collect()
    };
    assert!(matching(&all).len() > 1, "{all}");
    assert_eq!(matching(&filtered), ["BQ-2"], "{filtered}");
    assert!(
        cli(
            &ws,
            &["task", "relationships", "graph", "--filter", "owner="]
        )
        .is_err()
    );
}

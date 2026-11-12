//! DEV-332: with `server.public_url` set, tasks carry their web link
//! `<public_url>/task/<ID>` in `show`, `add`, JSON, REST and MCP.
use crate::common;
use crate::common::env_mutex::EnvVarGuard;
use lotar::api_server::{ApiServer, HttpRequest};
use serde_json::Value;
use std::collections::HashMap;
use std::path::Path;

const CONFIG: &str = "default:\n  project: DEV\nserver:\n  public_url: https://lotar.example.com/\n";

fn run(dir: &Path, args: &[&str]) -> String {
    let mut cmd = common::cargo_bin_silent();
    cmd.env("LOTAR_IGNORE_HOME_CONFIG", "1");
    cmd.current_dir(dir);
    let out = cmd.args(args).output().unwrap();
    let text = format!("{}{}", String::from_utf8_lossy(&out.stdout), String::from_utf8_lossy(&out.stderr));
    assert!(out.status.success(), "{args:?} failed:\n{text}");
    text
}

fn json(dir: &Path, args: &[&str]) -> Value {
    let mut full = vec!["--format", "json"];
    full.extend_from_slice(args);
    serde_json::from_str(&run(dir, &full)).unwrap()
}

fn workspace(config: Option<&str>) -> tempfile::TempDir {
    let tmp = tempfile::tempdir().unwrap();
    let tasks = tmp.path().join(".tasks");
    std::fs::create_dir_all(&tasks).unwrap();
    if let Some(config) = config {
        std::fs::write(tasks.join("config.yml"), config).unwrap();
    }
    tmp
}

#[test]
fn cli_prints_and_returns_the_task_link() {
    let tmp = workspace(Some(CONFIG));
    let dir = tmp.path();
    let url = "https://lotar.example.com/task/DEV-1";

    let added = run(dir, &["add", "First", "-p", "DEV"]);
    assert!(added.contains(&format!("Created task: DEV-1\n  Link: {url}\n")), "{added}");
    assert_eq!(json(dir, &["add", "Second", "-p", "DEV"])["task"]["url"], "https://lotar.example.com/task/DEV-2");

    let shown = run(dir, &["show", "DEV-1"]);
    assert!(shown.starts_with(&format!("DEV-1  First\n{url}\nStatus:")), "{shown}");
    assert_eq!(json(dir, &["show", "DEV-1"])["url"], url);
    assert_eq!(json(dir, &["list", "DEV-1"])["tasks"][0]["url"], url);

    // One link per row would make every list longer: text lists leave it out.
    let listed = run(dir, &["list"]);
    assert!(!listed.contains("https://"), "{listed}");
}

#[test]
fn without_public_url_nothing_changes() {
    let tmp = workspace(None);
    let dir = tmp.path();
    let added = run(dir, &["add", "First", "-p", "DEV"]);
    assert!(!added.contains("Link:"), "{added}");
    let shown = run(dir, &["show", "DEV-1"]);
    assert!(shown.starts_with("DEV-1  First\nStatus:"), "{shown}");
    assert!(json(dir, &["show", "DEV-1"]).get("url").is_none());
    assert!(json(dir, &["list"])["tasks"][0].get("url").is_none());
}

#[test]
fn config_set_checks_and_saves_public_url() {
    let tmp = workspace(None);
    let dir = tmp.path();
    run(dir, &["add", "First", "-p", "DEV"]);

    let mut cmd = common::cargo_bin_silent();
    cmd.env("LOTAR_IGNORE_HOME_CONFIG", "1").current_dir(dir);
    let out = cmd.args(["config", "set", "server.public_url", "lotar.example.com", "--global"]).output().unwrap();
    assert!(!out.status.success(), "a bare host has no scheme: {}", String::from_utf8_lossy(&out.stdout));

    run(dir, &["config", "set", "server.public_url", "https://lotar.example.com/", "--global"]);
    let config = std::fs::read_to_string(dir.join(".tasks/config.yml")).unwrap();
    assert!(config.contains("server:\n  public_url: https://lotar.example.com\n"), "{config}");
    assert_eq!(json(dir, &["show", "DEV-1"])["url"], "https://lotar.example.com/task/DEV-1");
}

#[test]
fn rest_and_mcp_carry_the_task_link() {
    let tmp = workspace(Some(CONFIG));
    let tasks_dir = tmp.path().join(".tasks");
    let _guard = EnvVarGuard::set("LOTAR_TASKS_DIR", &tasks_dir.to_string_lossy());
    let mut storage = lotar::Storage::new(&tasks_dir);
    let created = lotar::services::task_service::TaskService::create(
        &mut storage,
        lotar::api_types::TaskCreate { title: "Linked".into(), project: Some("DEV".into()), ..Default::default() },
    )
    .unwrap();
    let url = "https://lotar.example.com/task/DEV-1";
    assert_eq!(created.url.as_deref(), Some(url));

    let mut api = ApiServer::new();
    lotar::routes::initialize(&mut api);
    let query = HashMap::from([("id".to_string(), "DEV-1".to_string())]);
    let resp = api.handle_request(&HttpRequest { method: "GET".into(), path: "/api/tasks/get".into(), query, headers: HashMap::new(), body: Vec::new() });
    assert_eq!(resp.status, 200, "{}", String::from_utf8_lossy(&resp.body));
    assert_eq!(serde_json::from_slice::<Value>(&resp.body).unwrap()["data"]["url"], url);

    for (method, params) in [("task/get", serde_json::json!({"id": "DEV-1"})), ("task/list", serde_json::json!({"project": "DEV"}))] {
        let request = serde_json::json!({"jsonrpc": "2.0", "id": 1, "method": method, "params": params});
        let resp: Value = serde_json::from_str(&lotar::mcp::server::handle_json_line(&request.to_string())).unwrap();
        let text = resp["result"]["content"][0]["text"].as_str().unwrap_or_else(|| panic!("{method}: {resp}"));
        let payload: Value = serde_json::from_str(text).unwrap();
        let task = payload.get("tasks").map(|tasks| tasks[0].clone()).unwrap_or(payload);
        assert_eq!(task["url"], url, "{method}");
    }
}

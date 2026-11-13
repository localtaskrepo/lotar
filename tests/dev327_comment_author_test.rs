//! DEV-327: comments record who wrote them, from the identity the history
//! uses, on every surface; comments stored without an author stay readable.
use lotar::api_server::{ApiServer, HttpRequest};
use lotar::routes;
use serde_json::{Value, json};
use std::collections::HashMap;
use std::path::Path;

use crate::common;
use common::TestFixtures;
use common::env_mutex::EnvVarGuard;

fn run(dir: &Path, args: &[&str]) -> String {
    let out = common::lotar_cmd().unwrap().current_dir(dir).env("LOTAR_DEFAULT_REPORTER", "claude").args(args).output().unwrap();
    assert!(out.status.success(), "{args:?}: {}", String::from_utf8_lossy(&out.stderr));
    String::from_utf8_lossy(&out.stdout).into_owned()
}

#[test]
fn cli_comment_stores_author_and_show_prints_it() {
    let tf = TestFixtures::new();
    let dir = tf.get_temp_path();
    let created: Value = serde_json::from_str(&run(dir, &["--format", "json", "add", "Comment author"])).unwrap();
    let id = created["task"]["id"].as_str().expect("created id").to_string();

    let added: Value = serde_json::from_str(&run(dir, &["--format", "json", "comment", &id, "-m", "hi"])).unwrap();
    assert_eq!(added["added_comment"]["author"], "claude", "{added}");

    let listed: Value = serde_json::from_str(&run(dir, &["--format", "json", "comment", &id])).unwrap();
    assert_eq!(listed["items"][0]["author"], "claude", "{listed}");

    let shown = run(dir, &["show", &id]);
    assert!(shown.contains("claude: hi"), "show prints the author next to the comment:\n{shown}");

    let shown_json: Value = serde_json::from_str(&run(dir, &["--format", "json", "show", &id])).unwrap();
    let comments = shown_json["task"]["comments"].as_array().or_else(|| shown_json["comments"].as_array()).expect("comments in show JSON");
    assert_eq!(comments[0]["author"], "claude", "{shown_json}");
    assert_eq!(comments[0]["text"], "hi");
}

#[test]
fn cli_reads_comments_stored_without_author() {
    let tf = TestFixtures::new();
    let dir = tf.get_temp_path();
    let created: Value = serde_json::from_str(&run(dir, &["--format", "json", "add", "Legacy comment"])).unwrap();
    let id = created["task"]["id"].as_str().expect("created id").to_string();
    let (prefix, number) = id.split_once('-').unwrap();
    let file = dir.join(".tasks").join(prefix).join(format!("{number}.yml"));
    let mut yaml = std::fs::read_to_string(&file).unwrap();
    yaml.push_str("comments:\n- date: 2026-01-01T00:00:00Z\n  text: old note\n");
    std::fs::write(&file, yaml).unwrap();

    let shown = run(dir, &["show", &id]);
    assert!(shown.contains("2026-01-01T00:00:00Z  old note"), "{shown}");
    let listed: Value = serde_json::from_str(&run(dir, &["--format", "json", "comment", &id])).unwrap();
    assert_eq!(listed["items"][0]["text"], "old note");
    assert!(listed["items"][0].get("author").is_none(), "{listed}");

    run(dir, &["comment", &id, "-m", "new note"]);
    let stored = std::fs::read_to_string(&file).unwrap();
    assert!(stored.contains("author: claude"), "{stored}");
    assert_eq!(stored.matches("author:").count(), 1, "the old comment keeps no author:\n{stored}");
}

#[test]
fn env_reporter_outranks_project_default_reporter() {
    // DEV-403: a project's committed default.reporter (the owner) must not
    // override the agent identity from LOTAR_DEFAULT_REPORTER.
    let tf = TestFixtures::new();
    let dir = tf.get_temp_path();
    let created: Value = serde_json::from_str(&run(dir, &["--format", "json", "add", "First", "--project", "PR"])).unwrap();
    let id = created["task"]["id"].as_str().expect("created id").to_string();
    let (prefix, number) = id.split_once('-').unwrap();
    let project = dir.join(".tasks").join(prefix);
    std::fs::write(project.join("config.yml"), "default:\n  reporter: mallox\n").unwrap();

    run(dir, &["comment", &id, "-m", "agent note"]);
    let stored = std::fs::read_to_string(project.join(format!("{number}.yml"))).unwrap();
    assert!(stored.contains("author: claude"), "{stored}");
    assert!(!stored.contains("mallox"), "neither the comment nor the history names the project reporter:\n{stored}");

    let second: Value = serde_json::from_str(&run(dir, &["--format", "json", "add", "Second", "--project", prefix])).unwrap();
    assert_eq!(second["task"]["reporter"], "claude", "{second}");

    let out = common::lotar_cmd().unwrap().current_dir(dir).env_remove("LOTAR_DEFAULT_REPORTER").args(["whoami", "--project", prefix]).output().unwrap();
    assert!(String::from_utf8_lossy(&out.stdout).contains("mallox"), "without the env var the project reporter still applies");
}

struct ServerFixture {
    _temp: tempfile::TempDir,
    _env: Vec<EnvVarGuard>,
}

impl ServerFixture {
    fn new() -> Self {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join(".tasks");
        std::fs::create_dir_all(&root).unwrap();
        std::fs::write(
            root.join("config.yml"),
            "default.project: CA\ndefault.reporter: claude\nissue.states: [Todo, InProgress, Done]\nissue.priorities: [Low, Medium, High]\nissue.types: [Feature, Bug, Chore]\nauto.populate_members: false\nauto.identity: false\nauto.identity_git: false\n",
        )
        .unwrap();
        lotar::utils::identity::invalidate_identity_cache(None);
        let env = vec![
            EnvVarGuard::set("LOTAR_TASKS_DIR", root.to_str().unwrap()),
            EnvVarGuard::set("LOTAR_IGNORE_HOME_CONFIG", "1"),
            EnvVarGuard::set("LOTAR_TEST_FAST_IO", "1"),
            EnvVarGuard::clear("LOTAR_PROJECT"),
            EnvVarGuard::clear("LOTAR_DEFAULT_REPORTER"),
        ];
        Self { _temp: temp, _env: env }
    }
}

fn post(api: &ApiServer, route: &str, body: &Value) -> Value {
    let response = api.handle_request(&HttpRequest {
        method: "POST".into(),
        path: route.into(),
        query: HashMap::new(),
        headers: HashMap::new(),
        body: serde_json::to_vec(body).unwrap(),
    });
    assert!((200..300).contains(&response.status), "{route}: {}", String::from_utf8_lossy(&response.body));
    serde_json::from_slice::<Value>(&response.body).unwrap()["data"].clone()
}

#[test]
fn rest_comment_returns_author() {
    let _fixture = ServerFixture::new();
    let mut api = ApiServer::new();
    routes::initialize(&mut api);
    let task = post(&api, "/api/tasks/add", &json!({"project": "CA", "title": "REST comment author"}));
    let id = task["id"].as_str().unwrap();
    let updated = post(&api, "/api/tasks/comment", &json!({"id": id, "text": "via REST"}));
    assert_eq!(updated["comments"][0]["author"], "claude", "{updated}");
    assert_eq!(updated["history"].as_array().unwrap().last().unwrap()["actor"], "claude", "author matches the history actor");
}

fn mcp_call(id: u64, name: &str, arguments: &Value) -> Value {
    let line = serde_json::to_string(&json!({
        "jsonrpc": "2.0",
        "id": id,
        "method": "tools/call",
        "params": {"name": name, "arguments": arguments},
    }))
    .unwrap();
    let resp: Value = serde_json::from_str(&lotar::mcp::server::handle_json_line(&line)).unwrap();
    assert!(resp.get("error").is_none(), "{name}: {resp}");
    let text = resp["result"]["content"][0]["text"].as_str().unwrap_or_else(|| panic!("{name}: {resp}"));
    serde_json::from_str(text).unwrap()
}

#[test]
fn mcp_comment_returns_author() {
    let _fixture = ServerFixture::new();
    let created = mcp_call(1, "task_create", &json!({"project": "CA", "title": "MCP comment author"}));
    let id = created["task"]["id"].as_str().unwrap_or_else(|| panic!("{created}")).to_string();
    let commented = mcp_call(2, "task_comment_add", &json!({"id": id, "text": "via MCP"}));
    assert_eq!(commented["task"]["comments"][0]["author"], "claude", "{commented}");
}

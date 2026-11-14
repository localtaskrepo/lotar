//! DEV-490: a hand-off comment carries a report (commits, checks with
//! passed/failed/not_run, next step) on every surface; old YAML is unchanged.
use lotar::api_server::{ApiServer, HttpRequest};
use lotar::routes;
use serde_json::{Value, json};
use std::collections::HashMap;
use std::path::Path;

use crate::common;
use common::TestFixtures;
use common::env_mutex::EnvVarGuard;

fn lotar(dir: &Path, args: &[&str]) -> std::process::Output {
    common::lotar_cmd().unwrap().current_dir(dir).env("LOTAR_DEFAULT_REPORTER", "claude").args(args).output().unwrap()
}

fn run(dir: &Path, args: &[&str]) -> String {
    let out = lotar(dir, args);
    assert!(out.status.success(), "{args:?}: {}", String::from_utf8_lossy(&out.stderr));
    String::from_utf8_lossy(&out.stdout).into_owned()
}

fn expected_report() -> Value {
    json!({
        "commits": [{"hash": "5b7385dc", "what": "DEV-1 Thing"}],
        "checks": [
            {"command": "FOO=1 npm test", "result": "passed"},
            {"command": "npm run smoke", "result": "not_run", "reason": "no browser"},
        ],
        "next": "review agent",
    })
}

#[test]
fn cli_report_round_trips_through_yaml_list_and_show() {
    let tf = TestFixtures::new();
    let dir = tf.get_temp_path();
    let created: Value = serde_json::from_str(&run(dir, &["--format", "json", "add", "Report"])).unwrap();
    let id = created["task"]["id"].as_str().expect("created id").to_string();

    let added: Value = serde_json::from_str(&run(
        dir,
        &[
            "--format",
            "json",
            "comment",
            &id,
            "Done with it",
            "--commit",
            "5B7385DC: DEV-1 Thing",
            "--check",
            "FOO=1 npm test=passed",
            "--check",
            "npm run smoke=not-run: no browser",
            "--next",
            "review agent",
        ],
    ))
    .unwrap();
    assert_eq!(added["added_comment"]["report"], expected_report(), "{added}");

    let listed: Value = serde_json::from_str(&run(dir, &["--format", "json", "comment", &id])).unwrap();
    assert_eq!(listed["items"][0]["report"], expected_report(), "{listed}");
    assert_eq!(listed["items"][0]["text"], "Done with it");

    let shown = run(dir, &["show", &id]);
    for line in ["commit 5b7385dc: DEV-1 Thing", "check FOO=1 npm test: passed", "check npm run smoke: not run (no browser)", "next: review agent"] {
        assert!(shown.contains(line), "{line} in:\n{shown}");
    }
}

#[test]
fn cli_refuses_a_report_without_text_or_with_an_unknown_result() {
    let tf = TestFixtures::new();
    let dir = tf.get_temp_path();
    let created: Value = serde_json::from_str(&run(dir, &["--format", "json", "add", "Refused"])).unwrap();
    let id = created["task"]["id"].as_str().expect("created id").to_string();

    let no_text = lotar(dir, &["comment", &id, "--next", "x"]);
    assert!(!no_text.status.success(), "a report needs its prose text");
    let unknown = lotar(dir, &["comment", &id, "note", "--check", "npm test=maybe"]);
    assert!(!unknown.status.success());
    assert!(String::from_utf8_lossy(&unknown.stderr).contains("passed|failed|not_run"), "{}", String::from_utf8_lossy(&unknown.stderr));
    let not_hash = lotar(dir, &["comment", &id, "note", "--commit", "HEAD: latest"]);
    assert!(!not_hash.status.success());

    let listed: Value = serde_json::from_str(&run(dir, &["--format", "json", "comment", &id])).unwrap();
    assert_eq!(listed["comments"], 0, "nothing was stored: {listed}");
}

#[test]
fn comments_without_a_report_keep_their_yaml() {
    let tf = TestFixtures::new();
    let dir = tf.get_temp_path();
    let created: Value = serde_json::from_str(&run(dir, &["--format", "json", "add", "Legacy"])).unwrap();
    let id = created["task"]["id"].as_str().expect("created id").to_string();
    let (prefix, number) = id.split_once('-').unwrap();
    let file = dir.join(".tasks").join(prefix).join(format!("{number}.yml"));

    run(dir, &["comment", &id, "-m", "plain"]);
    let stored = std::fs::read_to_string(&file).unwrap();
    assert!(!stored.contains("  report:"), "a plain comment writes no report key:\n{stored}");

    // A newer version's report key survives this one's edits.
    let with_future = stored
        .replace("  text: plain\n", "  text: plain\n  report:\n    checks:\n    - command: npm test\n      result: flaky\n      seen: 3\n    owner: mallox\n");
    assert_ne!(with_future, stored);
    std::fs::write(&file, &with_future).unwrap();
    run(dir, &["comment", &id, "-m", "second"]);
    let resaved = std::fs::read_to_string(&file).unwrap();
    for kept in ["result: flaky", "seen: 3", "owner: mallox"] {
        assert!(resaved.contains(kept), "{kept} kept in:\n{resaved}");
    }
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
            "default.project: RP\ndefault.reporter: claude\nissue.states: [Todo, InProgress, Done]\nissue.priorities: [Low, Medium, High]\nissue.types: [Feature, Bug, Chore]\nauto.populate_members: false\nauto.identity: false\nauto.identity_git: false\n",
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

fn post(api: &ApiServer, route: &str, body: &Value) -> (u16, Value) {
    let response = api.handle_request(&HttpRequest {
        method: "POST".into(),
        path: route.into(),
        query: HashMap::new(),
        headers: HashMap::new(),
        body: serde_json::to_vec(body).unwrap(),
    });
    (response.status, serde_json::from_slice::<Value>(&response.body).unwrap())
}

#[test]
fn rest_report_round_trips_and_bad_results_are_400() {
    let _fixture = ServerFixture::new();
    let mut api = ApiServer::new();
    routes::initialize(&mut api);
    let (_, task) = post(&api, "/api/tasks/add", &json!({"project": "RP", "title": "REST report"}));
    let id = task["data"]["id"].as_str().unwrap();
    let report = json!({
        "commits": [{"hash": "5b7385dc", "what": " DEV-1 Thing "}],
        "checks": [
            {"command": "FOO=1 npm test", "result": "passed"},
            {"command": "npm run smoke", "result": "not_run", "reason": "no browser"},
        ],
        "next": "review agent",
    });
    let (status, body) = post(&api, "/api/tasks/comment", &json!({"id": id, "text": "via REST", "report": report}));
    assert_eq!(status, 200, "{body}");
    assert_eq!(body["data"]["comments"][0]["report"], expected_report(), "{body}");

    let (status, body) = post(&api, "/api/tasks/comment", &json!({"id": id, "text": "x", "report": {"checks": [{"command": "a", "result": "maybe"}]}}));
    assert_eq!(status, 400, "{body}");
    let (status, body) = post(&api, "/api/tasks/comment", &json!({"id": id, "report": {"next": "x"}}));
    assert_eq!(status, 400, "the prose text stays required: {body}");
}

fn mcp(id: u64, name: &str, arguments: &Value) -> Value {
    let line = serde_json::to_string(&json!({
        "jsonrpc": "2.0",
        "id": id,
        "method": "tools/call",
        "params": {"name": name, "arguments": arguments},
    }))
    .unwrap();
    serde_json::from_str(&lotar::mcp::server::handle_json_line(&line)).unwrap()
}

fn mcp_result(resp: &Value) -> Value {
    let text = resp["result"]["content"][0]["text"].as_str().unwrap_or_else(|| panic!("{resp}"));
    serde_json::from_str(text).unwrap()
}

#[test]
fn mcp_report_round_trips_and_bad_results_are_invalid_params() {
    let _fixture = ServerFixture::new();
    let created = mcp_result(&mcp(1, "task_create", &json!({"project": "RP", "title": "MCP report"})));
    let id = created["task"]["id"].as_str().unwrap_or_else(|| panic!("{created}")).to_string();
    let commented = mcp_result(&mcp(2, "task_comment_add", &json!({"id": id, "text": "via MCP", "report": expected_report()})));
    assert_eq!(commented["task"]["comments"][0]["report"], expected_report(), "{commented}");

    let refused = mcp(3, "task_comment_add", &json!({"id": id, "text": "x", "report": {"commits": [{"hash": "HEAD"}]}}));
    assert_eq!(refused["error"]["code"], -32602, "{refused}");
}

#[test]
fn reports_refuse_other_comment_types() {
    use lotar::services::task_questions::CommentRequest;
    for kind in ["question", "answer", "review"] {
        let mut body = json!({"text": "summary", "report": {"next": "review"}});
        body[kind] = json!({});
        let error = CommentRequest::from_json(&body).expect_err("mixed comment types must fail");
        assert!(error.contains("plain comment"), "{error}");
    }
}

#[test]
fn cli_review_keeps_its_commit_and_refuses_report_checks() {
    let fixture = TestFixtures::new();
    let dir = fixture.temp_dir.path();
    let added: Value = serde_json::from_str(&run(dir, &["--format", "json", "add", "Review example"])).unwrap();
    let id = added["task"]["id"].as_str().unwrap();
    let reviewed = lotar(dir, &["comment", id, "Reviewed", "--review", "pass", "--commit", "5b7385dc"]);
    assert!(reviewed.status.success(), "{}", String::from_utf8_lossy(&reviewed.stderr));
    let refused = lotar(dir, &["comment", id, "Mixed", "--review", "pass", "--check", "npm test=passed"]);
    assert!(!refused.status.success());
}

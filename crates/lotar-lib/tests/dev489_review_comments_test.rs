//! DEV-489: review comments with a verdict, the commit reviewed and the
//! findings, on every surface, and rules that match the verdict.
use lotar::api_server::{ApiServer, HttpRequest};
use lotar::routes;
use serde_json::{Value, json};
use std::collections::HashMap;
use std::path::Path;

use crate::common;
use common::TestFixtures;
use common::env_mutex::EnvVarGuard;

fn lotar(dir: &Path, args: &[&str]) -> std::process::Output {
    common::lotar_cmd().unwrap().current_dir(dir).env("LOTAR_DEFAULT_REPORTER", "codex").args(args).output().unwrap()
}

fn run(dir: &Path, args: &[&str]) -> String {
    let out = lotar(dir, args);
    assert!(out.status.success(), "{args:?}: {}", String::from_utf8_lossy(&out.stderr));
    String::from_utf8_lossy(&out.stdout).into_owned()
}

fn task_file(dir: &Path, id: &str) -> std::path::PathBuf {
    let (prefix, number) = id.split_once('-').unwrap();
    dir.join(".tasks").join(prefix).join(format!("{number}.yml"))
}

#[test]
fn cli_writes_a_review_and_keeps_old_yaml() {
    let tf = TestFixtures::new();
    let dir = tf.get_temp_path();
    let created: Value = serde_json::from_str(&run(dir, &["--format", "json", "add", "Review me"])).unwrap();
    let id = created["task"]["id"].as_str().unwrap().to_string();
    let file = task_file(dir, &id);
    let mut yaml = std::fs::read_to_string(&file).unwrap();
    yaml.push_str("comments:\n- date: 2026-01-01T00:00:00Z\n  text: old note\n  mood: newer-tool key\n");
    std::fs::write(&file, &yaml).unwrap();

    let refused = lotar(dir, &["comment", &id, "--review", "pass", "--blocker", "a blocking finding"]);
    assert!(!refused.status.success(), "a passing review has no blocking finding");

    let added: Value = serde_json::from_str(&run(
        dir,
        &[
            "--format",
            "json",
            "comment",
            &id,
            "--review",
            "block",
            "--commit",
            "abc1234def",
            "--finding",
            "a nit",
            "--blocker",
            "src/a.rs:7: data loss",
            "One problem.",
        ],
    ))
    .unwrap();
    let review = &added["added_comment"]["review"];
    assert_eq!(review["verdict"], "block", "{added}");
    assert_eq!(review["commit"], "abc1234def");
    assert_eq!(review["findings"][1], json!({"summary": "data loss", "file": "src/a.rs", "line": 7, "blocking": true}));
    assert_eq!(review["findings"][0], json!({"summary": "a nit"}), "defaults stay out of the stored finding");
    assert_eq!(
        added["added_comment"]["text"], "Review blocked at abc1234def.\n\nOne problem.\n\nFindings:\n- [blocking] src/a.rs:7: data loss\n- a nit",
        "the text keeps the review in prose"
    );

    let shown = run(dir, &["show", &id]);
    assert!(shown.contains("[review: block] codex: Review blocked at abc1234def."), "{shown}");
    let stored = std::fs::read_to_string(&file).unwrap();
    assert!(stored.contains("- date: 2026-01-01T00:00:00Z\n  text: old note\n  mood: newer-tool key\n"), "the old comment is unchanged:\n{stored}");

    run(dir, &["comment", &id, "--review", "pass"]);
    let shown = run(dir, &["comment", &id]);
    assert!(shown.contains("[review: pass] codex: Review passed."), "{shown}");
}

struct ServerFixture {
    root: std::path::PathBuf,
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
            "default.project: QA\ndefault.reporter: codex\nissue.states: [Todo, InProgress, Done]\nissue.priorities: [Low, Medium, High]\nissue.types: [Feature, Bug, Chore]\nauto.populate_members: false\nauto.identity: false\nauto.identity_git: false\n",
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
        Self { root, _temp: temp, _env: env }
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
fn rest_review_and_a_rule_on_the_verdict() {
    let fixture = ServerFixture::new();
    std::fs::write(
        fixture.root.join("automation.yml"),
        "automation:\n  rules:\n    - name: Review blocked\n      when:\n        comment: { review: block }\n      on:\n        commented:\n          set: { status: InProgress }\n          comment: \"Back to work on ${{comment.commit}} (${{comment.review}})\"\n",
    )
    .unwrap();
    let mut api = ApiServer::new();
    routes::initialize(&mut api);
    let (_, task) = post(&api, "/api/tasks/add", &json!({"project": "QA", "title": "REST review"}));
    let id = task["data"]["id"].as_str().unwrap().to_string();

    let (status, refused) = post(&api, "/api/tasks/comment", &json!({"id": id, "review": {"verdict": "maybe"}}));
    assert_eq!(status, 400, "{refused}");
    let (status, refused) = post(&api, "/api/tasks/comment", &json!({"id": id, "review": {"verdict": "pass"}, "question": {"options": []}}));
    assert_eq!(status, 400, "{refused}");

    let (status, passed) = post(&api, "/api/tasks/comment", &json!({"id": id, "review": {"verdict": "pass", "commit": "1111111"}}));
    assert_eq!(status, 200, "{passed}");
    assert_eq!(passed["data"]["comments"].as_array().unwrap().len(), 1, "a pass does not match the rule: {passed}");
    assert_eq!(passed["data"]["status"], "Todo");

    let body = json!({"id": id, "text": "Breaks the build.", "review": {"verdict": "block", "commit": "2222222", "findings": [{"summary": "fails", "blocking": true}]}});
    let (status, blocked) = post(&api, "/api/tasks/comment", &body);
    assert_eq!(status, 200, "{blocked}");
    let task = &blocked["data"];
    assert_eq!(task["comments"][1]["review"]["findings"][0]["blocking"], true, "{task}");
    assert_eq!(task["status"], "InProgress", "the rule matched the verdict: {task}");
    assert_eq!(task["comments"].as_array().unwrap().last().unwrap()["text"], "Back to work on 2222222 (block)");
}

fn mcp_result(id: u64, arguments: &Value) -> Value {
    let line = serde_json::to_string(&json!({
        "jsonrpc": "2.0",
        "id": id,
        "method": "tools/call",
        "params": {"name": "task_comment_add", "arguments": arguments},
    }))
    .unwrap();
    serde_json::from_str(&lotar::mcp::server::handle_json_line(&line)).unwrap()
}

#[test]
fn mcp_review_round_trip() {
    let _fixture = ServerFixture::new();
    let line = serde_json::to_string(&json!({
        "jsonrpc": "2.0", "id": 1, "method": "tools/call",
        "params": {"name": "task_create", "arguments": {"project": "QA", "title": "MCP review"}},
    }))
    .unwrap();
    let created: Value = serde_json::from_str(&lotar::mcp::server::handle_json_line(&line)).unwrap();
    let created: Value = serde_json::from_str(created["result"]["content"][0]["text"].as_str().unwrap()).unwrap();
    let id = created["task"]["id"].as_str().unwrap().to_string();

    let bad = mcp_result(2, &json!({"id": id, "review": {"verdict": "pass", "findings": [{"summary": "x", "line": 3}]}}));
    assert_eq!(bad["error"]["code"], -32602, "a line needs its file: {bad}");

    let resp = mcp_result(3, &json!({"id": id, "review": {"verdict": "pass", "findings": [{"summary": "tidy", "file": "README.md"}]}}));
    let result: Value = serde_json::from_str(resp["result"]["content"][0]["text"].as_str().unwrap_or_else(|| panic!("{resp}"))).unwrap();
    let comment = &result["task"]["comments"][0];
    assert_eq!(comment["review"], json!({"verdict": "pass", "findings": [{"summary": "tidy", "file": "README.md"}]}), "{comment}");
    assert_eq!(comment["text"], "Review passed.\n\nFindings:\n- README.md: tidy");
}

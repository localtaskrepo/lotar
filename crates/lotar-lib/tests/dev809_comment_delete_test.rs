//! DEV-809: delete a comment or withdraw a question (soft, kept in history),
//! restore it, and edit a comment from the CLI.
use lotar::api_server::{ApiServer, HttpRequest};
use lotar::routes;
use serde_json::{Value, json};
use std::collections::HashMap;
use std::path::Path;

use crate::common;
use common::TestFixtures;
use common::env_mutex::EnvVarGuard;

fn lotar(dir: &Path, reporter: &str, args: &[&str]) -> std::process::Output {
    common::lotar_cmd().unwrap().current_dir(dir).env("LOTAR_DEFAULT_REPORTER", reporter).args(args).output().unwrap()
}

fn run(dir: &Path, reporter: &str, args: &[&str]) -> String {
    let out = lotar(dir, reporter, args);
    assert!(out.status.success(), "{args:?}: {}", String::from_utf8_lossy(&out.stderr));
    String::from_utf8_lossy(&out.stdout).into_owned()
}

fn fails(dir: &Path, args: &[&str]) -> String {
    let out = lotar(dir, "claude", args);
    assert!(!out.status.success(), "{args:?} should fail: {}", String::from_utf8_lossy(&out.stdout));
    String::from_utf8_lossy(&out.stderr).into_owned()
}

fn json_of(text: &str) -> Value {
    serde_json::from_str(text).unwrap_or_else(|e| panic!("{e}: {text}"))
}

fn task_file(dir: &Path, id: &str) -> String {
    let (prefix, number) = id.split_once('-').unwrap();
    std::fs::read_to_string(dir.join(".tasks").join(prefix).join(format!("{number}.yml"))).unwrap()
}

fn add_task(dir: &Path) -> String {
    let created = json_of(&run(dir, "claude", &["--format", "json", "add", "Comment cleanup"]));
    created["task"]["id"].as_str().unwrap().to_string()
}

#[test]
fn cli_deletes_restores_and_edits_by_number_or_date() {
    let tf = TestFixtures::new();
    let dir = tf.get_temp_path();
    let id = add_task(dir);
    run(dir, "claude", &["comment", &id, "first note"]);
    run(dir, "claude", &["comment", &id, "second note"]);
    run(dir, "claude", &["comment", &id, "third note"]);

    let listed = run(dir, "claude", &["comment", &id]);
    assert!(listed.contains("#2  ") && listed.contains("claude: second note"), "{listed}");

    let deleted = json_of(&run(dir, "claude", &["--format", "json", "comment", &id, "--delete", "2"]));
    assert_eq!(deleted["action"], "task.comment.delete", "{deleted}");
    assert_eq!(deleted["status"], "success", "{deleted}");
    assert_eq!(deleted["index"], 2);
    assert!(deleted["comment"]["deleted_at"].is_string(), "{deleted}");
    let yaml = task_file(dir, &id);
    assert!(yaml.contains("second note") && yaml.contains("deleted_at"), "kept in the file: {yaml}");
    assert!(yaml.contains("comment#2.deleted_at"), "history records the delete: {yaml}");

    let again = json_of(&run(dir, "claude", &["--format", "json", "comment", &id, "--delete", "2"]));
    assert_eq!(again["status"], "unchanged", "deleting twice changes nothing: {again}");

    let listed = run(dir, "claude", &["comment", &id]);
    assert!(!listed.contains("second note"), "{listed}");
    assert!(listed.contains("#3  ") && listed.contains("third note"), "numbers stay put: {listed}");
    let shown = run(dir, "claude", &["show", &id]);
    assert!(!shown.contains("second note") && shown.contains("#3  "), "{shown}");
    let shown = json_of(&run(dir, "claude", &["--format", "json", "show", &id]));
    let texts: Vec<&str> = shown["comments"].as_array().unwrap().iter().map(|c| c["text"].as_str().unwrap()).collect();
    assert_eq!(texts, ["first note", "third note"], "{shown}");
    let with_deleted = run(dir, "claude", &["comment", &id, "--deleted"]);
    assert!(with_deleted.contains("[deleted] claude: second note"), "{with_deleted}");

    let refused = fails(dir, &["comment", &id, "--edit", "2", "changed"]);
    assert!(refused.contains("deleted"), "{refused}");

    let date = yaml.split("date: ").nth(2).unwrap().lines().next().unwrap().trim().trim_matches(|c| c == '\'' || c == '"').to_string();
    let restored = json_of(&run(dir, "claude", &["--format", "json", "comment", &id, "--restore", &date]));
    assert_eq!(restored["action"], "task.comment.restore", "{restored}");
    assert!(restored["comment"].get("deleted_at").is_none(), "{restored}");
    assert!(run(dir, "claude", &["comment", &id]).contains("second note"));

    let edited = json_of(&run(dir, "claude", &["--format", "json", "comment", &id, "--edit", "1", "first note, fixed"]));
    assert_eq!(edited["comment"]["text"], "first note, fixed", "{edited}");

    assert!(fails(dir, &["comment", &id, "--delete", "9"]).contains("9"));
    assert!(fails(dir, &["comment", &id, "--delete", "2", "text"]).contains("text"));
}

/// DEV-828: JSON lists carry each comment's stored number, the one
/// `--edit/--delete/--restore` take, so a reader needn't count positions.
#[test]
fn json_lists_number_comments_as_stored() {
    let tf = TestFixtures::new();
    let dir = tf.get_temp_path();
    let id = add_task(dir);
    for text in ["first note", "second note", "third note"] {
        run(dir, "claude", &["comment", &id, text]);
    }
    run(dir, "claude", &["comment", &id, "--delete", "2"]);

    let numbers = |items: &Value| -> Vec<(u64, String)> {
        items.as_array().unwrap().iter().map(|c| (c["number"].as_u64().unwrap(), c["text"].as_str().unwrap().to_string())).collect()
    };
    let expected = vec![(1, "first note".to_string()), (3, "third note".to_string())];
    let listed = json_of(&run(dir, "claude", &["--format", "json", "comment", &id]));
    assert_eq!(numbers(&listed["items"]), expected, "{listed}");
    let shown = json_of(&run(dir, "claude", &["--format", "json", "show", &id]));
    assert_eq!(numbers(&shown["comments"]), expected, "{shown}");
    let all = json_of(&run(dir, "claude", &["--format", "json", "comment", &id, "--deleted"]));
    assert_eq!(numbers(&all["items"]).iter().map(|(n, _)| *n).collect::<Vec<_>>(), [1, 2, 3], "{all}");
}

#[test]
fn withdrawn_question_is_closed_and_deleted_answer_reopens_it() {
    let tf = TestFixtures::new();
    let dir = tf.get_temp_path();
    let id = add_task(dir);
    run(dir, "claude", &["comment", &id, "--question", "Ship it?", "--option", "y: Yes", "--option", "n: No"]);
    assert!(run(dir, "claude", &["list", "--has", "question"]).contains(&id));

    run(dir, "claude", &["comment", &id, "--delete", "1"]);
    assert!(!run(dir, "claude", &["list", "--has", "question"]).contains(&id), "a withdrawn question is not open");
    let refused = fails(dir, &["comment", &id, "--answer", "y"]);
    assert!(refused.contains("withdrawn") || refused.contains("open question"), "{refused}");

    run(dir, "claude", &["comment", &id, "--restore", "1"]);
    run(dir, "mallox", &["comment", &id, "--answer", "y"]);
    assert!(!run(dir, "claude", &["list", "--has", "question"]).contains(&id), "answered");
    run(dir, "claude", &["comment", &id, "--delete", "2"]);
    assert!(run(dir, "claude", &["list", "--has", "question"]).contains(&id), "a deleted answer reopens the question");
    assert!(run(dir, "claude", &["show", &id]).contains("[open question]"));
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
            "default.project: QA\ndefault.reporter: mallox\nissue.states: [Todo, InProgress, Done]\nissue.priorities: [Low, Medium, High]\nissue.types: [Feature, Bug, Chore]\nauto.populate_members: false\nauto.identity: false\nauto.identity_git: false\n",
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
fn rest_deletes_and_restores_keeping_the_index() {
    let _fixture = ServerFixture::new();
    let mut api = ApiServer::new();
    routes::initialize(&mut api);
    let (_, task) = post(&api, "/api/tasks/add", &json!({"project": "QA", "title": "REST delete"}));
    let id = task["data"]["id"].as_str().unwrap().to_string();
    post(&api, "/api/tasks/comment", &json!({"id": id, "text": "keep"}));
    post(&api, "/api/tasks/comment", &json!({"id": id, "text": "oops"}));

    let (status, deleted) = post(&api, "/api/tasks/comment/delete", &json!({"id": id, "index": 1}));
    assert_eq!(status, 200, "{deleted}");
    let comments = deleted["data"]["comments"].as_array().unwrap();
    assert_eq!(comments.len(), 2, "REST keeps deleted comments: {deleted}");
    assert!(comments[1]["deleted_at"].is_string(), "{deleted}");
    let change = &deleted["data"]["history"].as_array().unwrap().last().unwrap()["changes"][0];
    assert_eq!(change["field"], "comment#2.deleted_at", "{deleted}");

    let (status, refused) = post(&api, "/api/tasks/comment/update", &json!({"id": id, "index": 1, "text": "edit"}));
    assert_eq!(status, 400, "{refused}");
    let (status, missing) = post(&api, "/api/tasks/comment/delete", &json!({"id": id}));
    assert_eq!(status, 400, "{missing}");
    let (status, _) = post(&api, "/api/tasks/comment/delete", &json!({"id": id, "index": 7}));
    assert_eq!(status, 400);

    let (status, restored) = post(&api, "/api/tasks/comment/restore", &json!({"id": id, "index": 1}));
    assert_eq!(status, 200, "{restored}");
    assert!(restored["data"]["comments"][1].get("deleted_at").is_none(), "{restored}");
}

fn mcp_call(id: u64, name: &str, arguments: &Value) -> Value {
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
fn mcp_deletes_and_restores() {
    let _fixture = ServerFixture::new();
    let created = mcp_result(&mcp_call(1, "task_create", &json!({"project": "QA", "title": "MCP delete"})));
    let id = created["task"]["id"].as_str().unwrap().to_string();
    mcp_result(&mcp_call(2, "task_comment_add", &json!({"id": id, "text": "Ask?", "question": {"options": [{"key": "a", "label": "A"}]}})));

    let deleted = mcp_result(&mcp_call(3, "task_comment_delete", &json!({"id": id, "index": 0})));
    assert_eq!(deleted["action"], "comment_delete", "{deleted}");
    assert_eq!(deleted["changed"], true);
    assert!(deleted["task"]["comments"][0]["deleted_at"].is_string(), "{deleted}");
    let again = mcp_result(&mcp_call(4, "task_comment_delete", &json!({"id": id, "index": 0})));
    assert_eq!(again["changed"], false, "{again}");

    let answer = mcp_call(5, "task_comment_add", &json!({"id": id, "answer": {"choice": "a"}}));
    assert!(answer.get("error").is_some() || answer["result"]["isError"] == true, "a withdrawn question can't be answered: {answer}");

    let restored = mcp_result(&mcp_call(6, "task_comment_restore", &json!({"id": id, "index": 0})));
    assert_eq!(restored["changed"], true, "{restored}");
    assert!(restored["task"]["comments"][0].get("deleted_at").is_none());
}

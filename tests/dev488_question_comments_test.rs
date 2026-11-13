//! DEV-488: question comments with options, and answers that record the
//! choice and set the ticket's fields, on every surface.
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

fn task_file(dir: &Path, id: &str) -> std::path::PathBuf {
    let (prefix, number) = id.split_once('-').unwrap();
    dir.join(".tasks").join(prefix).join(format!("{number}.yml"))
}

#[test]
fn cli_asks_answers_and_applies_the_option_set() {
    let tf = TestFixtures::new();
    let dir = tf.get_temp_path();
    let created: Value = serde_json::from_str(&run(dir, "claude", &["--format", "json", "add", "Pick one"])).unwrap();
    let id = created["task"]["id"].as_str().unwrap().to_string();

    let asked: Value = serde_json::from_str(&run(
        dir,
        "claude",
        &[
            "--format",
            "json",
            "comment",
            &id,
            "--question",
            "Which approach?",
            "--option",
            "a: Keep it",
            "--option",
            "b: Hand it to bob",
            "--set",
            "b:assignee=bob",
            "--set",
            "b:status=InProgress",
            "--set",
            "b:tags=handed,over",
            "--recommend",
            "b",
        ],
    ))
    .unwrap();
    let question = &asked["added_comment"];
    assert_eq!(question["question"]["recommended"], "b", "{asked}");
    assert_eq!(question["question"]["options"][1]["set"]["assignee"], "bob");
    assert!(question["text"].as_str().unwrap().contains("- b: Hand it to bob (recommended)"), "text keeps the options in prose: {question}");

    let shown = run(dir, "claude", &["show", &id]);
    assert!(shown.contains("[open question] claude: Which approach?"), "{shown}");
    assert!(run(dir, "claude", &["list", "--has", "question"]).contains(&id));
    assert!(run(dir, "claude", &["list", "has:question"]).contains(&id), "the query syntax filters too");

    let wrong = lotar(dir, "mallox", &["comment", &id, "--answer", "c"]);
    assert!(!wrong.status.success());
    assert!(String::from_utf8_lossy(&wrong.stderr).contains("not an option"), "{}", String::from_utf8_lossy(&wrong.stderr));

    let answered: Value = serde_json::from_str(&run(dir, "mallox", &["--format", "json", "comment", &id, "--answer", "B", "go ahead"])).unwrap();
    let answer = &answered["added_comment"];
    assert_eq!(answer["author"], "mallox", "{answered}");
    assert_eq!(answer["answer"]["choice"], json!(["b"]));
    assert_eq!(answer["answer"]["to"], question["date"]);
    assert_eq!(answer["text"], "Answer: b (Hand it to bob)\n\ngo ahead");

    let task: Value = serde_json::from_str(&run(dir, "claude", &["--format", "json", "show", &id])).unwrap();
    let task = task.get("task").unwrap_or(&task);
    assert_eq!(task["assignee"], "bob", "{task}");
    assert_eq!(task["status"], "InProgress");
    assert_eq!(task["tags"], json!(["handed", "over"]));
    let last = task["history"].as_array().unwrap().last().unwrap();
    assert_eq!(last["actor"], "mallox", "the answerer changed the fields: {last}");

    assert!(!run(dir, "claude", &["list", "--has", "question"]).contains(&id), "an answered question is not open");
    let again = lotar(dir, "mallox", &["comment", &id, "--answer", "a"]);
    assert!(!again.status.success(), "no open question is left to answer");
}

#[test]
fn cli_refuses_option_sets_it_cannot_apply() {
    let tf = TestFixtures::new();
    let dir = tf.get_temp_path();
    let created: Value = serde_json::from_str(&run(dir, "claude", &["--format", "json", "add", "Refuse"])).unwrap();
    let id = created["task"]["id"].as_str().unwrap().to_string();
    for set in ["a:run=rm -rf /", "a:status=Nope", "z:status=Done"] {
        let out = lotar(dir, "claude", &["comment", &id, "--question", "Q?", "--option", "a: A", "--set", set]);
        assert!(!out.status.success(), "{set} must be refused");
    }
    let stored = std::fs::read_to_string(task_file(dir, &id)).unwrap();
    assert!(!stored.contains("comments:"), "nothing was asked:\n{stored}");
}

#[test]
fn cli_free_text_answer_and_old_yaml_round_trip() {
    let tf = TestFixtures::new();
    let dir = tf.get_temp_path();
    let created: Value = serde_json::from_str(&run(dir, "claude", &["--format", "json", "add", "Old yaml"])).unwrap();
    let id = created["task"]["id"].as_str().unwrap().to_string();
    let file = task_file(dir, &id);
    let mut yaml = std::fs::read_to_string(&file).unwrap();
    yaml.push_str("comments:\n- date: 2026-01-01T00:00:00Z\n  text: old note\n  mood: newer-tool key\n");
    std::fs::write(&file, &yaml).unwrap();

    run(dir, "claude", &["comment", &id, "--question", "When?", "--option", "week: In a week", "--option", "ask: When you ask"]);
    run(dir, "mallox", &["comment", &id, "--answer", "--", "Next month"]);
    let stored = std::fs::read_to_string(&file).unwrap();
    assert!(stored.contains("- date: 2026-01-01T00:00:00Z\n  text: old note\n  mood: newer-tool key\n"), "the old comment is unchanged:\n{stored}");
    assert!(stored.contains("note: Next month"), "{stored}");
    assert!(!stored.contains("choice:"), "a free-text answer chooses nothing:\n{stored}");
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

fn question_body(id: &str) -> Value {
    json!({
        "id": id,
        "text": "Rework or close?",
        "question": {
            "options": [
                {"key": "rework", "label": "Rework it", "set": {"status": "InProgress"}},
                {"key": "close", "label": "Close it", "set": {"status": "Done"}}
            ],
            "recommended": "rework"
        }
    })
}

#[test]
fn rest_asks_and_answers_and_automation_sees_the_choice() {
    let fixture = ServerFixture::new();
    std::fs::write(
        fixture.root.join("automation.yml"),
        "automation:\n  rules:\n    - name: Closed by answer\n      when:\n        comment: { answer: close }\n      on:\n        commented:\n          comment: \"Chose ${{comment.choice}}\"\n",
    )
    .unwrap();
    let mut api = ApiServer::new();
    routes::initialize(&mut api);
    let (_, task) = post(&api, "/api/tasks/add", &json!({"project": "QA", "title": "REST question"}));
    let id = task["data"]["id"].as_str().unwrap().to_string();

    let (status, refused) =
        post(&api, "/api/tasks/comment", &json!({"id": id, "text": "Q?", "question": {"options": [{"key": "a", "label": "A", "set": {"run": "ls"}}]}}));
    assert_eq!(status, 400, "{refused}");

    let (status, asked) = post(&api, "/api/tasks/comment", &question_body(&id));
    assert_eq!(status, 200, "{asked}");
    let question = &asked["data"]["comments"][0];
    assert_eq!(question["question"]["options"][1]["set"]["status"], "Done", "{asked}");
    assert!(question["question"].get("multiple").is_none(), "defaults stay out of the stored question: {question}");

    let (status, answered) = post(&api, "/api/tasks/comment", &json!({"id": id, "answer": {"choice": "close"}, "text": "done here"}));
    assert_eq!(status, 200, "{answered}");
    let task = &answered["data"];
    assert_eq!(task["status"], "Done", "{task}");
    let comments = task["comments"].as_array().unwrap();
    assert_eq!(comments[1]["answer"]["choice"], json!(["close"]));
    assert_eq!(comments[1]["answer"]["note"], "done here");
    assert_eq!(comments.last().unwrap()["text"], "Chose close", "the commented rule saw the answer: {task}");
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
fn mcp_asks_and_answers() {
    let _fixture = ServerFixture::new();
    let created = mcp_result(&mcp_call(1, "task_create", &json!({"project": "QA", "title": "MCP question"})));
    let id = created["task"]["id"].as_str().unwrap().to_string();

    let asked = mcp_result(&mcp_call(2, "task_comment_add", &question_body(&id)));
    let date = asked["task"]["comments"][0]["date"].as_str().unwrap().to_string();

    let two = mcp_call(3, "task_comment_add", &json!({"id": id, "answer": {"choice": ["rework", "close"]}}));
    assert_eq!(two["error"]["code"], -32602, "one option only: {two}");

    let answered = mcp_result(&mcp_call(4, "task_comment_add", &json!({"id": id, "answer": {"to": date, "choice": "rework"}})));
    assert_eq!(answered["task"]["status"], "InProgress", "{answered}");
    assert_eq!(answered["task"]["comments"][1]["answer"]["to"], date);
}

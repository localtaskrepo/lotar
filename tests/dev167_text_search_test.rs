//! DEV-167: one set of search rules across REST, CLI and MCP: every word must
//! match, quoted phrases, `-word` exclusions, accents and typo tolerance, plus
//! the opt-in `relevance` sort (best match first).

use lotar::api_server::{ApiServer, HttpRequest};
use serde_json::Value;
use std::collections::HashMap;
mod common;
use common::env_mutex::EnvVarGuard;

const TASKS: [(u32, &str); 6] = [
    (
        1,
        "title: Fix the login redirect loop\ndescription: Happens after OAuth sign-in\n",
    ),
    (2, "title: Redirect users after logout\n"),
    (
        3,
        "title: Login page copy\ncomments:\n- date: 2026-01-02T00:00:00Z\n  text: The redirect loop also shows up on Safari\n",
    ),
    (4, "title: Café menu translations\ntags: [i18n]\n"),
    (5, "title: Login\n"),
    (
        6,
        "title: Authentication audit\nacceptance_criteria:\n- Every login is logged\n",
    ),
];

fn fixture() -> tempfile::TempDir {
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path().join("SRCH");
    std::fs::create_dir_all(&dir).unwrap();
    for (number, fields) in TASKS {
        std::fs::write(
            dir.join(format!("{number}.yml")),
            format!("{fields}status: Todo\npriority: Medium\ntype: Feature\ncreated: 2026-01-01T00:00:00Z\nmodified: 2026-01-0{number}T00:00:00Z\n"),
        )
        .unwrap();
    }
    tmp
}

fn rest_list(query: &str, sort_by: Option<&str>) -> Vec<String> {
    let mut api = ApiServer::new();
    lotar::routes::initialize(&mut api);
    let mut params = HashMap::from([
        ("project".to_string(), "SRCH".to_string()),
        ("q".to_string(), query.to_string()),
    ]);
    if let Some(sort_by) = sort_by {
        params.insert("sort_by".into(), sort_by.into());
    }
    let response = api.handle_request(&HttpRequest {
        method: "GET".into(),
        path: "/api/tasks/list".into(),
        query: params,
        headers: HashMap::new(),
        body: Vec::new(),
    });
    assert_eq!(
        response.status,
        200,
        "{}",
        String::from_utf8_lossy(&response.body)
    );
    let body: Value = serde_json::from_slice(&response.body).unwrap();
    body["data"]["tasks"]
        .as_array()
        .unwrap()
        .iter()
        .map(|task| task["id"].as_str().unwrap().to_owned())
        .collect()
}

fn sorted(mut ids: Vec<String>) -> Vec<String> {
    ids.sort_by(|a, b| lotar::storage::compare_task_ids(a, b));
    ids
}

fn ids(list: &[&str]) -> Vec<String> {
    list.iter().map(|id| format!("SRCH-{id}")).collect()
}

#[test]
fn rest_search_matches_words_phrases_exclusions_accents_and_typos() {
    let tmp = fixture();
    let _env = EnvVarGuard::set("LOTAR_TASKS_DIR", tmp.path().to_str().unwrap());

    // Every word, any order, any field (task 3 matches through a comment).
    assert_eq!(sorted(rest_list("redirect login", None)), ids(&["1", "3"]));
    assert_eq!(
        sorted(rest_list("\"redirect loop\"", None)),
        ids(&["1", "3"])
    );
    assert_eq!(sorted(rest_list("\"login redirect\"", None)), ids(&["1"]));
    assert_eq!(
        sorted(rest_list("login -oauth", None)),
        ids(&["3", "5", "6"])
    );
    assert_eq!(sorted(rest_list("cafe", None)), ids(&["4"]));
    assert_eq!(sorted(rest_list("redirct", None)), ids(&["1", "2", "3"]));
    assert_eq!(sorted(rest_list("SRCH-4", None)), ids(&["4"]));
    // The old single-substring behaviour still holds for one word.
    assert_eq!(sorted(rest_list("redir", None)), ids(&["1", "2", "3"]));
}

#[test]
fn rest_relevance_sort_puts_best_matches_first_and_default_order_is_unchanged() {
    let tmp = fixture();
    let _env = EnvVarGuard::set("LOTAR_TASKS_DIR", tmp.path().to_str().unwrap());

    // Default order stays modified desc even while searching.
    assert_eq!(rest_list("login", None), ids(&["6", "5", "3", "1"]));
    // Relevance: exact title, title prefix, title word, then the acceptance criterion.
    assert_eq!(
        rest_list("login", Some("relevance")),
        ids(&["5", "3", "1", "6"])
    );
    // Without a search, relevance falls back to the default order.
    let mut api = ApiServer::new();
    lotar::routes::initialize(&mut api);
    let response = api.handle_request(&HttpRequest {
        method: "GET".into(),
        path: "/api/tasks/list".into(),
        query: HashMap::from([
            ("project".to_string(), "SRCH".to_string()),
            ("sort_by".to_string(), "relevance".to_string()),
        ]),
        headers: HashMap::new(),
        body: Vec::new(),
    });
    assert_eq!(response.status, 200);
    let body: Value = serde_json::from_slice(&response.body).unwrap();
    let all: Vec<&str> = body["data"]["tasks"]
        .as_array()
        .unwrap()
        .iter()
        .map(|task| task["id"].as_str().unwrap())
        .collect();
    assert_eq!(
        all,
        ["SRCH-6", "SRCH-5", "SRCH-4", "SRCH-3", "SRCH-2", "SRCH-1"]
    );
}

#[test]
fn cli_list_uses_the_same_search_and_relevance_sort() {
    let tmp = fixture();
    let run = |args: &[&str]| -> Vec<String> {
        let output = common::lotar_cmd()
            .unwrap()
            .current_dir(tmp.path())
            .env("LOTAR_TASKS_DIR", tmp.path())
            .args(["--format", "json", "list"])
            .args(args)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        let json: Value = serde_json::from_slice(&output.stdout).unwrap();
        json["tasks"]
            .as_array()
            .unwrap_or_else(|| panic!("unexpected list output: {json}"))
            .iter()
            .map(|task| task["id"].as_str().unwrap().to_owned())
            .collect()
    };
    assert_eq!(sorted(run(&["redirect login"])), ids(&["1", "3"]));
    assert_eq!(sorted(run(&["login -oauth"])), ids(&["3", "5", "6"]));
    assert_eq!(
        run(&["login", "--sort-by", "relevance"]),
        ids(&["5", "3", "1", "6"])
    );
    assert_eq!(
        run(&["login", "--sort-by", "relevance", "--reverse"]),
        ids(&["6", "1", "3", "5"])
    );
}

#[test]
fn mcp_task_list_search_and_relevance_sort() {
    let tmp = fixture();
    let _env = EnvVarGuard::set("LOTAR_TASKS_DIR", tmp.path().to_str().unwrap());
    let call = |params: Value| -> Vec<String> {
        let request =
            serde_json::json!({"jsonrpc": "2.0", "id": 1, "method": "task/list", "params": params});
        let line = lotar::mcp::server::handle_json_line(&serde_json::to_string(&request).unwrap());
        let response: Value = serde_json::from_str(&line).unwrap();
        assert!(response.get("error").is_none(), "{response}");
        let text = response["result"]["content"][0]["text"].as_str().unwrap();
        let payload: Value = serde_json::from_str(text).unwrap();
        payload["tasks"]
            .as_array()
            .unwrap()
            .iter()
            .map(|task| task["id"].as_str().unwrap().to_owned())
            .collect()
    };
    assert_eq!(
        sorted(call(
            serde_json::json!({"project": "SRCH", "search": "\"login redirect\""})
        )),
        ids(&["1"])
    );
    assert_eq!(
        call(serde_json::json!({"project": "SRCH", "search": "login", "sort_by": "relevance"})),
        ids(&["5", "3", "1", "6"])
    );
}

//! DEV-197: `/regex/` and field-scoped (`title:`, `comment:` …) search terms
//! work the same over REST, CLI and MCP, and an invalid pattern is reported
//! instead of silently matching nothing.

use lotar::api_server::{ApiServer, HttpRequest};
use serde_json::Value;
use std::collections::HashMap;
mod common;
use common::env_mutex::EnvVarGuard;

const TASKS: [(u32, &str); 4] = [
    (
        1,
        "title: \"Fixes #142 in login\"\ndescription: OAuth2 redirect breaks\n",
    ),
    (
        2,
        "title: Login copy\ncomments:\n- date: 2026-01-02T00:00:00Z\n  text: Needs repro on Safari 17\n",
    ),
    (3, "title: Safari layout\n"),
    (4, "title: \"Fixed #7 typo\"\n"),
];

fn fixture() -> tempfile::TempDir {
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path().join("RGX");
    std::fs::create_dir_all(&dir).unwrap();
    for (number, fields) in TASKS {
        std::fs::write(
            dir.join(format!("{number}.yml")),
            format!("{fields}status: Todo\npriority: Medium\ntype: Feature\ncreated: 2026-01-01T00:00:00Z\n"),
        )
        .unwrap();
    }
    tmp
}

fn rest(query: &str) -> (u16, Value) {
    let mut api = ApiServer::new();
    lotar::routes::initialize(&mut api);
    let response = api.handle_request(&HttpRequest {
        method: "GET".into(),
        path: "/api/tasks/list".into(),
        query: HashMap::from([
            ("project".to_string(), "RGX".to_string()),
            ("q".to_string(), query.to_string()),
            ("sort_by".to_string(), "id".to_string()),
            ("order".to_string(), "asc".to_string()),
        ]),
        headers: HashMap::new(),
        body: Vec::new(),
    });
    (
        response.status,
        serde_json::from_slice(&response.body).unwrap(),
    )
}

fn rest_ids(query: &str) -> Vec<String> {
    let (status, body) = rest(query);
    assert_eq!(status, 200, "{body}");
    body["data"]["tasks"]
        .as_array()
        .unwrap_or_else(|| panic!("unexpected body for {query:?}: {body}"))
        .iter()
        .map(|task| task["id"].as_str().unwrap().to_owned())
        .collect()
}

fn ids(list: &[u32]) -> Vec<String> {
    list.iter().map(|n| format!("RGX-{n}")).collect()
}

#[test]
fn rest_supports_regex_and_field_scoped_terms() {
    let tmp = fixture();
    let _env = EnvVarGuard::set("LOTAR_TASKS_DIR", tmp.path().to_str().unwrap());

    assert_eq!(rest_ids(r"/fix(es|ed)? #\d+/"), ids(&[1, 4]));
    assert_eq!(rest_ids(r"/^fixes/"), ids(&[1]));
    assert_eq!(rest_ids("/oauth2?/ redirect"), ids(&[1]));
    assert_eq!(rest_ids("safari"), ids(&[2, 3]));
    assert_eq!(rest_ids("title:safari"), ids(&[3]));
    assert_eq!(rest_ids(r"comment:/safari \d+/"), ids(&[2]));
    assert_eq!(rest_ids("login -comment:repro"), ids(&[1]));
    assert_eq!(rest_ids("desc:redirct"), ids(&[1]));
}

#[test]
fn rest_rejects_an_invalid_pattern_with_a_clear_message() {
    let tmp = fixture();
    let _env = EnvVarGuard::set("LOTAR_TASKS_DIR", tmp.path().to_str().unwrap());
    let (status, body) = rest("login /fix(/");
    assert_eq!(status, 400, "{body}");
    let message = body.to_string();
    assert!(
        message.contains("Invalid search pattern /fix(/"),
        "{message}"
    );
}

#[test]
fn cli_and_mcp_share_the_rules_and_the_error() {
    let tmp = fixture();
    let cli = |query: &str| {
        common::lotar_cmd()
            .unwrap()
            .current_dir(tmp.path())
            .env("LOTAR_TASKS_DIR", tmp.path())
            .args(["--format", "json", "list", query, "--sort-by", "id"])
            .output()
            .unwrap()
    };
    let output = cli(r"title:/^fix/");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let json: Value = serde_json::from_slice(&output.stdout).unwrap();
    let found: Vec<&str> = json["tasks"]
        .as_array()
        .unwrap()
        .iter()
        .map(|task| task["id"].as_str().unwrap())
        .collect();
    assert_eq!(found, ["RGX-1", "RGX-4"]);
    let failed = cli("/fix(/");
    assert!(!failed.status.success());
    let all = format!(
        "{}{}",
        String::from_utf8_lossy(&failed.stdout),
        String::from_utf8_lossy(&failed.stderr)
    );
    assert!(all.contains("Invalid search pattern /fix(/"), "{all}");

    let _env = EnvVarGuard::set("LOTAR_TASKS_DIR", tmp.path().to_str().unwrap());
    let call = |search: &str| -> Value {
        let request = serde_json::json!({
            "jsonrpc": "2.0", "id": 1, "method": "task/list",
            "params": {"project": "RGX", "search": search, "sort_by": "id", "order": "asc"}
        });
        serde_json::from_str(&lotar::mcp::server::handle_json_line(
            &serde_json::to_string(&request).unwrap(),
        ))
        .unwrap()
    };
    let response = call("comments:repro");
    let text = response["result"]["content"][0]["text"].as_str().unwrap();
    let payload: Value = serde_json::from_str(text).unwrap();
    assert_eq!(payload["tasks"][0]["id"], "RGX-2");
    assert_eq!(payload["tasks"].as_array().unwrap().len(), 1);
    let error = call("/fix(/");
    assert_eq!(error["error"]["code"], -32602, "{error}");
    assert!(
        error["error"]["message"]
            .as_str()
            .unwrap()
            .contains("Invalid search pattern")
    );
}

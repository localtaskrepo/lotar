//! DEV-210: agent profiles can be added from the UI (POST /api/agents/profiles)
//! and set like other config fields (`config set agents`).

use lotar::api_server::{ApiServer, HttpRequest};
use serde_json::{Value, json};
use std::collections::HashMap;
mod common;
use common::env_mutex::EnvVarGuard;

fn call(method: &str, body: Value) -> (u16, Value) {
    let mut api = ApiServer::new();
    lotar::routes::initialize(&mut api);
    let response = api.handle_request(&HttpRequest {
        method: method.into(),
        path: "/api/agents/profiles".into(),
        query: HashMap::new(),
        headers: HashMap::new(),
        body: if body.is_null() {
            Vec::new()
        } else {
            serde_json::to_vec(&body).unwrap()
        },
    });
    (
        response.status,
        serde_json::from_slice(&response.body).unwrap(),
    )
}

fn workspace() -> tempfile::TempDir {
    let tmp = tempfile::tempdir().unwrap();
    std::fs::write(tmp.path().join("config.yml"), "default:\n  project: APP\n").unwrap();
    std::fs::create_dir_all(tmp.path().join("APP")).unwrap();
    std::fs::write(tmp.path().join("APP/config.yml"), "project:\n  name: App\n").unwrap();
    tmp
}

#[test]
fn adding_a_profile_writes_the_default_project_and_lists_it() {
    let tmp = workspace();
    let _env = EnvVarGuard::set("LOTAR_TASKS_DIR", tmp.path().to_str().unwrap());

    let (status, body) = call("GET", Value::Null);
    assert_eq!(status, 200);
    assert_eq!(body["data"]["profiles"], json!([]));
    assert!(body["data"]["installed_runners"].is_array());
    assert_eq!(body["data"]["project"], "APP");

    let (status, body) = call(
        "POST",
        json!({"name": "implement", "runner": "claude", "effort": "high"}),
    );
    assert_eq!(status, 200, "{body}");
    assert_eq!(body["data"]["updated"], true);
    let project_yaml = std::fs::read_to_string(tmp.path().join("APP/config.yml")).unwrap();
    assert!(project_yaml.contains("implement"), "{project_yaml}");
    assert!(project_yaml.contains("effort: high"), "{project_yaml}");

    let (_, body) = call("GET", Value::Null);
    assert_eq!(body["data"]["profiles"][0]["name"], "implement");
    assert_eq!(body["data"]["profiles"][0]["effort"], "high");

    // A plain second profile is stored in short form next to the first.
    let (status, _) = call("POST", json!({"name": "review", "runner": "codex"}));
    assert_eq!(status, 200);
    let (_, body) = call("GET", Value::Null);
    let names: Vec<&str> = body["data"]["profiles"]
        .as_array()
        .unwrap()
        .iter()
        .map(|p| p["name"].as_str().unwrap())
        .collect();
    assert_eq!(names, ["implement", "review"]);
}

#[test]
fn adding_refuses_duplicates_and_bad_values() {
    let tmp = workspace();
    let _env = EnvVarGuard::set("LOTAR_TASKS_DIR", tmp.path().to_str().unwrap());
    assert_eq!(
        call(
            "POST",
            json!({"name": "dev", "runner": "claude", "global": true})
        )
        .0,
        200
    );
    let global_yaml = std::fs::read_to_string(tmp.path().join("config.yml")).unwrap();
    assert!(global_yaml.contains("dev"), "{global_yaml}");

    for (request, message) in [
        (json!({"name": "dev", "runner": "codex"}), "already exists"),
        (
            json!({"name": "bad name", "runner": "claude"}),
            "Invalid agent name",
        ),
        (json!({"name": "x", "runner": "vim"}), "unsupported runner"),
        (
            json!({"name": "x", "runner": "claude", "model": "--yolo"}),
            "model",
        ),
        (
            json!({"name": "x", "runner": "claude", "permissions": "root"}),
            "Invalid permissions",
        ),
    ] {
        let (status, body) = call("POST", request.clone());
        assert_eq!(status, 400, "{request} -> {body}");
        assert!(body.to_string().contains(message), "{request} -> {body}");
    }
}

#[test]
fn config_set_accepts_an_agents_map() {
    let tmp = workspace();
    common::lotar_cmd()
        .unwrap()
        .env("LOTAR_TASKS_DIR", tmp.path())
        .env("LOTAR_TEST_SILENT", "1")
        .args([
            "config",
            "set",
            "agents",
            r#"{"implement": {"runner": "claude", "model": "sonnet"}, "quick": "copilot"}"#,
            "--project",
            "APP",
        ])
        .assert()
        .success();
    let yaml = std::fs::read_to_string(tmp.path().join("APP/config.yml")).unwrap();
    assert!(yaml.contains("model: sonnet"), "{yaml}");
    assert!(yaml.contains("quick: copilot"), "{yaml}");
}

//! DEV-209: `/api/agents/profiles` reports each profile's run settings so the
//! Agents page can show them and prefill the run dialog, in config order so
//! the assignee picker lists variants as written (DEV-768).

use crate::common;
use common::env_mutex::EnvVarGuard;
use lotar::api_server::{ApiServer, HttpRequest};
use serde_json::Value;
use std::collections::HashMap;

#[test]
fn profiles_include_description_model_effort_and_permissions_in_config_order() {
    let tmp = tempfile::tempdir().unwrap();
    std::fs::write(
        tmp.path().join("config.yml"),
        "default:\n  project: PRF\nagents:\n  test:\n    runner: codex\n    effort: low\n    permissions: read-only\n  implement:\n    runner: claude\n    description: Implements the ticket\n    model: sonnet\n    effort: high\n  quick: copilot\n",
    )
    .unwrap();
    let _env = EnvVarGuard::set("LOTAR_TASKS_DIR", tmp.path().to_str().unwrap());

    let mut api = ApiServer::new();
    lotar::routes::initialize(&mut api);
    let response = api.handle_request(&HttpRequest {
        method: "GET".into(),
        path: "/api/agents/profiles".into(),
        query: HashMap::new(),
        headers: HashMap::new(),
        body: Vec::new(),
    });
    assert_eq!(response.status, 200);
    let body: Value = serde_json::from_slice(&response.body).unwrap();
    let profiles = body["data"]["profiles"].as_array().unwrap();
    let names: Vec<&str> = profiles.iter().map(|p| p["name"].as_str().unwrap()).collect();
    assert_eq!(names, ["test", "implement", "quick"]);

    assert_eq!(profiles[1]["runner"], "claude");
    assert_eq!(profiles[1]["description"], "Implements the ticket");
    assert!(profiles[0].get("description").is_none());
    assert_eq!(profiles[1]["model"], "sonnet");
    assert_eq!(profiles[1]["effort"], "high");
    assert_eq!(profiles[1]["permissions"], "workspace");
    assert!(profiles[2].get("model").is_none());
    assert_eq!(profiles[2]["permissions"], "workspace");
    assert_eq!(profiles[0]["effort"], "low");
    assert_eq!(profiles[0]["permissions"], "read-only");
}

#[test]
fn profiles_without_a_project_come_from_the_default_project() {
    let tmp = tempfile::tempdir().unwrap();
    std::fs::write(tmp.path().join("config.yml"), "default:\n  project: WEB\n").unwrap();
    std::fs::create_dir_all(tmp.path().join("WEB")).unwrap();
    std::fs::write(tmp.path().join("WEB/config.yml"), "project:\n  name: Web\nagents:\n  implement: claude\n").unwrap();
    let _env = EnvVarGuard::set("LOTAR_TASKS_DIR", tmp.path().to_str().unwrap());

    let mut api = ApiServer::new();
    lotar::routes::initialize(&mut api);
    let response = api.handle_request(&HttpRequest {
        method: "GET".into(),
        path: "/api/agents/profiles".into(),
        query: HashMap::new(),
        headers: HashMap::new(),
        body: Vec::new(),
    });
    let body: Value = serde_json::from_slice(&response.body).unwrap();
    assert_eq!(body["data"]["profiles"][0]["name"], "implement", "{body}");
}

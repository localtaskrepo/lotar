//! DEV-545: `/api/tasks/values` gives the task panel the distinct people,
//! tags and custom field values of a project, without whole tasks.

use crate::common;
use common::env_mutex::EnvVarGuard;
use common::openapi_semantics::SpecValidator;
use lotar::api_server::{ApiServer, HttpRequest};
use serde_json::{Value, json};
use std::collections::HashMap;

fn write_task(dir: &std::path::Path, number: usize, extra: &str) {
    let body =
        format!("title: Task {number}\nstatus: Todo\npriority: Low\ntype: Feature\ncreated: 2026-01-01T00:00:00Z\nmodified: 2026-01-01T00:00:00Z\n{extra}");
    std::fs::write(dir.join(format!("{number}.yml")), body).unwrap();
}

fn call(api: &ApiServer, query: &[(&str, &str)]) -> (u16, Value) {
    let resp = api.handle_request(&HttpRequest {
        method: "GET".into(),
        path: "/api/tasks/values".into(),
        query: query.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect::<HashMap<_, _>>(),
        headers: HashMap::new(),
        body: Vec::new(),
    });
    (resp.status, serde_json::from_slice(&resp.body).unwrap_or(Value::Null))
}

#[test]
fn values_cover_every_active_task_of_the_project() {
    let tmp = tempfile::tempdir().unwrap();
    let tasks = tmp.path().join(".tasks");
    let demo = tasks.join("DEMO");
    let other = tasks.join("OTHER");
    std::fs::create_dir_all(&demo).unwrap();
    std::fs::create_dir_all(&other).unwrap();
    std::fs::write(tasks.join("config.yml"), "default:\n  project: DEMO\n").unwrap();
    write_task(
        &demo,
        1,
        "assignee: ' alice '\nreporter: '@claude'\ntags: [web, ' ', api]\ncustom_fields:\n  version: '1.2'\n  points: 3\n  labels: [a, 7, true]\n  done: true\n  blank: ''\n",
    );
    write_task(&demo, 2, "assignee: bob\ntags: [web]\ncustom_fields:\n  version: '1.2'\n");
    // Beyond the first 200 tasks the panel used to read.
    for number in 3..=250 {
        write_task(&demo, number, "");
    }
    write_task(&demo, 251, "reporter: carol\ntags: [old]\ncustom_fields:\n  version: '0.9'\n");
    write_task(&demo, 252, "assignee: deleted-person\ndeleted_at: 2026-02-01T00:00:00Z\n");
    write_task(&other, 1, "assignee: dave\ntags: [elsewhere]\n");
    let _guard = EnvVarGuard::set("LOTAR_TASKS_DIR", &tasks.to_string_lossy());
    let mut api = ApiServer::new();
    lotar::routes::initialize(&mut api);

    let (status, body) = call(&api, &[("project", "DEMO")]);
    assert_eq!(status, 200, "{body}");
    assert_eq!(
        body["data"],
        json!({
            "people": ["@claude", "alice", "bob", "carol"],
            "tags": ["api", "old", "web"],
            "custom_fields": {"labels": ["7", "a"], "points": ["3"], "version": ["0.9", "1.2"]},
        })
    );
    SpecValidator::load_published().validate_component("EnvelopeTaskValues", &body).unwrap();

    let (status, body) = call(&api, &[]);
    assert_eq!(status, 200, "{body}");
    assert_eq!(body["data"]["people"], json!(["@claude", "alice", "bob", "carol", "dave"]));
    assert_eq!(body["data"]["tags"], json!(["api", "elsewhere", "old", "web"]));

    let (status, _) = call(&api, &[("project", "NOPE")]);
    assert_eq!(status, 404);
}

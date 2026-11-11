//! DEV-276: REST bodies, status codes and response shapes follow the
//! documented contract.

use crate::common;
use common::env_mutex::EnvVarGuard;
use lotar::api_server::{ApiServer, HttpRequest};
use serde_json::{Value, json};
use std::collections::HashMap;

struct Api {
    _tmp: tempfile::TempDir,
    _guard: EnvVarGuard,
    api: ApiServer,
}

impl Api {
    fn new() -> Self {
        let tmp = tempfile::tempdir().unwrap();
        let tasks = tmp.path().join(".tasks");
        std::fs::create_dir_all(tasks.join("DEMO")).unwrap();
        std::fs::write(tasks.join("config.yml"), "default:\n  project: DEMO\n").unwrap();
        std::fs::write(
            tasks.join("DEMO").join("1.yml"),
            "title: One\nstatus: Todo\npriority: Low\ntype: Feature\ncreated: 2026-01-01T00:00:00Z\nmodified: 2026-01-01T00:00:00Z\n",
        )
        .unwrap();
        let guard = EnvVarGuard::set("LOTAR_TASKS_DIR", &tasks.to_string_lossy());
        let mut api = ApiServer::new();
        lotar::routes::initialize(&mut api);
        Self { _tmp: tmp, _guard: guard, api }
    }

    fn call(&self, method: &str, path: &str, query: &[(&str, &str)], body: &[u8]) -> (u16, Value) {
        let resp = self.api.handle_request(&HttpRequest {
            method: method.into(),
            path: path.into(),
            query: query.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect::<HashMap<_, _>>(),
            headers: HashMap::new(),
            body: body.to_vec(),
        });
        let json = serde_json::from_slice(&resp.body).unwrap_or(Value::Null);
        (resp.status, json)
    }
}

#[test]
fn malformed_bodies_and_unknown_fields_are_400s() {
    let api = Api::new();
    let (status, body) = api.call("POST", "/api/tasks/add", &[], b"{not json");
    assert_eq!(status, 400, "{body}");
    assert!(body.to_string().contains("Invalid body"), "{body}");
    let (status, _) = api.call("POST", "/api/config/set", &[], b"{oops");
    assert_eq!(status, 400);

    let update = json!({"id": "DEMO-1", "tittle": "typo"}).to_string();
    let (status, body) = api.call("POST", "/api/tasks/update", &[], update.as_bytes());
    assert_eq!(status, 400, "{body}");
    assert!(body.to_string().contains("tittle"), "{body}");

    let blank = json!({"id": "DEMO-1", "text": "   "}).to_string();
    let (status, _) = api.call("POST", "/api/tasks/comment", &[], blank.as_bytes());
    assert_eq!(status, 400);
}

#[test]
fn unknown_resources_and_wrong_methods_are_not_200s() {
    let api = Api::new();
    let (status, _) = api.call("GET", "/api/projects/stats", &[("project", "NOPE")], b"");
    assert_eq!(status, 404);
    let (status, _) = api.call("GET", "/api/projects/stats", &[("project", "DEMO")], b"");
    assert_eq!(status, 200);
    let (status, body) = api.call("DELETE", "/api/tasks/get", &[], b"");
    assert_eq!(status, 405, "{body}");
    let (status, _) = api.call("GET", "/api/activity/series", &[("group", "bogus")], b"");
    assert_eq!(status, 400);
    let (status, _) = api.call("GET", "/api/activity/series", &[("since", "garbage")], b"");
    assert_eq!(status, 400);
}

#[test]
fn task_collections_are_always_present() {
    let api = Api::new();
    let (status, body) = api.call("GET", "/api/tasks/list", &[], b"");
    assert_eq!(status, 200);
    let task = &body["data"]["tasks"][0];
    for key in ["tags", "relationships", "comments", "references", "sprints", "history", "custom_fields"] {
        assert!(task.get(key).is_some(), "missing {key}: {task}");
    }
    let (_, body) = api.call("GET", "/api/projects/list", &[("offset", "999")], b"");
    assert_eq!(body["data"]["projects"], json!([]), "{body}");
}

//! DEV-692: `GET /api/tasks/list` rows leave out description, comments and
//! history, `include=history` keeps history, and `ids=` answers where just
//! those tasks stand in the list.

use crate::common;
use common::env_mutex::EnvVarGuard;
use lotar::api_server::{ApiServer, HttpRequest};
use serde_json::Value;
use std::collections::HashMap;

struct Api {
    _tmp: tempfile::TempDir,
    _guard: EnvVarGuard,
    api: ApiServer,
}

const TASK_WITH_DETAILS: &str = "title: One\nstatus: Doing\npriority: Low\ntype: Feature\n\
created: 2026-01-01T00:00:00Z\nmodified: 2026-01-04T00:00:00Z\ndescription: A long description\n\
comments:\n- date: 2026-01-02T00:00:00Z\n  text: Plain comment\n\
- date: 2026-01-03T00:00:00Z\n  text: Ship it?\n  question:\n    options:\n    - key: y\n      label: Yes\n\
history:\n- at: 2026-01-02T00:00:00Z\n  changes:\n  - field: status\n    old: Todo\n    new: Doing\n";

impl Api {
    fn new() -> Self {
        let tmp = tempfile::tempdir().unwrap();
        let tasks = tmp.path().join(".tasks");
        std::fs::create_dir_all(tasks.join("DEMO")).unwrap();
        std::fs::write(tasks.join("config.yml"), "default:\n  project: DEMO\n").unwrap();
        std::fs::write(tasks.join("DEMO").join("1.yml"), TASK_WITH_DETAILS).unwrap();
        for n in 2..=4 {
            std::fs::write(
                tasks.join("DEMO").join(format!("{n}.yml")),
                format!("title: Task {n}\nstatus: Todo\npriority: Low\ntype: Feature\ncreated: 2026-01-0{n}T00:00:00Z\nmodified: 2026-01-0{n}T00:00:00Z\n"),
            )
            .unwrap();
        }
        let guard = EnvVarGuard::set("LOTAR_TASKS_DIR", &tasks.to_string_lossy());
        let mut api = ApiServer::new();
        lotar::routes::initialize(&mut api);
        Self { _tmp: tmp, _guard: guard, api }
    }

    fn list(&self, query: &[(&str, &str)]) -> (u16, Value) {
        let resp = self.api.handle_request(&HttpRequest {
            method: "GET".into(),
            path: "/api/tasks/list".into(),
            query: query.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect::<HashMap<_, _>>(),
            headers: HashMap::new(),
            body: Vec::new(),
        });
        (resp.status, serde_json::from_slice(&resp.body).unwrap_or(Value::Null))
    }
}

fn row<'a>(body: &'a Value, id: &str) -> &'a Value {
    body["data"]["tasks"].as_array().unwrap().iter().find(|task| task["id"] == id).unwrap_or_else(|| panic!("no {id}: {body}"))
}

#[test]
fn list_rows_leave_out_heavy_fields_and_summarize_them() {
    let api = Api::new();
    let (status, body) = api.list(&[]);
    assert_eq!(status, 200, "{body}");
    let task = row(&body, "DEMO-1");
    for key in ["description", "comments", "history"] {
        assert!(task.get(key).is_none(), "{key} in a list row: {task}");
    }
    assert_eq!(task["comment_count"], 2);
    assert_eq!(task["history_count"], 1);
    assert_eq!(task["open_question"]["text"], "Ship it?");
    assert_eq!(task["status_since"], "2026-01-02T00:00:00Z");
    assert!(body["data"].get("positions").is_none(), "{body}");
    let plain = row(&body, "DEMO-2");
    assert_eq!(plain["comment_count"], 0);
    assert!(plain.get("open_question").is_none(), "{plain}");
    assert_eq!(plain["status_since"], "2026-01-02T00:00:00Z");
}

#[test]
fn include_history_keeps_history_and_rejects_anything_else() {
    let api = Api::new();
    let (status, body) = api.list(&[("include", "history")]);
    assert_eq!(status, 200, "{body}");
    let task = row(&body, "DEMO-1");
    assert_eq!(task["history"][0]["changes"][0]["new"], "Doing");
    assert!(task.get("comments").is_none(), "{task}");
    let (status, body) = api.list(&[("include", "comments")]);
    assert_eq!(status, 400, "{body}");
}

#[test]
fn ids_answers_those_tasks_with_their_positions_and_the_full_total() {
    let api = Api::new();
    let (_, all) = api.list(&[]);
    let order: Vec<_> = all["data"]["tasks"].as_array().unwrap().iter().map(|t| t["id"].as_str().unwrap().to_string()).collect();
    assert_eq!(order.len(), 4);

    let (status, body) = api.list(&[("ids", "demo-3, DEMO-1,NOPE-9")]);
    assert_eq!(status, 200, "{body}");
    let data = &body["data"];
    assert_eq!(data["total"], 4);
    let ids: Vec<_> = data["tasks"].as_array().unwrap().iter().map(|t| t["id"].as_str().unwrap()).collect();
    assert_eq!(ids.len(), 2, "{body}");
    for id in ["DEMO-1", "DEMO-3"] {
        assert!(ids.contains(&id), "{body}");
        let at = order.iter().position(|other| other == id).unwrap();
        assert_eq!(data["positions"][id], at, "{body}");
    }

    // Filters still apply: DEMO-1 is not Todo.
    let (_, body) = api.list(&[("status", "Todo"), ("ids", "DEMO-1,DEMO-2")]);
    let ids: Vec<_> = body["data"]["tasks"].as_array().unwrap().iter().map(|t| t["id"].as_str().unwrap()).collect();
    assert_eq!(ids, ["DEMO-2"], "{body}");
    assert_eq!(body["data"]["total"], 3);

    let many = (1..=101).map(|n| format!("DEMO-{n}")).collect::<Vec<_>>().join(",");
    let (status, body) = api.list(&[("ids", &many)]);
    assert_eq!(status, 400, "{body}");
}

//! DEV-275: export honours `format`, unknown parameters are 400s, and CSV
//! cells are quoted correctly and guarded against formulas.

use crate::common;
use common::env_mutex::EnvVarGuard;
use lotar::api_server::{ApiServer, HttpRequest};
use std::collections::HashMap;

fn get(api: &ApiServer, path: &str, query: &[(&str, &str)]) -> (u16, String) {
    let resp = api.handle_request(&HttpRequest {
        method: "GET".into(),
        path: path.into(),
        query: query.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect::<HashMap<_, _>>(),
        headers: HashMap::new(),
        body: Vec::new(),
    });
    (resp.status, String::from_utf8_lossy(&resp.body).to_string())
}

fn workspace(custom_fields: &str) -> (tempfile::TempDir, EnvVarGuard, ApiServer) {
    let tmp = tempfile::tempdir().unwrap();
    let tasks = tmp.path().join(".tasks");
    std::fs::create_dir_all(tasks.join("DEMO")).unwrap();
    std::fs::write(tasks.join("config.yml"), format!("default:\n  project: DEMO\ncustom:\n  fields: {custom_fields}\n")).unwrap();
    for (n, title) in [(1, r#"Say "hi" now"#), (2, r#"=HYPERLINK("http://x.example","click")"#), (3, "plain, with comma")] {
        std::fs::write(
            tasks.join("DEMO").join(format!("{n}.yml")),
            format!(
                "title: '{}'\nstatus: Todo\npriority: Low\ntype: Feature\ncreated: 2026-01-01T00:00:00Z\nmodified: 2026-01-01T00:00:00Z\n",
                title.replace('\'', "''")
            ),
        )
        .unwrap();
    }
    let guard = EnvVarGuard::set("LOTAR_TASKS_DIR", &tasks.to_string_lossy());
    let mut api = ApiServer::new();
    lotar::routes::initialize(&mut api);
    (tmp, guard, api)
}

#[test]
fn export_honours_format_and_quotes_cells() {
    let (_tmp, _guard, api) = workspace("['*']");
    let (status, csv) = get(&api, "/api/tasks/export", &[("format", "csv")]);
    assert_eq!(status, 200, "{csv}");
    let lines: Vec<&str> = csv.lines().collect();
    assert_eq!(lines.len(), 4, "header + 3 rows: {csv}");
    assert!(csv.contains(r#"DEMO-1,"Say ""hi"" now",Todo"#), "{csv}");
    assert!(csv.contains(r#"DEMO-2,"'=HYPERLINK(""http://x.example"",""click"")",Todo"#), "{csv}");
    assert!(csv.contains(r#"DEMO-3,"plain, with comma",Todo"#), "{csv}");

    let (status, body) = get(&api, "/api/tasks/export", &[("format", "xml")]);
    assert_eq!(status, 400, "{body}");
}

#[test]
fn unknown_parameters_are_rejected_not_ignored() {
    let (_tmp, _guard, api) = workspace("[team]");
    for (key, value) in [("page", "1"), ("sort", "title"), ("bogus", "1")] {
        let (status, body) = get(&api, "/api/tasks/list", &[(key, value)]);
        assert_eq!(status, 400, "{key}: {body}");
        assert!(body.contains(key), "{key}: {body}");
    }
    let (status, body) = get(&api, "/api/tasks/list", &[("team", "core")]);
    assert_eq!(status, 200, "declared custom fields still filter: {body}");
}

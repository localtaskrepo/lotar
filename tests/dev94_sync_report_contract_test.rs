use lotar::api_server::{ApiServer, HttpRequest};
use lotar::api_types::SyncReportListResponse;
use std::collections::HashMap;
mod common;
use common::env_mutex::EnvVarGuard;

#[test]
fn empty_report_list_serializes_required_array() {
    let page = SyncReportListResponse {
        total: 0,
        limit: 20,
        offset: 0,
        reports: vec![],
    };
    let value = serde_json::to_value(page).unwrap();
    assert_eq!(value["reports"], serde_json::json!([]));
    assert!(
        serde_json::from_value::<SyncReportListResponse>(serde_json::json!({
            "total": 0, "limit": 20, "offset": 0
        }))
        .is_err()
    );
}

#[test]
fn empty_report_route_includes_array_in_global_and_project_scopes() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path().join(".tasks");
    std::fs::create_dir_all(root.join("TP")).unwrap();
    std::fs::write(root.join("config.yml"), "{}\n").unwrap();
    std::fs::write(root.join("TP/config.yml"), "project_name: Test\n").unwrap();
    let _scope = EnvVarGuard::set("LOTAR_TASKS_DIR", &root.to_string_lossy());
    let _home = EnvVarGuard::set("LOTAR_IGNORE_HOME_CONFIG", "1");
    let mut server = ApiServer::new();
    lotar::routes::initialize(&mut server);
    for project in [None, Some("TP")] {
        let mut query = HashMap::new();
        if let Some(project) = project {
            query.insert("project".to_string(), project.to_string());
        }
        let response = server.handle_request(&HttpRequest {
            method: "GET".to_string(),
            path: "/api/sync/reports/list".to_string(),
            query,
            headers: HashMap::new(),
            body: vec![],
        });
        assert_eq!(response.status, 200);
        let value: serde_json::Value = serde_json::from_slice(&response.body).unwrap();
        assert_eq!(value["data"]["reports"], serde_json::json!([]));
        assert_eq!(value["data"]["total"], 0);
    }
}

#[test]
fn openapi_report_list_requires_array() {
    let schema: serde_json::Value =
        serde_json::from_str(include_str!("../docs/openapi.json")).unwrap();
    let reports = &schema["components"]["schemas"]["SyncReportListResponse"];
    assert_eq!(reports["properties"]["reports"]["type"], "array");
    assert!(
        reports["required"]
            .as_array()
            .unwrap()
            .contains(&serde_json::json!("reports"))
    );
}

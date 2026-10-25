use super::*;

pub(super) fn register(api_server: &mut ApiServer) {
    api_server.register_handler("GET", "/api/config/show", |req: &HttpRequest| {
        let resolver = try_response!(resolve_tasks_dir());
        let scope = req.query.get("project").map(|s| s.as_str());
        match ConfigService::show(&resolver, scope) {
            Ok(val) => ok_json(200, json!({"data": val})),
            Err(e) => internal_error(e.to_string()),
        }
    });

    // GET /api/config/inspect

    api_server.register_handler("GET", "/api/config/inspect", |req: &HttpRequest| {
        let resolver = try_response!(resolve_tasks_dir());
        let scope = req.query.get("project").map(|s| s.as_str());
        match ConfigService::inspect(&resolver, scope) {
            Ok(val) => ok_json(200, json!({"data": val})),
            Err(e) => internal_error(e.to_string()),
        }
    });

    // GET /api/automation/show

    api_server.register_handler("POST", "/api/config/set", |req: &HttpRequest| {
        let resolver = try_response!(resolve_tasks_dir());
        let body: serde_json::Value = serde_json::from_slice(&req.body).unwrap_or(json!({}));
        let values = body
            .get("values")
            .and_then(|v| v.as_object())
            .cloned()
            .unwrap_or_default();
        let mut map = std::collections::BTreeMap::new();
        for (k, v) in values.iter() {
            map.insert(k.clone(), v.as_str().unwrap_or(&v.to_string()).to_string());
        }
        let global = body
            .get("global")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        let project = body.get("project").and_then(|v| v.as_str());
        match ConfigService::set(&resolver, &map, global, project) {
            Ok(outcome) => config_write_response(&resolver, outcome.updated, &outcome.validation),
            Err(e) => bad_request(e.to_string()),
        }
    });

    // POST /api/scan/run
}

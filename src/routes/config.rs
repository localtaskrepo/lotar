use super::*;

pub(super) fn register(api_server: &mut ApiServer) {
    api_server.register_handler("GET", "/api/config/show", |req: &HttpRequest| {
        let resolver = match TasksDirectoryResolver::resolve(None, None) {
            Ok(r) => r,
            Err(e) => return internal(json!({"error": {"code": "INTERNAL", "message": e}})),
        };
        let scope = req.query.get("project").map(|s| s.as_str());
        match ConfigService::show(&resolver, scope) {
            Ok(val) => ok_json(200, json!({"data": val})),
            Err(e) => internal(json!({"error": {"code":"INTERNAL", "message": e.to_string()}})),
        }
    });

    // GET /api/config/inspect

    api_server.register_handler("GET", "/api/config/inspect", |req: &HttpRequest| {
        let resolver = match TasksDirectoryResolver::resolve(None, None) {
            Ok(r) => r,
            Err(e) => return internal(json!({"error": {"code": "INTERNAL", "message": e}})),
        };
        let scope = req.query.get("project").map(|s| s.as_str());
        match ConfigService::inspect(&resolver, scope) {
            Ok(val) => ok_json(200, json!({"data": val})),
            Err(e) => internal(json!({"error": {"code":"INTERNAL", "message": e.to_string()}})),
        }
    });

    // GET /api/automation/show

    api_server.register_handler("POST", "/api/config/set", |req: &HttpRequest| {
        let resolver = match TasksDirectoryResolver::resolve(None, None) {
            Ok(r) => r,
            Err(e) => return internal(json!({"error": {"code": "INTERNAL", "message": e}})),
        };
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
            Ok(outcome) => {
                let actor =
                    crate::utils::identity::resolve_current_user(Some(resolver.path.as_path()));
                crate::api_events::emit_config_updated(actor.as_deref());

                let warnings: Vec<String> = outcome
                    .validation
                    .warnings
                    .iter()
                    .map(|w| w.to_string())
                    .collect();
                let info: Vec<String> = outcome
                    .validation
                    .info
                    .iter()
                    .map(|i| i.to_string())
                    .collect();
                let errors: Vec<String> = outcome
                    .validation
                    .errors
                    .iter()
                    .map(|err| err.to_string())
                    .collect();

                ok_json(
                    200,
                    json!({
                        "data": {
                            "updated": outcome.updated,
                            "warnings": warnings,
                            "info": info,
                            "errors": errors,
                        }
                    }),
                )
            }
            Err(e) => bad_request(e.to_string()),
        }
    });

    // POST /api/scan/run
}

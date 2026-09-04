use super::*;

pub(super) fn register(api_server: &mut ApiServer) {
    api_server.register_handler("GET", "/api/automation/show", |req: &HttpRequest| {
        let resolver = match TasksDirectoryResolver::resolve(None, None) {
            Ok(r) => r,
            Err(e) => return internal(json!({"error": {"code": "INTERNAL", "message": e}})),
        };
        let project = req.query.get("project").map(|s| s.as_str());
        match AutomationService::inspect(resolver.path.as_path(), project) {
            Ok(val) => ok_json(
                200,
                json!({
                    "data": {
                        "scope": val.scope.as_str(),
                        "source": val.source.as_str(),
                        "scope_exists": val.scope_exists,
                        "scope_yaml": val.scope_yaml,
                        "effective_yaml": val.effective_yaml,
                    }
                }),
            ),
            Err(e) => internal(json!({"error": {"code":"INTERNAL", "message": e.to_string()}})),
        }
    });

    // POST /api/automation/set

    api_server.register_handler("POST", "/api/automation/set", |req: &HttpRequest| {
        let resolver = match TasksDirectoryResolver::resolve(None, None) {
            Ok(r) => r,
            Err(e) => return internal(json!({"error": {"code": "INTERNAL", "message": e}})),
        };
        let body: serde_json::Value = serde_json::from_slice(&req.body).unwrap_or(json!({}));
        let yaml = match body.get("yaml").and_then(|v| v.as_str()) {
            Some(value) => value,
            None => return bad_request("Missing yaml payload".to_string()),
        };
        let project = body.get("project").and_then(|v| v.as_str());
        match AutomationService::set(resolver.path.as_path(), project, yaml) {
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

    // POST /api/automation/simulate - Preview what automation rules would do

    api_server.register_handler("POST", "/api/automation/simulate", |req: &HttpRequest| {
        let resolver = match TasksDirectoryResolver::resolve(None, None) {
            Ok(r) => r,
            Err(e) => return internal(json!({"error": {"code": "INTERNAL", "message": e}})),
        };
        let body: crate::api_types::AutomationSimulateRequest =
            match serde_json::from_slice(&req.body) {
                Ok(b) => b,
                Err(e) => return bad_request(format!("Invalid request: {}", e)),
            };

        let ticket_id = body.ticket_id.trim();
        if ticket_id.is_empty() {
            return bad_request("Missing ticket_id".to_string());
        }

        // Parse the event
        let event: AutomationEvent = match body.event.parse() {
            Ok(e) => e,
            Err(msg) => return bad_request(msg),
        };

        // Load the task
        let storage = crate::storage::manager::Storage::new(&resolver.path.clone());
        let task_before =
            match crate::services::task_service::TaskService::get(&storage, ticket_id, None) {
                Ok(t) => t,
                Err(e) => return bad_request(format!("Task not found: {}", e)),
            };

        // Simulate automation
        match AutomationService::simulate(resolver.path.as_path(), ticket_id, event) {
            Ok(result) => {
                let actions: Vec<crate::api_types::AutomationSimulatedAction> = result
                    .actions
                    .iter()
                    .map(|a| crate::api_types::AutomationSimulatedAction {
                        action: a.action.clone(),
                        description: a.description.clone(),
                    })
                    .collect();

                ok_json(
                    200,
                    json!({
                        "data": {
                            "matched": result.matched,
                            "rule_name": result.rule_name,
                            "actions": actions,
                            "task_before": task_before,
                            "task_after": result.task_after,
                        }
                    }),
                )
            }
            Err(e) => bad_request(e.to_string()),
        }
    });

    // GET /api/agents/profiles - List available agent profiles
}

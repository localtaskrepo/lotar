use super::*;

pub(super) fn register(api_server: &mut ApiServer) {
    api_server.register_handler("GET", "/api/automation/show", |req: &HttpRequest| {
        let resolver = try_response!(resolve_tasks_dir());
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
            Err(e) => internal_error(e.to_string()),
        }
    });

    // POST /api/automation/set

    api_server.register_handler("POST", "/api/automation/set", |req: &HttpRequest| {
        let resolver = try_response!(resolve_tasks_dir());
        let body: serde_json::Value = serde_json::from_slice(&req.body).unwrap_or(json!({}));
        let yaml = match body.get("yaml").and_then(|v| v.as_str()) {
            Some(value) => value,
            None => return bad_request("Missing yaml payload".to_string()),
        };
        let project = body.get("project").and_then(|v| v.as_str());
        match AutomationService::set(resolver.path.as_path(), project, yaml) {
            Ok(outcome) => config_write_response(&resolver, outcome.updated, &outcome.validation),
            Err(e) => bad_request(e.to_string()),
        }
    });

    // POST /api/automation/simulate - Preview what automation rules would do

    api_server.register_handler("POST", "/api/automation/simulate", |req: &HttpRequest| {
        let resolver = try_response!(resolve_tasks_dir());
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
        let storage = crate::storage::manager::Storage::new(&resolver.path);
        let task_before =
            match crate::services::task_service::TaskService::get(&storage, ticket_id, None) {
                Ok(t) => t,
                Err(e) => return bad_request(format!("Task not found: {}", e)),
            };

        // Simulate automation: the saved rules, or the rule being edited (DEV-241)
        let outcome = match body.rule.as_ref() {
            Some(value) => {
                match serde_json::from_value::<crate::automation::types::AutomationRule>(
                    value.clone(),
                ) {
                    Ok(rule) => AutomationService::simulate_rule(
                        resolver.path.as_path(),
                        ticket_id,
                        event,
                        &rule,
                    ),
                    Err(e) => return bad_request(format!("Invalid rule: {}", e)),
                }
            }
            None => AutomationService::simulate(resolver.path.as_path(), ticket_id, event),
        };
        match outcome {
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
                            "warnings": result.warnings,
                        }
                    }),
                )
            }
            Err(e) => bad_request(e.to_string()),
        }
    });

    // POST /api/automation/run - run manual (on.manual) actions (DEV-241)
    api_server.register_handler("POST", "/api/automation/run", |req: &HttpRequest| {
        let resolver = try_response!(resolve_tasks_dir());
        let body: crate::api_types::AutomationRunRequest = if req.body.is_empty() {
            Default::default()
        } else {
            match serde_json::from_slice(&req.body) {
                Ok(b) => b,
                Err(e) => return bad_request(format!("Invalid request: {}", e)),
            }
        };
        let mut storage = crate::storage::manager::Storage::new(&resolver.path);
        match AutomationService::run_manual(
            &mut storage,
            &body.rules,
            body.query.as_deref(),
            body.dry_run,
        ) {
            Ok(report) => {
                let entries = report
                    .entries
                    .into_iter()
                    .map(|entry| crate::api_types::AutomationRunEntry {
                        rule: entry.rule,
                        task: entry.task,
                        actions: entry
                            .actions
                            .into_iter()
                            .map(|a| crate::api_types::AutomationSimulatedAction {
                                action: a.action,
                                description: a.description,
                            })
                            .collect(),
                        warnings: entry.warnings,
                        error: entry.error,
                    })
                    .collect();
                ok_json(
                    200,
                    json!({"data": crate::api_types::AutomationRunResponse {
                        dry_run: report.dry_run,
                        rules: report.rules,
                        checked: report.checked,
                        entries,
                    }}),
                )
            }
            Err(e) => bad_request(e.to_string()),
        }
    });

    // GET /api/agents/profiles - List available agent profiles
}

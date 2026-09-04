use super::*;

pub(super) fn register(api_server: &mut ApiServer) {
    api_server.register_handler("POST", "/api/sync/pull", |req: &HttpRequest| {
        let resolver = match TasksDirectoryResolver::resolve(None, None) {
            Ok(r) => r,
            Err(e) => return internal(json!({"error": {"code": "INTERNAL", "message": e}})),
        };

        let body: serde_json::Value = serde_json::from_slice(&req.body).unwrap_or(json!({}));
        let payload: SyncRequest = match serde_json::from_value(body) {
            Ok(v) => v,
            Err(e) => return bad_request(format!("Invalid body: {}", e)),
        };

        match SyncService::pull(
            &resolver,
            &payload.remote,
            payload.project.as_deref(),
            payload.dry_run,
            payload.auth_profile.as_deref(),
            payload.task_id.as_deref(),
            payload.write_report,
            payload.include_report.unwrap_or(false),
            payload.client_run_id.as_deref(),
        ) {
            Ok(result) => ok_json(200, json!({"data": result})),
            Err(err) => match err {
                LoTaRError::ValidationError(_) => bad_request(err.to_string()),
                _ => internal(json!({"error": {"code": "INTERNAL", "message": err.to_string()}})),
            },
        }
    });

    // POST /api/sync/push

    api_server.register_handler("POST", "/api/sync/push", |req: &HttpRequest| {
        let resolver = match TasksDirectoryResolver::resolve(None, None) {
            Ok(r) => r,
            Err(e) => return internal(json!({"error": {"code": "INTERNAL", "message": e}})),
        };

        let body: serde_json::Value = serde_json::from_slice(&req.body).unwrap_or(json!({}));
        let payload: SyncRequest = match serde_json::from_value(body) {
            Ok(v) => v,
            Err(e) => return bad_request(format!("Invalid body: {}", e)),
        };

        match SyncService::push(
            &resolver,
            &payload.remote,
            payload.project.as_deref(),
            payload.dry_run,
            payload.auth_profile.as_deref(),
            payload.task_id.as_deref(),
            payload.write_report,
            payload.include_report.unwrap_or(false),
            payload.client_run_id.as_deref(),
        ) {
            Ok(result) => ok_json(200, json!({"data": result})),
            Err(err) => match err {
                LoTaRError::ValidationError(_) => bad_request(err.to_string()),
                _ => internal(json!({"error": {"code": "INTERNAL", "message": err.to_string()}})),
            },
        }
    });

    // POST /api/sync/validate

    api_server.register_handler("POST", "/api/sync/validate", |req: &HttpRequest| {
        let resolver = match TasksDirectoryResolver::resolve(None, None) {
            Ok(r) => r,
            Err(e) => return internal(json!({"error": {"code": "INTERNAL", "message": e}})),
        };

        let body: serde_json::Value = serde_json::from_slice(&req.body).unwrap_or(json!({}));
        let payload: SyncValidateRequest = match serde_json::from_value(body) {
            Ok(v) => v,
            Err(e) => return bad_request(format!("Invalid body: {}", e)),
        };

        match SyncService::validate(
            &resolver,
            payload.project.as_deref(),
            payload.remote.as_deref(),
            payload.remote_config,
            payload.auth_profile.as_deref(),
        ) {
            Ok(result) => ok_json(200, json!({"data": result})),
            Err(err) => match err {
                LoTaRError::ValidationError(_) => bad_request(err.to_string()),
                _ => internal(json!({"error": {"code": "INTERNAL", "message": err.to_string()}})),
            },
        }
    });

    // GET /api/sync/reports/list?project=PREFIX&limit=N&offset=N

    api_server.register_handler("GET", "/api/sync/reports/list", |req: &HttpRequest| {
        let resolver = match TasksDirectoryResolver::resolve(None, None) {
            Ok(r) => r,
            Err(e) => return internal(json!({"error": {"code": "INTERNAL", "message": e}})),
        };
        let mgr = match ConfigManager::new_manager_with_tasks_dir_readonly(&resolver.path) {
            Ok(mgr) => mgr,
            Err(err) => return bad_request(format!("Failed to load config: {}", err)),
        };
        let project = req
            .query
            .get("project")
            .cloned()
            .filter(|p| !p.trim().is_empty());
        let resolved = if let Some(prefix) = project.as_deref() {
            match mgr.get_project_config(prefix) {
                Ok(cfg) => cfg,
                Err(err) => {
                    return bad_request(format!(
                        "Failed to load project config '{}': {}",
                        prefix, err
                    ));
                }
            }
        } else {
            mgr.get_resolved_config().clone()
        };

        let limit: usize = req
            .query
            .get("limit")
            .and_then(|s| s.parse::<usize>().ok())
            .unwrap_or(20)
            .clamp(1, 200);
        let offset: usize = req
            .query
            .get("offset")
            .and_then(|s| s.parse::<usize>().ok())
            .unwrap_or(0);

        let filter = crate::services::sync_report_service::SyncReportListFilter {
            project,
            limit,
            offset,
        };
        match crate::services::sync_report_service::SyncReportService::list_reports(
            &resolver.path,
            &resolved,
            filter,
        ) {
            Ok(payload) => ok_json(200, json!({"data": payload})),
            Err(err) => bad_request(err.to_string()),
        }
    });

    // GET /api/sync/reports/get?path=<relative>[&project=PREFIX]

    api_server.register_handler("GET", "/api/sync/reports/get", |req: &HttpRequest| {
        let resolver = match TasksDirectoryResolver::resolve(None, None) {
            Ok(r) => r,
            Err(e) => return internal(json!({"error": {"code": "INTERNAL", "message": e}})),
        };
        let rel = match req.query.get("path") {
            Some(v) if !v.trim().is_empty() => v.trim().to_string(),
            _ => return bad_request("Missing report path".into()),
        };
        let mgr = match ConfigManager::new_manager_with_tasks_dir_readonly(&resolver.path) {
            Ok(mgr) => mgr,
            Err(err) => return bad_request(format!("Failed to load config: {}", err)),
        };
        let project = req
            .query
            .get("project")
            .cloned()
            .filter(|p| !p.trim().is_empty());
        let resolved = if let Some(prefix) = project.as_deref() {
            match mgr.get_project_config(prefix) {
                Ok(cfg) => cfg,
                Err(err) => {
                    return bad_request(format!(
                        "Failed to load project config '{}': {}",
                        prefix, err
                    ));
                }
            }
        } else {
            mgr.get_resolved_config().clone()
        };

        match crate::services::sync_report_service::SyncReportService::read_report(
            &resolver.path,
            &resolved,
            &rel,
        ) {
            Ok(report) => ok_json(200, json!({"data": report})),
            Err(err) => bad_request(err.to_string()),
        }
    });

    // POST /api/projects/create
}

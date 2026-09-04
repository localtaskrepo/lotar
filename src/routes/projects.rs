use super::*;

pub(super) fn register(api_server: &mut ApiServer) {
    api_server.register_handler("POST", "/api/projects/create", |req: &HttpRequest| {
        let resolver = match TasksDirectoryResolver::resolve(None, None) {
            Ok(r) => r,
            Err(e) => return internal(json!({"error": {"code": "INTERNAL", "message": e}})),
        };

        let body: serde_json::Value = serde_json::from_slice(&req.body).unwrap_or(json!({}));
        let name = body
            .get("name")
            .and_then(|v| v.as_str())
            .map(|s| s.trim().to_string())
            .unwrap_or_default();
        if name.is_empty() {
            return bad_request("Project name is required".into());
        }

        let prefix = body
            .get("prefix")
            .and_then(|v| v.as_str())
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty());

        let values_map = body.get("values").and_then(|v| v.as_object()).map(|obj| {
            let mut map = std::collections::BTreeMap::new();
            for (k, v) in obj {
                let value = v
                    .as_str()
                    .map(|s| s.to_string())
                    .unwrap_or_else(|| v.to_string());
                map.insert(k.clone(), value);
            }
            map
        });

        match ConfigService::create_project(
            &resolver,
            &name,
            prefix.as_deref(),
            values_map.as_ref(),
        ) {
            Ok(project) => {
                let actor =
                    crate::utils::identity::resolve_current_user(Some(resolver.path.as_path()));
                crate::api_events::emit_config_updated(actor.as_deref());
                ok_json(201, json!({"data": project}))
            }
            Err(e) => bad_request(e.to_string()),
        }
    });

    // GET /api/projects/list

    api_server.register_handler("GET", "/api/projects/list", |req: &HttpRequest| {
        let resolver = match TasksDirectoryResolver::resolve(None, None) {
            Ok(r) => r,
            Err(e) => return internal(json!({"error": {"code": "INTERNAL", "message": e}})),
        };

        let page = match crate::utils::pagination::parse_page(&req.query, 200, 500) {
            Ok(v) => v,
            Err(msg) => return bad_request(msg),
        };
        let storage = crate::storage::manager::Storage::new(&resolver.path);
        let mut projects = ProjectService::list(&storage);
        projects.sort_by(|a, b| a.prefix.cmp(&b.prefix));
        let total = projects.len();
        let (start, end) = crate::utils::pagination::slice_bounds(total, page.offset, page.limit);
        let page_projects = projects[start..end].to_vec();

        let payload = crate::api_types::ProjectListResponse {
            total,
            limit: page.limit,
            offset: page.offset,
            projects: page_projects,
        };

        ok_json(200, json!({"data": payload}))
    });

    // GET /api/projects/stats?project=PREFIX

    api_server.register_handler("GET", "/api/projects/stats", |req: &HttpRequest| {
        let name = match req.query.get("project") {
            Some(v) if !v.is_empty() => v.clone(),
            _ => return bad_request("Missing project".into()),
        };
        let resolver = match TasksDirectoryResolver::resolve(None, None) {
            Ok(r) => r,
            Err(e) => return internal(json!({"error": {"code": "INTERNAL", "message": e}})),
        };
        let storage = crate::storage::manager::Storage::new(&resolver.path);
        let stats = ProjectService::stats(&storage, &name);
        ok_json(200, json!({"data": stats}))
    });

    // GET /api/tasks/history?id=ID[&limit=N]
}

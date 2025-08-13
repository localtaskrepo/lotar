use crate::api_server::{ApiServer, HttpRequest, HttpResponse};
use crate::errors::LoTaRError;
use crate::services::{
    config_service::ConfigService, project_service::ProjectService, task_service::TaskService,
};
use crate::workspace::TasksDirectoryResolver;
use serde_json::json;

pub fn initialize(api_server: &mut ApiServer) {
    // POST /api/tasks/add
    api_server.register_handler("POST", "/api/tasks/add", |req: &HttpRequest| {
        // Resolve tasks root via resolver
        let resolver = match TasksDirectoryResolver::resolve(None, None) {
            Ok(r) => r,
            Err(e) => return internal(json!({"error": {"code": "INTERNAL", "message": e}})),
        };
        let mut storage = crate::storage::manager::Storage::new(resolver.path.clone());
        let body: serde_json::Value = serde_json::from_slice(&req.body).unwrap_or(json!({}));
        // Map JSON to AddArgs, then reuse AddHandler flow by building Task via services
        let add: crate::cli::TaskAddArgs = match serde_json::from_value(body.clone()) {
            Ok(v) => v,
            Err(e) => return bad_request(format!("Invalid body: {}", e)),
        };
        // Load config for validation/mapping
        let cfg_mgr = match crate::config::manager::ConfigManager::new_manager_with_tasks_dir_readonly(&resolver.path) {
            Ok(m) => m,
            Err(e) => return internal(json!({"error": {"code": "INTERNAL", "message": format!("Failed to load config: {}", e)}})),
        };
    let cfg = cfg_mgr.get_resolved_config();
        // Convert to TaskCreate DTO
        let req_create = crate::api_types::TaskCreate {
            title: add.title,
            project: req.query.get("project").cloned(),
            priority: match add.priority {
                Some(ref p) => match crate::types::Priority::parse_with_config(p, cfg) {
                    Ok(v) => Some(v),
                    Err(e) => return bad_request(e),
                },
                None => None,
            },
            task_type: match add.task_type {
                Some(ref t) => match crate::types::TaskType::parse_with_config(t, cfg) {
                    Ok(v) => Some(v),
                    Err(e) => return bad_request(e),
                },
                None => None,
            },
            reporter: None,
            assignee: add.assignee,
            due_date: add.due,
            effort: add.effort,
            description: add.description,
            category: add.category,
            tags: add.tags,
            custom_fields: if add.fields.is_empty() { None } else {
                let mut m = std::collections::HashMap::new();
                for (k, v) in add.fields.into_iter() {
                    m.insert(k, crate::types::custom_value_string(v));
                }
                Some(m)
            },
        };
        match TaskService::create(&mut storage, req_create) {
            Ok(task) => {
                let actor = crate::utils::identity::resolve_current_user(Some(resolver.path.as_path()));
                crate::api_events::emit_task_created(&task, actor.as_deref());
                ok_json(201, json!({"data": task}))
            },
            Err(e) => internal(json!({"error": {"code":"INTERNAL", "message": e.to_string()}})),
        }
    });

    // GET /api/tasks/list
    api_server.register_handler("GET", "/api/tasks/list", |req: &HttpRequest| {
        let resolver = match TasksDirectoryResolver::resolve(None, None) {
            Ok(r) => r,
            Err(e) => return internal(json!({"error": {"code": "INTERNAL", "message": e}})),
        };
        let storage = crate::storage::manager::Storage::new(resolver.path.clone());
        // Load config for validation of status/priority/type
        let cfg_mgr = match crate::config::manager::ConfigManager::new_manager_with_tasks_dir_readonly(&resolver.path) {
            Ok(m) => m,
            Err(e) => return internal(json!({"error": {"code": "INTERNAL", "message": format!("Failed to load config: {}", e)}})),
        };
        let cfg = cfg_mgr.get_resolved_config();

        // Helpers to parse comma-separated values and validate
    let parse_list = |key: &str| -> Vec<String> {
            req.query
                .get(key)
                .map(|s| s.split(',').map(|p| p.trim().to_string()).filter(|p| !p.is_empty()).collect())
                .unwrap_or_default()
        };
        let mut statuses = Vec::new();
        for s in parse_list("status") {
            match crate::types::TaskStatus::parse_with_config(&s, cfg) {
                Ok(v) => statuses.push(v),
                Err(msg) => return bad_request(msg),
            }
        }
        let mut priorities = Vec::new();
        for s in parse_list("priority") {
            match crate::types::Priority::parse_with_config(&s, cfg) {
                Ok(v) => priorities.push(v),
                Err(msg) => return bad_request(msg),
            }
        }
        let mut types_vec = Vec::new();
        for s in parse_list("type") {
            match crate::types::TaskType::parse_with_config(&s, cfg) {
                Ok(v) => types_vec.push(v),
                Err(msg) => return bad_request(msg),
            }
        }

        // Build filter from query
        let filter = crate::api_types::TaskListFilter {
            status: statuses,
            priority: priorities,
            task_type: types_vec,
            project: req.query.get("project").cloned(),
            category: req.query.get("category").cloned(),
            tags: req
                .query
                .get("tags")
                .map(|s| s.split(',').map(|s| s.trim().to_string()).collect())
                .unwrap_or_default(),
            text_query: req.query.get("q").cloned(),
        };
        let tasks = TaskService::list(&storage, &filter);
        ok_json(200, json!({"data": tasks.iter().map(|(_, t)| t).collect::<Vec<_>>(), "meta": {"count": tasks.len()}}))
    });

    // GET /api/tasks/get?id=ID[&project=PREFIX]
    api_server.register_handler("GET", "/api/tasks/get", |req: &HttpRequest| {
        let id = match req.query.get("id") {
            Some(v) if !v.is_empty() => v.clone(),
            _ => return bad_request("Missing id".into()),
        };
        let resolver = match TasksDirectoryResolver::resolve(None, None) {
            Ok(r) => r,
            Err(e) => return internal(json!({"error": {"code": "INTERNAL", "message": e}})),
        };
        let storage = crate::storage::manager::Storage::new(resolver.path);
        match TaskService::get(&storage, &id, req.query.get("project").map(|s| s.as_str())) {
            Ok(task) => ok_json(200, json!({"data": task})),
            Err(e) => match e {
                LoTaRError::TaskNotFound(_) => not_found(e.to_string()),
                _ => bad_request(e.to_string()),
            },
        }
    });

    // POST /api/tasks/update
    api_server.register_handler("POST", "/api/tasks/update", |req: &HttpRequest| {
        let resolver = match TasksDirectoryResolver::resolve(None, None) {
            Ok(r) => r,
            Err(e) => return internal(json!({"error": {"code": "INTERNAL", "message": e}})),
        };
        let mut storage = crate::storage::manager::Storage::new(resolver.path.clone());
        let body: serde_json::Value = serde_json::from_slice(&req.body).unwrap_or(json!({}));
        let edit: crate::cli::TaskEditArgs = match serde_json::from_value(body.clone()) {
            Ok(v) => v,
            Err(e) => return bad_request(format!("Invalid body: {}", e)),
        };
        // Config for validation
        let cfg_mgr = match crate::config::manager::ConfigManager::new_manager_with_tasks_dir_readonly(&resolver.path) {
            Ok(m) => m,
            Err(e) => return internal(json!({"error": {"code": "INTERNAL", "message": format!("Failed to load config: {}", e)}})),
        };
    let cfg = cfg_mgr.get_resolved_config();
        // Build patch
        let patch = crate::api_types::TaskUpdate {
            title: edit.title,
            status: None, // status change uses TaskStatusArgs route; keep None here
            priority: match edit.priority {
                Some(ref p) => match crate::types::Priority::parse_with_config(p, cfg) { Ok(v) => Some(v), Err(e) => return bad_request(e) },
                None => None,
            },
            task_type: match edit.task_type {
                Some(ref t) => match crate::types::TaskType::parse_with_config(t, cfg) { Ok(v) => Some(v), Err(e) => return bad_request(e) },
                None => None,
            },
            reporter: None,
            assignee: edit.assignee,
            due_date: edit.due,
            effort: edit.effort,
            description: edit.description,
            category: edit.category,
            tags: if edit.tags.is_empty() { None } else { Some(edit.tags) },
            custom_fields: if edit.fields.is_empty() { None } else {
                let mut m = std::collections::HashMap::new();
                for (k, v) in edit.fields.into_iter() { m.insert(k, crate::types::custom_value_string(v)); }
                Some(m)
            },
        };
        match TaskService::update(&mut storage, &edit.id, patch) {
            Ok(task) => {
                let actor = crate::utils::identity::resolve_current_user(Some(resolver.path.as_path()));
                crate::api_events::emit_task_updated(&task, actor.as_deref());
                ok_json(200, json!({"data": task}))
            },
            Err(e) => bad_request(e.to_string()),
        }
    });

    // POST /api/tasks/delete
    api_server.register_handler("POST", "/api/tasks/delete", |req: &HttpRequest| {
        let resolver = match TasksDirectoryResolver::resolve(None, None) {
            Ok(r) => r,
            Err(e) => return internal(json!({"error": {"code": "INTERNAL", "message": e}})),
        };
        let mut storage = crate::storage::manager::Storage::new(resolver.path);
        let body: serde_json::Value = serde_json::from_slice(&req.body).unwrap_or(json!({}));
        let del: crate::cli::TaskDeleteArgs = match serde_json::from_value(body.clone()) {
            Ok(v) => v,
            Err(e) => return bad_request(format!("Invalid body: {}", e)),
        };
        let deleted = TaskService::delete(
            &mut storage,
            &del.id,
            req.query.get("project").map(|s| s.as_str()),
        )
        .unwrap_or(false);
        if deleted {
            let actor = crate::utils::identity::resolve_current_user(None);
            crate::api_events::emit_task_deleted(&del.id, actor.as_deref());
        }
        ok_json(200, json!({"data": {"deleted": deleted}}))
    });

    // GET /api/config/show
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

    // POST /api/config/set
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
            Ok(_) => {
                let actor =
                    crate::utils::identity::resolve_current_user(Some(resolver.path.as_path()));
                crate::api_events::emit_config_updated(actor.as_deref());
                ok_json(200, json!({"data": {"updated": true}}))
            }
            Err(e) => bad_request(e.to_string()),
        }
    });

    // GET /api/projects/list
    api_server.register_handler("GET", "/api/projects/list", |_req: &HttpRequest| {
        let resolver = match TasksDirectoryResolver::resolve(None, None) {
            Ok(r) => r,
            Err(e) => return internal(json!({"error": {"code": "INTERNAL", "message": e}})),
        };
        let storage = crate::storage::manager::Storage::new(resolver.path);
        let projects = ProjectService::list(&storage);
        ok_json(200, json!({"data": projects}))
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
        let storage = crate::storage::manager::Storage::new(resolver.path);
        let stats = ProjectService::stats(&storage, &name);
        ok_json(200, json!({"data": stats}))
    });
}

fn ok_json(status: u16, v: serde_json::Value) -> HttpResponse {
    let body = serde_json::to_vec(&v).unwrap();
    HttpResponse {
        status,
        headers: vec![("Content-Type".into(), "application/json".into())],
        body,
    }
}

fn bad_request(msg: String) -> HttpResponse {
    let body = serde_json::to_vec(&json!({"error": {"code": "INVALID_ARGUMENT", "message": msg}}))
        .unwrap();
    HttpResponse {
        status: 400,
        headers: vec![("Content-Type".into(), "application/json".into())],
        body,
    }
}

fn internal(v: serde_json::Value) -> HttpResponse {
    let body = serde_json::to_vec(&v).unwrap();
    HttpResponse {
        status: 500,
        headers: vec![("Content-Type".into(), "application/json".into())],
        body,
    }
}

fn not_found(msg: String) -> HttpResponse {
    let body =
        serde_json::to_vec(&json!({"error": {"code": "NOT_FOUND", "message": msg}})).unwrap();
    HttpResponse {
        status: 404,
        headers: vec![("Content-Type".into(), "application/json".into())],
        body,
    }
}

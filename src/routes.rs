use crate::LoTaRError;
use crate::api_server::{ApiServer, HttpRequest, HttpResponse};
use crate::services::{
    config_service::ConfigService, project_service::ProjectService,
    reference_service::ReferenceService, task_service::TaskService,
};
use crate::workspace::TasksDirectoryResolver;
use serde_json::json;

fn task_to_dto(id: &str, task: &crate::storage::task::Task) -> crate::api_types::TaskDTO {
    crate::api_types::TaskDTO {
        id: id.to_string(),
        title: task.title.clone(),
        status: task.status.clone(),
        priority: task.priority.clone(),
        task_type: task.task_type.clone(),
        reporter: task.reporter.clone(),
        assignee: task.assignee.clone(),
        created: task.created.clone(),
        modified: task.modified.clone(),
        due_date: task.due_date.clone(),
        effort: task.effort.clone(),
        subtitle: task.subtitle.clone(),
        description: task.description.clone(),
        tags: task.tags.clone(),
        relationships: task.relationships.clone(),
        comments: task.comments.clone(),
        references: task.references.clone(),
        history: task.history.clone(),
        custom_fields: task.custom_fields.clone(),
    }
}

pub fn initialize(api_server: &mut ApiServer) {
    // GET /api/whoami -> current user resolved by identity
    api_server.register_handler("GET", "/api/whoami", |_req: &HttpRequest| {
        let who = crate::utils::identity::resolve_current_user(None).unwrap_or_default();
        ok_json(200, json!({"data": who}))
    });
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
            // Accept project from JSON body, fallback to query for backward-compat
            project: body
                .get("project")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string())
                .or_else(|| req.query.get("project").cloned()),
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
            tags: add.tags,
            relationships: match body.get("relationships") {
                Some(value) => match serde_json::from_value::<crate::types::TaskRelationships>(
                    value.clone(),
                ) {
                    Ok(rel) => {
                        if rel.is_empty() {
                            None
                        } else {
                            Some(rel)
                        }
                    }
                    Err(e) => return bad_request(format!("Invalid relationships payload: {}", e)),
                },
                None => None,
            },
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
            tags: req
                .query
                .get("tags")
                .map(|s| s.split(',').map(|s| s.trim().to_string()).collect())
                .unwrap_or_default(),
            text_query: req.query.get("q").cloned(),
        };
        let tasks = TaskService::list(&storage, &filter);
        // API parity: accept additional query keys (built-ins or declared custom fields)
        // Build filters map from unknown keys and assignee
        use std::collections::{BTreeMap, BTreeSet};
        let mut uf: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
        let known = ["project", "status", "priority", "type", "tags", "q"];
        // Assignee (supports @me)
        if let Some(a) = req.query.get("assignee") {
            let v = if a == "@me" {
                crate::utils::identity::resolve_current_user(Some(resolver.path.as_path()))
                    .unwrap_or_else(|| a.clone())
            } else {
                a.clone()
            };
            uf.entry("assignee".into()).or_default().insert(v);
        }
        // Other keys
        for (k, v) in req.query.iter() {
            if known.contains(&k.as_str()) || k == "assignee" {
                continue;
            }
            // CSV allowed
            for part in v.split(',').map(|s| s.trim()).filter(|s| !s.is_empty()) {
                uf.entry(k.clone()).or_default().insert(part.to_string());
            }
        }

        // Apply in-memory filters if any
        let mut tasks = tasks; // shadow mutable
        if !uf.is_empty() {
            let resolve_vals = |id: &str,
                                t: &crate::api_types::TaskDTO,
                                key: &str,
                                cfg: &crate::config::types::ResolvedConfig|
             -> Option<Vec<String>> {
                let raw = key.trim();
                let k = raw.to_lowercase();
                if let Some(canon) = crate::utils::fields::is_reserved_field(raw) {
                    match canon {
                        "assignee" => return Some(vec![t.assignee.clone().unwrap_or_default()]),
                        "reporter" => return Some(vec![t.reporter.clone().unwrap_or_default()]),
                        "type" => return Some(vec![t.task_type.to_string()]),
                        "status" => return Some(vec![t.status.to_string()]),
                        "priority" => return Some(vec![t.priority.to_string()]),
                        "project" => {
                            return Some(vec![id.split('-').next().unwrap_or("").to_string()])
                        }
                        "tags" => return Some(t.tags.clone()),
                        _ => {}
                    }
                }
                if let Some(rest) = k.strip_prefix("field:") {
                    let name = rest.trim();
                    let v = t.custom_fields.get(name)?;
                    return Some(vec![crate::types::custom_value_to_string(v)]);
                }
                if cfg.custom_fields.has_wildcard()
                    || cfg
                        .custom_fields
                        .values
                        .iter()
                        .any(|v| v.eq_ignore_ascii_case(raw))
                {
                    if let Some(vv) = t.custom_fields.get(raw) {
                        return Some(vec![crate::types::custom_value_to_string(vv)]);
                    }
                    let lname = raw.to_lowercase();
                    if let Some((_, vv)) = t
                        .custom_fields
                        .iter()
                        .find(|(k, _)| k.to_lowercase() == lname)
                    {
                        return Some(vec![crate::types::custom_value_to_string(vv)]);
                    }
                }
                None
            };

            tasks.retain(|(id, t)| {
                for (fk, allowed) in &uf {
                    let vals = match resolve_vals(id, t, fk, cfg) {
                        Some(vs) => vs.into_iter().filter(|s| !s.is_empty()).collect::<Vec<_>>(),
                        None => return false,
                    };
                    if vals.is_empty() {
                        return false;
                    }
                    let allowed_vec: Vec<String> = allowed.iter().cloned().collect();
                    if !crate::utils::fuzzy_match::fuzzy_set_match(&vals, &allowed_vec) {
                        return false;
                    }
                }
                true
            });
        }

        ok_json(
            200,
            json!({
                "data": tasks.iter().map(|(_, t)| t).collect::<Vec<_>>(),
                "meta": {"count": tasks.len()}
            }),
        )
    });

    // GET /api/tasks/export -> CSV of tasks using same filters as list
    api_server.register_handler("GET", "/api/tasks/export", |req: &HttpRequest| {
        let resolver = match TasksDirectoryResolver::resolve(None, None) {
            Ok(r) => r,
            Err(e) => return internal(json!({"error": {"code": "INTERNAL", "message": e}})),
        };
        let storage = crate::storage::manager::Storage::new(resolver.path.clone());
        let cfg_mgr = match crate::config::manager::ConfigManager::new_manager_with_tasks_dir_readonly(&resolver.path) {
            Ok(m) => m,
            Err(e) => return internal(json!({"error": {"code": "INTERNAL", "message": format!("Failed to load config: {}", e)}})),
        };
        let cfg = cfg_mgr.get_resolved_config();

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

        let filter = crate::api_types::TaskListFilter {
            status: statuses,
            priority: priorities,
            task_type: types_vec,
            project: req.query.get("project").cloned(),
            tags: req
                .query
                .get("tags")
                .map(|s| s.split(',').map(|s| s.trim().to_string()).collect())
                .unwrap_or_default(),
            text_query: req.query.get("q").cloned(),
        };
        let tasks = TaskService::list(&storage, &filter);

        // Build CSV (quoted where needed)
        fn esc(s: &str) -> String {
            let mut v = s.replace('"', "\"\"");
            if v.contains(',') || v.contains('\n') || v.contains('\r') {
                v = format!("\"{}\"", v);
            }
            v
        }
        let mut wtr = String::from("id,title,status,priority,type,assignee,due_date,tags\n");
        for (id, t) in tasks {
            let vals = [
                esc(&id),
                esc(&t.title.replace('\n', " ")),
                esc(&t.status.to_string()),
                esc(&t.priority.to_string()),
                esc(&t.task_type.to_string()),
                esc(&t.assignee.unwrap_or_default()),
                esc(&t.due_date.unwrap_or_default()),
                esc(&t.tags.join(";")),
            ];
            wtr.push_str(&vals.join(","));
            wtr.push('\n');
        }
        let headers = vec![
            ("Content-Type".to_string(), "text/csv; charset=utf-8".to_string()),
            ("Content-Disposition".to_string(), "attachment; filename=tasks.csv".to_string()),
        ];
        HttpResponse { status: 200, headers, body: wtr.into_bytes() }
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

    // GET /api/references/snippet?code=<path#Lx>
    api_server.register_handler("GET", "/api/references/snippet", |req: &HttpRequest| {
        let code = match req.query.get("code") {
            Some(v) if !v.trim().is_empty() => v.trim().to_string(),
            _ => return bad_request("Missing code reference".into()),
        };
        let parse_ctx = |key: &str| -> Option<usize> {
            req.query.get(key).and_then(|s| s.parse::<usize>().ok())
        };
        let normalize = |value: usize| -> usize {
            match value {
                0 => 1,
                v if v > 20 => 20,
                v => v,
            }
        };
        let default_context = parse_ctx("context").map(normalize).unwrap_or(6);
        let before = parse_ctx("before")
            .map(normalize)
            .unwrap_or(default_context);
        let after = parse_ctx("after").map(normalize).unwrap_or(default_context);
        let resolver = match TasksDirectoryResolver::resolve(None, None) {
            Ok(r) => r,
            Err(e) => return internal(json!({"error": {"code": "INTERNAL", "message": e}})),
        };
        let repo_root = match crate::utils::git::find_repo_root(&resolver.path) {
            Some(root) => root,
            None => return bad_request("Unable to locate git repository".into()),
        };
        match ReferenceService::snippet_for_code(&repo_root, &code, before, after) {
            Ok(snippet) => ok_json(200, json!({"data": snippet})),
            Err(msg) => bad_request(msg),
        }
    });

    // GET /api/tasks/suggest?q=TEXT[&project=PREFIX][&limit=N]
    api_server.register_handler("GET", "/api/tasks/suggest", |req: &HttpRequest| {
        let resolver = match TasksDirectoryResolver::resolve(None, None) {
            Ok(r) => r,
            Err(e) => return internal(json!({"error": {"code": "INTERNAL", "message": e}})),
        };
        let storage = crate::storage::manager::Storage::new(resolver.path);
        let q = req.query.get("q").cloned().unwrap_or_default();
        let project = req.query.get("project").cloned();
        let limit: usize = req
            .query
            .get("limit")
            .and_then(|s| s.parse::<usize>().ok())
            .unwrap_or(20);
        let filter = crate::api_types::TaskListFilter {
            text_query: if q.is_empty() { None } else { Some(q) },
            project,
            ..Default::default()
        };
        let mut list = crate::services::task_service::TaskService::list(&storage, &filter)
            .into_iter()
            .map(|(id, t)| json!({"id": id, "title": t.title}))
            .collect::<Vec<_>>();
        if list.len() > limit {
            list.truncate(limit);
        }
        ok_json(200, json!({"data": list}))
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
        let tags_override = match body.get("tags") {
            Some(serde_json::Value::Array(items)) => {
                let mut collected = Vec::with_capacity(items.len());
                for item in items {
                    match item.as_str() {
                        Some(s) => collected.push(s.to_string()),
                        None => return bad_request("Invalid tags payload".into()),
                    }
                }
                Some(collected)
            }
            Some(serde_json::Value::Null) => Some(Vec::new()),
            Some(_) => return bad_request("Invalid tags payload".into()),
            None => None,
        };
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
            reporter: edit.reporter,
            assignee: edit.assignee,
            due_date: edit.due,
            effort: edit.effort,
            description: edit.description,
            tags: tags_override.or(if edit.tags.is_empty() { None } else { Some(edit.tags) }),
            relationships: match body.get("relationships") {
                Some(value) => match serde_json::from_value::<crate::types::TaskRelationships>(
                    value.clone(),
                ) {
                    Ok(rel) => {
                        if rel.is_empty() {
                            None
                        } else {
                            Some(rel)
                        }
                    }
                    Err(e) => return bad_request(format!("Invalid relationships payload: {}", e)),
                },
                None => None,
            },
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

    // POST /api/tasks/status { id, status }
    api_server.register_handler("POST", "/api/tasks/status", |req: &HttpRequest| {
        let resolver = match TasksDirectoryResolver::resolve(None, None) {
            Ok(r) => r,
            Err(e) => return internal(json!({"error": {"code": "INTERNAL", "message": e}})),
        };
        let mut storage = crate::storage::manager::Storage::new(resolver.path.clone());
        let body: serde_json::Value = serde_json::from_slice(&req.body).unwrap_or(json!({}));
        let id = match body.get("id").and_then(|v| v.as_str()) {
            Some(s) if !s.is_empty() => s.to_string(),
            _ => return bad_request("Missing id".into()),
        };
        let new_status = match body.get("status").and_then(|v| v.as_str()) {
            Some(s) if !s.is_empty() => s.to_string(),
            _ => return bad_request("Missing status".into()),
        };
        // Load config for validation
        let cfg_mgr = match crate::config::manager::ConfigManager::new_manager_with_tasks_dir_readonly(&resolver.path) {
            Ok(m) => m,
            Err(e) => return internal(json!({"error": {"code": "INTERNAL", "message": format!("Failed to load config: {}", e)}})),
        };
        let cfg = cfg_mgr.get_resolved_config();
        // Validate status
        let parsed = match crate::types::TaskStatus::parse_with_config(&new_status, cfg) {
            Ok(s) => s,
            Err(msg) => return bad_request(msg),
        };
        let patch = crate::api_types::TaskUpdate {
            title: None,
            status: Some(parsed),
            priority: None,
            task_type: None,
            reporter: None,
            assignee: None,
            due_date: None,
            effort: None,
            description: None,
            tags: None,
            relationships: None,
            custom_fields: None,
        };
        match TaskService::update(&mut storage, &id, patch) {
            Ok(task) => {
                let actor = crate::utils::identity::resolve_current_user(Some(resolver.path.as_path()));
                crate::api_events::emit_task_updated(&task, actor.as_deref());
                ok_json(200, json!({"data": task}))
            }
            Err(e) => bad_request(e.to_string()),
        }
    });

    // POST /api/tasks/comment { id, text }
    api_server.register_handler("POST", "/api/tasks/comment", |req: &HttpRequest| {
        let resolver = match TasksDirectoryResolver::resolve(None, None) {
            Ok(r) => r,
            Err(e) => return internal(json!({"error": {"code": "INTERNAL", "message": e}})),
        };
        let mut storage = crate::storage::manager::Storage::new(resolver.path.clone());
        let body: serde_json::Value = serde_json::from_slice(&req.body).unwrap_or(json!({}));
        let id = match body.get("id").and_then(|v| v.as_str()) {
            Some(s) if !s.is_empty() => s.to_string(),
            _ => return bad_request("Missing id".into()),
        };
        let text = match body.get("text").and_then(|v| v.as_str()) {
            Some(s) if !s.is_empty() => s.to_string(),
            _ => return bad_request("Missing text".into()),
        };
        let project_prefix = id.split('-').next().unwrap_or("").to_string();
        let mut task = match storage.get(&id, project_prefix.clone()) {
            Some(t) => t,
            None => return not_found(format!("Task '{}' not found", id)),
        };
        let comment = crate::types::TaskComment {
            date: chrono::Utc::now().to_rfc3339(),
            text,
        };
        task.comments.push(comment);
        task.history.push(crate::types::TaskChangeLogEntry {
            at: task
                .comments
                .last()
                .map(|c| c.date.clone())
                .unwrap_or_else(|| chrono::Utc::now().to_rfc3339()),
            actor: crate::utils::identity::resolve_current_user(Some(resolver.path.as_path())),
            changes: vec![crate::types::TaskChange {
                field: "comment".into(),
                old: None,
                new: task.comments.last().map(|c| c.text.clone()),
            }],
        });
        task.modified = chrono::Utc::now().to_rfc3339();
        storage.edit(&id, &task);
        let dto = task_to_dto(&id, &task);
        let actor = crate::utils::identity::resolve_current_user(Some(resolver.path.as_path()));
        crate::api_events::emit_task_updated(&dto, actor.as_deref());
        ok_json(200, json!({"data": dto}))
    });

    // POST /api/tasks/comment/update { id, index, text }
    api_server.register_handler("POST", "/api/tasks/comment/update", |req: &HttpRequest| {
        let resolver = match TasksDirectoryResolver::resolve(None, None) {
            Ok(r) => r,
            Err(e) => return internal(json!({"error": {"code": "INTERNAL", "message": e}})),
        };
        let mut storage = crate::storage::manager::Storage::new(resolver.path.clone());
        let body: serde_json::Value = serde_json::from_slice(&req.body).unwrap_or(json!({}));
        let id = match body.get("id").and_then(|v| v.as_str()) {
            Some(s) if !s.is_empty() => s.to_string(),
            _ => return bad_request("Missing id".into()),
        };
        let index = match body.get("index").and_then(|v| v.as_u64()) {
            Some(i) => i as usize,
            None => return bad_request("Missing index".into()),
        };
        let text_raw = match body.get("text").and_then(|v| v.as_str()) {
            Some(s) => s,
            None => return bad_request("Missing text".into()),
        };
        let trimmed = text_raw.trim();
        if trimmed.is_empty() {
            return bad_request("Missing text".into());
        }
        let project_prefix = id.split('-').next().unwrap_or("").to_string();
        let mut task = match storage.get(&id, project_prefix.clone()) {
            Some(t) => t,
            None => return not_found(format!("Task '{}' not found", id)),
        };
        if index >= task.comments.len() {
            return bad_request("Invalid comment index".into());
        }
        let previous = task.comments[index].text.clone();
        if previous == trimmed {
            let dto = task_to_dto(&id, &task);
            return ok_json(200, json!({"data": dto}));
        }
        let new_text = trimmed.to_string();
        task.comments[index].text = new_text.clone();
        let now = chrono::Utc::now().to_rfc3339();
        task.history.push(crate::types::TaskChangeLogEntry {
            at: now.clone(),
            actor: crate::utils::identity::resolve_current_user(Some(resolver.path.as_path())),
            changes: vec![crate::types::TaskChange {
                field: format!("comment#{}", index + 1),
                old: Some(previous),
                new: Some(new_text.clone()),
            }],
        });
        task.modified = now;
        storage.edit(&id, &task);
        let dto = task_to_dto(&id, &task);
        let actor = crate::utils::identity::resolve_current_user(Some(resolver.path.as_path()));
        crate::api_events::emit_task_updated(&dto, actor.as_deref());
        ok_json(200, json!({"data": dto}))
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

    // GET /api/tasks/history?id=ID[&limit=N]
    api_server.register_handler("GET", "/api/tasks/history", |req: &HttpRequest| {
        let id = match req.query.get("id") {
            Some(v) if !v.is_empty() => v.clone(),
            _ => return bad_request("Missing id".into()),
        };
        let resolver = match TasksDirectoryResolver::resolve(None, None) {
            Ok(r) => r,
            Err(e) => return internal(json!({"error": {"code": "INTERNAL", "message": e}})),
        };
        // Find repo root and compute file rel path
        let cwd = std::env::current_dir().unwrap_or_else(|_| resolver.path.clone());
        let repo_root = match crate::utils::git::find_repo_root(&cwd) {
            Some(r) => r,
            None => return bad_request("Not inside a git repository".into()),
        };
        let tasks_abs = resolver.path.clone();
        let tasks_rel = match tasks_abs.strip_prefix(&repo_root) {
            Ok(p) => p.to_path_buf(),
            Err(_) => return bad_request("Tasks directory not inside repository".into()),
        };
        // Derive project and numeric from ID
        let project = match crate::storage::operations::StorageOperations::get_project_for_task(&id)
        {
            Some(p) => p,
            None => return bad_request("Invalid task id".into()),
        };
        let numeric: u64 = match id.split('-').nth(1).and_then(|s| s.parse().ok()) {
            Some(n) => n,
            None => return bad_request("Invalid task id".into()),
        };
        let file_rel = tasks_rel.join(&project).join(format!("{}.yml", numeric));
        let mut commits = match crate::services::audit_service::AuditService::list_commits_for_file(
            &repo_root, &file_rel,
        ) {
            Ok(v) => v,
            Err(e) => return internal(json!({"error": {"code": "INTERNAL", "message": e}})),
        };
        if let Some(limit_s) = req.query.get("limit") {
            if let Ok(limit) = limit_s.parse::<usize>() {
                if commits.len() > limit {
                    commits.truncate(limit);
                }
            }
        }
        ok_json(200, json!({"data": commits}))
    });

    // Activity endpoints
    // GET /api/activity/feed[?since=ISO][&until=ISO][&project=PREFIX][&limit=N]
    api_server.register_handler("GET", "/api/activity/feed", |req: &HttpRequest| {
        let resolver = match TasksDirectoryResolver::resolve(None, None) {
            Ok(r) => r,
            Err(e) => return internal(json!({"error": {"code": "INTERNAL", "message": e}})),
        };
        let cwd = std::env::current_dir().unwrap_or_else(|_| resolver.path.clone());
        let repo_root = match crate::utils::git::find_repo_root(&cwd) {
            Some(r) => r,
            None => return bad_request("Not inside a git repository".into()),
        };
        let tasks_abs = resolver.path.clone();
        let tasks_rel = match tasks_abs.strip_prefix(&repo_root) {
            Ok(p) => p.to_path_buf(),
            Err(_) => return bad_request("Tasks directory not inside repository".into()),
        };
        let now = chrono::Utc::now();
        let default_since = now - chrono::Duration::days(30);
        let since = req
            .query
            .get("since")
            .and_then(|s| chrono::DateTime::parse_from_rfc3339(s).ok())
            .map(|dt| dt.with_timezone(&chrono::Utc))
            .unwrap_or(default_since);
        let until = req
            .query
            .get("until")
            .and_then(|s| chrono::DateTime::parse_from_rfc3339(s).ok())
            .map(|dt| dt.with_timezone(&chrono::Utc))
            .unwrap_or(now);
        let project = req.query.get("project").map(|s| s.as_str());
        let limit = req
            .query
            .get("limit")
            .and_then(|s| s.parse::<usize>().ok())
            .unwrap_or(200);
        match crate::services::audit_service::AuditService::list_activity_feed(
            &repo_root, &tasks_rel, since, until, project, limit,
        ) {
            Ok(items) => ok_json(200, json!({"data": items})),
            Err(e) => internal(json!({"error": {"code": "INTERNAL", "message": e}})),
        }
    });

    // GET /api/activity/series?group=author|day|week|project[&since=ISO][&until=ISO][&project=PREFIX]
    api_server.register_handler("GET", "/api/activity/series", |req: &HttpRequest| {
        let resolver = match TasksDirectoryResolver::resolve(None, None) {
            Ok(r) => r,
            Err(e) => return internal(json!({"error": {"code": "INTERNAL", "message": e}})),
        };
        // Repo root and tasks relative
        let cwd = std::env::current_dir().unwrap_or_else(|_| resolver.path.clone());
        let repo_root = match crate::utils::git::find_repo_root(&cwd) {
            Some(r) => r,
            None => return bad_request("Not inside a git repository".into()),
        };
        let tasks_abs = resolver.path.clone();
        let tasks_rel = match tasks_abs.strip_prefix(&repo_root) {
            Ok(p) => p.to_path_buf(),
            Err(_) => return bad_request("Tasks directory not inside repository".into()),
        };
        // Parse group
        let gb = match req.query.get("group").map(|s| s.as_str()) {
            Some("author") => crate::services::audit_service::GroupBy::Author,
            Some("day") => crate::services::audit_service::GroupBy::Day,
            Some("week") => crate::services::audit_service::GroupBy::Week,
            Some("project") => crate::services::audit_service::GroupBy::Project,
            _ => crate::services::audit_service::GroupBy::Day,
        };
        // Parse since/until
        let now = chrono::Utc::now();
        let default_since = now - chrono::Duration::days(30);
        let since = req
            .query
            .get("since")
            .and_then(|s| chrono::DateTime::parse_from_rfc3339(s).ok())
            .map(|dt| dt.with_timezone(&chrono::Utc))
            .unwrap_or(default_since);
        let until = req
            .query
            .get("until")
            .and_then(|s| chrono::DateTime::parse_from_rfc3339(s).ok())
            .map(|dt| dt.with_timezone(&chrono::Utc))
            .unwrap_or(now);
        let project = req.query.get("project").map(|s| s.as_str());
        match crate::services::audit_service::AuditService::list_activity(
            &repo_root, &tasks_rel, since, until, gb, project,
        ) {
            Ok(items) => ok_json(200, json!({"data": items})),
            Err(e) => internal(json!({"error": {"code": "INTERNAL", "message": e}})),
        }
    });

    // GET /api/activity/authors[?since=ISO][&until=ISO][&project=PREFIX]
    api_server.register_handler("GET", "/api/activity/authors", |req: &HttpRequest| {
        let resolver = match TasksDirectoryResolver::resolve(None, None) {
            Ok(r) => r,
            Err(e) => return internal(json!({"error": {"code": "INTERNAL", "message": e}})),
        };
        let cwd = std::env::current_dir().unwrap_or_else(|_| resolver.path.clone());
        let repo_root = match crate::utils::git::find_repo_root(&cwd) {
            Some(r) => r,
            None => return bad_request("Not inside a git repository".into()),
        };
        let tasks_abs = resolver.path.clone();
        let tasks_rel = match tasks_abs.strip_prefix(&repo_root) {
            Ok(p) => p.to_path_buf(),
            Err(_) => return bad_request("Tasks directory not inside repository".into()),
        };
        let now = chrono::Utc::now();
        let default_since = now - chrono::Duration::days(30);
        let since = req
            .query
            .get("since")
            .and_then(|s| chrono::DateTime::parse_from_rfc3339(s).ok())
            .map(|dt| dt.with_timezone(&chrono::Utc))
            .unwrap_or(default_since);
        let until = req
            .query
            .get("until")
            .and_then(|s| chrono::DateTime::parse_from_rfc3339(s).ok())
            .map(|dt| dt.with_timezone(&chrono::Utc))
            .unwrap_or(now);
        let project = req.query.get("project").map(|s| s.as_str());
        match crate::services::audit_service::AuditService::list_authors_activity(
            &repo_root, &tasks_rel, since, until, project,
        ) {
            Ok(items) => ok_json(200, json!({"data": items})),
            Err(e) => internal(json!({"error": {"code": "INTERNAL", "message": e}})),
        }
    });

    // GET /api/activity/changed_tasks[?since=ISO][&until=ISO][&author=str][&project=PREFIX]
    api_server.register_handler("GET", "/api/activity/changed_tasks", |req: &HttpRequest| {
        let resolver = match TasksDirectoryResolver::resolve(None, None) {
            Ok(r) => r,
            Err(e) => return internal(json!({"error": {"code": "INTERNAL", "message": e}})),
        };
        let cwd = std::env::current_dir().unwrap_or_else(|_| resolver.path.clone());
        let repo_root = match crate::utils::git::find_repo_root(&cwd) {
            Some(r) => r,
            None => return bad_request("Not inside a git repository".into()),
        };
        let tasks_abs = resolver.path.clone();
        let tasks_rel = match tasks_abs.strip_prefix(&repo_root) {
            Ok(p) => p.to_path_buf(),
            Err(_) => return bad_request("Tasks directory not inside repository".into()),
        };
        let now = chrono::Utc::now();
        let default_since = now - chrono::Duration::days(30);
        let since = req
            .query
            .get("since")
            .and_then(|s| chrono::DateTime::parse_from_rfc3339(s).ok())
            .map(|dt| dt.with_timezone(&chrono::Utc))
            .unwrap_or(default_since);
        let until = req
            .query
            .get("until")
            .and_then(|s| chrono::DateTime::parse_from_rfc3339(s).ok())
            .map(|dt| dt.with_timezone(&chrono::Utc))
            .unwrap_or(now);
        let author = req.query.get("author").map(|s| s.as_str());
        let project = req.query.get("project").map(|s| s.as_str());
        match crate::services::audit_service::AuditService::list_changed_tasks(
            &repo_root, &tasks_rel, since, until, author, project,
        ) {
            Ok(items) => ok_json(200, json!({"data": items})),
            Err(e) => internal(json!({"error": {"code": "INTERNAL", "message": e}})),
        }
    });

    // GET /api/tasks/commit_diff?id=ID&commit=SHA
    api_server.register_handler("GET", "/api/tasks/commit_diff", |req: &HttpRequest| {
        let id = match req.query.get("id") {
            Some(v) if !v.is_empty() => v.clone(),
            _ => return bad_request("Missing id".into()),
        };
        let commit = match req.query.get("commit") {
            Some(v) if !v.is_empty() => v.clone(),
            _ => return bad_request("Missing commit".into()),
        };
        let resolver = match TasksDirectoryResolver::resolve(None, None) {
            Ok(r) => r,
            Err(e) => return internal(json!({"error": {"code": "INTERNAL", "message": e}})),
        };
        let cwd = std::env::current_dir().unwrap_or_else(|_| resolver.path.clone());
        let repo_root = match crate::utils::git::find_repo_root(&cwd) {
            Some(r) => r,
            None => return bad_request("Not inside a git repository".into()),
        };
        let tasks_abs = resolver.path.clone();
        let tasks_rel = match tasks_abs.strip_prefix(&repo_root) {
            Ok(p) => p.to_path_buf(),
            Err(_) => return bad_request("Tasks directory not inside repository".into()),
        };
        let project = match crate::storage::operations::StorageOperations::get_project_for_task(&id)
        {
            Some(p) => p,
            None => return bad_request("Invalid task id".into()),
        };
        let numeric: u64 = match id.split('-').nth(1).and_then(|s| s.parse().ok()) {
            Some(n) => n,
            None => return bad_request("Invalid task id".into()),
        };
        let file_rel = tasks_rel.join(&project).join(format!("{}.yml", numeric));
        match crate::services::audit_service::AuditService::show_file_diff(
            &repo_root, &commit, &file_rel,
        ) {
            Ok(diff) => ok_json(200, json!({"data": diff})),
            Err(e) => internal(json!({"error": {"code": "INTERNAL", "message": e}})),
        }
    });
}

fn ok_json(status: u16, v: serde_json::Value) -> HttpResponse {
    json_response(status, v).unwrap_or_else(json_serialize_error)
}

fn bad_request(msg: String) -> HttpResponse {
    ok_json(
        400,
        json!({"error": {"code": "INVALID_ARGUMENT", "message": msg}}),
    )
}

fn internal(v: serde_json::Value) -> HttpResponse {
    ok_json(500, v)
}

fn not_found(msg: String) -> HttpResponse {
    ok_json(404, json!({"error": {"code": "NOT_FOUND", "message": msg}}))
}

fn json_response(status: u16, v: serde_json::Value) -> Result<HttpResponse, serde_json::Error> {
    let body = serde_json::to_vec(&v)?;
    Ok(HttpResponse {
        status,
        headers: vec![("Content-Type".into(), "application/json".into())],
        body,
    })
}

fn json_serialize_error(e: serde_json::Error) -> HttpResponse {
    let fallback = json!({"error": {"code": "SERIALIZE", "message": e.to_string()}});
    HttpResponse {
        status: 500,
        headers: vec![("Content-Type".into(), "application/json".into())],
        body: serde_json::to_vec(&fallback).unwrap_or_else(|_| b"{}".to_vec()),
    }
}

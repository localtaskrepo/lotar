use super::*;

pub(super) fn register(api_server: &mut ApiServer) {
    api_server.register_handler("POST", "/api/tasks/add", |req: &HttpRequest| {
    // Resolve tasks root via resolver
    let resolver = match TasksDirectoryResolver::resolve(None, None) {
        Ok(r) => r,
        Err(e) => return internal(json!({"error": {"code": "INTERNAL", "message": e}})),
    };
    let mut storage = crate::storage::manager::Storage::new(&resolver.path.clone());
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
        reporter: body
            .get("reporter")
            .and_then(|v| v.as_str())
            .map(|s| s.trim())
            .filter(|s| !s.is_empty())
            .map(|s| s.to_string()),
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
        custom_fields: if add.fields.is_empty() {
            None
        } else {
            let mut m = std::collections::HashMap::new();
            for (k, v) in add.fields.into_iter() {
                m.insert(k, crate::types::custom_value_string(v));
            }
            Some(m)
        },
        sprints: body
            .get("sprints")
            .cloned()
            .and_then(|value| serde_json::from_value::<Vec<u32>>(value).ok())
            .unwrap_or_default(),
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
    let storage = crate::storage::manager::Storage::new(&resolver.path.clone());

    let page = match crate::utils::pagination::parse_page(&req.query, 50, 200) {
        Ok(v) => v,
        Err(msg) => return bad_request(msg),
    };
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
    let mut filter = crate::api_types::TaskListFilter {
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
        sprints: req
            .query
            .get("sprints")
            .map(|s| {
                s.split(',')
                    .filter_map(|p| p.trim().parse::<u32>().ok())
                    .collect()
            })
            .unwrap_or_default(),
        custom_fields: BTreeMap::new(),
    };
    // API parity: accept additional query keys (built-ins or declared custom fields)
    // Build filters map from unknown keys and assignee
    let mut uf: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    let known = [
        "project",
        "status",
        "priority",
        "type",
        "tags",
        "sprints",
        "q",
        "assignee",
        "order",
        "limit",
        "offset",
        "page_size",
        "per_page",
        "due",
        "recent",
        "needs",
    ];
    // Assignee (supports @me; __none__ means unassigned)
    let mut wants_unassigned = false;
    if let Some(a) = req.query.get("assignee") {
        if a == "__none__" {
            wants_unassigned = true;
        } else {
            let resolved = if a == "@me" {
                crate::utils::identity::resolve_current_user(Some(resolver.path.as_path()))
                    .unwrap_or_else(|| a.clone())
            } else {
                a.clone()
            };
            // Normalize: strip @ prefix from regular names for consistent matching.
            let v = crate::utils::member::normalize_member_value(&resolved, |name| {
                cfg.agent_profiles.contains_key(name)
            });
            uf.entry("assignee".into()).or_default().insert(v);
        }
    }
    // Other keys
    for (k, v) in req.query.iter() {
        if known.contains(&k.as_str()) || k == "assignee" {
            continue;
        }
        // CSV allowed
        for part in v.split(',').map(|s| s.trim()).filter(|s| !s.is_empty()) {
            if let Some(name) =
                crate::utils::custom_fields::resolve_filter_name(k, cfg)
            {
                filter
                    .custom_fields
                    .entry(name)
                    .or_default()
                    .push(part.to_string());
            } else {
                uf.entry(k.clone()).or_default().insert(part.to_string());
            }
        }
    }

    let tasks = TaskService::list(&storage, &filter);

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
                    "assignee" => {
                        let v = t.assignee.as_deref().unwrap_or("");
                        return Some(vec![v.trim_start_matches('@').to_string()]);
                    }
                    "reporter" => {
                        let v = t.reporter.as_deref().unwrap_or("");
                        return Some(vec![v.trim_start_matches('@').to_string()]);
                    }
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

    if wants_unassigned {
        tasks.retain(|(_, task)| {
            task.assignee
                .as_deref()
                .unwrap_or("")
                .trim()
                .is_empty()
        });
    }

    let due = req.query.get("due").map(|s| s.as_str()).unwrap_or("");
    let recent = req.query.get("recent").map(|s| s.as_str()).unwrap_or("");
    let needs_raw = req.query.get("needs").map(|s| s.as_str()).unwrap_or("");
    let needs: BTreeSet<&str> = needs_raw
        .split(',')
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
        .collect();

    if !due.is_empty() || !recent.is_empty() || !needs.is_empty() {
        use chrono::{DateTime, Duration, Local, NaiveDate, Utc};

        let today = Local::now().date_naive();
        let tomorrow = today + Duration::days(1);
        let soon_cutoff = today + Duration::days(7);
        let recent_cutoff = Utc::now() - Duration::days(7);

        tasks.retain(|(_, task)| {
            if !due.is_empty() {
                let Some(raw_due) = task.due_date.as_deref() else {
                    return false;
                };
                let Ok(due_date) = NaiveDate::parse_from_str(raw_due.trim(), "%Y-%m-%d") else {
                    return false;
                };

                match due {
                    "today"
                        if due_date != today => {
                            return false;
                        }
                    "soon"
                        if (due_date < tomorrow || due_date > soon_cutoff) => {
                            return false;
                        }
                    "later"
                        if due_date <= soon_cutoff => {
                            return false;
                        }
                    "overdue"
                        if due_date >= today => {
                            return false;
                        }
                    _ => {}
                }
            }

            if recent == "7d" {
                let Ok(modified) = DateTime::parse_from_rfc3339(task.modified.as_str()) else {
                    return false;
                };
                if modified.with_timezone(&Utc) < recent_cutoff {
                    return false;
                }
            }

            if !needs.is_empty() {
                if needs.contains("effort") {
                    let effort = task.effort.as_deref().unwrap_or("").trim();
                    if !effort.is_empty() {
                        return false;
                    }
                }
                if needs.contains("due") {
                    let due_val = task.due_date.as_deref().unwrap_or("").trim();
                    if !due_val.is_empty() {
                        return false;
                    }
                }
            }

            true
        });
    }

    let order = req.query.get("order").map(|s| s.as_str()).unwrap_or("desc");
    let desc = order != "asc";
    tasks.sort_by(|(ida, ta), (idb, tb)| {
        use std::cmp::Ordering;

        let mut cmp = ta.modified.cmp(&tb.modified);
        if desc {
            cmp = cmp.reverse();
        }
        if cmp != Ordering::Equal {
            return cmp;
        }
        if desc {
            idb.cmp(ida)
        } else {
            ida.cmp(idb)
        }
    });

    let total = tasks.len();
    let (start, end) = crate::utils::pagination::slice_bounds(total, page.offset, page.limit);
    let page_tasks = tasks[start..end]
        .iter()
        .map(|(_, task)| task.clone())
        .collect::<Vec<_>>();

    let payload = crate::api_types::TaskListResponse {
        total,
        limit: page.limit,
        offset: page.offset,
        tasks: page_tasks,
    };

    ok_json(200, json!({"data": payload}))
});

    // GET /api/sprints/list

    api_server.register_handler("GET", "/api/tasks/export", |req: &HttpRequest| {
    let resolver = match TasksDirectoryResolver::resolve(None, None) {
        Ok(r) => r,
        Err(e) => return internal(json!({"error": {"code": "INTERNAL", "message": e}})),
    };
    let storage = crate::storage::manager::Storage::new(&resolver.path.clone());
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

    let mut filter = crate::api_types::TaskListFilter {
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
        sprints: vec![],
        custom_fields: BTreeMap::new(),
    };
    let known = ["project", "status", "priority", "type", "tags", "q"];
    for (k, v) in req.query.iter() {
        if known.contains(&k.as_str()) {
            continue;
        }
        for part in v.split(',').map(|s| s.trim()).filter(|s| !s.is_empty()) {
            if let Some(name) =
                crate::utils::custom_fields::resolve_filter_name(k, cfg)
            {
                filter
                    .custom_fields
                    .entry(name)
                    .or_default()
                    .push(part.to_string());
            }
        }
    }
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
        let storage = crate::storage::manager::Storage::new(&resolver.path);
        match TaskService::get(&storage, &id, req.query.get("project").map(|s| s.as_str())) {
            Ok(task) => ok_json(200, json!({"data": task})),
            Err(e) => match e {
                LoTaRError::TaskNotFound(_) => not_found(e.to_string()),
                _ => bad_request(e.to_string()),
            },
        }
    });

    // GET /api/references/snippet?code=<path#x>

    api_server.register_handler("POST", "/api/tasks/attachments/upload", |req: &HttpRequest| {
    use base64::Engine;

    let body: serde_json::Value = serde_json::from_slice(&req.body).unwrap_or(json!({}));
    let payload: crate::api_types::AttachmentUploadRequest = match serde_json::from_value(body)
    {
        Ok(v) => v,
        Err(e) => return bad_request(format!("Invalid body: {}", e)),
    };

    if payload.id.trim().is_empty() {
        return bad_request("Missing task id".into());
    }
    if payload.filename.trim().is_empty() {
        return bad_request("Missing filename".into());
    }
    if payload.content_base64.trim().is_empty() {
        return bad_request("Missing attachment content".into());
    }

    let resolver = match TasksDirectoryResolver::resolve(None, None) {
        Ok(r) => r,
        Err(e) => return internal(json!({"error": {"code": "INTERNAL", "message": e}})),
    };

    let base_config = match resolution::load_and_merge_configs(Some(resolver.path.as_path())) {
        Ok(c) => c,
        Err(e) => return bad_request(format!("Failed to load config: {}", e)),
    };

    // Apply per-project overrides (if task id contains a project prefix)
    let config = if let Some(dash_pos) = payload.id.find('-') {
        let prefix = payload.id[..dash_pos].trim();
        if prefix.is_empty() {
            base_config
        } else {
            resolution::get_project_config(&base_config, prefix, resolver.path.as_path())
                .unwrap_or(base_config)
        }
    } else {
        base_config
    };

    // Enforce configured upload limit before decoding base64.
    match config.attachments_max_upload_mb {
        0 => {
            return bad_request(
                "Attachment uploads are disabled by configuration".to_string(),
            )
        }
        -1 => {}
        n if n > 0 => {
            // keep going; enforce after decoding
        }
        _ => {}
    }

    let bytes = match base64::engine::general_purpose::STANDARD
        .decode(payload.content_base64.trim())
    {
        Ok(b) => b,
        Err(_) => return bad_request("Invalid base64 content".into()),
    };

    if config.attachments_max_upload_mb > 0 {
        let max_bytes = match i128::from(config.attachments_max_upload_mb)
            .checked_mul(1024)
            .and_then(|v| v.checked_mul(1024))
        {
            Some(v) if v > 0 => v,
            _ => 0,
        };

        if max_bytes > 0 && (bytes.len() as i128) > max_bytes {
            return bad_request(format!(
                "Attachment too large: {} bytes (max {} MiB)",
                bytes.len(),
                config.attachments_max_upload_mb
            ));
        }
    }

    let root = match AttachmentService::resolve_attachments_root(resolver.path.as_path(), &config)
    {
        Ok(p) => p,
        Err(e) => return bad_request(e.to_string()),
    };

    let stored = match AttachmentService::store_bytes(&root, &payload.filename, &bytes) {
        Ok(name) => name,
        Err(e) => return internal(json!({"error": {"code": "INTERNAL", "message": e.to_string()}})),
    };

    let mut storage = crate::storage::manager::Storage::new(&resolver.path);
    match AttachmentService::attach_file_reference(&mut storage, &payload.id, &stored) {
        Ok((task, attached)) => ok_json(
            200,
            json!({"data": crate::api_types::AttachmentUploadResponse { stored_path: stored, attached, task }}),
        ),
        Err(e) => match e {
            LoTaRError::TaskNotFound(_) => not_found(e.to_string()),
            _ => bad_request(e.to_string()),
        },
    }
});

    // POST /api/tasks/attachments/remove

    api_server.register_handler(
    "POST",
    "/api/tasks/attachments/remove",
    |req: &HttpRequest| {
        let body: serde_json::Value = serde_json::from_slice(&req.body).unwrap_or(json!({}));
        let payload: crate::api_types::AttachmentRemoveRequest =
            match serde_json::from_value(body) {
                Ok(v) => v,
                Err(e) => return bad_request(format!("Invalid body: {}", e)),
            };

        if payload.id.trim().is_empty() {
            return bad_request("Missing task id".into());
        }
        if payload.stored_path.trim().is_empty() {
            return bad_request("Missing attachment path".into());
        }

        let resolver = match TasksDirectoryResolver::resolve(None, None) {
            Ok(r) => r,
            Err(e) => return internal(json!({"error": {"code": "INTERNAL", "message": e}})),
        };

        let base_config = match resolution::load_and_merge_configs(Some(resolver.path.as_path())) {
            Ok(c) => c,
            Err(e) => return bad_request(format!("Failed to load config: {}", e)),
        };

        let config = if let Some(dash_pos) = payload.id.find('-') {
            let prefix = payload.id[..dash_pos].trim();
            if prefix.is_empty() {
                base_config
            } else {
                resolution::get_project_config(&base_config, prefix, resolver.path.as_path())
                    .unwrap_or(base_config)
            }
        } else {
            base_config
        };
        let root = match AttachmentService::resolve_attachments_root(resolver.path.as_path(), &config) {
            Ok(p) => p,
            Err(e) => return bad_request(e.to_string()),
        };

        let mut storage = crate::storage::manager::Storage::new(&resolver.path);
        match AttachmentService::detach_file_reference(
            &mut storage,
            &payload.id,
            &payload.stored_path,
        ) {
            Ok(task) => {
                let hash_tag = AttachmentService::extract_hash_tag(&payload.stored_path);
                let still_referenced = match hash_tag.as_deref() {
                    Some(hash) => AttachmentService::is_hash_referenced(&storage, hash),
                    None => false,
                };

                let mut deleted = false;
                if !still_referenced {
                    if let Some(hash) = hash_tag.as_deref() {
                        deleted = AttachmentService::delete_all_by_hash(&root, hash) > 0;
                    } else if let Ok(path) =
                        AttachmentService::resolve_attachment_path(&root, &payload.stored_path)
                    {
                        deleted = std::fs::remove_file(path).is_ok();
                    }
                }
                ok_json(
                    200,
                    json!({"data": crate::api_types::AttachmentRemoveResponse { task, deleted, still_referenced }}),
                )
            }
            Err(e) => match e {
                LoTaRError::TaskNotFound(_) => not_found(e.to_string()),
                _ => bad_request(e.to_string()),
            },
        }
    },
);

    // POST /api/tasks/references/link/add

    api_server.register_handler(
        "POST",
        "/api/tasks/references/link/add",
        |req: &HttpRequest| {
            let body: serde_json::Value = serde_json::from_slice(&req.body).unwrap_or(json!({}));
            let payload: crate::api_types::LinkReferenceAddRequest =
                match serde_json::from_value(body) {
                    Ok(v) => v,
                    Err(e) => return bad_request(format!("Invalid body: {}", e)),
                };

            if payload.id.trim().is_empty() {
                return bad_request("Missing task id".into());
            }
            if payload.url.trim().is_empty() {
                return bad_request("Missing url".into());
            }

            let resolver = match TasksDirectoryResolver::resolve(None, None) {
                Ok(r) => r,
                Err(e) => return internal(json!({"error": {"code": "INTERNAL", "message": e}})),
            };

            let mut storage = crate::storage::manager::Storage::new(&resolver.path);
            match ReferenceService::attach_link_reference(&mut storage, &payload.id, &payload.url) {
                Ok((task, added)) => ok_json(
                    200,
                    json!({"data": crate::api_types::LinkReferenceAddResponse { task, added }}),
                ),
                Err(e) => match e {
                    LoTaRError::TaskNotFound(_) => not_found(e.to_string()),
                    _ => bad_request(e.to_string()),
                },
            }
        },
    );

    // POST /api/tasks/references/link/remove

    api_server.register_handler(
    "POST",
    "/api/tasks/references/link/remove",
    |req: &HttpRequest| {
        let body: serde_json::Value = serde_json::from_slice(&req.body).unwrap_or(json!({}));
        let payload: crate::api_types::LinkReferenceRemoveRequest = match serde_json::from_value(body) {
            Ok(v) => v,
            Err(e) => return bad_request(format!("Invalid body: {}", e)),
        };

        if payload.id.trim().is_empty() {
            return bad_request("Missing task id".into());
        }
        if payload.url.trim().is_empty() {
            return bad_request("Missing url".into());
        }

        let resolver = match TasksDirectoryResolver::resolve(None, None) {
            Ok(r) => r,
            Err(e) => return internal(json!({"error": {"code": "INTERNAL", "message": e}})),
        };

        let mut storage = crate::storage::manager::Storage::new(&resolver.path);
        match ReferenceService::detach_link_reference(&mut storage, &payload.id, &payload.url) {
            Ok((task, removed)) => ok_json(
                200,
                json!({"data": crate::api_types::LinkReferenceRemoveResponse { task, removed }}),
            ),
            Err(e) => match e {
                LoTaRError::TaskNotFound(_) => not_found(e.to_string()),
                _ => bad_request(e.to_string()),
            },
        }
    },
);

    // POST /api/tasks/references/code/add

    api_server.register_handler(
        "POST",
        "/api/tasks/references/code/add",
        |req: &HttpRequest| {
            let body: serde_json::Value = serde_json::from_slice(&req.body).unwrap_or(json!({}));
            let payload: crate::api_types::CodeReferenceAddRequest =
                match serde_json::from_value(body) {
                    Ok(v) => v,
                    Err(e) => return bad_request(format!("Invalid body: {}", e)),
                };

            if payload.id.trim().is_empty() {
                return bad_request("Missing task id".into());
            }
            if payload.code.trim().is_empty() {
                return bad_request("Missing code reference".into());
            }

            let resolver = match TasksDirectoryResolver::resolve(None, None) {
                Ok(r) => r,
                Err(e) => return internal(json!({"error": {"code": "INTERNAL", "message": e}})),
            };
            let repo_root = match crate::utils::git::find_repo_root(&resolver.path) {
                Some(root) => root,
                None => return bad_request("Unable to locate git repository".into()),
            };

            let mut storage = crate::storage::manager::Storage::new(&resolver.path);
            match ReferenceService::attach_code_reference(
                &mut storage,
                &repo_root,
                &payload.id,
                &payload.code,
            ) {
                Ok((task, added)) => ok_json(
                    200,
                    json!({"data": crate::api_types::CodeReferenceAddResponse { task, added }}),
                ),
                Err(e) => match e {
                    LoTaRError::TaskNotFound(_) => not_found(e.to_string()),
                    _ => bad_request(e.to_string()),
                },
            }
        },
    );

    // POST /api/tasks/references/code/remove

    api_server.register_handler(
    "POST",
    "/api/tasks/references/code/remove",
    |req: &HttpRequest| {
        let body: serde_json::Value = serde_json::from_slice(&req.body).unwrap_or(json!({}));
        let payload: crate::api_types::CodeReferenceRemoveRequest = match serde_json::from_value(body) {
            Ok(v) => v,
            Err(e) => return bad_request(format!("Invalid body: {}", e)),
        };

        if payload.id.trim().is_empty() {
            return bad_request("Missing task id".into());
        }
        if payload.code.trim().is_empty() {
            return bad_request("Missing code reference".into());
        }

        let resolver = match TasksDirectoryResolver::resolve(None, None) {
            Ok(r) => r,
            Err(e) => return internal(json!({"error": {"code": "INTERNAL", "message": e}})),
        };

        let mut storage = crate::storage::manager::Storage::new(&resolver.path);
        match ReferenceService::detach_code_reference(&mut storage, &payload.id, &payload.code) {
            Ok((task, removed)) => ok_json(
                200,
                json!({"data": crate::api_types::CodeReferenceRemoveResponse { task, removed }}),
            ),
            Err(e) => match e {
                LoTaRError::TaskNotFound(_) => not_found(e.to_string()),
                _ => bad_request(e.to_string()),
            },
        }
    },
);

    // POST /api/tasks/references/add

    api_server.register_handler("POST", "/api/tasks/references/add", |req: &HttpRequest| {
        let body: serde_json::Value = serde_json::from_slice(&req.body).unwrap_or(json!({}));
        let payload: crate::api_types::GenericReferenceAddRequest =
            match serde_json::from_value(body) {
                Ok(v) => v,
                Err(e) => return bad_request(format!("Invalid body: {}", e)),
            };

        if payload.id.trim().is_empty() {
            return bad_request("Missing task id".into());
        }
        if payload.kind.trim().is_empty() {
            return bad_request("Missing reference kind".into());
        }
        if payload.value.trim().is_empty() {
            return bad_request("Missing reference value".into());
        }

        let kind = payload.kind.trim().to_ascii_lowercase();
        if kind != "jira" && kind != "github" {
            return bad_request("Reference kind must be jira or github".into());
        }

        let resolver = match TasksDirectoryResolver::resolve(None, None) {
            Ok(r) => r,
            Err(e) => return internal(json!({"error": {"code": "INTERNAL", "message": e}})),
        };

        let mut storage = crate::storage::manager::Storage::new(&resolver.path);
        match ReferenceService::attach_platform_reference(
            &mut storage,
            &payload.id,
            &kind,
            &payload.value,
        ) {
            Ok((task, added)) => ok_json(
                200,
                json!({"data": crate::api_types::GenericReferenceAddResponse { task, added }}),
            ),
            Err(e) => match e {
                LoTaRError::TaskNotFound(_) => not_found(e.to_string()),
                _ => bad_request(e.to_string()),
            },
        }
    });

    // POST /api/tasks/references/remove

    api_server.register_handler(
    "POST",
    "/api/tasks/references/remove",
    |req: &HttpRequest| {
        let body: serde_json::Value = serde_json::from_slice(&req.body).unwrap_or(json!({}));
        let payload: crate::api_types::GenericReferenceRemoveRequest =
            match serde_json::from_value(body) {
                Ok(v) => v,
                Err(e) => return bad_request(format!("Invalid body: {}", e)),
            };

        if payload.id.trim().is_empty() {
            return bad_request("Missing task id".into());
        }
        if payload.kind.trim().is_empty() {
            return bad_request("Missing reference kind".into());
        }
        if payload.value.trim().is_empty() {
            return bad_request("Missing reference value".into());
        }

        let kind = payload.kind.trim().to_ascii_lowercase();
        if kind != "jira" && kind != "github" {
            return bad_request("Reference kind must be jira or github".into());
        }

        let resolver = match TasksDirectoryResolver::resolve(None, None) {
            Ok(r) => r,
            Err(e) => return internal(json!({"error": {"code": "INTERNAL", "message": e}})),
        };

        let mut storage = crate::storage::manager::Storage::new(&resolver.path);
        match ReferenceService::detach_platform_reference(
            &mut storage,
            &payload.id,
            &kind,
            &payload.value,
        ) {
            Ok((task, removed)) => ok_json(
                200,
                json!({"data": crate::api_types::GenericReferenceRemoveResponse { task, removed }}),
            ),
            Err(e) => match e {
                LoTaRError::TaskNotFound(_) => not_found(e.to_string()),
                _ => bad_request(e.to_string()),
            },
        }
    },
);

    // GET /api/tasks/suggest?q=TEXT[&project=PREFIX][&limit=N]

    api_server.register_handler("GET", "/api/tasks/suggest", |req: &HttpRequest| {
        let resolver = match TasksDirectoryResolver::resolve(None, None) {
            Ok(r) => r,
            Err(e) => return internal(json!({"error": {"code": "INTERNAL", "message": e}})),
        };
        let storage = crate::storage::manager::Storage::new(&resolver.path);
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
    let mut storage = crate::storage::manager::Storage::new(&resolver.path.clone());
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
        sprints: body
            .get("sprints")
            .cloned()
            .and_then(|v| serde_json::from_value::<Vec<u32>>(v).ok()),
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
    let mut storage = crate::storage::manager::Storage::new(&resolver.path.clone());
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
        sprints: None,
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
        let mut storage = crate::storage::manager::Storage::new(&resolver.path.clone());
        let body: serde_json::Value = serde_json::from_slice(&req.body).unwrap_or(json!({}));
        let id = match body.get("id").and_then(|v| v.as_str()) {
            Some(s) if !s.is_empty() => s.to_string(),
            _ => return bad_request("Missing id".into()),
        };
        let text = match body.get("text").and_then(|v| v.as_str()) {
            Some(s) if !s.is_empty() => s.to_string(),
            _ => return bad_request("Missing text".into()),
        };
        let dto = match TaskService::add_comment(&mut storage, &id, &text) {
            Ok(dto) => dto,
            Err(err) => {
                let msg = err.to_string();
                if msg.contains("not found") {
                    return not_found(format!("Task '{}' not found", id));
                }
                return internal(json!({
                    "error": { "code": "INTERNAL", "message": msg }
                }));
            }
        };
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
        let mut storage = crate::storage::manager::Storage::new(&resolver.path.clone());
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
        let mut task = match storage.get(&id, &project_prefix) {
            Some(t) => t,
            None => return not_found(format!("Task '{}' not found", id)),
        };
        if index >= task.comments.len() {
            return bad_request("Invalid comment index".into());
        }
        let previous = task.comments[index].text.clone();
        if previous == trimmed {
            let dto = match TaskService::get(&storage, &id, Some(&project_prefix)) {
                Ok(dto) => dto,
                Err(err) => {
                    return internal(
                        json!({"error": {"code": "INTERNAL", "message": err.to_string()}}),
                    );
                }
            };
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
        if let Err(err) = storage.edit(&id, &task) {
            return internal(json!({
                "error": {
                    "code": "INTERNAL",
                    "message": err.to_string(),
                }
            }));
        }
        let dto = match TaskService::get(&storage, &id, Some(&project_prefix)) {
            Ok(dto) => dto,
            Err(err) => {
                return internal(
                    json!({"error": {"code": "INTERNAL", "message": err.to_string()}}),
                );
            }
        };
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
        let mut storage = crate::storage::manager::Storage::new(&resolver.path);
        let body: serde_json::Value = serde_json::from_slice(&req.body).unwrap_or(json!({}));
        let del: crate::cli::TaskDeleteArgs = match serde_json::from_value(body.clone()) {
            Ok(v) => v,
            Err(e) => return bad_request(format!("Invalid body: {}", e)),
        };
        let deleted = match TaskService::delete(
            &mut storage,
            &del.id,
            req.query.get("project").map(|s| s.as_str()),
        ) {
            Ok(value) => value,
            Err(err) => {
                return internal(json!({
                    "error": {
                        "code": "INTERNAL",
                        "message": err.to_string(),
                    }
                }));
            }
        };
        if deleted {
            let actor = crate::utils::identity::resolve_current_user(None);
            crate::api_events::emit_task_deleted(&del.id, actor.as_deref());
        }
        ok_json(200, json!({"data": {"deleted": deleted}}))
    });

    // GET /api/config/show

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
        if let Some(limit_s) = req.query.get("limit")
            && let Ok(limit) = limit_s.parse::<usize>()
            && commits.len() > limit
        {
            commits.truncate(limit);
        }
        ok_json(200, json!({"data": commits}))
    });

    // Activity endpoints
    // GET /api/activity/feed[?since=ISO][&until=ISO][&project=PREFIX][&limit=N]

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

use super::*;

pub(super) fn register(api_server: &mut ApiServer) {
    api_server.register_handler("POST", "/api/tasks/add", |req: &HttpRequest| {
        let resolver = match TasksDirectoryResolver::resolve(None, None) {
            Ok(r) => r,
            Err(e) => return internal(json!({"error": {"code": "INTERNAL", "message": e}})),
        };
        let mut storage = crate::storage::manager::Storage::new(&resolver.path.clone());
        let body: serde_json::Value = serde_json::from_slice(&req.body).unwrap_or(json!({}));
        let req_create = match parse_task_create_body(&body, req) {
            Ok(v) => v,
            Err(msg) => return bad_request(msg),
        };
        match TaskService::create(&mut storage, req_create) {
            Ok(task) => {
                let actor =
                    crate::utils::identity::resolve_current_user(Some(resolver.path.as_path()));
                crate::api_events::emit_task_created(&task, actor.as_deref());
                ok_json(201, json!({"data": task}))
            }
            Err(e) => match e {
                LoTaRError::ValidationError(msg) => bad_request(msg),
                LoTaRError::SprintNotFound(id) => bad_request(format!("Sprint not found: {}", id)),
                other => {
                    internal(json!({"error": {"code":"INTERNAL", "message": other.to_string()}}))
                }
            },
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
    let cfg = match crate::config::resolution::load_and_merge_configs(Some(resolver.path.as_path()))
    {
        Ok(c) => c,
        Err(e) => {
            return internal(
                json!({"error": {"code": "INTERNAL", "message": format!("Failed to load config: {}", e)}}),
            );
        }
    };

    // Build filter + unknown-key map from query
    let (filter, uf) = match parse_task_query(&req.query, &cfg, resolver.path.as_path()) {
        Ok(v) => v,
        Err(msg) => return bad_request(msg),
    };

    let tasks = TaskService::list(&storage, &filter);

    // Apply in-memory filters if any
    let mut tasks = tasks; // shadow mutable
    apply_unknown_key_filters(&mut tasks, &uf, &cfg);

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
    let cfg = match crate::config::resolution::load_and_merge_configs(Some(resolver.path.as_path()))
    {
        Ok(c) => c,
        Err(e) => {
            return internal(
                json!({"error": {"code": "INTERNAL", "message": format!("Failed to load config: {}", e)}}),
            );
        }
    };

    let (filter, uf) = match parse_task_query(&req.query, &cfg, resolver.path.as_path()) {
        Ok(v) => v,
        Err(msg) => return bad_request(msg),
    };
    let mut tasks = TaskService::list(&storage, &filter);
    apply_unknown_key_filters(&mut tasks, &uf, &cfg);

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
        let id = match body.get("id").and_then(|v| v.as_str()) {
            Some(s) if !s.trim().is_empty() => s.trim().to_string(),
            _ => return bad_request("Missing id".into()),
        };
        let patch = match parse_task_update_body(&body) {
            Ok(v) => v,
            Err(msg) => return bad_request(msg),
        };
        match TaskService::update(&mut storage, &id, patch) {
            Ok(task) => {
                let actor =
                    crate::utils::identity::resolve_current_user(Some(resolver.path.as_path()));
                crate::api_events::emit_task_updated(&task, actor.as_deref());
                ok_json(200, json!({"data": task}))
            }
            Err(e) => match e {
                LoTaRError::TaskNotFound(_) => not_found(e.to_string()),
                other => bad_request(other.to_string()),
            },
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
            Some(s) if !s.trim().is_empty() => s.trim().to_string(),
            _ => return bad_request("Missing id".into()),
        };
        let new_status = match body.get("status").and_then(|v| v.as_str()) {
            Some(s) if !s.trim().is_empty() => s.trim().to_string(),
            _ => return bad_request("Missing status".into()),
        };
        // Validation happens in the service against the task's project config.
        let patch = crate::api_types::TaskUpdate {
            status: Some(new_status),
            ..Default::default()
        };
        match TaskService::update(&mut storage, &id, patch) {
            Ok(task) => {
                let actor =
                    crate::utils::identity::resolve_current_user(Some(resolver.path.as_path()));
                crate::api_events::emit_task_updated(&task, actor.as_deref());
                ok_json(200, json!({"data": task}))
            }
            Err(e) => match e {
                LoTaRError::TaskNotFound(_) => not_found(e.to_string()),
                other => bad_request(other.to_string()),
            },
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
        let dto = match TaskService::update_comment(&mut storage, &id, index, trimmed) {
            Ok(dto) => dto,
            Err(err) => {
                let msg = err.to_string();
                if msg.contains("not found") {
                    return not_found(format!("Task '{}' not found", id));
                }
                if msg.contains("Invalid comment index") {
                    return bad_request("Invalid comment index".into());
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

fn parse_string_field(body: &serde_json::Value, key: &str) -> Result<Option<String>, String> {
    match body.get(key) {
        None | Some(serde_json::Value::Null) => Ok(None),
        Some(serde_json::Value::String(s)) => Ok(Some(s.clone())),
        Some(_) => Err(format!("Invalid {} payload", key)),
    }
}

fn parse_clearable_string_field(
    body: &serde_json::Value,
    keys: &[&str],
) -> Result<Option<String>, String> {
    for key in keys {
        match body.get(*key) {
            None => continue,
            Some(serde_json::Value::Null) => return Ok(Some(String::new())),
            Some(serde_json::Value::String(s)) => return Ok(Some(s.clone())),
            Some(_) => return Err(format!("Invalid {} payload", key)),
        }
    }
    Ok(None)
}

fn parse_string_list_field(
    body: &serde_json::Value,
    key: &str,
) -> Result<Option<Vec<String>>, String> {
    match body.get(key) {
        None => Ok(None),
        Some(serde_json::Value::Null) => Ok(Some(Vec::new())),
        Some(serde_json::Value::Array(items)) => {
            let mut collected = Vec::with_capacity(items.len());
            for item in items {
                match item.as_str() {
                    Some(s) => collected.push(s.to_string()),
                    None => return Err(format!("Invalid {} payload", key)),
                }
            }
            Ok(Some(collected))
        }
        Some(_) => Err(format!("Invalid {} payload", key)),
    }
}

fn parse_sprints_field(body: &serde_json::Value) -> Result<Option<Vec<u32>>, String> {
    match body.get("sprints") {
        None => Ok(None),
        Some(serde_json::Value::Null) => Ok(Some(Vec::new())),
        Some(serde_json::Value::Array(items)) => {
            let mut collected = Vec::with_capacity(items.len());
            for item in items {
                let id = item
                    .as_u64()
                    .and_then(|v| u32::try_from(v).ok())
                    .ok_or_else(|| "sprints must be an array of positive integers".to_string())?;
                if id == 0 {
                    return Err("sprints must be an array of positive integers".to_string());
                }
                collected.push(id);
            }
            Ok(Some(collected))
        }
        Some(_) => Err("sprints must be an array of positive integers".to_string()),
    }
}

fn parse_relationships_field(
    body: &serde_json::Value,
    key: &str,
) -> Result<Option<crate::types::TaskRelationships>, String> {
    match body.get(key) {
        None => Ok(None),
        Some(serde_json::Value::Null) => Ok(Some(crate::types::TaskRelationships::default())),
        Some(value) => {
            match serde_json::from_value::<crate::types::TaskRelationships>(value.clone()) {
                Ok(rel) => Ok(Some(rel)),
                Err(e) => Err(format!("Invalid relationships payload: {}", e)),
            }
        }
    }
}

fn parse_custom_fields_body(
    body: &serde_json::Value,
) -> Result<Option<crate::types::CustomFields>, String> {
    let mut merged: Option<crate::types::CustomFields> = None;

    if let Some(fields) = body.get("fields")
        && !fields.is_null()
    {
        let entries = parse_legacy_fields(fields)?;
        let mut map = crate::types::CustomFields::new();
        for (key, value) in entries {
            map.insert(key, crate::types::custom_value_string(value));
        }
        merged = Some(map);
    }

    match body.get("custom_fields") {
        None | Some(serde_json::Value::Null) => {}
        Some(serde_json::Value::Object(obj)) => {
            let map = merged.unwrap_or_default();
            let mut map = map;
            for (key, value) in obj {
                map.insert(key.clone(), crate::types::custom_value_from_json(value));
            }
            merged = Some(map);
        }
        Some(_) => return Err("custom_fields must be an object".into()),
    }

    Ok(merged)
}

fn parse_legacy_fields(fields: &serde_json::Value) -> Result<Vec<(String, String)>, String> {
    match fields {
        serde_json::Value::Null => Ok(Vec::new()),
        serde_json::Value::Object(map) => Ok(map
            .iter()
            .map(|(k, v)| (k.clone(), v.as_str().unwrap_or(&v.to_string()).to_string()))
            .collect()),
        serde_json::Value::Array(items) => {
            let mut out = Vec::with_capacity(items.len());
            for item in items {
                match item.as_str() {
                    Some(entry) => match entry.split_once('=') {
                        Some((k, v)) => out.push((k.trim().to_string(), v.trim().to_string())),
                        None => {
                            return Err(format!("Invalid key=value entry: {}", entry));
                        }
                    },
                    None => {
                        return Err("fields entries must be key=value strings".to_string());
                    }
                }
            }
            Ok(out)
        }
        _ => Err("fields must be an object or an array of key=value strings".to_string()),
    }
}

fn parse_task_create_body(
    body: &serde_json::Value,
    req: &HttpRequest,
) -> Result<crate::api_types::TaskCreate, String> {
    let title = body
        .get("title")
        .and_then(|v| v.as_str())
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .ok_or_else(|| "Missing required field: title".to_string())?;

    let project = body
        .get("project")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
        .or_else(|| req.query.get("project").cloned());

    Ok(crate::api_types::TaskCreate {
        title,
        project,
        status: parse_string_field(body, "status")?,
        priority: parse_string_field(body, "priority")?,
        task_type: parse_string_field(body, "task_type")?.or(parse_string_field(body, "type")?),
        reporter: parse_string_field(body, "reporter")?,
        assignee: parse_string_field(body, "assignee")?,
        due_date: parse_string_field(body, "due_date")?.or(parse_string_field(body, "due")?),
        effort: parse_string_field(body, "effort")?,
        description: parse_string_field(body, "description")?,
        tags: parse_string_list_field(body, "tags")?.unwrap_or_default(),
        acceptance_criteria: parse_string_list_field(body, "acceptance_criteria")?
            .unwrap_or_default(),
        relationships: match parse_relationships_field(body, "relationships")? {
            None => None,
            Some(rel) if rel.is_empty() => None,
            Some(rel) => Some(rel),
        },
        custom_fields: parse_custom_fields_body(body)?,
        sprints: parse_sprints_field(body)?.unwrap_or_default(),
    })
}

fn parse_task_update_body(
    body: &serde_json::Value,
) -> Result<crate::api_types::TaskUpdate, String> {
    Ok(crate::api_types::TaskUpdate {
        title: parse_string_field(body, "title")?,
        status: parse_string_field(body, "status")?,
        priority: parse_string_field(body, "priority")?,
        task_type: parse_string_field(body, "task_type")?.or(parse_string_field(body, "type")?),
        reporter: parse_clearable_string_field(body, &["reporter"])?,
        assignee: parse_clearable_string_field(body, &["assignee"])?,
        due_date: parse_clearable_string_field(body, &["due_date", "due"])?,
        effort: parse_clearable_string_field(body, &["effort"])?,
        description: parse_clearable_string_field(body, &["description"])?,
        tags: parse_string_list_field(body, "tags")?,
        acceptance_criteria: parse_string_list_field(body, "acceptance_criteria")?,
        relationships: parse_relationships_field(body, "relationships")?,
        custom_fields: match body.get("custom_fields") {
            None => parse_custom_fields_body(body)?,
            Some(serde_json::Value::Null) => Some(crate::types::CustomFields::new()),
            Some(serde_json::Value::Object(obj)) => {
                let mut map = crate::types::CustomFields::new();
                for (key, value) in obj {
                    map.insert(key.clone(), crate::types::custom_value_from_json(value));
                }
                Some(map)
            }
            Some(_) => return Err("custom_fields must be an object or null".into()),
        },
        sprints: parse_sprints_field(body)?,
    })
}

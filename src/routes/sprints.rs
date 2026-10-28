use super::*;

pub(super) fn register(api_server: &mut ApiServer) {
    api_server.register_handler("GET", "/api/sprints/backlog/order", |_req: &HttpRequest| {
        let resolver = try_response!(resolve_tasks_dir());
        let storage = crate::storage::manager::Storage {
            root_path: resolver.path,
        };
        match crate::services::task_order::get_order(&storage, None) {
            Ok(response) => ok_json(200, json!({"data": response})),
            Err(error) => map_lotar_error(error),
        }
    });
    api_server.register_handler("POST", "/api/sprints/order", |req: &HttpRequest| {
        let body: crate::api_types::TaskOrderRequest = try_response!(parse_body(req));
        let resolver = try_response!(resolve_tasks_dir());
        let storage = crate::storage::manager::Storage {
            root_path: resolver.path,
        };
        match crate::services::task_order::reorder(&storage, &body) {
            Ok(response) => ok_json(200, json!({"data": response})),
            Err(error) => map_lotar_error(error),
        }
    });
    api_server.register_handler("GET", "/api/sprints/list", |req: &HttpRequest| {
        let resolver = try_response!(resolve_tasks_dir());

        let page = match crate::utils::pagination::parse_page(&req.query, 200, 500) {
            Ok(v) => v,
            Err(msg) => return bad_request(msg),
        };

        let storage = match crate::storage::manager::Storage::try_open(&resolver.path.clone()) {
            Some(storage) => storage,
            None => {
                let payload = SprintListResponse {
                    status: "ok".to_string(),
                    total: 0,
                    count: 0,
                    limit: page.limit,
                    offset: page.offset,
                    sprints: Vec::new(),
                    missing_sprints: Vec::new(),
                    integrity: None,
                };
                return ok_json(200, json!({"data": payload}));
            }
        };
        let records = match SprintService::list(&storage) {
            Ok(records) => records,
            Err(err) => {
                return internal_error(format!("Failed to load sprints: {}", err));
            }
        };

        let now = Utc::now();
        let mut sprints: Vec<SprintListItem> = records
            .iter()
            .map(|record| sprint_record_to_list_item(record, now))
            .collect();

        sprints.sort_by_key(|a| a.id);
        let total = sprints.len();
        let (start, end) = crate::utils::pagination::slice_bounds(total, page.offset, page.limit);
        let page_sprints = sprints[start..end].to_vec();

        let integrity = sprint_integrity::IntegrityCheck::unchanged(
            sprint_integrity::detect_missing_sprints(&storage, &records),
        );
        let payload = SprintListResponse {
            status: "ok".to_string(),
            total,
            count: page_sprints.len(),
            limit: page.limit,
            offset: page.offset,
            sprints: page_sprints,
            missing_sprints: integrity.after.missing_sprints.clone(),
            integrity: integrity.diagnostics(),
        };

        ok_json(200, json!({"data": payload}))
    });

    // POST /api/sprints/create

    api_server.register_handler("POST", "/api/sprints/create", |req: &HttpRequest| {
        let resolver = try_response!(resolve_tasks_dir());

        let mut storage = crate::storage::manager::Storage::new(&resolver.path);

        let body: SprintCreateRequest = try_response!(parse_body(req));

        let resolved_config = try_response!(load_merged_config(&resolver));

        let sprint = sprint_from_create_request(&body);
        let defaults = if body.skip_defaults {
            None
        } else {
            Some(&resolved_config.sprint_defaults)
        };

        let outcome = match SprintService::create(&mut storage, sprint, defaults) {
            Ok(outcome) => outcome,
            // Typed classification (DEV-58): storage faults are 500s.
            Err(err) => return map_lotar_error(err),
        };

        let response = SprintCreateResponse {
            status: "ok".to_string(),
            sprint: sprint_record_to_list_item(&outcome.record, Utc::now()),
            warnings: outcome
                .warnings
                .iter()
                .map(|warning| warning.message().to_string())
                .collect(),
            applied_defaults: outcome.applied_defaults.clone(),
        };

        ok_json(200, json!({"data": response}))
    });

    // POST /api/sprints/add

    api_server.register_handler("POST", "/api/sprints/add", |req: &HttpRequest| {
        let resolver = try_response!(resolve_tasks_dir());
        let mut storage = crate::storage::manager::Storage::new(&resolver.path);
        let mut records = match SprintService::list(&storage) {
            Ok(records) => records,
            Err(err) => {
                return internal_error(format!("Failed to load sprints: {}", err));
            }
        };

        let body: SprintAssignmentRequest = try_response!(parse_body(req));

        let integrity = try_response!(sprint_integrity_check(
            &mut storage,
            &mut records,
            body.cleanup_missing,
            None
        ));

        let sprint_reference = body.sprint.as_ref().map(|selector| selector.as_reference());
        let tasks = try_response!(assignment_tasks(&body, &storage, &resolver));
        let outcome = match sprint_assignment::assign_tasks(
            &mut storage,
            &mut records,
            &tasks,
            sprint_reference.as_deref(),
            body.allow_closed,
            body.force_single,
        ) {
            Ok(outcome) => outcome,
            Err(msg) => return bad_request(msg),
        };

        let messages: Vec<String> = outcome
            .replaced
            .iter()
            .filter_map(|info| info.describe())
            .collect();
        let replaced_payload: Vec<crate::api_types::SprintReassignment> = outcome
            .replaced
            .iter()
            .map(|info| crate::api_types::SprintReassignment {
                task_id: info.task_id.clone(),
                previous: info.previous.clone(),
            })
            .collect();

        let response = SprintAssignmentResponse {
            status: "ok".to_string(),
            action: outcome.action.as_str().to_string(),
            sprint_id: outcome.sprint_id,
            sprint_label: outcome.sprint_label,
            modified: outcome.modified,
            unchanged: outcome.unchanged,
            replaced: replaced_payload,
            messages,
            integrity: integrity.diagnostics(),
        };
        ok_json(200, json!({"data": response}))
    });

    // POST /api/sprints/remove

    api_server.register_handler("POST", "/api/sprints/remove", |req: &HttpRequest| {
        let resolver = try_response!(resolve_tasks_dir());
        let mut storage = crate::storage::manager::Storage::new(&resolver.path);
        let mut records = match SprintService::list(&storage) {
            Ok(records) => records,
            Err(err) => {
                return internal_error(format!("Failed to load sprints: {}", err));
            }
        };

        let body: SprintAssignmentRequest = try_response!(parse_body(req));

        let integrity = try_response!(sprint_integrity_check(
            &mut storage,
            &mut records,
            body.cleanup_missing,
            None
        ));

        let sprint_reference = body.sprint.as_ref().map(|selector| selector.as_reference());
        let tasks = try_response!(assignment_tasks(&body, &storage, &resolver));
        let outcome = match sprint_assignment::remove_tasks(
            &mut storage,
            &mut records,
            &tasks,
            sprint_reference.as_deref(),
        ) {
            Ok(outcome) => outcome,
            Err(msg) => return bad_request(msg),
        };

        let messages: Vec<String> = outcome
            .replaced
            .iter()
            .filter_map(|info| info.describe())
            .collect();
        let replaced_payload: Vec<crate::api_types::SprintReassignment> = outcome
            .replaced
            .iter()
            .map(|info| crate::api_types::SprintReassignment {
                task_id: info.task_id.clone(),
                previous: info.previous.clone(),
            })
            .collect();

        let response = SprintAssignmentResponse {
            status: "ok".to_string(),
            action: outcome.action.as_str().to_string(),
            sprint_id: outcome.sprint_id,
            sprint_label: outcome.sprint_label,
            modified: outcome.modified,
            unchanged: outcome.unchanged,
            replaced: replaced_payload,
            messages,
            integrity: integrity.diagnostics(),
        };
        ok_json(200, json!({"data": response}))
    });

    // POST /api/sprints/delete

    api_server.register_handler("POST", "/api/sprints/delete", |req: &HttpRequest| {
        let resolver = try_response!(resolve_tasks_dir());
        let mut storage = crate::storage::manager::Storage::new(&resolver.path);

        let body: SprintDeleteRequest = try_response!(parse_body(req));

        let sprint_id = body.sprint;
        let existing = match SprintService::get(&storage, sprint_id) {
            Ok(record) => record,
            // Typed classification (DEV-58): unknown sprints are 404s;
            // storage faults are 500s.
            Err(err) => return map_lotar_error(err),
        };

        match SprintService::delete(&mut storage, sprint_id) {
            Ok(true) => {}
            Ok(false) => return not_found(format!("Sprint not found: {sprint_id}")),
            Err(err) => return map_lotar_error(err),
        }

        let mut records = match SprintService::list(&storage) {
            Ok(records) => records,
            Err(err) => {
                return internal_error(format!("Failed to reload sprints: {}", err));
            }
        };

        let integrity = try_response!(sprint_integrity_check(
            &mut storage,
            &mut records,
            body.cleanup_missing,
            Some(sprint_id)
        ));

        let response = SprintDeleteResponse {
            status: "ok".to_string(),
            deleted: true,
            sprint_id,
            sprint_label: existing
                .sprint
                .plan
                .as_ref()
                .and_then(|plan| plan.label.clone()),
            removed_references: integrity
                .cleanup
                .as_ref()
                .map(|outcome| outcome.removed_references)
                .unwrap_or(0),
            updated_tasks: integrity
                .cleanup
                .as_ref()
                .map(|outcome| outcome.updated_tasks)
                .unwrap_or(0),
            integrity: integrity.diagnostics(),
        };
        ok_json(200, json!({"data": response}))
    });

    // GET /api/sprints/backlog

    api_server.register_handler("GET", "/api/sprints/backlog", |req: &HttpRequest| {
        let resolver = try_response!(resolve_tasks_dir());

        let mut storage = match crate::storage::manager::Storage::try_open(&resolver.path.clone()) {
            Some(storage) => storage,
            None => {
                let payload = SprintBacklogResponse {
                    status: "ok".to_string(),
                    count: 0,
                    truncated: false,
                    tasks: Vec::new(),
                    missing_sprints: Vec::new(),
                    integrity: None,
                };
                return ok_json(200, json!({"data": payload}));
            }
        };

        let limit = req
            .query
            .get("limit")
            .and_then(|value| value.parse::<usize>().ok())
            .unwrap_or(20);
        if limit == 0 {
            return bad_request("--limit must be greater than zero".to_string());
        }

        let cleanup_missing = req
            .query
            .get("cleanup_missing")
            .map(|value| {
                let lowered = value.to_ascii_lowercase();
                matches!(lowered.as_str(), "1" | "true" | "yes")
            })
            .unwrap_or(false);

        let mut records = match SprintService::list(&storage) {
            Ok(records) => records,
            Err(err) => {
                return internal_error(format!("Failed to load sprints: {}", err));
            }
        };

        let integrity = try_response!(sprint_integrity_check(
            &mut storage,
            &mut records,
            cleanup_missing,
            None
        ));

        // The shared task query (DEV-236): the /api/tasks/list parameters
        // (including query); `tag` is accepted for `tags`.
        let mut params = req.query.clone();
        for transport in ["limit", "cleanup_missing", "cursor", "offset"] {
            params.remove(transport);
        }
        if let Some(tag) = params.remove("tag") {
            params.entry("tags".to_string()).or_insert(tag);
        }
        let cfg = try_response!(load_merged_config(&resolver));
        let input = match crate::services::task_list::TaskQueryInput::from_params(&params) {
            Ok(input) => input,
            Err(e) => return bad_request(e.to_string()),
        };
        let manual_order = input.sort_by.is_none();
        let options = sprint_assignment::SprintBacklogOptions {
            query: match input.resolve(&cfg, resolver.path.as_path()) {
                Ok(query) => query,
                Err(e) => return bad_request(e.to_string()),
            },
            manual_order,
            limit,
        };

        let result = match sprint_assignment::fetch_backlog(&storage, &options) {
            Ok(result) => result,
            Err(msg) => return bad_request(msg),
        };

        let tasks: Vec<SprintBacklogItem> = result
            .entries
            .into_iter()
            .map(|entry| SprintBacklogItem {
                id: entry.id,
                title: entry.title,
                status: entry.status,
                priority: entry.priority,
                assignee: entry.assignee,
                due_date: entry.due_date,
                tags: entry.tags,
            })
            .collect();

        let payload = SprintBacklogResponse {
            status: "ok".to_string(),
            count: tasks.len(),
            truncated: result.truncated,
            tasks,
            missing_sprints: integrity.after.missing_sprints.clone(),
            integrity: integrity.diagnostics(),
        };

        ok_json(200, json!({"data": payload}))
    });

    // GET /api/sprints/summary

    api_server.register_handler("GET", "/api/sprints/summary", |req: &HttpRequest| {
        let sprint_id = match req
            .query
            .get("sprint")
            .and_then(|value| value.parse::<u32>().ok())
        {
            Some(id) if id > 0 => id,
            _ => {
                return bad_request(
                    "Query parameter 'sprint' must be a positive integer".to_string(),
                );
            }
        };

        let resolver = try_response!(resolve_tasks_dir());

        let storage = crate::storage::manager::Storage::new(&resolver.path);

        let record = match SprintService::get(&storage, sprint_id) {
            Ok(record) => record,
            // Typed classification (DEV-58): unknown sprints are 404s;
            // storage faults are 500s.
            Err(err) => return map_lotar_error(err),
        };

        let resolved_config = try_response!(load_merged_config(&resolver));

        let summary = compute_sprint_summary(&storage, &record, &resolved_config, Utc::now());
        ok_json(200, json!({"data": summary.payload}))
    });

    // GET /api/sprints/burndown

    api_server.register_handler("GET", "/api/sprints/burndown", |req: &HttpRequest| {
        let sprint_id = match req
            .query
            .get("sprint")
            .and_then(|value| value.parse::<u32>().ok())
        {
            Some(id) if id > 0 => id,
            _ => {
                return bad_request(
                    "Query parameter 'sprint' must be a positive integer".to_string(),
                );
            }
        };

        let resolver = try_response!(resolve_tasks_dir());

        let storage = crate::storage::manager::Storage::new(&resolver.path);

        let record = match SprintService::get(&storage, sprint_id) {
            Ok(record) => record,
            // Typed classification (DEV-58): unknown sprints are 404s;
            // storage faults are 500s.
            Err(err) => return map_lotar_error(err),
        };

        let resolved_config = try_response!(load_merged_config(&resolver));

        let context = match compute_sprint_burndown(&storage, &record, &resolved_config, Utc::now())
        {
            Ok(ctx) => ctx,
            Err(msg) => return bad_request(msg),
        };

        ok_json(200, json!({"data": context.payload}))
    });

    // POST /api/sprints/update

    api_server.register_handler("POST", "/api/sprints/update", |req: &HttpRequest| {
        let resolver = try_response!(resolve_tasks_dir());

        let mut storage = crate::storage::manager::Storage::new(&resolver.path);

        let body: SprintUpdateRequest = try_response!(parse_body(req));

        if body.sprint == 0 {
            return bad_request("Sprint identifier must be provided".to_string());
        }

        let patch = sprint_patch_from_request(&body);

        let outcome = match SprintService::update_with_patch(&mut storage, body.sprint, &patch) {
            Ok(outcome) => outcome,
            // Typed classification (DEV-58): unknown sprints are 404s;
            // storage faults are 500s.
            Err(err) => return map_lotar_error(err),
        };

        let payload = SprintUpdateResponse {
            status: "ok".to_string(),
            sprint: sprint_record_to_list_item(&outcome.record, Utc::now()),
            warnings: outcome
                .warnings
                .iter()
                .map(|warning| warning.message().to_string())
                .collect(),
        };

        ok_json(200, json!({"data": payload}))
    });

    // GET /api/sprints/velocity

    api_server.register_handler("GET", "/api/sprints/velocity", |req: &HttpRequest| {
        let resolver = try_response!(resolve_tasks_dir());

        let include_active = req
            .query
            .get("include_active")
            .map(|value| {
                let lowered = value.to_ascii_lowercase();
                matches!(lowered.as_str(), "1" | "true" | "yes")
            })
            .unwrap_or(false);

        let limit = req
            .query
            .get("limit")
            .and_then(|value| value.parse::<usize>().ok())
            .unwrap_or(DEFAULT_VELOCITY_WINDOW);
        if limit == 0 {
            return bad_request("--limit must be greater than zero".to_string());
        }

        let metric = match req.query.get("metric") {
            Some(value) => {
                let lowered = value.to_ascii_lowercase();
                match lowered.as_str() {
                    "tasks" => SprintBurndownMetric::Tasks,
                    "points" => SprintBurndownMetric::Points,
                    "hours" => SprintBurndownMetric::Hours,
                    _ => {
                        return bad_request(format!(
                            "Unsupported metric '{}'. Use tasks, points, or hours.",
                            value
                        ));
                    }
                }
            }
            None => SprintBurndownMetric::Points,
        };

        let storage = match crate::storage::manager::Storage::try_open(&resolver.path.clone()) {
            Some(storage) => storage,
            None => {
                let empty = VelocityComputation {
                    metric,
                    entries: Vec::new(),
                    total_matching: 0,
                    truncated: false,
                    skipped_incomplete: false,
                    average_velocity: None,
                    average_completion_ratio: None,
                };
                let payload = empty.to_payload(include_active);
                return ok_json(200, json!({"data": payload}));
            }
        };

        let records = match SprintService::list(&storage) {
            Ok(records) => records,
            Err(err) => {
                return internal_error(format!("Failed to load sprints: {}", err));
            }
        };

        if records.is_empty() {
            let empty = VelocityComputation {
                metric,
                entries: Vec::new(),
                total_matching: 0,
                truncated: false,
                skipped_incomplete: false,
                average_velocity: None,
                average_completion_ratio: None,
            };
            let payload = empty.to_payload(include_active);
            return ok_json(200, json!({"data": payload}));
        }

        let resolved_config = try_response!(load_merged_config(&resolver));

        let options = VelocityOptions {
            limit,
            include_active,
            metric,
        };

        let computation =
            compute_velocity(&storage, &records, &resolved_config, &options, Utc::now());
        let payload = computation.to_payload(include_active);

        ok_json(200, json!({"data": payload}))
    });

    // GET /api/tasks/export -> CSV of tasks using same filters as list
}

/// The tasks a sprint add/remove request names, plus those its `query`
/// selects with the shared task query (DEV-236).
fn assignment_tasks(
    body: &SprintAssignmentRequest,
    storage: &crate::storage::manager::Storage,
    resolver: &TasksDirectoryResolver,
) -> Result<Vec<String>, HttpResponse> {
    let mut tasks = body.tasks.clone();
    if let Some(search) = body.query.as_deref().filter(|q| !q.trim().is_empty()) {
        let cfg = load_merged_config(resolver)?;
        let selected =
            crate::services::task_list::select_ids(storage, &cfg, resolver.path.as_path(), search)
                .map_err(|e| bad_request(e.to_string()))?;
        tasks.extend(selected);
        tasks.sort();
        tasks.dedup();
    }
    Ok(tasks)
}

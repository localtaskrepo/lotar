use crate::LoTaRError;
use crate::api_server::{ApiServer, HttpRequest, HttpResponse};
use crate::config::manager::ConfigManager;
use crate::config::resolution;
use crate::services::sprint_assignment;
use crate::services::sprint_integrity;
use crate::services::sprint_metrics::SprintBurndownMetric;
use crate::services::sprint_reports::{compute_sprint_burndown, compute_sprint_summary};
use crate::services::sprint_velocity::{
    DEFAULT_VELOCITY_WINDOW, VelocityComputation, VelocityOptions, compute_velocity,
};
use crate::services::{
    attachment_service::AttachmentService, config_service::ConfigService,
    project_service::ProjectService, reference_service::ReferenceService,
    sprint_service::SprintService, sync_service::SyncService, task_service::TaskService,
};
use crate::storage::sprint::{Sprint, SprintActual, SprintCapacity, SprintPlan};
use crate::workspace::TasksDirectoryResolver;
use crate::{
    api_types::{
        SprintAssignmentRequest, SprintAssignmentResponse, SprintBacklogItem,
        SprintBacklogResponse, SprintCleanupMetric, SprintCleanupSummary, SprintCreateRequest,
        SprintCreateResponse, SprintDeleteRequest, SprintDeleteResponse,
        SprintIntegrityDiagnostics, SprintListItem, SprintListResponse, SprintUpdateRequest,
        SprintUpdateResponse, SyncRequest, SyncValidateRequest,
    },
    types::TaskStatus,
};
use chrono::Utc;
use serde_json::json;
use std::collections::{BTreeMap, BTreeSet};

fn make_cleanup_summary(outcome: &sprint_integrity::SprintCleanupOutcome) -> SprintCleanupSummary {
    SprintCleanupSummary {
        removed_references: outcome.removed_references,
        updated_tasks: outcome.updated_tasks,
        removed_by_sprint: outcome
            .removed_by_sprint
            .iter()
            .map(|metric| SprintCleanupMetric {
                sprint_id: metric.sprint_id,
                count: metric.count,
            })
            .collect(),
        remaining_missing: outcome.remaining_missing.clone(),
    }
}

fn make_integrity_payload(
    baseline: &sprint_integrity::MissingSprintReport,
    current: &sprint_integrity::MissingSprintReport,
    cleanup: Option<&sprint_integrity::SprintCleanupOutcome>,
) -> Option<SprintIntegrityDiagnostics> {
    if baseline.missing_sprints.is_empty() && cleanup.is_none() {
        return None;
    }

    Some(SprintIntegrityDiagnostics {
        missing_sprints: current.missing_sprints.clone(),
        tasks_with_missing: if baseline.tasks_with_missing > 0 {
            Some(baseline.tasks_with_missing)
        } else {
            None
        },
        auto_cleanup: cleanup.map(make_cleanup_summary),
    })
}

fn sprint_record_to_list_item(
    record: &crate::services::sprint_service::SprintRecord,
    reference: chrono::DateTime<Utc>,
) -> SprintListItem {
    let lifecycle = crate::services::sprint_status::derive_status(&record.sprint, reference);
    let plan = record.sprint.plan.as_ref();
    let capacity = plan.and_then(|plan| plan.capacity.as_ref());
    SprintListItem {
        id: record.id,
        label: record
            .sprint
            .plan
            .as_ref()
            .and_then(|plan| plan.label.clone()),
        display_name: sprint_assignment::sprint_display_name(record),
        created: record.sprint.created.clone(),
        modified: record.sprint.modified.clone(),
        state: lifecycle.state.as_str().to_string(),
        planned_start: lifecycle.planned_start.map(|dt| dt.to_rfc3339()),
        planned_end: lifecycle.planned_end.map(|dt| dt.to_rfc3339()),
        actual_start: lifecycle.actual_start.map(|dt| dt.to_rfc3339()),
        actual_end: lifecycle.actual_end.map(|dt| dt.to_rfc3339()),
        computed_end: lifecycle.computed_end.map(|dt| dt.to_rfc3339()),
        goal: plan.and_then(|plan| plan.goal.clone()),
        plan_length: plan.and_then(|plan| plan.length.clone()),
        overdue_after: plan.and_then(|plan| plan.overdue_after.clone()),
        notes: plan.and_then(|plan| plan.notes.clone()),
        capacity_points: capacity.and_then(|capacity| capacity.points),
        capacity_hours: capacity.and_then(|capacity| capacity.hours),
        warnings: lifecycle
            .warnings
            .iter()
            .map(|warning| warning.message())
            .collect(),
    }
}

fn sprint_from_create_request(payload: &SprintCreateRequest) -> Sprint {
    let mut plan = SprintPlan::default();

    if let Some(label) = clean_opt_string(payload.label.clone()) {
        plan.label = Some(label);
    }
    if let Some(goal) = clean_opt_string(payload.goal.clone()) {
        plan.goal = Some(goal);
    }
    if let Some(length) = clean_opt_string(payload.plan_length.clone()) {
        plan.length = Some(length);
    }
    if let Some(ends_at) = clean_opt_string(payload.ends_at.clone()) {
        plan.ends_at = Some(ends_at);
    }
    if let Some(starts_at) = clean_opt_string(payload.starts_at.clone()) {
        plan.starts_at = Some(starts_at);
    }
    if let Some(points) = payload.capacity_points {
        plan.capacity
            .get_or_insert_with(SprintCapacity::default)
            .points = Some(points);
    }
    if let Some(hours) = payload.capacity_hours {
        plan.capacity
            .get_or_insert_with(SprintCapacity::default)
            .hours = Some(hours);
    }
    if let Some(overdue_after) = clean_opt_string(payload.overdue_after.clone()) {
        plan.overdue_after = Some(overdue_after);
    }
    if let Some(notes) = payload
        .notes
        .clone()
        .filter(|value| !value.trim().is_empty())
    {
        plan.notes = Some(notes);
    }

    let mut sprint = Sprint::default();
    if plan_has_values(&plan) {
        sprint.plan = Some(plan);
    }
    sprint
}

fn apply_update_to_sprint(target: &mut Sprint, payload: &SprintUpdateRequest) {
    if let Some(label) = payload.label.clone() {
        let plan = target.plan.get_or_insert_with(SprintPlan::default);
        plan.label = clean_opt_string(Some(label));
    }
    if let Some(goal) = payload.goal.clone() {
        let plan = target.plan.get_or_insert_with(SprintPlan::default);
        plan.goal = clean_opt_string(Some(goal));
    }
    if let Some(length) = payload.plan_length.clone() {
        let plan = target.plan.get_or_insert_with(SprintPlan::default);
        plan.length = clean_opt_string(Some(length));
    }
    if let Some(ends_at) = payload.ends_at.clone() {
        let plan = target.plan.get_or_insert_with(SprintPlan::default);
        plan.ends_at = clean_opt_string(Some(ends_at));
    }
    if let Some(starts_at) = payload.starts_at.clone() {
        let plan = target.plan.get_or_insert_with(SprintPlan::default);
        plan.starts_at = clean_opt_string(Some(starts_at));
    }
    if let Some(overdue_after) = payload.overdue_after.clone() {
        let plan = target.plan.get_or_insert_with(SprintPlan::default);
        plan.overdue_after = clean_opt_string(Some(overdue_after));
    }
    if let Some(notes) = payload.notes.clone() {
        let plan = target.plan.get_or_insert_with(SprintPlan::default);
        plan.notes = clean_opt_string(Some(notes));
    }
    if let Some(capacity_points) = payload.capacity_points {
        match capacity_points {
            Some(value) => {
                let plan = target.plan.get_or_insert_with(SprintPlan::default);
                plan.capacity
                    .get_or_insert_with(SprintCapacity::default)
                    .points = Some(value);
            }
            None => {
                if let Some(plan) = target.plan.as_mut()
                    && let Some(capacity) = plan.capacity.as_mut()
                {
                    capacity.points = None;
                    if capacity.points.is_none() && capacity.hours.is_none() {
                        plan.capacity = None;
                    }
                }
            }
        }
    }
    if let Some(capacity_hours) = payload.capacity_hours {
        match capacity_hours {
            Some(value) => {
                let plan = target.plan.get_or_insert_with(SprintPlan::default);
                plan.capacity
                    .get_or_insert_with(SprintCapacity::default)
                    .hours = Some(value);
            }
            None => {
                if let Some(plan) = target.plan.as_mut()
                    && let Some(capacity) = plan.capacity.as_mut()
                {
                    capacity.hours = None;
                    if capacity.points.is_none() && capacity.hours.is_none() {
                        plan.capacity = None;
                    }
                }
            }
        }
    }

    if let Some(actual_started_at) = payload.actual_started_at.clone() {
        match actual_started_at {
            Some(value) => {
                let actual = target.actual.get_or_insert_with(SprintActual::default);
                actual.started_at = clean_opt_string(Some(value));
            }
            None => {
                if let Some(actual) = target.actual.as_mut() {
                    actual.started_at = None;
                }
            }
        }
    }

    if let Some(actual_closed_at) = payload.actual_closed_at.clone() {
        match actual_closed_at {
            Some(value) => {
                let actual = target.actual.get_or_insert_with(SprintActual::default);
                actual.closed_at = clean_opt_string(Some(value));
            }
            None => {
                if let Some(actual) = target.actual.as_mut() {
                    actual.closed_at = None;
                }
            }
        }
    }

    let should_clear_actual = matches!(
        target.actual.as_ref(),
        Some(actual) if actual.started_at.is_none() && actual.closed_at.is_none()
    );
    if should_clear_actual {
        target.actual = None;
    }

    if let Some(plan) = target.plan.as_ref()
        && !plan_has_values(plan)
    {
        target.plan = None;
    }
}

fn plan_has_values(plan: &SprintPlan) -> bool {
    plan.label.is_some()
        || plan.goal.is_some()
        || plan.length.is_some()
        || plan.ends_at.is_some()
        || plan.starts_at.is_some()
        || plan.capacity.is_some()
        || plan.overdue_after.is_some()
        || plan.notes.is_some()
}

fn clean_opt_string(input: Option<String>) -> Option<String> {
    input.and_then(|value| {
        let trimmed = value.trim();
        if trimmed.is_empty() {
            None
        } else {
            Some(trimmed.to_string())
        }
    })
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
        let storage = crate::storage::manager::Storage::new(resolver.path.clone());

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
            sprints: vec![],
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
                let v = if a == "@me" {
                    crate::utils::identity::resolve_current_user(Some(resolver.path.as_path()))
                        .unwrap_or_else(|| a.clone())
                } else {
                    a.clone()
                };
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
                        "today" => {
                            if due_date != today {
                                return false;
                            }
                        }
                        "soon" => {
                            if due_date < tomorrow || due_date > soon_cutoff {
                                return false;
                            }
                        }
                        "later" => {
                            if due_date <= soon_cutoff {
                                return false;
                            }
                        }
                        "overdue" => {
                            if due_date >= today {
                                return false;
                            }
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
    api_server.register_handler("GET", "/api/sprints/list", |req: &HttpRequest| {
        let resolver = match TasksDirectoryResolver::resolve(None, None) {
            Ok(r) => r,
            Err(e) => return internal(json!({"error": {"code": "INTERNAL", "message": e}})),
        };

        let page = match crate::utils::pagination::parse_page(&req.query, 200, 500) {
            Ok(v) => v,
            Err(msg) => return bad_request(msg),
        };

        let storage = match crate::storage::manager::Storage::try_open(resolver.path.clone()) {
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
                return internal(json!({
                    "error": {
                        "code": "INTERNAL",
                        "message": format!("Failed to load sprints: {}", err)
                    }
                }));
            }
        };

        let now = Utc::now();
        let mut sprints: Vec<SprintListItem> = records
            .iter()
            .map(|record| sprint_record_to_list_item(record, now))
            .collect();

        sprints.sort_by(|a, b| a.id.cmp(&b.id));
        let total = sprints.len();
        let (start, end) = crate::utils::pagination::slice_bounds(total, page.offset, page.limit);
        let page_sprints = sprints[start..end].to_vec();

        let integrity_report = sprint_integrity::detect_missing_sprints(&storage, &records);
        let payload = SprintListResponse {
            status: "ok".to_string(),
            total,
            count: page_sprints.len(),
            limit: page.limit,
            offset: page.offset,
            sprints: page_sprints,
            missing_sprints: integrity_report.missing_sprints.clone(),
            integrity: make_integrity_payload(&integrity_report, &integrity_report, None),
        };

        ok_json(200, json!({"data": payload}))
    });

    // POST /api/sprints/create
    api_server.register_handler("POST", "/api/sprints/create", |req: &HttpRequest| {
        let resolver = match TasksDirectoryResolver::resolve(None, None) {
            Ok(r) => r,
            Err(e) => return internal(json!({"error": {"code": "INTERNAL", "message": e}})),
        };

        let mut storage = crate::storage::manager::Storage::new(resolver.path.clone());

        let body: SprintCreateRequest = match serde_json::from_slice(&req.body) {
            Ok(payload) => payload,
            Err(err) => return bad_request(format!("Invalid body: {}", err)),
        };

        let resolved_config =
            match resolution::load_and_merge_configs(Some(resolver.path.as_path())) {
                Ok(config) => config,
                Err(err) => {
                    return internal(json!({
                        "error": {
                            "code": "INTERNAL",
                            "message": format!("Failed to load config: {}", err)
                        }
                    }));
                }
            };

        let sprint = sprint_from_create_request(&body);
        let defaults = if body.skip_defaults {
            None
        } else {
            Some(&resolved_config.sprint_defaults)
        };

        let outcome = match SprintService::create(&mut storage, sprint, defaults) {
            Ok(outcome) => outcome,
            Err(err) => {
                return internal(json!({
                    "error": {
                        "code": "INTERNAL",
                        "message": format!("Failed to create sprint: {}", err)
                    }
                }));
            }
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
        let resolver = match TasksDirectoryResolver::resolve(None, None) {
            Ok(r) => r,
            Err(e) => return internal(json!({"error": {"code": "INTERNAL", "message": e}})),
        };
        let mut storage = crate::storage::manager::Storage::new(resolver.path.clone());
        let mut records = match SprintService::list(&storage) {
            Ok(records) => records,
            Err(err) => {
                return internal(json!({
                    "error": {
                        "code": "INTERNAL",
                        "message": format!("Failed to load sprints: {}", err)
                    }
                }));
            }
        };

        let body: SprintAssignmentRequest = match serde_json::from_slice(&req.body) {
            Ok(payload) => payload,
            Err(err) => return bad_request(format!("Invalid body: {}", err)),
        };

        let mut integrity_report = sprint_integrity::detect_missing_sprints(&storage, &records);
        let baseline_report = integrity_report.clone();
        let mut cleanup_outcome: Option<sprint_integrity::SprintCleanupOutcome> = None;

        if body.cleanup_missing && !integrity_report.missing_sprints.is_empty() {
            match sprint_integrity::cleanup_missing_sprint_refs(&mut storage, &mut records, None) {
                Ok(outcome) => {
                    integrity_report = sprint_integrity::detect_missing_sprints(&storage, &records);
                    cleanup_outcome = Some(outcome);
                }
                Err(err) => {
                    return internal(json!({
                        "error": {
                            "code": "INTERNAL",
                            "message": format!("Failed to clean up sprint references: {}", err)
                        }
                    }));
                }
            }
        }

        let sprint_reference = body.sprint.as_ref().map(|selector| selector.as_reference());
        let outcome = match sprint_assignment::assign_tasks(
            &mut storage,
            &records,
            &body.tasks,
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
            integrity: make_integrity_payload(
                &baseline_report,
                &integrity_report,
                cleanup_outcome.as_ref(),
            ),
        };

        ok_json(200, json!({"data": response}))
    });

    // POST /api/sprints/remove
    api_server.register_handler("POST", "/api/sprints/remove", |req: &HttpRequest| {
        let resolver = match TasksDirectoryResolver::resolve(None, None) {
            Ok(r) => r,
            Err(e) => return internal(json!({"error": {"code": "INTERNAL", "message": e}})),
        };
        let mut storage = crate::storage::manager::Storage::new(resolver.path.clone());
        let mut records = match SprintService::list(&storage) {
            Ok(records) => records,
            Err(err) => {
                return internal(json!({
                    "error": {
                        "code": "INTERNAL",
                        "message": format!("Failed to load sprints: {}", err)
                    }
                }));
            }
        };

        let body: SprintAssignmentRequest = match serde_json::from_slice(&req.body) {
            Ok(payload) => payload,
            Err(err) => return bad_request(format!("Invalid body: {}", err)),
        };

        let mut integrity_report = sprint_integrity::detect_missing_sprints(&storage, &records);
        let baseline_report = integrity_report.clone();
        let mut cleanup_outcome: Option<sprint_integrity::SprintCleanupOutcome> = None;

        if body.cleanup_missing && !integrity_report.missing_sprints.is_empty() {
            match sprint_integrity::cleanup_missing_sprint_refs(&mut storage, &mut records, None) {
                Ok(outcome) => {
                    integrity_report = sprint_integrity::detect_missing_sprints(&storage, &records);
                    cleanup_outcome = Some(outcome);
                }
                Err(err) => {
                    return internal(json!({
                        "error": {
                            "code": "INTERNAL",
                            "message": format!("Failed to clean up sprint references: {}", err)
                        }
                    }));
                }
            }
        }

        let sprint_reference = body.sprint.as_ref().map(|selector| selector.as_reference());
        let outcome = match sprint_assignment::remove_tasks(
            &mut storage,
            &records,
            &body.tasks,
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
            integrity: make_integrity_payload(
                &baseline_report,
                &integrity_report,
                cleanup_outcome.as_ref(),
            ),
        };

        ok_json(200, json!({"data": response}))
    });

    // POST /api/sprints/delete
    api_server.register_handler("POST", "/api/sprints/delete", |req: &HttpRequest| {
        let resolver = match TasksDirectoryResolver::resolve(None, None) {
            Ok(r) => r,
            Err(e) => return internal(json!({"error": {"code": "INTERNAL", "message": e}})),
        };
        let mut storage = crate::storage::manager::Storage::new(resolver.path.clone());

        let body: SprintDeleteRequest = match serde_json::from_slice(&req.body) {
            Ok(payload) => payload,
            Err(err) => return bad_request(format!("Invalid body: {}", err)),
        };

        let sprint_id = body.sprint;
        let existing = match SprintService::get(&storage, sprint_id) {
            Ok(record) => record,
            Err(LoTaRError::SprintNotFound(_)) => {
                return not_found(format!("Sprint #{} not found", sprint_id));
            }
            Err(err) => {
                return internal(json!({
                    "error": {
                        "code": "INTERNAL",
                        "message": format!("Failed to load sprint: {}", err)
                    }
                }));
            }
        };

        match SprintService::delete(&mut storage, sprint_id) {
            Ok(true) => {}
            Ok(false) => return not_found(format!("Sprint #{} not found", sprint_id)),
            Err(err) => {
                return internal(json!({
                    "error": {
                        "code": "INTERNAL",
                        "message": format!("Failed to delete sprint: {}", err)
                    }
                }));
            }
        }

        let mut records = match SprintService::list(&storage) {
            Ok(records) => records,
            Err(err) => {
                return internal(json!({
                    "error": {
                        "code": "INTERNAL",
                        "message": format!("Failed to reload sprints: {}", err)
                    }
                }));
            }
        };

        let mut integrity_report = sprint_integrity::detect_missing_sprints(&storage, &records);
        let baseline_report = integrity_report.clone();
        let mut cleanup_outcome: Option<sprint_integrity::SprintCleanupOutcome> = None;

        if body.cleanup_missing {
            match sprint_integrity::cleanup_missing_sprint_refs(
                &mut storage,
                &mut records,
                Some(sprint_id),
            ) {
                Ok(outcome) => {
                    integrity_report = sprint_integrity::detect_missing_sprints(&storage, &records);
                    cleanup_outcome = Some(outcome);
                }
                Err(err) => {
                    return internal(json!({
                        "error": {
                            "code": "INTERNAL",
                            "message": format!(
                                "Failed to clean up sprint references: {}",
                                err
                            )
                        }
                    }));
                }
            }
        }

        let response = SprintDeleteResponse {
            status: "ok".to_string(),
            deleted: true,
            sprint_id,
            sprint_label: existing
                .sprint
                .plan
                .as_ref()
                .and_then(|plan| plan.label.clone()),
            removed_references: cleanup_outcome
                .as_ref()
                .map(|outcome| outcome.removed_references)
                .unwrap_or(0),
            updated_tasks: cleanup_outcome
                .as_ref()
                .map(|outcome| outcome.updated_tasks)
                .unwrap_or(0),
            integrity: make_integrity_payload(
                &baseline_report,
                &integrity_report,
                cleanup_outcome.as_ref(),
            ),
        };

        ok_json(200, json!({"data": response}))
    });

    // GET /api/sprints/backlog
    api_server.register_handler("GET", "/api/sprints/backlog", |req: &HttpRequest| {
        let resolver = match TasksDirectoryResolver::resolve(None, None) {
            Ok(r) => r,
            Err(e) => return internal(json!({"error": {"code": "INTERNAL", "message": e}})),
        };

        let mut storage = match crate::storage::manager::Storage::try_open(resolver.path.clone()) {
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

        let statuses: Vec<TaskStatus> = req
            .query
            .get("status")
            .map(|value| {
                value
                    .split(',')
                    .map(|token| token.trim())
                    .filter(|token| !token.is_empty())
                    .map(TaskStatus::from)
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();

        let tags: Vec<String> = req
            .query
            .get("tag")
            .or_else(|| req.query.get("tags"))
            .map(|value| {
                value
                    .split(',')
                    .map(|token| token.trim())
                    .filter(|token| !token.is_empty())
                    .map(|token| token.to_string())
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();

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
                return internal(json!({
                    "error": {
                        "code": "INTERNAL",
                        "message": format!("Failed to load sprints: {}", err)
                    }
                }));
            }
        };

        let mut integrity_report = sprint_integrity::detect_missing_sprints(&storage, &records);
        let baseline_report = integrity_report.clone();
        let mut cleanup_outcome: Option<sprint_integrity::SprintCleanupOutcome> = None;

        if cleanup_missing && !integrity_report.missing_sprints.is_empty() {
            match sprint_integrity::cleanup_missing_sprint_refs(&mut storage, &mut records, None) {
                Ok(outcome) => {
                    integrity_report = sprint_integrity::detect_missing_sprints(&storage, &records);
                    cleanup_outcome = Some(outcome);
                }
                Err(err) => {
                    return internal(json!({
                        "error": {
                            "code": "INTERNAL",
                            "message": format!("Failed to clean up sprint references: {}", err)
                        }
                    }));
                }
            }
        }

        let options = sprint_assignment::SprintBacklogOptions {
            project: req.query.get("project").cloned(),
            tags,
            statuses,
            assignee: req.query.get("assignee").cloned(),
            limit,
        };

        let result = match sprint_assignment::fetch_backlog(&storage, options) {
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
            missing_sprints: integrity_report.missing_sprints.clone(),
            integrity: make_integrity_payload(
                &baseline_report,
                &integrity_report,
                cleanup_outcome.as_ref(),
            ),
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

        let resolver = match TasksDirectoryResolver::resolve(None, None) {
            Ok(r) => r,
            Err(e) => {
                return internal(json!({
                    "error": {
                        "code": "INTERNAL",
                        "message": e,
                    }
                }));
            }
        };

        let storage = crate::storage::manager::Storage::new(resolver.path.clone());

        let record = match SprintService::get(&storage, sprint_id) {
            Ok(record) => record,
            Err(err) => {
                return match err {
                    LoTaRError::SprintNotFound(_) => {
                        bad_request(format!("Sprint {} not found", sprint_id))
                    }
                    other => internal(json!({
                        "error": {
                            "code": "INTERNAL",
                            "message": format!("Failed to load sprint: {}", other),
                        }
                    })),
                };
            }
        };

        let resolved_config =
            match resolution::load_and_merge_configs(Some(resolver.path.as_path())) {
                Ok(cfg) => cfg,
                Err(err) => {
                    return internal(json!({
                        "error": {
                            "code": "INTERNAL",
                            "message": format!("Failed to load config: {}", err),
                        }
                    }));
                }
            };

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

        let resolver = match TasksDirectoryResolver::resolve(None, None) {
            Ok(r) => r,
            Err(e) => {
                return internal(json!({
                    "error": {
                        "code": "INTERNAL",
                        "message": e,
                    }
                }));
            }
        };

        let storage = crate::storage::manager::Storage::new(resolver.path.clone());

        let record = match SprintService::get(&storage, sprint_id) {
            Ok(record) => record,
            Err(err) => {
                return match err {
                    LoTaRError::SprintNotFound(_) => {
                        bad_request(format!("Sprint {} not found", sprint_id))
                    }
                    other => internal(json!({
                        "error": {
                            "code": "INTERNAL",
                            "message": format!("Failed to load sprint: {}", other),
                        }
                    })),
                };
            }
        };

        let resolved_config =
            match resolution::load_and_merge_configs(Some(resolver.path.as_path())) {
                Ok(cfg) => cfg,
                Err(err) => {
                    return internal(json!({
                        "error": {
                            "code": "INTERNAL",
                            "message": format!("Failed to load config: {}", err),
                        }
                    }));
                }
            };

        let context = match compute_sprint_burndown(&storage, &record, &resolved_config, Utc::now())
        {
            Ok(ctx) => ctx,
            Err(msg) => return bad_request(msg),
        };

        ok_json(200, json!({"data": context.payload}))
    });

    // POST /api/sprints/update
    api_server.register_handler("POST", "/api/sprints/update", |req: &HttpRequest| {
        let resolver = match TasksDirectoryResolver::resolve(None, None) {
            Ok(r) => r,
            Err(e) => return internal(json!({"error": {"code": "INTERNAL", "message": e}})),
        };

        let mut storage = crate::storage::manager::Storage::new(resolver.path.clone());

        let body: SprintUpdateRequest = match serde_json::from_slice(&req.body) {
            Ok(payload) => payload,
            Err(err) => return bad_request(format!("Invalid body: {}", err)),
        };

        if body.sprint == 0 {
            return bad_request("Sprint identifier must be provided".to_string());
        }

        let existing = match SprintService::get(&storage, body.sprint) {
            Ok(record) => record,
            Err(err) => {
                return match err {
                    LoTaRError::SprintNotFound(_) => {
                        bad_request(format!("Sprint {} not found", body.sprint))
                    }
                    other => internal(json!({
                        "error": {
                            "code": "INTERNAL",
                            "message": format!("Failed to load sprint: {}", other)
                        }
                    })),
                };
            }
        };

        let mut sprint = existing.sprint.clone();
        apply_update_to_sprint(&mut sprint, &body);

        let outcome = match SprintService::update(&mut storage, body.sprint, sprint) {
            Ok(outcome) => outcome,
            Err(err) => {
                return internal(json!({
                    "error": {
                        "code": "INTERNAL",
                        "message": format!("Failed to update sprint: {}", err)
                    }
                }));
            }
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
        let resolver = match TasksDirectoryResolver::resolve(None, None) {
            Ok(r) => r,
            Err(e) => return internal(json!({"error": {"code": "INTERNAL", "message": e}})),
        };

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

        let storage = match crate::storage::manager::Storage::try_open(resolver.path.clone()) {
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
                return internal(json!({
                    "error": {
                        "code": "INTERNAL",
                        "message": format!("Failed to load sprints: {}", err)
                    }
                }));
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

        let resolved_config =
            match resolution::load_and_merge_configs(Some(resolver.path.as_path())) {
                Ok(config) => config,
                Err(err) => {
                    return internal(json!({
                        "error": {
                            "code": "INTERNAL",
                            "message": format!("Failed to load config: {}", err)
                        }
                    }));
                }
            };

        let options = VelocityOptions {
            limit,
            include_active,
            metric,
        };

        let computation =
            compute_velocity(&storage, records, &resolved_config, options, Utc::now());
        let payload = computation.to_payload(include_active);

        ok_json(200, json!({"data": payload}))
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
        let storage = crate::storage::manager::Storage::new(resolver.path);
        match TaskService::get(&storage, &id, req.query.get("project").map(|s| s.as_str())) {
            Ok(task) => ok_json(200, json!({"data": task})),
            Err(e) => match e {
                LoTaRError::TaskNotFound(_) => not_found(e.to_string()),
                _ => bad_request(e.to_string()),
            },
        }
    });

    // GET /api/references/snippet?code=<path#x>
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

    // GET /api/references/files?q=TEXT[&limit=N]
    api_server.register_handler("GET", "/api/references/files", |req: &HttpRequest| {
        let q = req.query.get("q").cloned().unwrap_or_default();
        let q = q.trim().to_string();
        if q.is_empty() {
            return ok_json(200, json!({"data": Vec::<String>::new()}));
        }

        let limit: usize = req
            .query
            .get("limit")
            .and_then(|s| s.parse::<usize>().ok())
            .unwrap_or(20);
        let limit = limit.clamp(1, 200);

        let resolver = match TasksDirectoryResolver::resolve(None, None) {
            Ok(r) => r,
            Err(e) => return internal(json!({"error": {"code": "INTERNAL", "message": e}})),
        };
        let repo_root = match crate::utils::git::find_repo_root(&resolver.path) {
            Some(root) => root,
            None => return bad_request("Unable to locate git repository".into()),
        };

        let files = ReferenceService::suggest_repo_files(&repo_root, &q, limit);
        ok_json(200, json!({"data": files}))
    });

    // GET /api/attachments/get?path=<relative>
    api_server.register_handler("GET", "/api/attachments/get", |req: &HttpRequest| {
        let rel = match req.query.get("path") {
            Some(v) if !v.trim().is_empty() => v.trim().to_string(),
            _ => return bad_request("Missing attachment path".into()),
        };
        let download = req
            .query
            .get("download")
            .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
            .unwrap_or(false);

        let resolver = match TasksDirectoryResolver::resolve(None, None) {
            Ok(r) => r,
            Err(e) => return internal(json!({"error": {"code": "INTERNAL", "message": e}})),
        };
        let base_config = match resolution::load_and_merge_configs(Some(resolver.path.as_path())) {
            Ok(c) => c,
            Err(e) => return bad_request(format!("Failed to load config: {}", e)),
        };
        let config = if let Some(project) = req.query.get("project").map(|s| s.as_str()) {
            resolution::get_project_config(&base_config, project, resolver.path.as_path())
                .unwrap_or(base_config)
        } else {
            base_config
        };
        let root =
            match AttachmentService::resolve_attachments_root(resolver.path.as_path(), &config) {
                Ok(p) => p,
                Err(e) => return bad_request(e.to_string()),
            };

        let resolved = match AttachmentService::resolve_attachment_path(&root, &rel) {
            Ok(p) => p,
            Err(msg) if msg.contains("not found") => return not_found(msg),
            Err(msg) => return bad_request(msg),
        };

        let bytes = match std::fs::read(&resolved) {
            Ok(b) => b,
            Err(_) => return not_found("Attachment not found".into()),
        };

        let content_type = match resolved
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("")
            .to_ascii_lowercase()
            .as_str()
        {
            "png" => "image/png",
            "jpg" | "jpeg" => "image/jpeg",
            "gif" => "image/gif",
            "webp" => "image/webp",
            "svg" => "image/svg+xml",
            "pdf" => "application/pdf",
            "txt" | "log" | "md" => "text/plain; charset=utf-8",
            _ => "application/octet-stream",
        };

        let stored_filename = resolved
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or("attachment");
        let filename = AttachmentService::download_filename(stored_filename);
        let disposition = if download {
            format!("attachment; filename=\"{}\"", filename)
        } else {
            format!("inline; filename=\"{}\"", filename)
        };

        HttpResponse {
            status: 200,
            headers: vec![
                ("Content-Type".to_string(), content_type.to_string()),
                ("Content-Disposition".to_string(), disposition),
            ],
            body: bytes,
        }
    });

    // GET /api/attachments/h/<hash>/<filename>
    // This is primarily for browser "Save Link As" and copy-link ergonomics.
    api_server.register_prefix_handler("GET", "/api/attachments/h", |req: &HttpRequest| {
        let download = req
            .query
            .get("download")
            .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
            .unwrap_or(false);

        let raw_path = req.path.trim_end_matches('/');
        let prefix = "/api/attachments/h";
        let rest = raw_path
            .get(prefix.len()..)
            .unwrap_or("")
            .trim_start_matches('/');

        let (hash_tag, requested_name) = match rest.split_once('/') {
            Some((h, name)) if !h.trim().is_empty() && !name.trim().is_empty() => (h.trim(), name),
            _ => return bad_request("Missing attachment hash".into()),
        };

        let resolver = match TasksDirectoryResolver::resolve(None, None) {
            Ok(r) => r,
            Err(e) => return internal(json!({"error": {"code": "INTERNAL", "message": e}})),
        };
        let base_config = match resolution::load_and_merge_configs(Some(resolver.path.as_path())) {
            Ok(c) => c,
            Err(e) => return bad_request(format!("Failed to load config: {}", e)),
        };
        let config = if let Some(project) = req.query.get("project").map(|s| s.as_str()) {
            resolution::get_project_config(&base_config, project, resolver.path.as_path())
                .unwrap_or(base_config)
        } else {
            base_config
        };
        let root =
            match AttachmentService::resolve_attachments_root(resolver.path.as_path(), &config) {
                Ok(p) => p,
                Err(e) => return bad_request(e.to_string()),
            };

        let resolved = match AttachmentService::find_attachment_by_hash(&root, hash_tag) {
            Some(p) => p,
            None => return not_found("Attachment not found".into()),
        };

        let bytes = match std::fs::read(&resolved) {
            Ok(b) => b,
            Err(_) => return not_found("Attachment not found".into()),
        };

        let content_type = match resolved
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("")
            .to_ascii_lowercase()
            .as_str()
        {
            "png" => "image/png",
            "jpg" | "jpeg" => "image/jpeg",
            "gif" => "image/gif",
            "webp" => "image/webp",
            "svg" => "image/svg+xml",
            "pdf" => "application/pdf",
            "txt" | "log" | "md" => "text/plain; charset=utf-8",
            _ => "application/octet-stream",
        };

        // For Save Link As, browsers often use the URL path segment.
        // Still send Content-Disposition to help other download flows.
        let requested_leaf = requested_name.split('/').next_back().unwrap_or("").trim();
        let stored_filename = resolved
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or("attachment");
        let computed = AttachmentService::download_filename(stored_filename);
        let filename = if requested_leaf.contains('.') {
            requested_leaf.to_string()
        } else {
            computed
        };

        let disposition = if download {
            format!("attachment; filename=\"{}\"", filename)
        } else {
            format!("inline; filename=\"{}\"", filename)
        };

        HttpResponse {
            status: 200,
            headers: vec![
                ("Content-Type".to_string(), content_type.to_string()),
                ("Content-Disposition".to_string(), disposition),
            ],
            body: bytes,
        }
    });

    // POST /api/tasks/attachments/upload
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

        let mut storage = crate::storage::manager::Storage::new(resolver.path);
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

            let mut storage = crate::storage::manager::Storage::new(resolver.path);
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

            let mut storage = crate::storage::manager::Storage::new(resolver.path);
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

            let mut storage = crate::storage::manager::Storage::new(resolver.path);
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

            let mut storage = crate::storage::manager::Storage::new(resolver.path);
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

            let mut storage = crate::storage::manager::Storage::new(resolver.path);
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

        let mut storage = crate::storage::manager::Storage::new(resolver.path);
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

            let mut storage = crate::storage::manager::Storage::new(resolver.path);
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
        let mut storage = crate::storage::manager::Storage::new(resolver.path);
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

    // POST /api/sync/pull
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
        let storage = crate::storage::manager::Storage::new(resolver.path);
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

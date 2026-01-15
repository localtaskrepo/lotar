use serde_json::{Value, json};
use std::fmt::Write as _;

use super::super::hints::{EnumHints, enum_hints_to_value};
use super::super::{
    JsonRpcRequest, JsonRpcResponse, MCP_DEFAULT_BACKLOG_LIMIT, MCP_DEFAULT_SPRINT_LIST_LIMIT,
    MCP_MAX_BACKLOG_LIMIT, MCP_MAX_CURSOR, MCP_MAX_SPRINT_LIST_LIMIT, err,
    make_mcp_integrity_payload, ok, parse_cursor_value, parse_limit_value,
};
use crate::api_types::{
    SprintCreateRequest, SprintCreateResponse, SprintListItem, SprintUpdateRequest,
    SprintUpdateResponse,
};
use crate::config::manager::ConfigManager;
use crate::config::resolution;
use crate::services::sprint_assignment::{self, SprintBacklogOptions};
use crate::services::sprint_integrity;
use crate::services::sprint_metrics::SprintBurndownMetric;
use crate::services::sprint_reports::{compute_sprint_burndown, compute_sprint_summary};
use crate::services::sprint_service::SprintService;
use crate::services::sprint_status;
use crate::services::sprint_velocity::{
    DEFAULT_VELOCITY_WINDOW, VelocityComputation, VelocityOptions, compute_velocity,
};
use crate::storage::manager::Storage;
use crate::storage::sprint::{Sprint, SprintActual, SprintCapacity, SprintPlan};
use crate::types::TaskStatus;
use crate::workspace::TasksDirectoryResolver;
use chrono::Utc;

fn parse_sprint_reference(params: &Value) -> Result<Option<String>, &'static str> {
    if let Some(raw) = params.get("sprint_id") {
        match raw {
            Value::Null => {}
            Value::Number(num) => {
                if let Some(value) = num.as_u64() {
                    return Ok(Some(value.to_string()));
                }
                return Err("sprint_id must be a positive integer");
            }
            Value::String(text) => {
                let trimmed = text.trim();
                if trimmed.is_empty() {
                    return Ok(None);
                }
                let normalized = trimmed.strip_prefix('#').unwrap_or(trimmed);
                if normalized.parse::<u32>().is_ok() {
                    return Ok(Some(normalized.to_string()));
                }
                return Err("sprint_id must be a positive integer");
            }
            _ => return Err("sprint_id must be a positive integer"),
        }
    }

    match params.get("sprint") {
        None | Some(Value::Null) => Ok(None),
        Some(Value::Number(num)) => Ok(Some(num.to_string())),
        Some(Value::String(text)) => {
            let trimmed = text.trim();
            if trimmed.is_empty() {
                Err("sprint must be a sprint reference like '#1' or keyword when provided")
            } else {
                Ok(Some(trimmed.to_string()))
            }
        }
        _ => Err("sprint must be a sprint reference like '#1' or keyword when provided"),
    }
}

fn parse_sprint_id(params: &Value) -> Result<u32, &'static str> {
    if let Some(raw) = params.get("sprint_id") {
        match raw {
            Value::Null => {}
            Value::Number(num) => {
                if let Some(value) = num.as_u64().and_then(|v| u32::try_from(v).ok())
                    && value > 0
                {
                    return Ok(value);
                }
                return Err("sprint_id must be a positive integer");
            }
            Value::String(text) => {
                let trimmed = text.trim();
                if trimmed.is_empty() {
                    return Err("sprint_id must be a positive integer");
                }
                let normalized = trimmed.strip_prefix('#').unwrap_or(trimmed);
                if let Ok(value) = normalized.parse::<u32>()
                    && value > 0
                {
                    return Ok(value);
                }
                return Err("sprint_id must be a positive integer");
            }
            _ => return Err("sprint_id must be a positive integer"),
        }
    }

    match params.get("sprint") {
        Some(Value::Number(num)) => num
            .as_u64()
            .and_then(|v| u32::try_from(v).ok())
            .filter(|v| *v > 0)
            .ok_or("sprint must be a positive integer"),
        Some(Value::String(text)) => {
            let trimmed = text.trim();
            if trimmed.is_empty() {
                return Err("Missing sprint id");
            }
            trimmed
                .strip_prefix('#')
                .unwrap_or(trimmed)
                .parse::<u32>()
                .ok()
                .filter(|v| *v > 0)
                .ok_or("sprint must be a positive integer")
        }
        _ => Err("Missing sprint id"),
    }
}

fn sprint_record_to_list_item(
    record: &crate::services::sprint_service::SprintRecord,
    reference: chrono::DateTime<Utc>,
) -> SprintListItem {
    let lifecycle = sprint_status::derive_status(&record.sprint, reference);
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
        capacity_points: capacity.and_then(|cap| cap.points),
        capacity_hours: capacity.and_then(|cap| cap.hours),
        warnings: lifecycle
            .warnings
            .iter()
            .map(|warning| warning.message())
            .collect(),
    }
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
    if payload.capacity_points.is_some() || payload.capacity_hours.is_some() {
        plan.capacity = Some(SprintCapacity {
            points: payload.capacity_points,
            hours: payload.capacity_hours,
        });
    }
    if let Some(overdue_after) = clean_opt_string(payload.overdue_after.clone()) {
        plan.overdue_after = Some(overdue_after);
    }
    if let Some(notes) = clean_opt_string(payload.notes.clone()) {
        plan.notes = Some(notes);
    }

    let mut sprint = Sprint::default();
    if !plan_has_values(&plan) {
        sprint.plan = None;
    } else {
        sprint.plan = Some(plan);
    }
    sprint
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

fn apply_update_to_sprint(target: &mut Sprint, payload: &SprintUpdateRequest) {
    let plan = target.plan.get_or_insert_with(SprintPlan::default);

    if payload.label.is_some() {
        plan.label = clean_opt_string(payload.label.clone());
    }
    if payload.goal.is_some() {
        plan.goal = clean_opt_string(payload.goal.clone());
    }
    if payload.plan_length.is_some() {
        plan.length = clean_opt_string(payload.plan_length.clone());
    }
    if payload.ends_at.is_some() {
        plan.ends_at = clean_opt_string(payload.ends_at.clone());
    }
    if payload.starts_at.is_some() {
        plan.starts_at = clean_opt_string(payload.starts_at.clone());
    }
    if payload.capacity_points.is_some() || payload.capacity_hours.is_some() {
        let cap = plan.capacity.get_or_insert_with(SprintCapacity::default);
        if let Some(value) = &payload.capacity_points {
            cap.points = *value;
        }
        if let Some(value) = &payload.capacity_hours {
            cap.hours = *value;
        }
        if cap.points.is_none() && cap.hours.is_none() {
            plan.capacity = None;
        }
    }
    if payload.overdue_after.is_some() {
        plan.overdue_after = clean_opt_string(payload.overdue_after.clone());
    }
    if payload.notes.is_some() {
        plan.notes = clean_opt_string(payload.notes.clone());
    }

    if payload.actual_started_at.is_some() || payload.actual_closed_at.is_some() {
        let actual = target.actual.get_or_insert_with(SprintActual::default);
        if let Some(value) = &payload.actual_started_at {
            actual.started_at = value.clone();
        }
        if let Some(value) = &payload.actual_closed_at {
            actual.closed_at = value.clone();
        }

        let should_clear_actual = matches!(
            target.actual.as_ref(),
            Some(actual) if actual.started_at.is_none() && actual.closed_at.is_none()
        );
        if should_clear_actual {
            target.actual = None;
        }
    }

    if let Some(plan) = target.plan.as_ref()
        && !plan_has_values(plan)
    {
        target.plan = None;
    }
}

pub(crate) fn handle_sprint_list(req: JsonRpcRequest) -> JsonRpcResponse {
    let resolver = match TasksDirectoryResolver::resolve(None, None) {
        Ok(r) => r,
        Err(e) => {
            return err(
                req.id,
                -32603,
                "Internal error",
                Some(json!({"message": e})),
            );
        }
    };

    let include_integrity = req
        .params
        .get("include_integrity")
        .and_then(|v| v.as_bool())
        .unwrap_or(true);

    let limit = match parse_limit_value(req.params.get("limit"), MCP_DEFAULT_SPRINT_LIST_LIMIT) {
        Ok(value) if (1..=MCP_MAX_SPRINT_LIST_LIMIT).contains(&value) => value,
        Ok(_) => {
            return err(
                req.id,
                -32602,
                &format!("limit must be between 1 and {}", MCP_MAX_SPRINT_LIST_LIMIT),
                None,
            );
        }
        Err(msg) => return err(req.id, -32602, msg, None),
    };

    let cursor_value = req
        .params
        .get("cursor")
        .or_else(|| req.params.get("offset"));
    let cursor = match parse_cursor_value(cursor_value) {
        Ok(value) if value <= MCP_MAX_CURSOR => value,
        Ok(_) => {
            return err(
                req.id,
                -32602,
                &format!("cursor must be <= {}", MCP_MAX_CURSOR),
                None,
            );
        }
        Err(msg) => return err(req.id, -32602, msg, None),
    };

    let storage = match Storage::try_open(resolver.path.clone()) {
        Some(storage) => storage,
        None => {
            let mut payload = serde_json::Map::new();
            payload.insert("status".to_string(), Value::String("ok".to_string()));
            payload.insert("count".to_string(), Value::from(0u64));
            payload.insert("total".to_string(), Value::from(0u64));
            payload.insert("cursor".to_string(), Value::from(0u64));
            payload.insert("limit".to_string(), Value::from(limit as u64));
            payload.insert("hasMore".to_string(), Value::Bool(false));
            payload.insert("nextCursor".to_string(), Value::Null);
            payload.insert("sprints".to_string(), Value::Array(Vec::new()));
            if include_integrity {
                payload.insert("missing_sprints".to_string(), Value::Array(Vec::new()));
            }
            let payload = Value::Object(payload);
            return ok(
                req.id,
                json!({
                    "content": [ { "type": "text", "text": serde_json::to_string_pretty(&payload).unwrap_or_else(|_| "{}".into()) } ]
                }),
            );
        }
    };

    let records = match SprintService::list(&storage) {
        Ok(records) => records,
        Err(error) => {
            return err(
                req.id,
                -32603,
                "Internal error",
                Some(json!({"message": format!("Failed to load sprints: {}", error)})),
            );
        }
    };

    let now = Utc::now();
    let mut sprints: Vec<SprintListItem> = records
        .iter()
        .map(|record| sprint_record_to_list_item(record, now))
        .collect();
    sprints.sort_by(|a, b| a.id.cmp(&b.id));

    let total = sprints.len();
    let start = cursor.min(total);
    let end = (start + limit).min(total);
    let page = sprints[start..end].to_vec();
    let has_more = end < total;
    let next_cursor = if has_more { Some(end) } else { None };

    let (missing_sprints, integrity) = if include_integrity {
        let report = sprint_integrity::detect_missing_sprints(&storage, &records);
        (
            Value::Array(
                report
                    .missing_sprints
                    .iter()
                    .map(|id| Value::from(*id))
                    .collect(),
            ),
            make_mcp_integrity_payload(&report, &report, None),
        )
    } else {
        (Value::Null, None)
    };

    let mut payload = serde_json::Map::new();
    payload.insert("status".to_string(), Value::String("ok".to_string()));
    payload.insert("count".to_string(), Value::from(page.len() as u64));
    payload.insert("total".to_string(), Value::from(total as u64));
    payload.insert("cursor".to_string(), Value::from(start as u64));
    payload.insert("limit".to_string(), Value::from(limit as u64));
    payload.insert("hasMore".to_string(), Value::Bool(has_more));
    payload.insert(
        "nextCursor".to_string(),
        next_cursor
            .map(|pos| Value::from(pos as u64))
            .unwrap_or(Value::Null),
    );
    payload.insert(
        "sprints".to_string(),
        serde_json::to_value(&page).unwrap_or_else(|_| Value::Array(Vec::new())),
    );
    if include_integrity {
        payload.insert("missing_sprints".to_string(), missing_sprints);
        if let Some(integrity) = integrity {
            payload.insert("integrity".to_string(), integrity);
        }
    }

    let payload = Value::Object(payload);
    ok(
        req.id,
        json!({
            "content": [ { "type": "text", "text": serde_json::to_string_pretty(&payload).unwrap_or_else(|_| "{}".into()) } ]
        }),
    )
}

pub(crate) fn handle_sprint_get(req: JsonRpcRequest) -> JsonRpcResponse {
    let sprint_id = match parse_sprint_id(&req.params) {
        Ok(id) => id,
        Err(msg) => return err(req.id, -32602, msg, None),
    };

    let resolver = match TasksDirectoryResolver::resolve(None, None) {
        Ok(r) => r,
        Err(e) => {
            return err(
                req.id,
                -32603,
                "Internal error",
                Some(json!({"message": e})),
            );
        }
    };

    let storage = Storage::new(resolver.path);
    let record = match SprintService::get(&storage, sprint_id) {
        Ok(record) => record,
        Err(crate::errors::LoTaRError::SprintNotFound(_)) => {
            return err(
                req.id,
                -32004,
                &format!("Sprint #{} not found", sprint_id),
                None,
            );
        }
        Err(error) => {
            return err(
                req.id,
                -32603,
                "Internal error",
                Some(json!({"message": error.to_string()})),
            );
        }
    };

    let item = sprint_record_to_list_item(&record, Utc::now());
    let payload = json!({"status": "ok", "sprint": item});
    ok(
        req.id,
        json!({
            "content": [ { "type": "text", "text": serde_json::to_string_pretty(&payload).unwrap_or_else(|_| "{}".into()) } ]
        }),
    )
}

pub(crate) fn handle_sprint_create(req: JsonRpcRequest) -> JsonRpcResponse {
    let resolver = match TasksDirectoryResolver::resolve(None, None) {
        Ok(r) => r,
        Err(e) => {
            return err(
                req.id,
                -32603,
                "Internal error",
                Some(json!({"message": e})),
            );
        }
    };

    let body: SprintCreateRequest = match serde_json::from_value(req.params.clone()) {
        Ok(payload) => payload,
        Err(error) => return err(req.id, -32602, &format!("Invalid params: {}", error), None),
    };

    let resolved_config = match resolution::load_and_merge_configs(Some(resolver.path.as_path())) {
        Ok(cfg) => cfg,
        Err(error) => {
            return err(
                req.id,
                -32603,
                "Internal error",
                Some(json!({"message": format!("Failed to load config: {}", error)})),
            );
        }
    };

    let mut storage = Storage::new(resolver.path);
    let sprint = sprint_from_create_request(&body);
    let defaults = if body.skip_defaults {
        None
    } else {
        Some(&resolved_config.sprint_defaults)
    };

    let outcome = match SprintService::create(&mut storage, sprint, defaults) {
        Ok(outcome) => outcome,
        Err(error) => {
            return err(
                req.id,
                -32603,
                "Failed to create sprint",
                Some(json!({"message": error.to_string()})),
            );
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

    ok(
        req.id,
        json!({
            "content": [ { "type": "text", "text": serde_json::to_string_pretty(&response).unwrap_or_else(|_| "{}".into()) } ]
        }),
    )
}

pub(crate) fn handle_sprint_update(req: JsonRpcRequest) -> JsonRpcResponse {
    let sprint_id = match parse_sprint_id(&req.params) {
        Ok(id) => id,
        Err(msg) => return err(req.id, -32602, msg, None),
    };

    let resolver = match TasksDirectoryResolver::resolve(None, None) {
        Ok(r) => r,
        Err(e) => {
            return err(
                req.id,
                -32603,
                "Internal error",
                Some(json!({"message": e})),
            );
        }
    };

    let mut params = req.params.clone();
    if let Value::Object(ref mut obj) = params {
        obj.insert("sprint".to_string(), Value::from(sprint_id));
    }
    let body: SprintUpdateRequest = match serde_json::from_value(params) {
        Ok(payload) => payload,
        Err(error) => return err(req.id, -32602, &format!("Invalid params: {}", error), None),
    };

    let mut storage = Storage::new(resolver.path);
    let existing = match SprintService::get(&storage, sprint_id) {
        Ok(record) => record,
        Err(crate::errors::LoTaRError::SprintNotFound(_)) => {
            return err(
                req.id,
                -32004,
                &format!("Sprint #{} not found", sprint_id),
                None,
            );
        }
        Err(error) => {
            return err(
                req.id,
                -32603,
                "Internal error",
                Some(json!({"message": error.to_string()})),
            );
        }
    };

    let mut sprint = existing.sprint.clone();
    apply_update_to_sprint(&mut sprint, &body);

    let outcome = match SprintService::update(&mut storage, sprint_id, sprint) {
        Ok(outcome) => outcome,
        Err(error) => {
            return err(
                req.id,
                -32603,
                "Failed to update sprint",
                Some(json!({"message": error.to_string()})),
            );
        }
    };

    let response = SprintUpdateResponse {
        status: "ok".to_string(),
        sprint: sprint_record_to_list_item(&outcome.record, Utc::now()),
        warnings: outcome
            .warnings
            .iter()
            .map(|warning| warning.message().to_string())
            .collect(),
    };

    ok(
        req.id,
        json!({
            "content": [ { "type": "text", "text": serde_json::to_string_pretty(&response).unwrap_or_else(|_| "{}".into()) } ]
        }),
    )
}

pub(crate) fn handle_sprint_summary(req: JsonRpcRequest) -> JsonRpcResponse {
    let sprint_id = match parse_sprint_id(&req.params) {
        Ok(id) => id,
        Err(msg) => return err(req.id, -32602, msg, None),
    };

    let resolver = match TasksDirectoryResolver::resolve(None, None) {
        Ok(r) => r,
        Err(e) => {
            return err(
                req.id,
                -32603,
                "Internal error",
                Some(json!({"message": e})),
            );
        }
    };

    let storage = Storage::new(resolver.path.clone());
    let record = match SprintService::get(&storage, sprint_id) {
        Ok(record) => record,
        Err(crate::errors::LoTaRError::SprintNotFound(_)) => {
            return err(
                req.id,
                -32004,
                &format!("Sprint #{} not found", sprint_id),
                None,
            );
        }
        Err(error) => {
            return err(
                req.id,
                -32603,
                "Internal error",
                Some(json!({"message": error.to_string()})),
            );
        }
    };

    let resolved_config = match resolution::load_and_merge_configs(Some(resolver.path.as_path())) {
        Ok(cfg) => cfg,
        Err(error) => {
            return err(
                req.id,
                -32603,
                "Internal error",
                Some(json!({"message": format!("Failed to load config: {}", error)})),
            );
        }
    };

    let summary = compute_sprint_summary(&storage, &record, &resolved_config, Utc::now());
    ok(
        req.id,
        json!({
            "content": [ { "type": "text", "text": serde_json::to_string_pretty(&summary.payload).unwrap_or_else(|_| "{}".into()) } ]
        }),
    )
}

pub(crate) fn handle_sprint_burndown(req: JsonRpcRequest) -> JsonRpcResponse {
    let sprint_id = match parse_sprint_id(&req.params) {
        Ok(id) => id,
        Err(msg) => return err(req.id, -32602, msg, None),
    };

    let resolver = match TasksDirectoryResolver::resolve(None, None) {
        Ok(r) => r,
        Err(e) => {
            return err(
                req.id,
                -32603,
                "Internal error",
                Some(json!({"message": e})),
            );
        }
    };

    let storage = Storage::new(resolver.path.clone());
    let record = match SprintService::get(&storage, sprint_id) {
        Ok(record) => record,
        Err(crate::errors::LoTaRError::SprintNotFound(_)) => {
            return err(
                req.id,
                -32004,
                &format!("Sprint #{} not found", sprint_id),
                None,
            );
        }
        Err(error) => {
            return err(
                req.id,
                -32603,
                "Internal error",
                Some(json!({"message": error.to_string()})),
            );
        }
    };

    let resolved_config = match resolution::load_and_merge_configs(Some(resolver.path.as_path())) {
        Ok(cfg) => cfg,
        Err(error) => {
            return err(
                req.id,
                -32603,
                "Internal error",
                Some(json!({"message": format!("Failed to load config: {}", error)})),
            );
        }
    };

    let ctx = match compute_sprint_burndown(&storage, &record, &resolved_config, Utc::now()) {
        Ok(ctx) => ctx,
        Err(msg) => return err(req.id, -32602, msg.as_str(), None),
    };

    ok(
        req.id,
        json!({
            "content": [ { "type": "text", "text": serde_json::to_string_pretty(&ctx.payload).unwrap_or_else(|_| "{}".into()) } ]
        }),
    )
}

pub(crate) fn handle_sprint_velocity(req: JsonRpcRequest) -> JsonRpcResponse {
    let include_active = req
        .params
        .get("include_active")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);

    let limit = match parse_limit_value(req.params.get("limit"), DEFAULT_VELOCITY_WINDOW) {
        Ok(value) if value > 0 => value,
        _ => DEFAULT_VELOCITY_WINDOW,
    };

    let metric = match req.params.get("metric").and_then(|v| v.as_str()) {
        Some(raw) => {
            let lowered = raw.trim().to_ascii_lowercase();
            match lowered.as_str() {
                "tasks" => SprintBurndownMetric::Tasks,
                "points" => SprintBurndownMetric::Points,
                "hours" => SprintBurndownMetric::Hours,
                _ => {
                    return err(
                        req.id,
                        -32602,
                        &format!("Unsupported metric '{}'. Use tasks, points, or hours.", raw),
                        None,
                    );
                }
            }
        }
        None => SprintBurndownMetric::Points,
    };

    let resolver = match TasksDirectoryResolver::resolve(None, None) {
        Ok(r) => r,
        Err(e) => {
            return err(
                req.id,
                -32603,
                "Internal error",
                Some(json!({"message": e})),
            );
        }
    };

    let storage = match Storage::try_open(resolver.path.clone()) {
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
            return ok(
                req.id,
                json!({
                    "content": [ { "type": "text", "text": serde_json::to_string_pretty(&payload).unwrap_or_else(|_| "{}".into()) } ]
                }),
            );
        }
    };

    let records = match SprintService::list(&storage) {
        Ok(records) => records,
        Err(error) => {
            return err(
                req.id,
                -32603,
                "Internal error",
                Some(json!({"message": format!("Failed to load sprints: {}", error)})),
            );
        }
    };

    let resolved_config = match resolution::load_and_merge_configs(Some(resolver.path.as_path())) {
        Ok(cfg) => cfg,
        Err(error) => {
            return err(
                req.id,
                -32603,
                "Internal error",
                Some(json!({"message": format!("Failed to load config: {}", error)})),
            );
        }
    };

    let options = VelocityOptions {
        limit,
        include_active,
        metric,
    };
    let computed = compute_velocity(&storage, records, &resolved_config, options, Utc::now());
    let payload = computed.to_payload(include_active);

    ok(
        req.id,
        json!({
            "content": [ { "type": "text", "text": serde_json::to_string_pretty(&payload).unwrap_or_else(|_| "{}".into()) } ]
        }),
    )
}

pub(crate) fn handle_sprint_add(req: JsonRpcRequest) -> JsonRpcResponse {
    let resolver = match TasksDirectoryResolver::resolve(None, None) {
        Ok(r) => r,
        Err(e) => {
            return err(
                req.id,
                -32603,
                "Internal error",
                Some(json!({"message": e})),
            );
        }
    };
    let mut storage = Storage::new(resolver.path.clone());
    let mut records = match SprintService::list(&storage) {
        Ok(records) => records,
        Err(e) => {
            return err(
                req.id,
                -32603,
                "Internal error",
                Some(json!({"message": format!("Failed to load sprints: {}", e)})),
            );
        }
    };

    let cleanup_missing = req
        .params
        .get("cleanup_missing")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);

    let mut integrity_report = sprint_integrity::detect_missing_sprints(&storage, &records);
    let baseline_report = integrity_report.clone();
    let mut cleanup_outcome: Option<sprint_integrity::SprintCleanupOutcome> = None;

    if cleanup_missing && !integrity_report.missing_sprints.is_empty() {
        match sprint_integrity::cleanup_missing_sprint_refs(&mut storage, &mut records, None) {
            Ok(outcome) => {
                integrity_report = sprint_integrity::detect_missing_sprints(&storage, &records);
                cleanup_outcome = Some(outcome);
            }
            Err(error) => {
                return err(
                    req.id,
                    -32603,
                    "Internal error",
                    Some(json!({"message": format!(
                        "Failed to clean up sprint references: {}",
                        error
                    )})),
                );
            }
        }
    }

    let raw_tasks = req.params.get("tasks");
    if raw_tasks.is_none() {
        return err(req.id, -32602, "Missing required field: tasks", None);
    }
    let tasks: Vec<String> = match raw_tasks.unwrap() {
        Value::String(value) => {
            let trimmed = value.trim();
            if trimmed.is_empty() {
                Vec::new()
            } else {
                vec![trimmed.to_string()]
            }
        }
        Value::Array(values) => values
            .iter()
            .filter_map(|v| v.as_str())
            .map(|s| s.trim())
            .filter(|s| !s.is_empty())
            .map(|s| s.to_string())
            .collect(),
        _ => {
            return err(
                req.id,
                -32602,
                "tasks must be a string or array of strings",
                None,
            );
        }
    };

    let sprint_ref = match parse_sprint_reference(&req.params) {
        Ok(v) => v,
        Err(msg) => return err(req.id, -32602, msg, None),
    };

    let allow_closed = req
        .params
        .get("allow_closed")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);

    let force_single = req
        .params
        .get("force_single")
        .or_else(|| req.params.get("force"))
        .and_then(|v| v.as_bool())
        .unwrap_or(false);

    let outcome = match sprint_assignment::assign_tasks(
        &mut storage,
        &records,
        &tasks,
        sprint_ref.as_deref(),
        allow_closed,
        force_single,
    ) {
        Ok(outcome) => outcome,
        Err(msg) => return err(req.id, -32602, msg.as_str(), None),
    };

    let mut payload = serde_json::Map::new();
    payload.insert("status".to_string(), Value::String("ok".to_string()));
    payload.insert(
        "action".to_string(),
        Value::String(outcome.action.as_str().to_string()),
    );
    payload.insert("sprint_id".to_string(), Value::from(outcome.sprint_id));
    if let Some(label) = outcome.sprint_label.clone() {
        payload.insert("sprint_label".to_string(), Value::String(label));
    } else {
        payload.insert("sprint_label".to_string(), Value::Null);
    }
    payload.insert(
        "modified".to_string(),
        Value::Array(
            outcome
                .modified
                .iter()
                .map(|id| Value::String(id.clone()))
                .collect(),
        ),
    );
    payload.insert(
        "unchanged".to_string(),
        Value::Array(
            outcome
                .unchanged
                .iter()
                .map(|id| Value::String(id.clone()))
                .collect(),
        ),
    );
    let reassignment_messages: Vec<String> = outcome
        .replaced
        .iter()
        .filter_map(|info| info.describe())
        .collect();

    let replaced: Vec<Value> = outcome
        .replaced
        .into_iter()
        .map(|info| {
            let mut entry = serde_json::Map::new();
            entry.insert("task_id".to_string(), Value::String(info.task_id));
            entry.insert(
                "previous".to_string(),
                Value::Array(info.previous.iter().map(|id| Value::from(*id)).collect()),
            );
            Value::Object(entry)
        })
        .collect();
    payload.insert("replaced".to_string(), Value::Array(replaced));
    if !reassignment_messages.is_empty() {
        payload.insert(
            "messages".to_string(),
            Value::Array(
                reassignment_messages
                    .iter()
                    .map(|msg| Value::String(msg.clone()))
                    .collect(),
            ),
        );
    }
    payload.insert(
        "missing_sprints".to_string(),
        Value::Array(
            integrity_report
                .missing_sprints
                .iter()
                .map(|id| Value::from(*id))
                .collect(),
        ),
    );
    if let Some(integrity) = make_mcp_integrity_payload(
        &baseline_report,
        &integrity_report,
        cleanup_outcome.as_ref(),
    ) {
        payload.insert("integrity".to_string(), integrity);
    }

    let payload = Value::Object(payload);
    let mut content_items = Vec::new();
    if !reassignment_messages.is_empty() {
        content_items.push(json!({
            "type": "text",
            "text": reassignment_messages.join("\n"),
        }));
    }
    content_items.push(json!({
        "type": "text",
        "text": serde_json::to_string_pretty(&payload).unwrap_or_else(|_| "{}".into()),
    }));

    ok(req.id, json!({ "content": content_items }))
}

pub(crate) fn handle_sprint_remove(req: JsonRpcRequest) -> JsonRpcResponse {
    let resolver = match TasksDirectoryResolver::resolve(None, None) {
        Ok(r) => r,
        Err(e) => {
            return err(
                req.id,
                -32603,
                "Internal error",
                Some(json!({"message": e})),
            );
        }
    };
    let mut storage = Storage::new(resolver.path.clone());
    let mut records = match SprintService::list(&storage) {
        Ok(records) => records,
        Err(e) => {
            return err(
                req.id,
                -32603,
                "Internal error",
                Some(json!({"message": format!("Failed to load sprints: {}", e)})),
            );
        }
    };

    let cleanup_missing = req
        .params
        .get("cleanup_missing")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);

    let mut integrity_report = sprint_integrity::detect_missing_sprints(&storage, &records);
    let baseline_report = integrity_report.clone();
    let mut cleanup_outcome: Option<sprint_integrity::SprintCleanupOutcome> = None;

    if cleanup_missing && !integrity_report.missing_sprints.is_empty() {
        match sprint_integrity::cleanup_missing_sprint_refs(&mut storage, &mut records, None) {
            Ok(outcome) => {
                integrity_report = sprint_integrity::detect_missing_sprints(&storage, &records);
                cleanup_outcome = Some(outcome);
            }
            Err(error) => {
                return err(
                    req.id,
                    -32603,
                    "Internal error",
                    Some(json!({"message": format!(
                        "Failed to clean up sprint references: {}",
                        error
                    )})),
                );
            }
        }
    }

    let raw_tasks = req.params.get("tasks");
    if raw_tasks.is_none() {
        return err(req.id, -32602, "Missing required field: tasks", None);
    }
    let tasks: Vec<String> = match raw_tasks.unwrap() {
        Value::String(value) => {
            let trimmed = value.trim();
            if trimmed.is_empty() {
                Vec::new()
            } else {
                vec![trimmed.to_string()]
            }
        }
        Value::Array(values) => values
            .iter()
            .filter_map(|v| v.as_str())
            .map(|s| s.trim())
            .filter(|s| !s.is_empty())
            .map(|s| s.to_string())
            .collect(),
        _ => {
            return err(
                req.id,
                -32602,
                "tasks must be a string or array of strings",
                None,
            );
        }
    };

    let sprint_ref = match parse_sprint_reference(&req.params) {
        Ok(v) => v,
        Err(msg) => return err(req.id, -32602, msg, None),
    };

    let outcome = match sprint_assignment::remove_tasks(
        &mut storage,
        &records,
        &tasks,
        sprint_ref.as_deref(),
    ) {
        Ok(outcome) => outcome,
        Err(msg) => return err(req.id, -32602, msg.as_str(), None),
    };

    let mut payload = serde_json::Map::new();
    payload.insert("status".to_string(), Value::String("ok".to_string()));
    payload.insert(
        "action".to_string(),
        Value::String(outcome.action.as_str().to_string()),
    );
    payload.insert("sprint_id".to_string(), Value::from(outcome.sprint_id));
    if let Some(label) = outcome.sprint_label.clone() {
        payload.insert("sprint_label".to_string(), Value::String(label));
    } else {
        payload.insert("sprint_label".to_string(), Value::Null);
    }
    payload.insert(
        "modified".to_string(),
        Value::Array(
            outcome
                .modified
                .iter()
                .map(|id| Value::String(id.clone()))
                .collect(),
        ),
    );
    payload.insert(
        "unchanged".to_string(),
        Value::Array(
            outcome
                .unchanged
                .iter()
                .map(|id| Value::String(id.clone()))
                .collect(),
        ),
    );
    payload.insert(
        "missing_sprints".to_string(),
        Value::Array(
            integrity_report
                .missing_sprints
                .iter()
                .map(|id| Value::from(*id))
                .collect(),
        ),
    );
    if let Some(integrity) = make_mcp_integrity_payload(
        &baseline_report,
        &integrity_report,
        cleanup_outcome.as_ref(),
    ) {
        payload.insert("integrity".to_string(), integrity);
    }
    let payload = Value::Object(payload);

    ok(
        req.id,
        json!({
            "content": [ { "type": "text", "text": serde_json::to_string_pretty(&payload).unwrap_or_else(|_| "{}".into()) } ]
        }),
    )
}

pub(crate) fn handle_sprint_delete(req: JsonRpcRequest) -> JsonRpcResponse {
    let resolver = match TasksDirectoryResolver::resolve(None, None) {
        Ok(r) => r,
        Err(e) => {
            return err(
                req.id,
                -32603,
                "Internal error",
                Some(json!({"message": e})),
            );
        }
    };
    let mut storage = Storage::new(resolver.path.clone());

    let sprint_id = match req.params.get("sprint_id") {
        Some(Value::Number(num)) => num.as_u64().and_then(|value| u32::try_from(value).ok()),
        Some(Value::String(text)) => {
            let trimmed = text.trim();
            if trimmed.is_empty() {
                None
            } else {
                trimmed
                    .strip_prefix('#')
                    .unwrap_or(trimmed)
                    .parse::<u32>()
                    .ok()
            }
        }
        Some(Value::Null) | None => None,
        _ => return err(req.id, -32602, "sprint_id must be a positive integer", None),
    }
    .or_else(|| match req.params.get("sprint") {
        Some(Value::Number(num)) => num.as_u64().and_then(|value| u32::try_from(value).ok()),
        Some(Value::String(text)) => {
            let trimmed = text.trim();
            if trimmed.is_empty() {
                None
            } else {
                trimmed
                    .strip_prefix('#')
                    .unwrap_or(trimmed)
                    .parse::<u32>()
                    .ok()
            }
        }
        Some(Value::Null) | None => None,
        _ => None,
    });

    let sprint_id = match sprint_id {
        Some(id) => id,
        None => return err(req.id, -32602, "Missing or invalid sprint id", None),
    };

    let cleanup_missing = req
        .params
        .get("cleanup_missing")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);

    let record = match SprintService::get(&storage, sprint_id) {
        Ok(record) => record,
        Err(crate::errors::LoTaRError::SprintNotFound(_)) => {
            return err(
                req.id,
                -32004,
                &format!("Sprint #{} not found", sprint_id),
                None,
            );
        }
        Err(error) => {
            return err(
                req.id,
                -32603,
                "Internal error",
                Some(json!({"message": error.to_string()})),
            );
        }
    };

    let display_name = sprint_assignment::sprint_display_name(&record);
    let sprint_label = record
        .sprint
        .plan
        .as_ref()
        .and_then(|plan| plan.label.clone());

    match SprintService::delete(&mut storage, sprint_id) {
        Ok(true) => {}
        Ok(false) => {
            return err(
                req.id,
                -32004,
                &format!("Sprint #{} not found", sprint_id),
                None,
            );
        }
        Err(error) => {
            return err(
                req.id,
                -32603,
                "Failed to delete sprint",
                Some(json!({"message": error.to_string()})),
            );
        }
    }

    let mut records = match SprintService::list(&storage) {
        Ok(records) => records,
        Err(error) => {
            return err(
                req.id,
                -32603,
                "Internal error",
                Some(json!({"message": error.to_string()})),
            );
        }
    };

    let mut integrity_report = sprint_integrity::detect_missing_sprints(&storage, &records);
    let baseline_report = integrity_report.clone();
    let mut cleanup_outcome: Option<sprint_integrity::SprintCleanupOutcome> = None;

    if cleanup_missing {
        match sprint_integrity::cleanup_missing_sprint_refs(
            &mut storage,
            &mut records,
            Some(sprint_id),
        ) {
            Ok(outcome) => {
                integrity_report = sprint_integrity::detect_missing_sprints(&storage, &records);
                cleanup_outcome = Some(outcome);
            }
            Err(error) => {
                return err(
                    req.id,
                    -32603,
                    "Failed to clean sprint references",
                    Some(json!({"message": error.to_string()})),
                );
            }
        }
    }

    let removed_references = cleanup_outcome
        .as_ref()
        .map(|outcome| outcome.removed_references)
        .unwrap_or(0);
    let updated_tasks = cleanup_outcome
        .as_ref()
        .map(|outcome| outcome.updated_tasks)
        .unwrap_or(0);

    let mut summary = format!("Deleted {}.", display_name);

    if cleanup_missing {
        if removed_references > 0 {
            let _ = write!(
                summary,
                " Removed {} dangling sprint reference(s) across {} task(s).",
                removed_references, updated_tasks
            );
        } else {
            summary.push_str(" No dangling sprint references required cleanup.");
        }
    } else if !integrity_report.missing_sprints.is_empty() {
        let missing = integrity_report
            .missing_sprints
            .iter()
            .map(|id| format!("#{}", id))
            .collect::<Vec<_>>()
            .join(", ");
        let _ = write!(
            summary,
            " Missing sprint references detected: {}. Re-run with cleanup_missing=true to remove them.",
            missing
        );
    }

    let mut payload = serde_json::Map::new();
    payload.insert("status".to_string(), Value::String("ok".to_string()));
    payload.insert("deleted".to_string(), Value::Bool(true));
    payload.insert("sprint_id".to_string(), Value::from(sprint_id));
    if let Some(label) = sprint_label.clone() {
        payload.insert("sprint_label".to_string(), Value::String(label));
    } else {
        payload.insert("sprint_label".to_string(), Value::Null);
    }
    payload.insert(
        "removed_references".to_string(),
        Value::from(removed_references as u64),
    );
    payload.insert(
        "updated_tasks".to_string(),
        Value::from(updated_tasks as u64),
    );
    if let Some(integrity) = make_mcp_integrity_payload(
        &baseline_report,
        &integrity_report,
        cleanup_outcome.as_ref(),
    ) {
        payload.insert("integrity".to_string(), integrity);
    }

    let payload = Value::Object(payload);

    ok(
        req.id,
        json!({
            "content": [
                { "type": "text", "text": format!("{}", summary) },
                { "type": "text", "text": serde_json::to_string_pretty(&payload).unwrap_or_else(|_| "{}".into()) }
            ]
        }),
    )
}

pub(crate) fn handle_sprint_backlog(req: JsonRpcRequest) -> JsonRpcResponse {
    let resolver = match TasksDirectoryResolver::resolve(None, None) {
        Ok(r) => r,
        Err(e) => {
            return err(
                req.id,
                -32603,
                "Internal error",
                Some(json!({"message": e})),
            );
        }
    };

    let project = req
        .params
        .get("project")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());
    let project_scope: Vec<String> = project.iter().cloned().collect();
    let enum_hints = ConfigManager::new_manager_with_tasks_dir_readonly(&resolver.path)
        .ok()
        .and_then(|mgr| {
            let cfg = mgr.get_resolved_config();
            EnumHints::from_resolved_config(cfg, &project_scope)
        });

    let mut storage = match Storage::try_open(resolver.path.clone()) {
        Some(storage) => storage,
        None => {
            let mut payload = serde_json::Map::new();
            payload.insert("status".to_string(), Value::String("ok".to_string()));
            payload.insert("count".to_string(), Value::from(0u64));
            payload.insert("truncated".to_string(), Value::Bool(false));
            payload.insert("tasks".to_string(), Value::Array(Vec::new()));
            if let Some(hints) = enum_hints.as_ref() {
                payload.insert("enumHints".to_string(), enum_hints_to_value(hints));
            }
            let payload = Value::Object(payload);
            return ok(
                req.id,
                json!({
                    "content": [ { "type": "text", "text": serde_json::to_string_pretty(&payload).unwrap_or_else(|_| "{}".into()) } ]
                }),
            );
        }
    };

    let limit = match parse_limit_value(req.params.get("limit"), MCP_DEFAULT_BACKLOG_LIMIT) {
        Ok(value) if (1..=MCP_MAX_BACKLOG_LIMIT).contains(&value) => value,
        Ok(_) => {
            return err(
                req.id,
                -32602,
                &format!("limit must be between 1 and {}", MCP_MAX_BACKLOG_LIMIT),
                None,
            );
        }
        Err(msg) => return err(req.id, -32602, msg, None),
    };
    let cursor_value = req
        .params
        .get("cursor")
        .or_else(|| req.params.get("offset"));
    let cursor = match parse_cursor_value(cursor_value) {
        Ok(value) if value <= MCP_MAX_CURSOR => value,
        Ok(_) => {
            return err(
                req.id,
                -32602,
                &format!("cursor must be <= {}", MCP_MAX_CURSOR),
                None,
            );
        }
        Err(msg) => return err(req.id, -32602, msg, None),
    };

    let cleanup_missing = req
        .params
        .get("cleanup_missing")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);

    fn parse_string_vec(value: Option<&Value>) -> Vec<String> {
        match value {
            Some(Value::String(s)) => s
                .split(',')
                .map(|token| token.trim())
                .filter(|token| !token.is_empty())
                .map(|token| token.to_string())
                .collect(),
            Some(Value::Array(arr)) => arr
                .iter()
                .filter_map(|v| v.as_str())
                .map(|s| s.trim())
                .filter(|s| !s.is_empty())
                .map(|s| s.to_string())
                .collect(),
            _ => Vec::new(),
        }
    }

    let status_tokens = parse_string_vec(req.params.get("status"));
    let statuses: Vec<TaskStatus> = status_tokens
        .iter()
        .map(|token| TaskStatus::from(token.as_str()))
        .collect();

    let tags = parse_string_vec(req.params.get("tag"));

    let options = SprintBacklogOptions {
        project: project.clone(),
        tags,
        statuses,
        assignee: req
            .params
            .get("assignee")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string()),
        limit: 0,
    };

    let mut records = match SprintService::list(&storage) {
        Ok(records) => records,
        Err(e) => {
            return err(
                req.id,
                -32603,
                "Internal error",
                Some(json!({"message": format!("Failed to load sprints: {}", e)})),
            );
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
            Err(error) => {
                return err(
                    req.id,
                    -32603,
                    "Internal error",
                    Some(json!({"message": format!(
                        "Failed to clean up sprint references: {}",
                        error
                    )})),
                );
            }
        }
    }

    let result = match sprint_assignment::fetch_backlog(&storage, options) {
        Ok(result) => result,
        Err(msg) => return err(req.id, -32602, msg.as_str(), None),
    };
    let total_entries = result.entries.len();
    let start = cursor.min(total_entries);
    let end = (start + limit).min(total_entries);
    let page_entries = result.entries[start..end].to_vec();
    let has_more = end < total_entries;
    let next_cursor = if has_more { Some(end) } else { None };

    let mut payload = serde_json::Map::new();
    payload.insert("status".to_string(), Value::String("ok".to_string()));
    payload.insert("count".to_string(), Value::from(page_entries.len() as u64));
    payload.insert("total".to_string(), Value::from(total_entries as u64));
    payload.insert("truncated".to_string(), Value::Bool(has_more));
    payload.insert("hasMore".to_string(), Value::Bool(has_more));
    payload.insert("cursor".to_string(), Value::from(start as u64));
    payload.insert(
        "nextCursor".to_string(),
        next_cursor
            .map(|pos| Value::from(pos as u64))
            .unwrap_or(Value::Null),
    );
    payload.insert(
        "tasks".to_string(),
        serde_json::to_value(&page_entries).unwrap_or_else(|_| Value::Array(Vec::new())),
    );
    payload.insert(
        "missing_sprints".to_string(),
        Value::Array(
            integrity_report
                .missing_sprints
                .iter()
                .map(|id| Value::from(*id))
                .collect(),
        ),
    );
    if let Some(integrity) = make_mcp_integrity_payload(
        &baseline_report,
        &integrity_report,
        cleanup_outcome.as_ref(),
    ) {
        payload.insert("integrity".to_string(), integrity);
    }
    if let Some(hints) = enum_hints.as_ref() {
        payload.insert("enumHints".to_string(), enum_hints_to_value(hints));
    }

    let payload = Value::Object(payload);

    ok(
        req.id,
        json!({
            "content": [ { "type": "text", "text": serde_json::to_string_pretty(&payload).unwrap_or_else(|_| "{}".into()) } ]
        }),
    )
}

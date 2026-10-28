use serde_json::{Value, json};
use std::fmt::Write as _;

use super::super::hints::{EnumHints, enum_hints_to_value};
use super::super::{
    JsonRpcRequest, JsonRpcResponse, MCP_DEFAULT_BACKLOG_LIMIT, MCP_DEFAULT_SPRINT_LIST_LIMIT,
    MCP_MAX_BACKLOG_LIMIT, MCP_MAX_CURSOR, MCP_MAX_SPRINT_LIST_LIMIT, domain_service_error, err,
    integrity_value, internal_error, ok, parse_cursor_value, parse_limit_value, resolve_tasks_dir,
    text_result,
};
use crate::api_types::{
    SprintCreateRequest, SprintCreateResponse, SprintListItem, SprintUpdateRequest,
    SprintUpdateResponse,
};
use crate::config::resolution;
use crate::services::sprint_assignment::{self, SprintBacklogOptions};
use crate::services::sprint_integrity;
use crate::services::sprint_metrics::SprintBurndownMetric;
use crate::services::sprint_reports::{compute_sprint_burndown, compute_sprint_summary};
use crate::services::sprint_service::{
    SprintService, sprint_from_create_request, sprint_record_to_list_item,
};
use crate::services::sprint_velocity::{
    DEFAULT_VELOCITY_WINDOW, VelocityComputation, VelocityOptions, compute_velocity,
};
use crate::storage::manager::Storage;
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

pub(crate) fn handle_sprint_list(req: JsonRpcRequest) -> JsonRpcResponse {
    let resolver = try_rpc!(resolve_tasks_dir(&req.id));

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
        .filter(|value| !value.is_null())
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

    let storage = match Storage::try_open(&resolver.path.clone()) {
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
            return text_result(req.id, &payload);
        }
    };

    let records = match SprintService::list(&storage) {
        Ok(records) => records,
        Err(error) => {
            return internal_error(req.id, format!("Failed to load sprints: {}", error));
        }
    };

    let now = Utc::now();
    let mut sprints: Vec<SprintListItem> = records
        .iter()
        .map(|record| sprint_record_to_list_item(record, now))
        .collect();
    sprints.sort_by_key(|a| a.id);

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
            integrity_value(&sprint_integrity::IntegrityCheck::unchanged(report.clone())),
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
    text_result(req.id, &payload)
}

pub(crate) fn handle_sprint_get(req: JsonRpcRequest) -> JsonRpcResponse {
    let sprint_id = match parse_sprint_id(&req.params) {
        Ok(id) => id,
        Err(msg) => return err(req.id, -32602, msg, None),
    };

    let resolver = try_rpc!(resolve_tasks_dir(&req.id));

    let storage = Storage::new(&resolver.path);
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
            return internal_error(req.id, error.to_string());
        }
    };

    let item = sprint_record_to_list_item(&record, Utc::now());
    let payload = json!({"status": "ok", "sprint": item});
    text_result(req.id, &payload)
}

pub(crate) fn handle_sprint_create(req: JsonRpcRequest) -> JsonRpcResponse {
    let resolver = try_rpc!(resolve_tasks_dir(&req.id));

    let body: SprintCreateRequest = match serde_json::from_value(req.params.clone()) {
        Ok(payload) => payload,
        Err(error) => return err(req.id, -32602, &format!("Invalid params: {}", error), None),
    };

    let resolved_config = match resolution::load_and_merge_configs(Some(resolver.path.as_path())) {
        Ok(cfg) => cfg,
        Err(error) => {
            return internal_error(req.id, format!("Failed to load config: {}", error));
        }
    };

    let mut storage = Storage::new(&resolver.path);
    let sprint = sprint_from_create_request(&body);
    let defaults = if body.skip_defaults {
        None
    } else {
        Some(&resolved_config.sprint_defaults)
    };

    let outcome = match SprintService::create(&mut storage, sprint, defaults) {
        Ok(outcome) => outcome,
        Err(error) => {
            return domain_service_error(req.id, "Failed to create sprint", &error);
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

    text_result(req.id, &response)
}

pub(crate) fn handle_sprint_update(req: JsonRpcRequest) -> JsonRpcResponse {
    let sprint_id = match parse_sprint_id(&req.params) {
        Ok(id) => id,
        Err(msg) => return err(req.id, -32602, msg, None),
    };

    let resolver = try_rpc!(resolve_tasks_dir(&req.id));

    let mut params = req.params.clone();
    if let Value::Object(ref mut obj) = params {
        obj.insert("sprint".to_string(), Value::from(sprint_id));
    }
    let body: SprintUpdateRequest = match serde_json::from_value(params) {
        Ok(payload) => payload,
        Err(error) => return err(req.id, -32602, &format!("Invalid params: {}", error), None),
    };

    let mut storage = Storage::new(&resolver.path);
    let patch = crate::services::sprint_service::SprintMetadataPatch::from_request(&body);

    let outcome = match SprintService::update_with_patch(&mut storage, sprint_id, &patch) {
        Ok(outcome) => outcome,
        Err(crate::errors::LoTaRError::SprintNotFound(_)) => {
            return err(
                req.id,
                -32004,
                &format!("Sprint #{} not found", sprint_id),
                None,
            );
        }
        Err(error) => {
            return domain_service_error(req.id, "Failed to update sprint", &error);
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

    text_result(req.id, &response)
}

pub(crate) fn handle_sprint_summary(req: JsonRpcRequest) -> JsonRpcResponse {
    let sprint_id = match parse_sprint_id(&req.params) {
        Ok(id) => id,
        Err(msg) => return err(req.id, -32602, msg, None),
    };

    let resolver = try_rpc!(resolve_tasks_dir(&req.id));

    let storage = Storage::new(&resolver.path.clone());
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
            return internal_error(req.id, error.to_string());
        }
    };

    let resolved_config = match resolution::load_and_merge_configs(Some(resolver.path.as_path())) {
        Ok(cfg) => cfg,
        Err(error) => {
            return internal_error(req.id, format!("Failed to load config: {}", error));
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

    let resolver = try_rpc!(resolve_tasks_dir(&req.id));

    let storage = Storage::new(&resolver.path.clone());
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
            return internal_error(req.id, error.to_string());
        }
    };

    let resolved_config = match resolution::load_and_merge_configs(Some(resolver.path.as_path())) {
        Ok(cfg) => cfg,
        Err(error) => {
            return internal_error(req.id, format!("Failed to load config: {}", error));
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

    let resolver = try_rpc!(resolve_tasks_dir(&req.id));

    let storage = match Storage::try_open(&resolver.path.clone()) {
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
            return text_result(req.id, &payload);
        }
    };

    let records = match SprintService::list(&storage) {
        Ok(records) => records,
        Err(error) => {
            return internal_error(req.id, format!("Failed to load sprints: {}", error));
        }
    };

    let resolved_config = match resolution::load_and_merge_configs(Some(resolver.path.as_path())) {
        Ok(cfg) => cfg,
        Err(error) => {
            return internal_error(req.id, format!("Failed to load config: {}", error));
        }
    };

    let options = VelocityOptions {
        limit,
        include_active,
        metric,
    };
    let computed = compute_velocity(&storage, &records, &resolved_config, &options, Utc::now());
    let payload = computed.to_payload(include_active);

    text_result(req.id, &payload)
}

/// What sprint membership tools (add/remove) share: storage, sprint records
/// after the optional missing-sprint cleanup, the task ids and the sprint.
struct MembershipRequest {
    storage: Storage,
    records: Vec<crate::services::sprint_service::SprintRecord>,
    integrity: sprint_integrity::IntegrityCheck,
    tasks: Vec<String>,
    sprint_ref: Option<String>,
}

fn membership_request(req: &JsonRpcRequest) -> Result<MembershipRequest, JsonRpcResponse> {
    let id = || req.id.clone();
    let resolver = resolve_tasks_dir(&req.id)?;
    let mut storage = Storage::new(&resolver.path);
    let mut records = SprintService::list(&storage)
        .map_err(|e| internal_error(id(), format!("Failed to load sprints: {}", e)))?;
    let cleanup_missing = req
        .params
        .get("cleanup_missing")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    let integrity =
        sprint_integrity::check_missing_sprints(&mut storage, &mut records, cleanup_missing, None)
            .map_err(|e| {
                internal_error(id(), format!("Failed to clean up sprint references: {}", e))
            })?;
    // `query` selects tasks with the shared task query (DEV-236); `tasks`
    // may then be omitted.
    let search = req
        .params
        .get("query")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|q| !q.is_empty());
    let raw_tasks = match req.params.get("tasks") {
        Some(value) => value.clone(),
        None if search.is_some() => Value::Array(Vec::new()),
        None => return Err(err(id(), -32602, "Missing required field: tasks", None)),
    };
    let mut tasks: Vec<String> = match &raw_tasks {
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
            return Err(err(
                id(),
                -32602,
                "tasks must be a string or array of strings",
                None,
            ));
        }
    };
    if let Some(search) = search {
        let base = resolution::load_and_merge_configs(Some(resolver.path.as_path()))
            .map_err(|e| internal_error(id(), format!("Failed to load config: {e}")))?;
        let selected = crate::services::task_list::select_ids(
            &storage,
            &base,
            resolver.path.as_path(),
            search,
        )
        .map_err(|e| super::tasks::query_error(id(), e, None, &resolver.path, &base))?;
        tasks.extend(selected);
        tasks.sort();
        tasks.dedup();
    }
    let sprint_ref =
        parse_sprint_reference(&req.params).map_err(|msg| err(id(), -32602, msg, None))?;
    Ok(MembershipRequest {
        storage,
        records,
        integrity,
        tasks,
        sprint_ref,
    })
}

/// Response fields every sprint membership tool returns.
fn membership_payload(
    outcome: &crate::services::sprint_assignment::SprintAssignmentOutcome,
) -> serde_json::Map<String, Value> {
    let mut payload = serde_json::Map::new();
    payload.insert("status".into(), json!("ok"));
    payload.insert("action".into(), json!(outcome.action.as_str()));
    payload.insert("sprint_id".into(), json!(outcome.sprint_id));
    payload.insert("sprint_label".into(), json!(outcome.sprint_label));
    payload.insert("modified".into(), json!(outcome.modified));
    payload.insert("unchanged".into(), json!(outcome.unchanged));
    payload
}

pub(crate) fn handle_sprint_add(req: JsonRpcRequest) -> JsonRpcResponse {
    let MembershipRequest {
        mut storage,
        mut records,
        integrity,
        tasks,
        sprint_ref,
    } = try_rpc!(membership_request(&req));

    let allow_closed = req
        .params
        .get("allow_closed")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);

    let force_single = req
        .params
        .get("force_single")
        .and_then(|v| v.as_bool())
        .or_else(|| req.params.get("force").and_then(|v| v.as_bool()))
        .unwrap_or(false);

    let outcome = match sprint_assignment::assign_tasks(
        &mut storage,
        &mut records,
        &tasks,
        sprint_ref.as_deref(),
        allow_closed,
        force_single,
    ) {
        Ok(outcome) => outcome,
        Err(msg) => return err(req.id, -32602, msg.as_str(), None),
    };

    let mut payload = membership_payload(&outcome);
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
            integrity
                .after
                .missing_sprints
                .iter()
                .map(|id| Value::from(*id))
                .collect(),
        ),
    );
    if let Some(integrity) = integrity_value(&integrity) {
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
    let MembershipRequest {
        mut storage,
        mut records,
        integrity,
        tasks,
        sprint_ref,
    } = try_rpc!(membership_request(&req));

    let outcome = match sprint_assignment::remove_tasks(
        &mut storage,
        &mut records,
        &tasks,
        sprint_ref.as_deref(),
    ) {
        Ok(outcome) => outcome,
        Err(msg) => return err(req.id, -32602, msg.as_str(), None),
    };

    let mut payload = membership_payload(&outcome);
    payload.insert(
        "missing_sprints".to_string(),
        Value::Array(
            integrity
                .after
                .missing_sprints
                .iter()
                .map(|id| Value::from(*id))
                .collect(),
        ),
    );
    if let Some(integrity) = integrity_value(&integrity) {
        payload.insert("integrity".to_string(), integrity);
    }
    let payload = Value::Object(payload);

    text_result(req.id, &payload)
}

pub(crate) fn handle_sprint_delete(req: JsonRpcRequest) -> JsonRpcResponse {
    let resolver = try_rpc!(resolve_tasks_dir(&req.id));
    let mut storage = Storage::new(&resolver.path.clone());

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
            return internal_error(req.id, error.to_string());
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
            return domain_service_error(req.id, "Failed to delete sprint", &error);
        }
    }

    let mut records = match SprintService::list(&storage) {
        Ok(records) => records,
        Err(error) => {
            return internal_error(req.id, error.to_string());
        }
    };

    let integrity = match sprint_integrity::check_missing_sprints(
        &mut storage,
        &mut records,
        cleanup_missing,
        Some(sprint_id),
    ) {
        Ok(check) => check,
        Err(error) => {
            return err(
                req.id,
                -32603,
                "Failed to clean sprint references",
                Some(json!({"message": error.to_string()})),
            );
        }
    };

    let removed_references = integrity
        .cleanup
        .as_ref()
        .map(|outcome| outcome.removed_references)
        .unwrap_or(0);
    let updated_tasks = integrity
        .cleanup
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
    } else if !integrity.after.missing_sprints.is_empty() {
        let missing = integrity
            .after
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
    if let Some(integrity) = integrity_value(&integrity) {
        payload.insert("integrity".to_string(), integrity);
    }

    let payload = Value::Object(payload);

    ok(
        req.id,
        json!({
            "content": [
                { "type": "text", "text": summary.to_string() },
                { "type": "text", "text": serde_json::to_string_pretty(&payload).unwrap_or_else(|_| "{}".into()) }
            ]
        }),
    )
}

pub(crate) fn handle_sprint_reorder(req: JsonRpcRequest) -> JsonRpcResponse {
    let request: crate::api_types::TaskOrderRequest =
        match serde_json::from_value(req.params.clone()) {
            Ok(request) => request,
            Err(error) => {
                return err(
                    req.id,
                    -32602,
                    &format!("Invalid order request: {error}"),
                    None,
                );
            }
        };
    let resolver = match TasksDirectoryResolver::resolve(None, None) {
        Ok(resolver) => resolver,
        Err(error) => return err(req.id, -32603, &error, None),
    };
    let storage = Storage {
        root_path: resolver.path,
    };
    match crate::services::task_order::reorder(&storage, &request) {
        Ok(response) => ok(
            req.id,
            json!({"content": [{"type": "text", "text": serde_json::to_string(&response).expect("serializable order")}]}),
        ),
        Err(error) => {
            let code = match &error {
                crate::errors::LoTaRError::ValidationError(_)
                | crate::errors::LoTaRError::TaskNotFound(_)
                | crate::errors::LoTaRError::SprintNotFound(_)
                | crate::errors::LoTaRError::InvalidTaskId(_) => -32602,
                _ => -32603,
            };
            err(req.id, code, &error.to_string(), None)
        }
    }
}

pub(crate) fn handle_sprint_backlog(req: JsonRpcRequest) -> JsonRpcResponse {
    let resolver = try_rpc!(resolve_tasks_dir(&req.id));

    let project = req
        .params
        .get("project")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());
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
        .filter(|value| !value.is_null())
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

    // The shared task query (DEV-236): every task_list filter, plus query.
    let input = match super::tasks::task_list_input(&req.params) {
        Ok(input) => input,
        Err(message) => return err(req.id, -32602, &message, None),
    };
    let manual_order = input.sort_by.is_none();
    let base = match resolution::load_and_merge_configs(Some(resolver.path.as_path())) {
        Ok(config) => config,
        Err(error) => {
            return err(
                req.id,
                -32603,
                "Failed to resolve configuration",
                Some(json!({"message": error.to_string()})),
            );
        }
    };
    let query = match input.resolve(&base, resolver.path.as_path()) {
        Ok(query) => query,
        Err(error) => {
            return super::tasks::query_error(
                req.id,
                error,
                project.as_deref(),
                &resolver.path,
                &base,
            );
        }
    };
    let project_scope: Vec<String> = project.iter().cloned().collect();
    let enum_hints = EnumHints::from_resolved_config(&query.config, &project_scope);
    let options = SprintBacklogOptions {
        query,
        manual_order,
        limit: 0,
    };

    let mut storage = match Storage::try_open(&resolver.path.clone()) {
        Some(storage) => storage,
        None => {
            let mut payload = json!({
                "status": "ok", "count": 0, "total": 0, "cursor": 0,
                "limit": limit, "hasMore": false, "nextCursor": null,
                "truncated": false, "tasks": [], "missing_sprints": []
            });
            if let Some(hints) = enum_hints.as_ref() {
                payload["enumHints"] = enum_hints_to_value(hints);
            }
            return text_result(req.id, &payload);
        }
    };

    let mut records = match SprintService::list(&storage) {
        Ok(records) => records,
        Err(e) => {
            return internal_error(req.id, format!("Failed to load sprints: {}", e));
        }
    };

    let integrity = match sprint_integrity::check_missing_sprints(
        &mut storage,
        &mut records,
        cleanup_missing,
        None,
    ) {
        Ok(check) => check,
        Err(error) => {
            return internal_error(
                req.id,
                format!("Failed to clean up sprint references: {}", error),
            );
        }
    };

    let result = match sprint_assignment::fetch_backlog(&storage, &options) {
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
    payload.insert("limit".to_string(), Value::from(limit as u64));
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
            integrity
                .after
                .missing_sprints
                .iter()
                .map(|id| Value::from(*id))
                .collect(),
        ),
    );
    if let Some(integrity) = integrity_value(&integrity) {
        payload.insert("integrity".to_string(), integrity);
    }
    if let Some(hints) = enum_hints.as_ref() {
        payload.insert("enumHints".to_string(), enum_hints_to_value(hints));
    }

    let payload = Value::Object(payload);

    text_result(req.id, &payload)
}

//! `release_*` tools (DEV-393), the MCP side of `lotar release`.

use serde_json::{Value, json};

use super::super::{JsonRpcRequest, JsonRpcResponse, domain_service_error, err, parse_cursor_value, parse_limit_value, resolve_tasks_dir, text_result};
use crate::api_types::{
    ReleaseAssignmentResponse, ReleaseBacklogResponse, ReleaseDeleteResponse, ReleaseGetResponse, ReleaseListResponse, ReleaseMutationResponse,
};
use crate::errors::LoTaRError;
use crate::services::release_service::{ReleaseAssignmentAction, ReleasePatch, ReleaseRecord, ReleaseService};
use crate::storage::manager::Storage;
use crate::storage::release::Release;

fn string_param(params: &Value, key: &str) -> Option<String> {
    params.get(key).and_then(Value::as_str).map(str::to_string)
}

/// The `release` parameter: an id or a `#id`, version or label.
fn release_param(params: &Value) -> Result<String, &'static str> {
    match params.get("release") {
        Some(Value::Number(num)) => num.as_u64().map(|id| id.to_string()).ok_or("release must be a positive integer or a reference"),
        Some(Value::String(text)) if !text.trim().is_empty() => Ok(text.clone()),
        _ => Err("release is required: an id, '#<id>', version or label"),
    }
}

fn domain_error(req_id: Option<Value>, error: &LoTaRError) -> JsonRpcResponse {
    domain_service_error(req_id, "Release request failed", error)
}

fn resolve(req_id: &Option<Value>, root: &std::path::Path, params: &Value) -> Result<ReleaseRecord, Box<JsonRpcResponse>> {
    let reference = release_param(params).map_err(|message| Box::new(err(req_id.clone(), -32602, message, None)))?;
    ReleaseService::resolve(root, &reference).map_err(|error| Box::new(domain_error(req_id.clone(), &error)))
}

fn mutation_result(req_id: Option<Value>, root: &std::path::Path, record: &ReleaseRecord, changed: bool) -> JsonRpcResponse {
    let storage = Storage { root_path: root.to_path_buf() };
    let (release, _) = ReleaseService::detail(&storage, record);
    text_result(req_id, &ReleaseMutationResponse { status: "ok".to_string(), release, changed })
}

pub(crate) fn handle_release_list(req: JsonRpcRequest) -> JsonRpcResponse {
    let resolver = try_rpc!(resolve_tasks_dir(&req.id));
    let storage = Storage { root_path: resolver.path };
    match ReleaseService::list_items(&storage) {
        Ok(releases) => text_result(req.id, &ReleaseListResponse { status: "ok".to_string(), total: releases.len(), releases }),
        Err(error) => domain_error(req.id, &error),
    }
}

pub(crate) fn handle_release_get(req: JsonRpcRequest) -> JsonRpcResponse {
    let resolver = try_rpc!(resolve_tasks_dir(&req.id));
    let record = try_rpc!(resolve(&req.id, &resolver.path, &req.params));
    let storage = Storage { root_path: resolver.path };
    let (release, tasks) = ReleaseService::detail(&storage, &record);
    text_result(req.id, &ReleaseGetResponse { status: "ok".to_string(), release, tasks, history: record.release.history })
}

pub(crate) fn handle_release_create(req: JsonRpcRequest) -> JsonRpcResponse {
    let resolver = try_rpc!(resolve_tasks_dir(&req.id));
    let Some(version) = string_param(&req.params, "version") else {
        return err(req.id, -32602, "version is required", None);
    };
    let release = Release {
        version,
        label: string_param(&req.params, "label"),
        notes: string_param(&req.params, "notes"),
        planned_date: string_param(&req.params, "planned_date"),
        released_at: string_param(&req.params, "released_at"),
        ..Release::default()
    };
    match ReleaseService::create(&resolver.path, release) {
        Ok(record) => mutation_result(req.id, &resolver.path, &record, true),
        Err(error) => domain_error(req.id, &error),
    }
}

pub(crate) fn handle_release_update(req: JsonRpcRequest) -> JsonRpcResponse {
    let resolver = try_rpc!(resolve_tasks_dir(&req.id));
    let record = try_rpc!(resolve(&req.id, &resolver.path, &req.params));
    let field = |key: &str| string_param(&req.params, key);
    let patch = ReleasePatch::from_text(field("version"), field("label"), field("notes"), field("planned_date"), field("released_at"));
    if patch.is_empty() {
        return err(req.id, -32602, "Nothing to update: give version, label, notes, planned_date or released_at", None);
    }
    match ReleaseService::update(&resolver.path, record.id, &patch) {
        Ok((record, changed)) => mutation_result(req.id, &resolver.path, &record, changed),
        Err(error) => domain_error(req.id, &error),
    }
}

pub(crate) fn handle_release_release(req: JsonRpcRequest) -> JsonRpcResponse {
    let resolver = try_rpc!(resolve_tasks_dir(&req.id));
    let record = try_rpc!(resolve(&req.id, &resolver.path, &req.params));
    match ReleaseService::mark_released(&resolver.path, record.id, string_param(&req.params, "at")) {
        Ok((record, changed)) => mutation_result(req.id, &resolver.path, &record, changed),
        Err(error) => domain_error(req.id, &error),
    }
}

pub(crate) fn handle_release_reopen(req: JsonRpcRequest) -> JsonRpcResponse {
    let resolver = try_rpc!(resolve_tasks_dir(&req.id));
    let record = try_rpc!(resolve(&req.id, &resolver.path, &req.params));
    match ReleaseService::reopen(&resolver.path, record.id) {
        Ok((record, changed)) => mutation_result(req.id, &resolver.path, &record, changed),
        Err(error) => domain_error(req.id, &error),
    }
}

pub(crate) fn handle_release_delete(req: JsonRpcRequest) -> JsonRpcResponse {
    let resolver = try_rpc!(resolve_tasks_dir(&req.id));
    let record = try_rpc!(resolve(&req.id, &resolver.path, &req.params));
    let mut storage = Storage::new(&resolver.path);
    match ReleaseService::delete(&mut storage, record.id) {
        Ok(removed_from) => text_result(req.id, &ReleaseDeleteResponse { status: "ok".to_string(), deleted: true, release_id: record.id, removed_from }),
        Err(error) => domain_error(req.id, &error),
    }
}

pub(crate) fn handle_release_add(req: JsonRpcRequest) -> JsonRpcResponse {
    assign(req, ReleaseAssignmentAction::Add)
}

pub(crate) fn handle_release_move(req: JsonRpcRequest) -> JsonRpcResponse {
    assign(req, ReleaseAssignmentAction::Move)
}

pub(crate) fn handle_release_remove(req: JsonRpcRequest) -> JsonRpcResponse {
    assign(req, ReleaseAssignmentAction::Remove)
}

fn assign(req: JsonRpcRequest, action: ReleaseAssignmentAction) -> JsonRpcResponse {
    let resolver = try_rpc!(resolve_tasks_dir(&req.id));
    let reference = match release_param(&req.params) {
        Ok(reference) => reference,
        Err(message) => return err(req.id, -32602, message, None),
    };
    let named: Vec<String> = match req.params.get("tasks") {
        Some(Value::String(id)) => vec![id.clone()],
        Some(Value::Array(items)) => items.iter().filter_map(Value::as_str).map(str::to_string).collect(),
        _ => Vec::new(),
    };
    let config = match crate::config::resolution::load_and_merge_configs(Some(&resolver.path)) {
        Ok(config) => config,
        Err(error) => return domain_error(req.id, &LoTaRError::ValidationError(error.to_string())),
    };
    let mut storage = Storage::new(&resolver.path);
    let tasks = match ReleaseService::selected_tasks(&storage, &config, &named, string_param(&req.params, "query").as_deref()) {
        Ok(tasks) => tasks,
        Err(error) => return err(req.id, -32602, &error.to_string(), None),
    };
    match ReleaseService::assign(&mut storage, &reference, &tasks, action) {
        Ok(outcome) => text_result(
            req.id,
            &ReleaseAssignmentResponse {
                status: "ok".to_string(),
                action: action.as_str().to_string(),
                release_id: outcome.record.id,
                release_version: outcome.record.release.version.clone(),
                modified: outcome.modified,
                unchanged: outcome.unchanged,
            },
        ),
        Err(message) => domain_error(req.id, &LoTaRError::ValidationError(message)),
    }
}

pub(crate) fn handle_release_backlog(req: JsonRpcRequest) -> JsonRpcResponse {
    let resolver = try_rpc!(resolve_tasks_dir(&req.id));
    let limit = match parse_limit_value(req.params.get("limit"), 50) {
        Ok(limit) if (1..=200).contains(&limit) => limit,
        Ok(_) => return err(req.id, -32602, "limit must be between 1 and 200", None),
        Err(message) => return err(req.id, -32602, message, None),
    };
    let offset = match parse_cursor_value(req.params.get("cursor")) {
        Ok(offset) => offset,
        Err(message) => return err(req.id, -32602, message, None),
    };
    let mut input = crate::services::task_list::TaskQueryInput { project: string_param(&req.params, "project"), ..Default::default() };
    if let Some(query) = string_param(&req.params, "query")
        && let Err(error) = input.add_search(&crate::utils::search_syntax::parse(&query))
    {
        return err(req.id, -32602, &error.to_string(), None);
    }
    let config = match crate::config::resolution::load_and_merge_configs(Some(&resolver.path)) {
        Ok(config) => config,
        Err(error) => return domain_error(req.id, &LoTaRError::ValidationError(error.to_string())),
    };
    let storage = Storage { root_path: resolver.path };
    let tasks = match ReleaseService::backlog(&storage, input, &config) {
        Ok(tasks) => tasks,
        Err(error) => return err(req.id, -32602, &error.to_string(), None),
    };
    let total = tasks.len();
    let page: Vec<_> = tasks.into_iter().skip(offset).take(limit).map(|(_, task)| task).collect();
    let payload = ReleaseBacklogResponse { status: "ok".to_string(), total, count: page.len(), limit, offset, tasks: page };
    let mut value = serde_json::to_value(&payload).unwrap_or_else(|_| json!({}));
    if let Some(object) = value.as_object_mut() {
        let next = offset + payload.count;
        object.insert("hasMore".to_string(), json!(next < total));
        object.insert("nextCursor".to_string(), if next < total { json!(next) } else { Value::Null });
    }
    text_result(req.id, &value)
}

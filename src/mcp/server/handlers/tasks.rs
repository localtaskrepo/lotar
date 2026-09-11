use serde_json::{Value, json};

use super::super::hints::{EnumHints, enum_hints_to_value, make_enum_error_data};
use super::super::{
    JsonRpcRequest, JsonRpcResponse, MCP_DEFAULT_TASK_LIST_LIMIT, MCP_MAX_TASK_LIST_LIMIT, err, ok,
    parse_cursor_value, parse_limit_value,
};
use crate::api_types::{TaskCreate, TaskDTO, TaskListFilter, TaskUpdate};
use crate::cli::project::ProjectResolver;
use crate::cli::validation::CliValidator;
use crate::config::manager::ConfigManager;
use crate::errors::LoTaRError;
use crate::services::reference_service::ReferenceService;
use crate::services::task_service::TaskService;
use crate::storage::manager::Storage;
use crate::types::{TaskChange, TaskChangeLogEntry, TaskComment, TaskRelationships};
use crate::utils::git::find_repo_root;
use crate::utils::identity;
use crate::workspace::TasksDirectoryResolver;
use std::collections::BTreeMap;

fn now_rfc3339() -> String {
    chrono::Utc::now().to_rfc3339()
}

fn parse_sprint_ids(value: Option<&Value>) -> Result<Vec<u32>, &'static str> {
    fn parse_one(raw: &Value) -> Option<u32> {
        match raw {
            Value::Number(num) => num.as_u64().and_then(|v| u32::try_from(v).ok()),
            Value::String(text) => {
                let trimmed = text.trim();
                if trimmed.is_empty() {
                    return None;
                }
                trimmed
                    .strip_prefix('#')
                    .unwrap_or(trimmed)
                    .parse::<u32>()
                    .ok()
            }
            _ => None,
        }
    }

    let Some(v) = value else {
        return Ok(Vec::new());
    };

    let mut out = Vec::new();
    match v {
        Value::Null => {}
        Value::Array(items) => {
            for item in items {
                if let Some(id) = parse_one(item)
                    && id > 0
                {
                    out.push(id);
                }
            }
        }
        Value::Number(_) | Value::String(_) => {
            if let Some(id) = parse_one(v)
                && id > 0
            {
                out.push(id);
            }
        }
        _ => return Err("sprints must be a sprint id, '#<id>', or an array of them"),
    }
    out.sort_unstable();
    out.dedup();
    Ok(out)
}

fn parse_tags_params(params: &Value) -> Vec<String> {
    fn parse_multi(value: Option<&Value>) -> Vec<String> {
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

    let mut tags = Vec::new();

    if let Some(tag) = params.get("tag").and_then(|v| v.as_str()) {
        let trimmed = tag.trim();
        if !trimmed.is_empty() {
            tags.push(trimmed.to_string());
        }
    }

    tags.extend(parse_multi(params.get("tags")));
    tags.sort();
    tags.dedup();
    tags
}

/// Strict sprint id parsing shared by create and update: the value must be
/// absent/null or an array of positive integers; invalid entries are rejected
/// instead of silently dropped (matching REST).
fn parse_strict_sprint_ids(value: Option<&Value>) -> Result<Vec<u32>, String> {
    let Some(v) = value else {
        return Ok(Vec::new());
    };
    match v {
        Value::Null => Ok(Vec::new()),
        Value::Array(items) => {
            let mut ids = Vec::with_capacity(items.len());
            for item in items {
                let id = item
                    .as_u64()
                    .and_then(|raw| u32::try_from(raw).ok())
                    .filter(|id| *id > 0)
                    .ok_or_else(|| "sprints must be an array of positive integers".to_string())?;
                ids.push(id);
            }
            Ok(ids)
        }
        _ => Err("sprints must be an array of positive integers".to_string()),
    }
}

fn parse_task_update_patch(
    req_id: Option<Value>,
    patch_val: &Value,
) -> Result<TaskUpdate, JsonRpcResponse> {
    if !patch_val.is_object() {
        return Err(err(req_id, -32602, "Invalid patch (expected object)", None));
    }

    let mut patch = TaskUpdate::default();

    if let Some(s) = patch_val.get("title").and_then(|v| v.as_str()) {
        patch.title = Some(s.to_string());
    }

    if let Some(s) = patch_val.get("status").and_then(|v| v.as_str()) {
        patch.status = Some(s.to_string());
    }

    if let Some(s) = patch_val.get("priority").and_then(|v| v.as_str()) {
        patch.priority = Some(s.to_string());
    }

    if let Some(s) = patch_val
        .get("type")
        .or_else(|| patch_val.get("task_type"))
        .and_then(|v| v.as_str())
    {
        patch.task_type = Some(s.to_string());
    }

    // Clearable scalars: null clears (empty sentinel), string sets, matching
    // the REST tri-state contract.
    fn clearable_string(patch_val: &Value, key: &str) -> Option<String> {
        match patch_val.get(key) {
            Some(Value::Null) => Some(String::new()),
            Some(Value::String(s)) => Some(s.clone()),
            _ => None,
        }
    }
    if let Some(v) = clearable_string(patch_val, "reporter") {
        patch.reporter = Some(v);
    }
    if let Some(v) = clearable_string(patch_val, "assignee") {
        patch.assignee = Some(v);
    }
    if let Some(v) = clearable_string(patch_val, "due_date") {
        patch.due_date = Some(v);
    }
    if let Some(v) = clearable_string(patch_val, "effort") {
        patch.effort = Some(v);
    }
    if let Some(v) = clearable_string(patch_val, "description") {
        patch.description = Some(v);
    }

    if let Some(arr) = patch_val.get("tags") {
        match arr {
            Value::Null => patch.tags = Some(Vec::new()),
            Value::Array(items) => {
                patch.tags = Some(
                    items
                        .iter()
                        .filter_map(|x| x.as_str().map(|s| s.to_string()))
                        .collect(),
                );
            }
            _ => {
                return Err(err(
                    req_id,
                    -32602,
                    "tags must be an array of strings or null",
                    None,
                ));
            }
        }
    }

    if let Some(arr) = patch_val.get("acceptance_criteria") {
        match arr {
            Value::Null => patch.acceptance_criteria = Some(Vec::new()),
            Value::Array(items) => {
                patch.acceptance_criteria = Some(
                    items
                        .iter()
                        .filter_map(|x| x.as_str().map(|s| s.to_string()))
                        .collect(),
                );
            }
            _ => {
                return Err(err(
                    req_id,
                    -32602,
                    "acceptance_criteria must be an array of strings or null",
                    None,
                ));
            }
        }
    }

    if let Some(rel_val) = patch_val.get("relationships") {
        match rel_val {
            Value::Null => patch.relationships = Some(TaskRelationships::default()),
            value => match serde_json::from_value::<TaskRelationships>(value.clone()) {
                Ok(rel) => patch.relationships = Some(rel),
                Err(e) => {
                    return Err(err(
                        req_id,
                        -32602,
                        &format!("Invalid relationships payload: {}", e),
                        None,
                    ));
                }
            },
        }
    }

    if let Some(custom_fields_val) = patch_val.get("custom_fields") {
        match custom_fields_val {
            Value::Null => patch.custom_fields = Some(std::collections::HashMap::new()),
            Value::Object(obj) => {
                let mut custom_fields_map = std::collections::HashMap::new();
                for (k, v) in obj.iter() {
                    custom_fields_map.insert(k.clone(), crate::types::custom_value_from_json(v));
                }
                patch.custom_fields = Some(custom_fields_map);
            }
            _ => {
                return Err(err(
                    req_id,
                    -32602,
                    "custom_fields must be an object or null",
                    None,
                ));
            }
        }
    }

    if patch_val.get("sprints").is_some() {
        match patch_val.get("sprints") {
            Some(Value::Null) => patch.sprints = Some(Vec::new()),
            Some(Value::Array(items)) => {
                let mut ids = Vec::new();
                for item in items {
                    let valid = item
                        .as_u64()
                        .and_then(|v| u32::try_from(v).ok())
                        .is_some_and(|v| v > 0);
                    if !valid {
                        return Err(err(
                            req_id,
                            -32602,
                            "sprints must be an array of positive integers or null",
                            None,
                        ));
                    }
                    ids.push(item.as_u64().unwrap() as u32);
                }
                patch.sprints = Some(ids);
            }
            _ => {
                return Err(err(
                    req_id,
                    -32602,
                    "sprints must be an array of positive integers or null",
                    None,
                ));
            }
        }
    }

    Ok(patch)
}

/// Best-effort enum hints scoped to a project's resolved configuration so
/// validation errors can suggest the values that project actually allows.
fn enum_hints_for_project(
    tasks_root: &std::path::Path,
    project: Option<&str>,
) -> Option<EnumHints> {
    let scope: Vec<String> = project.iter().map(|p| p.to_string()).collect();
    let cfg = crate::config::resolution::config_for_project(tasks_root, project).ok()?;
    EnumHints::from_resolved_config(&cfg, &scope)
}

/// Map a service failure to a JSON-RPC error response. Validation errors
/// embed the project's allowed values in their message; `hint_data` carries
/// optional structured enum suggestions for MCP hosts.
fn task_mutation_error(
    req_id: Option<Value>,
    error: LoTaRError,
    fallback_code: i64,
    fallback_message: &str,
    hint_data: Option<Value>,
) -> JsonRpcResponse {
    match error {
        LoTaRError::TaskNotFound(id) => err(
            req_id,
            -32004,
            "Task not found",
            Some(json!({"message": format!("Task '{}' not found", id)})),
        ),
        LoTaRError::ValidationError(msg) => {
            // Membership failures keep the operation envelope (`Task
            // create/update failed` + `data.message`) clients already parse;
            // other validation errors use the invalid-params code with
            // optional enum suggestions.
            if msg.contains("configured members") {
                err(
                    req_id,
                    fallback_code,
                    fallback_message,
                    Some(json!({"message": msg})),
                )
            } else {
                err(
                    req_id,
                    -32602,
                    &format!("Validation failed: {}", msg),
                    hint_data,
                )
            }
        }
        other => err(
            req_id,
            fallback_code,
            fallback_message,
            Some(json!({"message": other.to_string()})),
        ),
    }
}

pub(crate) fn handle_task_create(req: JsonRpcRequest) -> JsonRpcResponse {
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

    let title = match req.params.get("title").and_then(|v| v.as_str()) {
        Some(s) if !s.trim().is_empty() => s.to_string(),
        _ => return err(req.id, -32602, "Missing required field: title", None),
    };
    let project = req
        .params
        .get("project")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());
    let enum_hints = enum_hints_for_project(resolver.path.as_path(), project.as_deref());
    let project_cfg =
        crate::config::resolution::config_for_project(resolver.path.as_path(), project.as_deref())
            .unwrap_or_else(|_| {
                crate::config::types::ResolvedConfig::from_global(
                    crate::config::types::GlobalConfig::default(),
                )
            });
    // Pre-validate enums with the target project's config so failures carry
    // structured suggestions; the service re-validates authoritatively.
    if let Some(raw) = req.params.get("status").and_then(|v| v.as_str())
        && let Err(e) = crate::types::TaskStatus::parse_with_config(raw, &project_cfg)
    {
        let data = enum_hints
            .as_ref()
            .and_then(|h| make_enum_error_data("status", raw, &h.statuses));
        return err(
            req.id,
            -32602,
            &format!("Status validation failed: {}", e),
            data,
        );
    }
    if let Some(raw) = req.params.get("priority").and_then(|v| v.as_str())
        && let Err(e) = crate::types::Priority::parse_with_config(raw, &project_cfg)
    {
        let data = enum_hints
            .as_ref()
            .and_then(|h| make_enum_error_data("priority", raw, &h.priorities));
        return err(
            req.id,
            -32602,
            &format!("Priority validation failed: {}", e),
            data,
        );
    }
    let type_param = req
        .params
        .get("type")
        .or_else(|| req.params.get("task_type"))
        .and_then(|v| v.as_str());
    if let Some(raw) = type_param
        && let Err(e) = crate::types::TaskType::parse_with_config(raw, &project_cfg)
    {
        let data = enum_hints
            .as_ref()
            .and_then(|h| make_enum_error_data("type", raw, &h.types));
        return err(
            req.id,
            -32602,
            &format!("Type validation failed: {}", e),
            data,
        );
    }

    let acceptance_criteria = req
        .params
        .get("acceptance_criteria")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|x| x.as_str().map(|s| s.to_string()))
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();

    let custom_fields_map = req
        .params
        .get("custom_fields")
        .and_then(|v| v.as_object())
        .map(|o| {
            let mut m = std::collections::HashMap::new();
            for (k, v) in o.iter() {
                m.insert(k.clone(), crate::types::custom_value_from_json(v));
            }
            m
        })
        .unwrap_or_default();
    let custom_fields = if custom_fields_map.is_empty() {
        None
    } else {
        Some(custom_fields_map)
    };

    let relationships = match req.params.get("relationships") {
        Some(value) => match serde_json::from_value::<TaskRelationships>(value.clone()) {
            Ok(rel) => {
                if rel.is_empty() {
                    None
                } else {
                    Some(rel)
                }
            }
            Err(e) => {
                return err(
                    req.id,
                    -32602,
                    &format!("Invalid relationships payload: {}", e),
                    None,
                );
            }
        },
        None => None,
    };

    fn opt_string(params: &Value, key: &str) -> Option<String> {
        params
            .get(key)
            .and_then(|v| v.as_str())
            .map(|s| s.to_string())
    }

    let dto = TaskCreate {
        title,
        project,
        // Enum strings are validated project-aware inside TaskService::create.
        status: opt_string(&req.params, "status"),
        priority: opt_string(&req.params, "priority"),
        task_type: opt_string(&req.params, "task_type").or_else(|| opt_string(&req.params, "type")),
        reporter: opt_string(&req.params, "reporter"),
        assignee: opt_string(&req.params, "assignee"),
        due_date: opt_string(&req.params, "due_date"),
        effort: opt_string(&req.params, "effort"),
        description: opt_string(&req.params, "description"),
        tags: req
            .params
            .get("tags")
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|x| x.as_str().map(|s| s.to_string()))
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default(),
        acceptance_criteria,
        relationships,
        custom_fields,
        sprints: match parse_strict_sprint_ids(req.params.get("sprints")) {
            Ok(ids) => ids,
            Err(msg) => return err(req.id, -32602, &msg, None),
        },
    };

    let mut storage = Storage::new(&resolver.path.clone());
    match TaskService::create(&mut storage, dto) {
        Ok(task) => {
            let response_body = make_task_create_payload(&task, &req.params, enum_hints.as_ref());
            ok(
                req.id,
                json!({
                    "content": [ { "type": "text", "text": serde_json::to_string_pretty(&response_body).unwrap_or_else(|_| "{}".into()) } ]
                }),
            )
        }
        Err(e) => task_mutation_error(req.id, e, -32000, "Task create failed", None),
    }
}

fn make_task_create_payload(
    task: &TaskDTO,
    params: &Value,
    enum_hints: Option<&EnumHints>,
) -> Value {
    let mut root = serde_json::Map::new();
    root.insert(
        "task".into(),
        serde_json::to_value(task).unwrap_or(Value::Null),
    );

    let mut metadata = serde_json::Map::new();
    let applied_defaults = applied_defaults_for_task_create(params, task);
    if !applied_defaults.is_empty() {
        metadata.insert("appliedDefaults".into(), Value::Array(applied_defaults));
    }
    if let Some(hints) = enum_hints {
        metadata.insert("enumHints".into(), enum_hints_to_value(hints));
    }
    if !metadata.is_empty() {
        root.insert("metadata".into(), Value::Object(metadata));
    }

    Value::Object(root)
}

fn applied_defaults_for_task_create(params: &Value, task: &TaskDTO) -> Vec<Value> {
    fn provided(params: &Value, key: &str) -> bool {
        params.get(key).is_some_and(|value| {
            if let Some(s) = value.as_str() {
                !s.trim().is_empty()
            } else {
                !value.is_null()
            }
        })
    }

    #[allow(clippy::needless_pass_by_value)]
    fn push_default(acc: &mut Vec<Value>, field: &str, value: Value) {
        acc.push(json!({ "field": field, "value": value }));
    }

    let mut defaults = Vec::new();
    if !provided(params, "priority") {
        push_default(
            &mut defaults,
            "priority",
            Value::String(task.priority.to_string()),
        );
    }
    let provided_type = provided(params, "type") || provided(params, "task_type");
    if !provided_type {
        push_default(
            &mut defaults,
            "type",
            Value::String(task.task_type.to_string()),
        );
    }
    if !provided(params, "status") {
        push_default(
            &mut defaults,
            "status",
            Value::String(task.status.to_string()),
        );
    }
    if !provided(params, "reporter")
        && let Some(reporter) = task.reporter.as_ref()
    {
        push_default(&mut defaults, "reporter", Value::String(reporter.clone()));
    }
    if !provided(params, "assignee")
        && let Some(assignee) = task.assignee.as_ref()
    {
        push_default(&mut defaults, "assignee", Value::String(assignee.clone()));
    }
    if params.get("tags").is_none() && !task.tags.is_empty() {
        push_default(
            &mut defaults,
            "tags",
            serde_json::to_value(&task.tags).unwrap_or(Value::Array(Vec::new())),
        );
    }

    defaults
}

pub(crate) fn handle_task_get(req: JsonRpcRequest) -> JsonRpcResponse {
    let Some(id) = req.params.get("id").and_then(|v| v.as_str()) else {
        return err(req.id, -32602, "Missing id", None);
    };
    let project = req.params.get("project").and_then(|v| v.as_str());
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
    let storage = Storage::new(&resolver.path);
    match TaskService::get(&storage, id, project) {
        Ok(task) => ok(
            req.id,
            json!({
                "content": [ { "type": "text", "text": serde_json::to_string_pretty(&task).unwrap_or_else(|_| "{}".into()) } ]
            }),
        ),
        Err(e) => err(
            req.id,
            -32004,
            "Task not found",
            Some(json!({"message": e.to_string()})),
        ),
    }
}

pub(crate) fn handle_task_update(req: JsonRpcRequest) -> JsonRpcResponse {
    let Some(id) = req.params.get("id").and_then(|v| v.as_str()) else {
        return err(req.id, -32602, "Missing id", None);
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
    let patch_val = req.params.get("patch").cloned().unwrap_or(json!({}));
    let patch = match parse_task_update_patch(req.id.clone(), &patch_val) {
        Ok(patch) => patch,
        Err(resp) => return resp,
    };

    // Pre-validate enum strings against the task's project configuration so
    // failures carry structured suggestions; the service re-validates
    // authoritatively with the same resolved config.
    let project_prefix = crate::storage::TaskId::parse(id)
        .map(|parsed| parsed.project)
        .unwrap_or_default();
    let project_cfg = crate::config::resolution::config_for_project(
        resolver.path.as_path(),
        Some(project_prefix.as_str()),
    )
    .unwrap_or_else(|_| {
        crate::config::types::ResolvedConfig::from_global(
            crate::config::types::GlobalConfig::default(),
        )
    });
    let enum_hints = enum_hints_for_project(resolver.path.as_path(), Some(&project_prefix));
    if let Some(raw) = patch.status.as_deref()
        && let Err(e) = crate::types::TaskStatus::parse_with_config(raw, &project_cfg)
    {
        let data = enum_hints
            .as_ref()
            .and_then(|h| make_enum_error_data("status", raw, &h.statuses));
        return err(
            req.id,
            -32602,
            &format!("Status validation failed: {}", e),
            data,
        );
    }
    if let Some(raw) = patch.priority.as_deref()
        && let Err(e) = crate::types::Priority::parse_with_config(raw, &project_cfg)
    {
        let data = enum_hints
            .as_ref()
            .and_then(|h| make_enum_error_data("priority", raw, &h.priorities));
        return err(
            req.id,
            -32602,
            &format!("Priority validation failed: {}", e),
            data,
        );
    }
    if let Some(raw) = patch.task_type.as_deref()
        && let Err(e) = crate::types::TaskType::parse_with_config(raw, &project_cfg)
    {
        let data = enum_hints
            .as_ref()
            .and_then(|h| make_enum_error_data("type", raw, &h.types));
        return err(
            req.id,
            -32602,
            &format!("Type validation failed: {}", e),
            data,
        );
    }

    let mut storage = Storage::new(&resolver.path.clone());
    match TaskService::update(&mut storage, id, patch) {
        Ok(task) => ok(
            req.id,
            json!({
                "content": [ { "type": "text", "text": serde_json::to_string_pretty(&task).unwrap_or_else(|_| "{}".into()) } ]
            }),
        ),
        Err(e) => task_mutation_error(req.id, e, -32005, "Task update failed", None),
    }
}

pub(crate) fn handle_task_comment_add(req: JsonRpcRequest) -> JsonRpcResponse {
    let id = match req.params.get("id").and_then(|v| v.as_str()) {
        Some(s) if !s.trim().is_empty() => s.trim().to_string(),
        _ => return err(req.id, -32602, "Missing id", None),
    };
    let text = match req.params.get("text").and_then(|v| v.as_str()) {
        Some(s) if !s.trim().is_empty() => s.trim().to_string(),
        _ => return err(req.id, -32602, "Missing text", None),
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

    let mut storage = Storage::new(&resolver.path.clone());

    let dto = match TaskService::add_comment(&mut storage, &id, &text) {
        Ok(dto) => dto,
        Err(error) => {
            let msg = error.to_string();
            if msg.contains("not found") {
                return err(
                    req.id,
                    -32004,
                    "Task not found",
                    Some(json!({"message": format!("Task '{}' not found", id)})),
                );
            }
            return err(
                req.id,
                -32603,
                "Internal error",
                Some(json!({"message": msg})),
            );
        }
    };

    let payload = json!({
        "status": "ok",
        "action": "comment_add",
        "id": id,
        "task": dto,
    });

    ok(
        req.id,
        json!({
            "content": [ { "type": "text", "text": serde_json::to_string_pretty(&payload).unwrap_or_else(|_| "{}".into()) } ]
        }),
    )
}

pub(crate) fn handle_task_comment_update(req: JsonRpcRequest) -> JsonRpcResponse {
    let id = match req.params.get("id").and_then(|v| v.as_str()) {
        Some(s) if !s.trim().is_empty() => s.trim().to_string(),
        _ => return err(req.id, -32602, "Missing id", None),
    };
    let index = match req.params.get("index") {
        Some(Value::Number(num)) => num.as_u64().and_then(|v| usize::try_from(v).ok()),
        Some(Value::String(text)) => text.trim().parse::<usize>().ok(),
        _ => None,
    };
    let Some(index) = index else {
        return err(req.id, -32602, "Missing index", None);
    };
    let text = match req.params.get("text").and_then(|v| v.as_str()) {
        Some(s) if !s.trim().is_empty() => s.trim().to_string(),
        _ => return err(req.id, -32602, "Missing text", None),
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

    let mut storage = Storage::new(&resolver.path.clone());

    let dto = match TaskService::update_comment(&mut storage, &id, index, &text) {
        Ok(dto) => dto,
        Err(error) => {
            let msg = error.to_string();
            if msg.contains("not found") {
                return err(
                    req.id,
                    -32004,
                    "Task not found",
                    Some(json!({"message": format!("Task '{}' not found", id)})),
                );
            }
            if msg.contains("Invalid comment index") {
                return err(req.id, -32602, "Invalid comment index", None);
            }
            return err(
                req.id,
                -32603,
                "Internal error",
                Some(json!({"message": msg})),
            );
        }
    };

    let payload = json!({
        "status": "ok",
        "action": "comment_update",
        "id": id,
        "index": index,
        "task": dto,
    });

    ok(
        req.id,
        json!({
            "content": [ { "type": "text", "text": serde_json::to_string_pretty(&payload).unwrap_or_else(|_| "{}".into()) } ]
        }),
    )
}

pub(crate) fn handle_task_bulk_update(req: JsonRpcRequest) -> JsonRpcResponse {
    let ids = match req.params.get("ids").and_then(|v| v.as_array()) {
        Some(arr) => arr
            .iter()
            .filter_map(|v| v.as_str())
            .map(|s| s.trim())
            .filter(|s| !s.is_empty())
            .map(|s| s.to_string())
            .collect::<Vec<_>>(),
        None => return err(req.id, -32602, "Missing ids", None),
    };
    if ids.is_empty() {
        return err(req.id, -32602, "ids must not be empty", None);
    }

    let patch_val = req.params.get("patch").cloned().unwrap_or(json!({}));

    let stop_on_error = req
        .params
        .get("stop_on_error")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);

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

    // Shape errors fail the whole call before any mutation; per-task enum
    // validation happens inside TaskService against each task's project.
    let patch = match parse_task_update_patch(req.id.clone(), &patch_val) {
        Ok(patch) => patch,
        Err(resp) => return resp,
    };

    let mut storage = Storage::new(&resolver.path.clone());
    let mut updated: Vec<TaskDTO> = Vec::new();
    let mut failed: Vec<Value> = Vec::new();

    for id in ids {
        match TaskService::update(&mut storage, &id, patch.clone()) {
            Ok(task) => updated.push(task),
            Err(e) => {
                failed.push(json!({"id": id, "error": e.to_string()}));
                if stop_on_error {
                    break;
                }
            }
        }
    }

    let payload = json!({
        "status": "ok",
        "updated": updated,
        "failed": failed,
    });

    ok(
        req.id,
        json!({
            "content": [ { "type": "text", "text": serde_json::to_string_pretty(&payload).unwrap_or_else(|_| "{}".into()) } ]
        }),
    )
}

pub(crate) fn handle_task_bulk_comment_add(req: JsonRpcRequest) -> JsonRpcResponse {
    let ids = match req.params.get("ids").and_then(|v| v.as_array()) {
        Some(arr) => arr
            .iter()
            .filter_map(|v| v.as_str())
            .map(|s| s.trim())
            .filter(|s| !s.is_empty())
            .map(|s| s.to_string())
            .collect::<Vec<_>>(),
        None => return err(req.id, -32602, "Missing ids", None),
    };
    if ids.is_empty() {
        return err(req.id, -32602, "ids must not be empty", None);
    }
    let text = match req.params.get("text").and_then(|v| v.as_str()) {
        Some(s) if !s.trim().is_empty() => s.trim().to_string(),
        _ => return err(req.id, -32602, "Missing text", None),
    };
    let stop_on_error = req
        .params
        .get("stop_on_error")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);

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

    let mut storage = Storage::new(&resolver.path.clone());
    let mut updated: Vec<TaskDTO> = Vec::new();
    let mut failed: Vec<Value> = Vec::new();

    for id in ids {
        let project_prefix = crate::storage::TaskId::parse(&id)
            .ok()
            .map(|parsed| parsed.project)
            .unwrap_or_default();
        let mut task = match storage.get(&id, &project_prefix) {
            Some(task) => task,
            None => {
                failed.push(json!({"id": id, "error": "Task not found"}));
                if stop_on_error {
                    break;
                }
                continue;
            }
        };

        let now = now_rfc3339();
        task.comments.push(TaskComment {
            date: now.clone(),
            text: text.clone(),
        });
        task.history.push(TaskChangeLogEntry {
            at: now.clone(),
            actor: identity::resolve_current_user(Some(resolver.path.as_path())),
            changes: vec![TaskChange {
                field: "comment_added".into(),
                old: None,
                new: None,
            }],
        });
        task.modified = now;

        if let Err(error) = storage.edit(&id, &task) {
            failed.push(json!({"id": id, "error": error.to_string()}));
            if stop_on_error {
                break;
            }
            continue;
        }

        match TaskService::get(&storage, &id, Some(&project_prefix)) {
            Ok(dto) => updated.push(dto),
            Err(error) => {
                failed.push(json!({"id": id, "error": error.to_string()}));
                if stop_on_error {
                    break;
                }
            }
        }
    }

    let payload = json!({
        "status": "ok",
        "updated": updated,
        "failed": failed,
    });

    ok(
        req.id,
        json!({
            "content": [ { "type": "text", "text": serde_json::to_string_pretty(&payload).unwrap_or_else(|_| "{}".into()) } ]
        }),
    )
}

pub(crate) fn handle_task_bulk_reference_add(req: JsonRpcRequest) -> JsonRpcResponse {
    handle_task_bulk_reference_mutation(req, true)
}

pub(crate) fn handle_task_bulk_reference_remove(req: JsonRpcRequest) -> JsonRpcResponse {
    handle_task_bulk_reference_mutation(req, false)
}

fn handle_task_bulk_reference_mutation(req: JsonRpcRequest, is_add: bool) -> JsonRpcResponse {
    let ids = match req.params.get("ids").and_then(|v| v.as_array()) {
        Some(arr) => arr
            .iter()
            .filter_map(|v| v.as_str())
            .map(|s| s.trim())
            .filter(|s| !s.is_empty())
            .map(|s| s.to_string())
            .collect::<Vec<_>>(),
        None => return err(req.id, -32602, "Missing ids", None),
    };
    if ids.is_empty() {
        return err(req.id, -32602, "ids must not be empty", None);
    }
    let kind = match req.params.get("kind").and_then(|v| v.as_str()) {
        Some(s) if !s.trim().is_empty() => s.trim().to_ascii_lowercase(),
        _ => return err(req.id, -32602, "Missing kind", None),
    };
    let value = match req.params.get("value").and_then(|v| v.as_str()) {
        Some(s) if !s.trim().is_empty() => s.trim().to_string(),
        _ => return err(req.id, -32602, "Missing value", None),
    };

    let stop_on_error = req
        .params
        .get("stop_on_error")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);

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

    let mut storage = Storage::new(&resolver.path.clone());
    let repo_root = if kind == "code" || kind == "file" {
        match find_repo_root(storage.root_path.as_path()) {
            Some(root) => Some(root),
            None => {
                return err(
                    req.id,
                    -32000,
                    if is_add {
                        "Task reference add failed"
                    } else {
                        "Task reference remove failed"
                    },
                    Some(json!({"message": "Unable to locate git repository"})),
                );
            }
        }
    } else {
        None
    };

    let mut updated: Vec<Value> = Vec::new();
    let mut failed: Vec<Value> = Vec::new();

    // File references that live inside the attachments store participate in
    // the blob lifecycle: hold the cross-process store lock across the whole
    // batch (store-lock -> task-lock order) so adds/detaches serialize with
    // upload creation and remove reclamation. Other kinds and outside paths
    // stay unlocked.
    let _store_guard = if kind == "file" {
        let first_project = ids
            .first()
            .and_then(|id| crate::storage::TaskId::parse(id).ok())
            .map(|parsed| parsed.project);
        let cfg =
            crate::config::resolution::config_for_project(&resolver.path, first_project.as_deref())
                .unwrap_or_else(|_| {
                    crate::config::types::ResolvedConfig::from_global(
                        crate::config::types::GlobalConfig::default(),
                    )
                });
        match crate::services::attachment_service::AttachmentService::lock_store_for_repo_path(
            &resolver.path,
            &cfg,
            repo_root.as_deref().unwrap_or(&resolver.path),
            &value,
        ) {
            Ok(guard) => guard,
            Err(e) => {
                return err(
                    req.id,
                    -32000,
                    "Task reference update failed",
                    Some(json!({"message": e.to_string()})),
                );
            }
        }
    } else {
        None
    };

    for id in ids {
        let normalized_id = if let Some(project_override) =
            req.params.get("project").and_then(|v| v.as_str())
        {
            let mut project_resolver = match ProjectResolver::new(&resolver) {
                Ok(r) => r,
                Err(e) => {
                    failed.push(json!({"id": id, "error": format!("Failed to initialize project resolver: {}", e)}));
                    if stop_on_error {
                        break;
                    }
                    continue;
                }
            };
            match project_resolver.get_full_task_id(&id, Some(project_override)) {
                Ok(full) => full,
                Err(e) => {
                    failed.push(json!({"id": id, "error": e}));
                    if stop_on_error {
                        break;
                    }
                    continue;
                }
            }
        } else {
            id.clone()
        };

        let result: Result<(TaskDTO, bool), String> = match (kind.as_str(), is_add) {
            ("link", true) => {
                ReferenceService::attach_link_reference(&mut storage, &normalized_id, &value)
            }
            ("link", false) => {
                ReferenceService::detach_link_reference(&mut storage, &normalized_id, &value)
            }
            ("code", true) => match repo_root.as_deref() {
                Some(root) => ReferenceService::attach_code_reference(
                    &mut storage,
                    root,
                    &normalized_id,
                    &value,
                ),
                None => Err(LoTaRError::ValidationError(
                    "unable to locate git repository".to_string(),
                )),
            },
            ("code", false) => {
                ReferenceService::detach_code_reference(&mut storage, &normalized_id, &value)
            }
            ("file", true) => match repo_root.as_deref() {
                Some(root) => ReferenceService::attach_file_reference(
                    &mut storage,
                    root,
                    &normalized_id,
                    &value,
                ),
                None => Err(LoTaRError::ValidationError(
                    "unable to locate git repository".to_string(),
                )),
            },
            ("file", false) => match repo_root.as_deref() {
                Some(root) => ReferenceService::detach_file_reference(
                    &mut storage,
                    root,
                    &normalized_id,
                    &value,
                ),
                None => Err(LoTaRError::ValidationError(
                    "unable to locate git repository".to_string(),
                )),
            },
            ("jira", true) => ReferenceService::attach_platform_reference(
                &mut storage,
                &normalized_id,
                "jira",
                &value,
            ),
            ("jira", false) => ReferenceService::detach_platform_reference(
                &mut storage,
                &normalized_id,
                "jira",
                &value,
            ),
            ("github", true) => ReferenceService::attach_platform_reference(
                &mut storage,
                &normalized_id,
                "github",
                &value,
            ),
            ("github", false) => ReferenceService::detach_platform_reference(
                &mut storage,
                &normalized_id,
                "github",
                &value,
            ),
            _ => {
                return err(
                    req.id,
                    -32602,
                    "Invalid kind",
                    Some(json!({"message": "kind must be one of: link, file, code, jira, github"})),
                );
            }
        }
        .map_err(|e| e.to_string());

        match result {
            Ok((task, changed)) => {
                updated.push(json!({"id": normalized_id, "changed": changed, "task": task}))
            }
            Err(error) => {
                failed.push(json!({"id": normalized_id, "error": error}));
                if stop_on_error {
                    break;
                }
            }
        }
    }

    let payload = json!({
        "status": "ok",
        "action": if is_add { "add" } else { "remove" },
        "kind": kind,
        "value": value,
        "updated": updated,
        "failed": failed,
    });

    ok(
        req.id,
        json!({
            "content": [ { "type": "text", "text": serde_json::to_string_pretty(&payload).unwrap_or_else(|_| "{}".into()) } ]
        }),
    )
}

pub(crate) fn handle_task_reference_add(req: JsonRpcRequest) -> JsonRpcResponse {
    handle_task_reference_mutation(req, true)
}

pub(crate) fn handle_task_reference_remove(req: JsonRpcRequest) -> JsonRpcResponse {
    handle_task_reference_mutation(req, false)
}

fn handle_task_reference_mutation(req: JsonRpcRequest, is_add: bool) -> JsonRpcResponse {
    let id = match req.params.get("id").and_then(|v| v.as_str()) {
        Some(s) if !s.trim().is_empty() => s.trim().to_string(),
        _ => return err(req.id, -32602, "Missing id", None),
    };
    let project = req.params.get("project").and_then(|v| v.as_str());
    let kind = match req.params.get("kind").and_then(|v| v.as_str()) {
        Some(s) if !s.trim().is_empty() => s.trim().to_ascii_lowercase(),
        _ => return err(req.id, -32602, "Missing kind", None),
    };
    let value = match req.params.get("value").and_then(|v| v.as_str()) {
        Some(s) if !s.trim().is_empty() => s.trim().to_string(),
        _ => return err(req.id, -32602, "Missing value", None),
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

    let mut project_resolver = match ProjectResolver::new(&resolver) {
        Ok(r) => r,
        Err(e) => {
            return err(
                req.id,
                -32603,
                "Internal error",
                Some(json!({"message": format!("Failed to initialize project resolver: {}", e)})),
            );
        }
    };

    let full_id = match project_resolver.get_full_task_id(&id, project) {
        Ok(v) => v,
        Err(e) => {
            return err(
                req.id,
                -32602,
                "Invalid task id",
                Some(json!({"message": e})),
            );
        }
    };

    // File references that live inside the attachments store participate in
    // the blob lifecycle: hold the cross-process store lock across the
    // mutation (store-lock -> task-lock order), exactly like the batch
    // reference handler, REST upload/remove, and the CLI. Other kinds and
    // outside paths stay unlocked.
    let _store_guard = if kind == "file" {
        let repo_root = match find_repo_root(resolver.path.as_path()) {
            Some(root) => root,
            None => {
                return err(
                    req.id,
                    -32000,
                    if is_add {
                        "Task reference add failed"
                    } else {
                        "Task reference remove failed"
                    },
                    Some(json!({"message": "Unable to locate git repository"})),
                );
            }
        };
        let project = crate::storage::TaskId::parse(&full_id)
            .ok()
            .map(|parsed| parsed.project);
        let cfg = crate::config::resolution::config_for_project(&resolver.path, project.as_deref())
            .unwrap_or_else(|_| {
                crate::config::types::ResolvedConfig::from_global(
                    crate::config::types::GlobalConfig::default(),
                )
            });
        match crate::services::attachment_service::AttachmentService::lock_store_for_repo_path(
            &resolver.path,
            &cfg,
            &repo_root,
            &value,
        ) {
            Ok(guard) => guard,
            Err(e) => {
                return err(
                    req.id,
                    -32000,
                    if is_add {
                        "Task reference add failed"
                    } else {
                        "Task reference remove failed"
                    },
                    Some(json!({"message": e.to_string()})),
                );
            }
        }
    } else {
        None
    };

    let mut storage = Storage::new(&resolver.path);
    let result: Result<(TaskDTO, bool), String> = match (kind.as_str(), is_add) {
        ("link", true) => ReferenceService::attach_link_reference(&mut storage, &full_id, &value),
        ("link", false) => ReferenceService::detach_link_reference(&mut storage, &full_id, &value),
        ("code", true) => {
            let repo_root = match find_repo_root(storage.root_path.as_path()) {
                Some(root) => root,
                None => {
                    return err(
                        req.id,
                        -32000,
                        "Task reference add failed",
                        Some(json!({"message": "Unable to locate git repository"})),
                    );
                }
            };
            ReferenceService::attach_code_reference(&mut storage, &repo_root, &full_id, &value)
        }
        ("code", false) => ReferenceService::detach_code_reference(&mut storage, &full_id, &value),
        ("file", true) => {
            let repo_root = match find_repo_root(storage.root_path.as_path()) {
                Some(root) => root,
                None => {
                    return err(
                        req.id,
                        -32000,
                        "Task reference add failed",
                        Some(json!({"message": "Unable to locate git repository"})),
                    );
                }
            };
            ReferenceService::attach_file_reference(&mut storage, &repo_root, &full_id, &value)
        }
        ("file", false) => {
            let repo_root = match find_repo_root(storage.root_path.as_path()) {
                Some(root) => root,
                None => {
                    return err(
                        req.id,
                        -32000,
                        "Task reference remove failed",
                        Some(json!({"message": "Unable to locate git repository"})),
                    );
                }
            };
            ReferenceService::detach_file_reference(&mut storage, &repo_root, &full_id, &value)
        }
        ("jira", true) => {
            ReferenceService::attach_platform_reference(&mut storage, &full_id, "jira", &value)
        }
        ("jira", false) => {
            ReferenceService::detach_platform_reference(&mut storage, &full_id, "jira", &value)
        }
        ("github", true) => {
            ReferenceService::attach_platform_reference(&mut storage, &full_id, "github", &value)
        }
        ("github", false) => {
            ReferenceService::detach_platform_reference(&mut storage, &full_id, "github", &value)
        }
        _ => {
            return err(
                req.id,
                -32602,
                "Invalid kind",
                Some(json!({"message": "kind must be one of: link, file, code, jira, github"})),
            );
        }
    }
    .map_err(|e| e.to_string());

    match result {
        Ok((task, changed)) => {
            let payload = json!({
                "task": task,
                "changed": changed,
                "action": if is_add { "add" } else { "remove" },
                "kind": kind,
                "value": value,
                "id": full_id,
            });
            ok(
                req.id,
                json!({
                    "content": [ { "type": "text", "text": serde_json::to_string_pretty(&payload).unwrap_or_else(|_| "{}".into()) } ]
                }),
            )
        }
        Err(message) => err(
            req.id,
            -32000,
            if is_add {
                "Task reference add failed"
            } else {
                "Task reference remove failed"
            },
            Some(json!({"message": message})),
        ),
    }
}

pub(crate) fn handle_task_delete(req: JsonRpcRequest) -> JsonRpcResponse {
    let Some(id) = req.params.get("id").and_then(|v| v.as_str()) else {
        return err(req.id, -32602, "Missing id", None);
    };
    let project = req.params.get("project").and_then(|v| v.as_str());
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
    let mut storage = Storage::new(&resolver.path);
    match TaskService::delete(&mut storage, id, project) {
        Ok(deleted) => ok(
            req.id,
            json!({
                "content": [ { "type": "text", "text": format!("deleted={}", deleted) } ]
            }),
        ),
        Err(e) => err(
            req.id,
            -32006,
            "Task delete failed",
            Some(json!({"message": e.to_string()})),
        ),
    }
}

pub(crate) fn handle_task_list(req: JsonRpcRequest) -> JsonRpcResponse {
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
    let cfg_mgr = match ConfigManager::new_manager_with_tasks_dir_readonly(&resolver.path) {
        Ok(m) => m,
        Err(e) => {
            return err(
                req.id,
                -32603,
                "Internal error",
                Some(json!({"message": format!("Failed to load config: {}", e)})),
            );
        }
    };
    let cfg = cfg_mgr.get_resolved_config();
    let validator = CliValidator::new(cfg);

    fn parse_vec<T, F>(v: Option<&Value>, f: F) -> Vec<T>
    where
        F: Fn(&str) -> Result<T, String>,
    {
        match v {
            Some(Value::String(s)) => f(s).ok().into_iter().collect(),
            Some(Value::Array(arr)) => arr
                .iter()
                .filter_map(|it| it.as_str().and_then(|s| f(s).ok()))
                .collect(),
            _ => vec![],
        }
    }

    let status = parse_vec(req.params.get("status"), |s| validator.validate_status(s));
    let priority = parse_vec(req.params.get("priority"), |s| {
        validator.validate_priority(s)
    });
    let task_type = parse_vec(
        req.params
            .get("type")
            .or_else(|| req.params.get("task_type")),
        |s| validator.validate_task_type(s),
    );
    let project = req
        .params
        .get("project")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());
    let project_scope: Vec<String> = project.iter().cloned().collect();
    let enum_hints = EnumHints::from_resolved_config(cfg, &project_scope);
    let tags = parse_tags_params(&req.params);
    let text_query = req
        .params
        .get("search")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());

    let mut custom_fields: BTreeMap<String, Vec<String>> = BTreeMap::new();
    if let Some(raw_fields) = req.params.get("custom_fields") {
        let Some(map) = raw_fields.as_object() else {
            return err(
                req.id,
                -32602,
                "custom_fields must be an object with string or array values",
                None,
            );
        };
        for (name, value) in map.iter() {
            let mut collected: Vec<String> = Vec::new();
            match value {
                Value::String(s) => {
                    let trimmed = s.trim();
                    if !trimmed.is_empty() {
                        collected.push(trimmed.to_string());
                    }
                }
                Value::Array(items) => {
                    for item in items {
                        if let Some(s) = item.as_str() {
                            let trimmed = s.trim();
                            if !trimmed.is_empty() {
                                collected.push(trimmed.to_string());
                            }
                        }
                    }
                }
                Value::Null => {}
                _ => {
                    return err(
                        req.id,
                        -32602,
                        "custom_fields entries must be strings or arrays of strings",
                        None,
                    );
                }
            }
            if !collected.is_empty() {
                custom_fields.insert(name.clone(), collected);
            }
        }
    }

    let mut filter = TaskListFilter {
        status,
        priority,
        task_type,
        project: project.clone(),
        tags,
        text_query,
        sprints: match parse_sprint_ids(req.params.get("sprints")) {
            Ok(v) => v,
            Err(msg) => return err(req.id, -32602, msg, None),
        },
        assignee: Vec::new(),
        assignee_none: false,
        custom_fields,
    };

    // Assignee filter (supports @me; resolved here, matched by TaskService::list)
    if let Some(raw) = req.params.get("assignee").and_then(|v| v.as_str()) {
        let trimmed = raw.trim();
        if !trimmed.is_empty() {
            if trimmed.eq_ignore_ascii_case("__none__") {
                filter.assignee_none = true;
            } else if trimmed.eq_ignore_ascii_case("@me") {
                // Fail closed: an unresolvable identity must never widen the
                // query to all tasks.
                match identity::resolve_current_user(Some(resolver.path.as_path())) {
                    Some(user) => filter.assignee.push(user),
                    None => {
                        return err(
                            req.id,
                            -32002,
                            "Could not resolve @me: no identity configured",
                            Some(
                                json!({"message": "Set default_reporter in config, git user.name, or USER environment"}),
                            ),
                        );
                    }
                }
            } else {
                filter.assignee.push(trimmed.to_string());
            }
        }
    }

    let storage = Storage::new(&resolver.path.clone());
    let tasks = TaskService::list(&storage, &filter)
        .into_iter()
        .map(|(_, t)| t)
        .collect::<Vec<_>>();
    let limit = match parse_limit_value(req.params.get("limit"), MCP_DEFAULT_TASK_LIST_LIMIT) {
        Ok(value) if (1..=MCP_MAX_TASK_LIST_LIMIT).contains(&value) => value,
        Ok(_) => {
            return err(
                req.id,
                -32602,
                &format!("limit must be between 1 and {}", MCP_MAX_TASK_LIST_LIMIT),
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
        Ok(value) => value,
        Err(msg) => return err(req.id, -32602, msg, None),
    };
    let total = tasks.len();
    let start = cursor.min(total);
    let end = (start + limit).min(total);
    let page = tasks[start..end].to_vec();
    let next_cursor = if end < total { Some(end) } else { None };
    let page_number = start.checked_div(limit).map(|q| q + 1).unwrap_or(1);
    let total_pages = if limit == 0 {
        1
    } else {
        total.div_ceil(limit).max(1)
    };
    let shown_start = if page.is_empty() { 0 } else { start + 1 };
    let shown_end = end;
    let has_more = next_cursor.is_some();
    let message = if total == 0 {
        "No tasks match the filters.".to_string()
    } else if has_more {
        let remaining = total - end;
        format!(
            "Showing tasks {}–{} of {} (page {} of {}). {} more task(s) available — call task_list again with cursor={} to fetch the next page.",
            shown_start, shown_end, total, page_number, total_pages, remaining, end
        )
    } else if start > 0 {
        format!(
            "Showing tasks {}–{} of {} (page {} of {}, last page).",
            shown_start, shown_end, total, page_number, total_pages
        )
    } else {
        format!("Showing all {} matching task(s).", total)
    };

    let mut payload = serde_json::Map::new();
    payload.insert("status".into(), Value::String("ok".into()));
    payload.insert("message".into(), Value::String(message));
    payload.insert("count".into(), Value::from(page.len() as u64));
    payload.insert("total".into(), Value::from(total as u64));
    payload.insert("cursor".into(), Value::from(start as u64));
    payload.insert("limit".into(), Value::from(limit as u64));
    payload.insert("page".into(), Value::from(page_number as u64));
    payload.insert("totalPages".into(), Value::from(total_pages as u64));
    payload.insert("hasMore".into(), Value::Bool(has_more));
    payload.insert(
        "nextCursor".into(),
        next_cursor
            .map(|pos| Value::from(pos as u64))
            .unwrap_or(Value::Null),
    );
    payload.insert(
        "tasks".into(),
        serde_json::to_value(&page).unwrap_or_else(|_| Value::Array(Vec::new())),
    );
    if let Some(hints) = enum_hints {
        payload.insert("enumHints".into(), enum_hints_to_value(&hints));
    }

    ok(
        req.id,
        json!({
            "content": [ { "type": "text", "text": serde_json::to_string_pretty(&Value::Object(payload)).unwrap_or_else(|_| "{}".into()) } ]
        }),
    )
}

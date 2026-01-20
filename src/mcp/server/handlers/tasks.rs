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
use crate::services::reference_service::ReferenceService;
use crate::services::task_service::TaskService;
use crate::storage::manager::Storage;
use crate::types::{
    CustomFieldValue, TaskChange, TaskChangeLogEntry, TaskComment, TaskRelationships,
};
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

fn parse_task_update_patch(
    req_id: Option<Value>,
    tasks_root: &std::path::Path,
    validator: &CliValidator,
    enum_hints: Option<&EnumHints>,
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
        match validator.validate_status(s) {
            Ok(v) => patch.status = Some(v),
            Err(e) => {
                let data = enum_hints.and_then(|h| make_enum_error_data("status", s, &h.statuses));
                return Err(err(
                    req_id,
                    -32602,
                    &format!("Status validation failed: {}", e),
                    data,
                ));
            }
        }
    }

    if let Some(s) = patch_val.get("priority").and_then(|v| v.as_str()) {
        match validator.validate_priority(s) {
            Ok(v) => patch.priority = Some(v),
            Err(e) => {
                let data =
                    enum_hints.and_then(|h| make_enum_error_data("priority", s, &h.priorities));
                return Err(err(
                    req_id,
                    -32602,
                    &format!("Priority validation failed: {}", e),
                    data,
                ));
            }
        }
    }

    if let Some(s) = patch_val
        .get("type")
        .or_else(|| patch_val.get("task_type"))
        .and_then(|v| v.as_str())
    {
        match validator.validate_task_type(s) {
            Ok(v) => patch.task_type = Some(v),
            Err(e) => {
                let data = enum_hints.and_then(|h| make_enum_error_data("type", s, &h.types));
                return Err(err(
                    req_id,
                    -32602,
                    &format!("Type validation failed: {}", e),
                    data,
                ));
            }
        }
    }

    if let Some(s) = patch_val.get("reporter").and_then(|v| v.as_str()) {
        patch.reporter = identity::resolve_me_alias(s, Some(tasks_root));
    }
    if let Some(s) = patch_val.get("assignee").and_then(|v| v.as_str()) {
        patch.assignee = identity::resolve_me_alias(s, Some(tasks_root));
    }

    if let Some(s) = patch_val.get("due_date").and_then(|v| v.as_str()) {
        patch.due_date = Some(s.to_string());
    }
    if let Some(s) = patch_val.get("effort").and_then(|v| v.as_str()) {
        patch.effort = Some(s.to_string());
    }
    if let Some(s) = patch_val.get("description").and_then(|v| v.as_str()) {
        patch.description = Some(s.to_string());
    }

    if let Some(arr) = patch_val.get("tags").and_then(|v| v.as_array()) {
        patch.tags = Some(
            arr.iter()
                .filter_map(|x| x.as_str().map(|s| s.to_string()))
                .collect(),
        );
    }

    if let Some(rel_val) = patch_val.get("relationships") {
        if rel_val.is_null() {
            patch.relationships = Some(TaskRelationships::default());
        } else {
            match serde_json::from_value::<TaskRelationships>(rel_val.clone()) {
                Ok(rel) => {
                    if rel.is_empty() {
                        patch.relationships = Some(TaskRelationships::default());
                    } else {
                        patch.relationships = Some(rel);
                    }
                }
                Err(e) => {
                    return Err(err(
                        req_id,
                        -32602,
                        &format!("Invalid relationships payload: {}", e),
                        None,
                    ));
                }
            }
        }
    }

    if patch_val.get("custom_fields").is_some() {
        let mut custom_fields_map = std::collections::HashMap::new();
        let custom_fields_val = patch_val.get("custom_fields").unwrap();
        if custom_fields_val.is_null() {
            patch.custom_fields = Some(custom_fields_map);
        } else {
            let Some(obj) = custom_fields_val.as_object() else {
                return Err(err(
                    req_id,
                    -32602,
                    "custom_fields must be an object or null",
                    None,
                ));
            };

            fn json_to_custom(val: &serde_json::Value) -> CustomFieldValue {
                #[cfg(feature = "schema")]
                {
                    val.clone()
                }
                #[cfg(not(feature = "schema"))]
                {
                    serde_yaml::to_value(val).unwrap_or(serde_yaml::Value::Null)
                }
            }

            for (k, v) in obj.iter() {
                custom_fields_map.insert(k.clone(), json_to_custom(v));
            }

            patch.custom_fields = Some(custom_fields_map);
        }
    }

    if patch_val.get("sprints").is_some() {
        match patch_val.get("sprints") {
            Some(Value::Null) => patch.sprints = Some(Vec::new()),
            Some(Value::Array(items)) => {
                let mut ids = Vec::new();
                for item in items {
                    if let Value::Number(num) = item
                        && let Some(v) = num.as_u64().and_then(|v| u32::try_from(v).ok())
                        && v > 0
                    {
                        ids.push(v);
                    }
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

    let title = match req.params.get("title").and_then(|v| v.as_str()) {
        Some(s) if !s.is_empty() => s.to_string(),
        _ => return err(req.id, -32602, "Missing required field: title", None),
    };
    let project = req
        .params
        .get("project")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());
    let project_scope: Vec<String> = project.iter().cloned().collect();
    let enum_hints = EnumHints::from_resolved_config(cfg, &project_scope);

    let priority = if let Some(s) = req.params.get("priority").and_then(|v| v.as_str()) {
        match validator.validate_priority(s) {
            Ok(v) => Some(v),
            Err(e) => {
                let data = enum_hints
                    .as_ref()
                    .and_then(|h| make_enum_error_data("priority", s, &h.priorities));
                return err(
                    req.id,
                    -32602,
                    &format!("Priority validation failed: {}", e),
                    data,
                );
            }
        }
    } else {
        None
    };
    let task_type = if let Some(s) = req
        .params
        .get("type")
        .or_else(|| req.params.get("task_type"))
        .and_then(|v| v.as_str())
    {
        match validator.validate_task_type(s) {
            Ok(v) => Some(v),
            Err(e) => {
                let data = enum_hints
                    .as_ref()
                    .and_then(|h| make_enum_error_data("type", s, &h.types));
                return err(
                    req.id,
                    -32602,
                    &format!("Type validation failed: {}", e),
                    data,
                );
            }
        }
    } else {
        None
    };
    let assignee = req
        .params
        .get("assignee")
        .and_then(|v| v.as_str())
        .and_then(|s| identity::resolve_me_alias(s, Some(resolver.path.as_path())));
    let due_date = req
        .params
        .get("due_date")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());
    let effort = req
        .params
        .get("effort")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());
    let description = req
        .params
        .get("description")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());
    let tags = req
        .params
        .get("tags")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|x| x.as_str().map(|s| s.to_string()))
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();

    fn json_to_custom(val: &serde_json::Value) -> CustomFieldValue {
        #[cfg(feature = "schema")]
        {
            val.clone()
        }
        #[cfg(not(feature = "schema"))]
        {
            serde_yaml::to_value(val).unwrap_or(serde_yaml::Value::Null)
        }
    }

    let custom_fields_map = req
        .params
        .get("custom_fields")
        .and_then(|v| v.as_object())
        .map(|o| {
            let mut m = std::collections::HashMap::new();
            for (k, v) in o.iter() {
                m.insert(k.clone(), json_to_custom(v));
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

    let dto = TaskCreate {
        title,
        project,
        priority,
        task_type,
        reporter: req
            .params
            .get("reporter")
            .and_then(|v| v.as_str())
            .and_then(|s| identity::resolve_me_alias(s, Some(resolver.path.as_path()))),
        assignee,
        due_date,
        effort,
        description,
        tags,
        relationships,
        custom_fields,
        sprints: req
            .params
            .get("sprints")
            .cloned()
            .and_then(|v| serde_json::from_value::<Vec<u32>>(v).ok())
            .unwrap_or_default(),
    };

    let mut storage = Storage::new(resolver.path.clone());
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
        Err(e) => err(
            req.id,
            -32000,
            "Task create failed",
            Some(json!({"message": e.to_string()})),
        ),
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
    let id = req
        .params
        .get("id")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());
    if id.is_none() {
        return err(req.id, -32602, "Missing id", None);
    }
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
    let storage = Storage::new(resolver.path);
    match TaskService::get(&storage, &id.unwrap(), project) {
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
    let id = req
        .params
        .get("id")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());
    if id.is_none() {
        return err(req.id, -32602, "Missing id", None);
    }
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
    let enum_hints = EnumHints::from_resolved_config(cfg, &[]);
    let patch_val = req.params.get("patch").cloned().unwrap_or(json!({}));
    let patch = match parse_task_update_patch(
        req.id.clone(),
        resolver.path.as_path(),
        &validator,
        enum_hints.as_ref(),
        &patch_val,
    ) {
        Ok(patch) => patch,
        Err(resp) => return resp,
    };
    let mut storage = Storage::new(resolver.path.clone());
    match TaskService::update(&mut storage, &id.unwrap(), patch) {
        Ok(task) => ok(
            req.id,
            json!({
                "content": [ { "type": "text", "text": serde_json::to_string_pretty(&task).unwrap_or_else(|_| "{}".into()) } ]
            }),
        ),
        Err(e) => err(
            req.id,
            -32005,
            "Task update failed",
            Some(json!({"message": e.to_string()})),
        ),
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

    let project_prefix = id.split('-').next().unwrap_or("").to_string();
    let mut storage = Storage::new(resolver.path.clone());

    let mut task = match storage.get(&id, project_prefix.clone()) {
        Some(task) => task,
        None => {
            return err(
                req.id,
                -32004,
                "Task not found",
                Some(json!({"message": format!("Task '{}' not found", id)})),
            );
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
            field: "comment".into(),
            old: None,
            new: Some(text.clone()),
        }],
    });
    task.modified = now;

    if let Err(error) = storage.edit(&id, &task) {
        return err(
            req.id,
            -32603,
            "Internal error",
            Some(json!({"message": error.to_string()})),
        );
    }

    let dto = match TaskService::get(&storage, &id, Some(&project_prefix)) {
        Ok(dto) => dto,
        Err(error) => {
            return err(
                req.id,
                -32603,
                "Internal error",
                Some(json!({"message": error.to_string()})),
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

    let project_prefix = id.split('-').next().unwrap_or("").to_string();
    let mut storage = Storage::new(resolver.path.clone());

    let mut task = match storage.get(&id, project_prefix.clone()) {
        Some(task) => task,
        None => {
            return err(
                req.id,
                -32004,
                "Task not found",
                Some(json!({"message": format!("Task '{}' not found", id)})),
            );
        }
    };

    if index >= task.comments.len() {
        return err(req.id, -32602, "Invalid comment index", None);
    }

    let previous = task.comments[index].text.clone();
    if previous != text {
        task.comments[index].text = text.clone();
        let now = now_rfc3339();
        task.history.push(TaskChangeLogEntry {
            at: now.clone(),
            actor: identity::resolve_current_user(Some(resolver.path.as_path())),
            changes: vec![TaskChange {
                field: format!("comment#{}", index + 1),
                old: Some(previous),
                new: Some(text.clone()),
            }],
        });
        task.modified = now;
        if let Err(error) = storage.edit(&id, &task) {
            return err(
                req.id,
                -32603,
                "Internal error",
                Some(json!({"message": error.to_string()})),
            );
        }
    }

    let dto = match TaskService::get(&storage, &id, Some(&project_prefix)) {
        Ok(dto) => dto,
        Err(error) => {
            return err(
                req.id,
                -32603,
                "Internal error",
                Some(json!({"message": error.to_string()})),
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
    let enum_hints = EnumHints::from_resolved_config(cfg, &[]);

    let patch = match parse_task_update_patch(
        req.id.clone(),
        resolver.path.as_path(),
        &validator,
        enum_hints.as_ref(),
        &patch_val,
    ) {
        Ok(patch) => patch,
        Err(resp) => return resp,
    };

    let mut storage = Storage::new(resolver.path.clone());
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

    let mut storage = Storage::new(resolver.path.clone());
    let mut updated: Vec<TaskDTO> = Vec::new();
    let mut failed: Vec<Value> = Vec::new();

    for id in ids {
        let project_prefix = id.split('-').next().unwrap_or("").to_string();
        let mut task = match storage.get(&id, project_prefix.clone()) {
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
                field: "comment".into(),
                old: None,
                new: Some(text.clone()),
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

    let mut storage = Storage::new(resolver.path.clone());
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
            ("code", true) => ReferenceService::attach_code_reference(
                &mut storage,
                repo_root.as_ref().unwrap(),
                &normalized_id,
                &value,
            ),
            ("code", false) => {
                ReferenceService::detach_code_reference(&mut storage, &normalized_id, &value)
            }
            ("file", true) => ReferenceService::attach_file_reference(
                &mut storage,
                repo_root.as_ref().unwrap(),
                &normalized_id,
                &value,
            ),
            ("file", false) => ReferenceService::detach_file_reference(
                &mut storage,
                repo_root.as_ref().unwrap(),
                &normalized_id,
                &value,
            ),
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

    let mut storage = Storage::new(resolver.path);
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
    let id = req.params.get("id").and_then(|v| v.as_str());
    if id.is_none() {
        return err(req.id, -32602, "Missing id", None);
    }
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
    let mut storage = Storage::new(resolver.path);
    match TaskService::delete(&mut storage, id.unwrap(), project) {
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

    let filter = TaskListFilter {
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
        custom_fields,
    };
    let storage = Storage::new(resolver.path.clone());
    let mut tasks = TaskService::list(&storage, &filter)
        .into_iter()
        .map(|(_, t)| t)
        .collect::<Vec<_>>();

    if let Some(raw) = req.params.get("assignee").and_then(|v| v.as_str()) {
        let trimmed = raw.trim();
        if !trimmed.is_empty() {
            let target = if trimmed.eq_ignore_ascii_case("@me") {
                identity::resolve_current_user(Some(storage.root_path.as_path()))
            } else {
                Some(trimmed.to_string())
            };
            match target {
                Some(user) => {
                    tasks.retain(|task| task.assignee.as_deref() == Some(user.as_str()));
                }
                None => tasks.clear(),
            }
        }
    }
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

    let mut payload = serde_json::Map::new();
    payload.insert("status".into(), Value::String("ok".into()));
    payload.insert("count".into(), Value::from(page.len() as u64));
    payload.insert("total".into(), Value::from(total as u64));
    payload.insert("cursor".into(), Value::from(start as u64));
    payload.insert("limit".into(), Value::from(limit as u64));
    payload.insert("hasMore".into(), Value::Bool(next_cursor.is_some()));
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

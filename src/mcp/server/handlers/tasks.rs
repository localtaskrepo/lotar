use serde_json::{Value, json};

use super::super::hints::{EnumHints, enum_hints_to_value, make_enum_error_data};
use super::super::{
    JsonRpcRequest, JsonRpcResponse, MCP_DEFAULT_TASK_LIST_LIMIT, MCP_MAX_TASK_LIST_LIMIT, err, ok,
    parse_cursor_value, parse_limit_value,
};
use crate::api_types::{TaskCreate, TaskDTO, TaskListFilter, TaskUpdate};
use crate::cli::validation::CliValidator;
use crate::config::manager::ConfigManager;
use crate::services::task_service::TaskService;
use crate::storage::manager::Storage;
use crate::types::{CustomFieldValue, TaskRelationships};
use crate::utils::identity;
use crate::workspace::TasksDirectoryResolver;
use std::collections::BTreeMap;

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

    let mut storage = Storage::new(resolver.path);
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
    if !patch_val.is_object() {
        return err(req.id, -32602, "Invalid patch (expected object)", None);
    }
    let mut patch = TaskUpdate::default();
    if let Some(s) = patch_val.get("title").and_then(|v| v.as_str()) {
        patch.title = Some(s.to_string());
    }
    if let Some(s) = patch_val.get("status").and_then(|v| v.as_str()) {
        match validator.validate_status(s) {
            Ok(v) => patch.status = Some(v),
            Err(e) => {
                let data = enum_hints
                    .as_ref()
                    .and_then(|h| make_enum_error_data("status", s, &h.statuses));
                return err(
                    req.id,
                    -32602,
                    &format!("Status validation failed: {}", e),
                    data,
                );
            }
        }
    }
    if let Some(s) = patch_val.get("priority").and_then(|v| v.as_str()) {
        match validator.validate_priority(s) {
            Ok(v) => patch.priority = Some(v),
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
    }
    if let Some(s) = patch_val
        .get("type")
        .or_else(|| patch_val.get("task_type"))
        .and_then(|v| v.as_str())
    {
        match validator.validate_task_type(s) {
            Ok(v) => patch.task_type = Some(v),
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
    }
    if let Some(s) = patch_val.get("reporter").and_then(|v| v.as_str()) {
        patch.reporter = identity::resolve_me_alias(s, Some(resolver.path.as_path()));
    }
    if let Some(s) = patch_val.get("assignee").and_then(|v| v.as_str()) {
        patch.assignee = identity::resolve_me_alias(s, Some(resolver.path.as_path()));
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
                    return err(
                        req.id,
                        -32602,
                        &format!("Invalid relationships payload: {}", e),
                        None,
                    );
                }
            }
        }
    }
    let mut custom_fields_map = patch.custom_fields.take().unwrap_or_default();
    let mut custom_fields_provided = false;
    if let Some(obj) = patch_val.get("custom_fields").and_then(|v| v.as_object()) {
        custom_fields_provided = true;
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
    }
    if custom_fields_provided || !custom_fields_map.is_empty() {
        patch.custom_fields = Some(custom_fields_map);
    }
    let mut storage = Storage::new(resolver.path);
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
    let tag = req
        .params
        .get("tag")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());
    let mut tags: Vec<String> = vec![];
    if let Some(t) = tag {
        tags.push(t);
    }
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
        sprints: Vec::new(),
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
    let cursor = match parse_cursor_value(req.params.get("cursor")) {
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
            .map(|pos| Value::String(pos.to_string()))
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

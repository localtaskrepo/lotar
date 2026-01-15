use serde_json::{Value, json};

use super::super::{
    JsonRpcRequest, JsonRpcResponse, MCP_DEFAULT_PROJECT_LIST_LIMIT, MCP_MAX_CURSOR,
    MCP_MAX_PROJECT_LIST_LIMIT, err, ok, parse_cursor_value, parse_limit_value,
};
use crate::services::project_service::ProjectService;
use crate::workspace::TasksDirectoryResolver;

pub(crate) fn handle_project_list(req: JsonRpcRequest) -> JsonRpcResponse {
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
    let storage = crate::storage::manager::Storage::new(resolver.path);
    let mut projects = ProjectService::list(&storage);
    projects.sort_by(|a, b| a.prefix.cmp(&b.prefix));

    let limit = match parse_limit_value(req.params.get("limit"), MCP_DEFAULT_PROJECT_LIST_LIMIT) {
        Ok(value) if (1..=MCP_MAX_PROJECT_LIST_LIMIT).contains(&value) => value,
        Ok(_) => {
            return err(
                req.id,
                -32602,
                &format!("limit must be between 1 and {}", MCP_MAX_PROJECT_LIST_LIMIT),
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

    let total = projects.len();
    let start = cursor.min(total);
    let end = (start + limit).min(total);
    let page = projects[start..end].to_vec();
    let has_more = end < total;
    let next_cursor = if has_more { Some(end) } else { None };

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
            .map(|pos| Value::String(pos.to_string()))
            .unwrap_or(Value::Null),
    );
    payload.insert(
        "projects".to_string(),
        serde_json::to_value(&page).unwrap_or_else(|_| Value::Array(Vec::new())),
    );
    ok(
        req.id,
        json!({
            "content": [
                {
                    "type": "text",
                    "text": serde_json::to_string_pretty(&Value::Object(payload))
                        .unwrap_or_else(|_| "{}".into())
                }
            ]
        }),
    )
}

pub(crate) fn handle_project_stats(req: JsonRpcRequest) -> JsonRpcResponse {
    let name = req.params.get("name").and_then(|v| v.as_str());
    if name.is_none() {
        return err(req.id, -32602, "Missing name", None);
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
    let storage = crate::storage::manager::Storage::new(resolver.path);
    let stats = ProjectService::stats(&storage, name.unwrap());
    ok(
        req.id,
        json!({
            "content": [ { "type": "text", "text": serde_json::to_string_pretty(&stats).unwrap_or_else(|_| "{}".into()) } ]
        }),
    )
}

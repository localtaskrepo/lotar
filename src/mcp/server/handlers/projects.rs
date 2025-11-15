use serde_json::json;

use super::super::{JsonRpcRequest, JsonRpcResponse, err, ok};
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
    let projects = ProjectService::list(&storage);
    ok(
        req.id,
        json!({
            "content": [ { "type": "text", "text": serde_json::to_string_pretty(&projects).unwrap_or_else(|_| "[]".into()) } ]
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

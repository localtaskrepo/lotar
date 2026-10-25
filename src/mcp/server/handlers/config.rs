use serde_json::json;
use std::collections::BTreeMap;

use super::super::{JsonRpcRequest, JsonRpcResponse, err, ok, resolve_tasks_dir, text_result};
use crate::services::config_service::ConfigService;

pub(crate) fn handle_config_show(req: JsonRpcRequest) -> JsonRpcResponse {
    let global = req
        .params
        .get("global")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    let project = req.params.get("project").and_then(|v| v.as_str());
    let resolver = try_rpc!(resolve_tasks_dir(&req.id));
    let scope = if global { None } else { project };
    match ConfigService::show(&resolver, scope) {
        Ok(val) => text_result(req.id, &val),
        Err(e) => err(
            req.id,
            -32001,
            "Config error",
            Some(json!({"message": e.to_string()})),
        ),
    }
}

pub(crate) fn handle_config_set(req: JsonRpcRequest) -> JsonRpcResponse {
    let resolver = try_rpc!(resolve_tasks_dir(&req.id));
    let values = req
        .params
        .get("values")
        .and_then(|v| v.as_object())
        .cloned()
        .unwrap_or_default();
    let mut map = BTreeMap::new();
    for (k, v) in values.iter() {
        let Some(text) = v.as_str() else {
            return err(req.id, -32602, "Config values must be strings", None);
        };
        map.insert(k.clone(), text.to_string());
    }
    let global = req
        .params
        .get("global")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    let project = req.params.get("project").and_then(|v| v.as_str());
    match ConfigService::set(&resolver, &map, global, project) {
        Ok(outcome) => {
            let mut lines = vec!["Configuration updated".to_string()];

            if !outcome.validation.warnings.is_empty() {
                lines.push("Warnings:".to_string());
                for warning in &outcome.validation.warnings {
                    lines.push(format!("- {}", warning));
                }
            }

            if !outcome.validation.info.is_empty() {
                lines.push("Info:".to_string());
                for info in &outcome.validation.info {
                    lines.push(format!("- {}", info));
                }
            }

            ok(
                req.id,
                json!({
                    "content": [ { "type": "text", "text": lines.join("\n") } ]
                }),
            )
        }
        Err(e) => err(
            req.id,
            -32002,
            "Config set failed",
            Some(json!({"message": e.to_string()})),
        ),
    }
}

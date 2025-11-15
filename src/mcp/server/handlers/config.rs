use serde_json::json;
use std::collections::BTreeMap;

use super::super::{JsonRpcRequest, JsonRpcResponse, err, ok};
use crate::services::config_service::ConfigService;
use crate::workspace::TasksDirectoryResolver;

pub(crate) fn handle_config_show(req: JsonRpcRequest) -> JsonRpcResponse {
    let global = req
        .params
        .get("global")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
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
    let scope = if global { None } else { project };
    match ConfigService::show(&resolver, scope) {
        Ok(val) => ok(
            req.id,
            json!({
                "content": [ { "type": "text", "text": serde_json::to_string_pretty(&val).unwrap_or_else(|_| "{}".into()) } ]
            }),
        ),
        Err(e) => err(
            req.id,
            -32001,
            "Config error",
            Some(json!({"message": e.to_string()})),
        ),
    }
}

pub(crate) fn handle_config_set(req: JsonRpcRequest) -> JsonRpcResponse {
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
    let values = req
        .params
        .get("values")
        .and_then(|v| v.as_object())
        .cloned()
        .unwrap_or_default();
    let mut map = BTreeMap::new();
    for (k, v) in values.iter() {
        map.insert(k.clone(), v.as_str().unwrap_or(&v.to_string()).to_string());
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

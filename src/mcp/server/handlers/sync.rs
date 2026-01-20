use super::super::{JsonRpcRequest, JsonRpcResponse, err, ok};
use crate::errors::LoTaRError;
use crate::services::sync_service::{SyncDirection, SyncService};
use crate::workspace::TasksDirectoryResolver;
use serde_json::json;

pub(crate) fn handle_sync_pull(req: JsonRpcRequest) -> JsonRpcResponse {
    handle_sync(req, SyncDirection::Pull)
}

pub(crate) fn handle_sync_push(req: JsonRpcRequest) -> JsonRpcResponse {
    handle_sync(req, SyncDirection::Push)
}

fn handle_sync(req: JsonRpcRequest, direction: SyncDirection) -> JsonRpcResponse {
    let remote = match req.params.get("remote").and_then(|v| v.as_str()) {
        Some(value) if !value.trim().is_empty() => value.trim().to_string(),
        _ => return err(req.id, -32602, "Missing remote", None),
    };
    let project = req.params.get("project").and_then(|v| v.as_str());
    let task_id = req.params.get("task_id").and_then(|v| v.as_str());
    let auth_profile = req.params.get("auth_profile").and_then(|v| v.as_str());
    let dry_run = req
        .params
        .get("dry_run")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    let include_report = req
        .params
        .get("include_report")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    let write_report = req.params.get("write_report").and_then(|v| v.as_bool());
    let client_run_id = req.params.get("client_run_id").and_then(|v| v.as_str());

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

    let outcome = match direction {
        SyncDirection::Pull => SyncService::pull(
            &resolver,
            &remote,
            project,
            dry_run,
            auth_profile,
            task_id,
            write_report,
            include_report,
            client_run_id,
        ),
        SyncDirection::Push => SyncService::push(
            &resolver,
            &remote,
            project,
            dry_run,
            auth_profile,
            task_id,
            write_report,
            include_report,
            client_run_id,
        ),
    };

    match outcome {
        Ok(result) => {
            let payload = serde_json::to_string_pretty(&result).unwrap_or_else(|_| "{}".into());
            ok(
                req.id,
                json!({
                    "content": [ { "type": "text", "text": payload } ]
                }),
            )
        }
        Err(sync_error) => {
            let code = match sync_error {
                LoTaRError::ValidationError(_) => -32602,
                _ => -32000,
            };
            err(
                req.id,
                code,
                "Sync failed",
                Some(json!({"message": sync_error.to_string()})),
            )
        }
    }
}

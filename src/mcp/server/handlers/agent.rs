use serde_json::json;

use super::super::{JsonRpcRequest, JsonRpcResponse, err, ok};
use crate::api_types::AgentJobCreateRequest;
use crate::services::agent_job_service::AgentJobService;
use crate::workspace::TasksDirectoryResolver;

pub(crate) fn handle_agent_run(req: JsonRpcRequest) -> JsonRpcResponse {
    let ticket_id = match req.params.get("ticket_id").and_then(|v| v.as_str()) {
        Some(id) => id.to_string(),
        None => {
            return err(
                req.id,
                -32602,
                "Missing required parameter: ticket_id",
                None,
            );
        }
    };
    let prompt = match req.params.get("prompt").and_then(|v| v.as_str()) {
        Some(p) => p.to_string(),
        None => return err(req.id, -32602, "Missing required parameter: prompt", None),
    };

    let resolver = match TasksDirectoryResolver::resolve(None, None) {
        Ok(r) => r,
        Err(e) => {
            return err(
                req.id,
                -32603,
                &format!("Cannot resolve tasks directory: {e}"),
                None,
            );
        }
    };

    let create_req = AgentJobCreateRequest {
        ticket_id,
        prompt,
        runner: req
            .params
            .get("runner")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string()),
        agent: req
            .params
            .get("agent")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string()),
    };

    match AgentJobService::start_job(create_req, &resolver) {
        Ok(job) => {
            let payload = serde_json::to_value(&job).unwrap_or(json!({}));
            ok(
                req.id,
                json!({
                    "content": [{"type": "text", "text": serde_json::to_string_pretty(&payload).unwrap_or_default()}]
                }),
            )
        }
        Err(e) => err(req.id, -32603, &e.to_string(), None),
    }
}

pub(crate) fn handle_agent_status(req: JsonRpcRequest) -> JsonRpcResponse {
    let id = match req.params.get("id").and_then(|v| v.as_str()) {
        Some(id) => id.to_string(),
        None => return err(req.id, -32602, "Missing required parameter: id", None),
    };

    match AgentJobService::get_job(&id) {
        Some(job) => {
            let payload = serde_json::to_value(&job).unwrap_or(json!({}));
            ok(
                req.id,
                json!({
                    "content": [{"type": "text", "text": serde_json::to_string_pretty(&payload).unwrap_or_default()}]
                }),
            )
        }
        None => err(req.id, -32602, "Job not found", None),
    }
}

pub(crate) fn handle_agent_list_jobs(req: JsonRpcRequest) -> JsonRpcResponse {
    let mut jobs = AgentJobService::list_jobs();
    jobs.sort_by(|a, b| b.created_at.cmp(&a.created_at));
    let queue_stats = AgentJobService::queue_stats();

    let payload = json!({
        "jobs": jobs,
        "queue_stats": queue_stats,
    });
    ok(
        req.id,
        json!({
            "content": [{"type": "text", "text": serde_json::to_string_pretty(&payload).unwrap_or_default()}]
        }),
    )
}

pub(crate) fn handle_agent_cancel(req: JsonRpcRequest) -> JsonRpcResponse {
    let id = match req.params.get("id").and_then(|v| v.as_str()) {
        Some(id) => id.to_string(),
        None => return err(req.id, -32602, "Missing required parameter: id", None),
    };

    match AgentJobService::cancel_job(&id) {
        Ok(Some(job)) => {
            let payload = json!({ "cancelled": true, "job": job });
            ok(
                req.id,
                json!({
                    "content": [{"type": "text", "text": serde_json::to_string_pretty(&payload).unwrap_or_default()}]
                }),
            )
        }
        Ok(None) => err(req.id, -32602, "Job not found", None),
        Err(e) => err(req.id, -32603, &e.to_string(), None),
    }
}

pub(crate) fn handle_agent_send_message(req: JsonRpcRequest) -> JsonRpcResponse {
    let id = match req.params.get("id").and_then(|v| v.as_str()) {
        Some(id) => id.to_string(),
        None => return err(req.id, -32602, "Missing required parameter: id", None),
    };
    let message = match req.params.get("message").and_then(|v| v.as_str()) {
        Some(m) => m.to_string(),
        None => return err(req.id, -32602, "Missing required parameter: message", None),
    };

    match AgentJobService::send_message(&id, &message) {
        Ok(job) => {
            let payload = serde_json::to_value(&job).unwrap_or(json!({}));
            ok(
                req.id,
                json!({
                    "content": [{"type": "text", "text": serde_json::to_string_pretty(&payload).unwrap_or_default()}]
                }),
            )
        }
        Err(e) => err(req.id, -32603, &e.to_string(), None),
    }
}

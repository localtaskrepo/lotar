use serde_json::{Value, json};
use std::fmt::Write as _;

use super::super::hints::{EnumHints, enum_hints_to_value};
use super::super::{
    JsonRpcRequest, JsonRpcResponse, MCP_DEFAULT_BACKLOG_LIMIT, MCP_MAX_BACKLOG_LIMIT,
    MCP_MAX_CURSOR, err, make_mcp_integrity_payload, ok, parse_cursor_value, parse_limit_value,
};
use crate::config::manager::ConfigManager;
use crate::services::sprint_assignment::{self, SprintBacklogOptions};
use crate::services::sprint_integrity;
use crate::services::sprint_service::SprintService;
use crate::storage::manager::Storage;
use crate::types::TaskStatus;
use crate::workspace::TasksDirectoryResolver;

pub(crate) fn handle_sprint_add(req: JsonRpcRequest) -> JsonRpcResponse {
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
    let mut records = match SprintService::list(&storage) {
        Ok(records) => records,
        Err(e) => {
            return err(
                req.id,
                -32603,
                "Internal error",
                Some(json!({"message": format!("Failed to load sprints: {}", e)})),
            );
        }
    };

    let cleanup_missing = req
        .params
        .get("cleanup_missing")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);

    let mut integrity_report = sprint_integrity::detect_missing_sprints(&storage, &records);
    let baseline_report = integrity_report.clone();
    let mut cleanup_outcome: Option<sprint_integrity::SprintCleanupOutcome> = None;

    if cleanup_missing && !integrity_report.missing_sprints.is_empty() {
        match sprint_integrity::cleanup_missing_sprint_refs(&mut storage, &mut records, None) {
            Ok(outcome) => {
                integrity_report = sprint_integrity::detect_missing_sprints(&storage, &records);
                cleanup_outcome = Some(outcome);
            }
            Err(error) => {
                return err(
                    req.id,
                    -32603,
                    "Internal error",
                    Some(json!({"message": format!(
                        "Failed to clean up sprint references: {}",
                        error
                    )})),
                );
            }
        }
    }

    let raw_tasks = req.params.get("tasks");
    if raw_tasks.is_none() {
        return err(req.id, -32602, "Missing required field: tasks", None);
    }
    let tasks: Vec<String> = match raw_tasks.unwrap() {
        Value::String(value) => {
            let trimmed = value.trim();
            if trimmed.is_empty() {
                Vec::new()
            } else {
                vec![trimmed.to_string()]
            }
        }
        Value::Array(values) => values
            .iter()
            .filter_map(|v| v.as_str())
            .map(|s| s.trim())
            .filter(|s| !s.is_empty())
            .map(|s| s.to_string())
            .collect(),
        _ => {
            return err(
                req.id,
                -32602,
                "tasks must be a string or array of strings",
                None,
            );
        }
    };

    let sprint_ref = req.params.get("sprint").map(|value| match value {
        Value::Number(num) => num.to_string(),
        Value::String(text) => text.trim().to_string(),
        _ => String::new(),
    });

    if sprint_ref
        .as_ref()
        .is_some_and(|reference| reference.is_empty())
    {
        return err(
            req.id,
            -32602,
            "sprint must be a numeric id or keyword when provided",
            None,
        );
    }

    let allow_closed = req
        .params
        .get("allow_closed")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);

    let force_single = req
        .params
        .get("force_single")
        .or_else(|| req.params.get("force"))
        .and_then(|v| v.as_bool())
        .unwrap_or(false);

    let outcome = match sprint_assignment::assign_tasks(
        &mut storage,
        &records,
        &tasks,
        sprint_ref.as_deref(),
        allow_closed,
        force_single,
    ) {
        Ok(outcome) => outcome,
        Err(msg) => return err(req.id, -32602, msg.as_str(), None),
    };

    let mut payload = serde_json::Map::new();
    payload.insert("status".to_string(), Value::String("ok".to_string()));
    payload.insert(
        "action".to_string(),
        Value::String(outcome.action.as_str().to_string()),
    );
    payload.insert("sprint_id".to_string(), Value::from(outcome.sprint_id));
    if let Some(label) = outcome.sprint_label.clone() {
        payload.insert("sprint_label".to_string(), Value::String(label));
    } else {
        payload.insert("sprint_label".to_string(), Value::Null);
    }
    payload.insert(
        "modified".to_string(),
        Value::Array(
            outcome
                .modified
                .iter()
                .map(|id| Value::String(id.clone()))
                .collect(),
        ),
    );
    payload.insert(
        "unchanged".to_string(),
        Value::Array(
            outcome
                .unchanged
                .iter()
                .map(|id| Value::String(id.clone()))
                .collect(),
        ),
    );
    let reassignment_messages: Vec<String> = outcome
        .replaced
        .iter()
        .filter_map(|info| info.describe())
        .collect();

    let replaced: Vec<Value> = outcome
        .replaced
        .into_iter()
        .map(|info| {
            let mut entry = serde_json::Map::new();
            entry.insert("task_id".to_string(), Value::String(info.task_id));
            entry.insert(
                "previous".to_string(),
                Value::Array(info.previous.iter().map(|id| Value::from(*id)).collect()),
            );
            Value::Object(entry)
        })
        .collect();
    payload.insert("replaced".to_string(), Value::Array(replaced));
    if !reassignment_messages.is_empty() {
        payload.insert(
            "messages".to_string(),
            Value::Array(
                reassignment_messages
                    .iter()
                    .map(|msg| Value::String(msg.clone()))
                    .collect(),
            ),
        );
    }
    payload.insert(
        "missing_sprints".to_string(),
        Value::Array(
            integrity_report
                .missing_sprints
                .iter()
                .map(|id| Value::from(*id))
                .collect(),
        ),
    );
    if let Some(integrity) = make_mcp_integrity_payload(
        &baseline_report,
        &integrity_report,
        cleanup_outcome.as_ref(),
    ) {
        payload.insert("integrity".to_string(), integrity);
    }

    let payload = Value::Object(payload);
    let mut content_items = Vec::new();
    if !reassignment_messages.is_empty() {
        content_items.push(json!({
            "type": "text",
            "text": reassignment_messages.join("\n"),
        }));
    }
    content_items.push(json!({
        "type": "text",
        "text": serde_json::to_string_pretty(&payload).unwrap_or_else(|_| "{}".into()),
    }));

    ok(req.id, json!({ "content": content_items }))
}

pub(crate) fn handle_sprint_remove(req: JsonRpcRequest) -> JsonRpcResponse {
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
    let mut records = match SprintService::list(&storage) {
        Ok(records) => records,
        Err(e) => {
            return err(
                req.id,
                -32603,
                "Internal error",
                Some(json!({"message": format!("Failed to load sprints: {}", e)})),
            );
        }
    };

    let cleanup_missing = req
        .params
        .get("cleanup_missing")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);

    let mut integrity_report = sprint_integrity::detect_missing_sprints(&storage, &records);
    let baseline_report = integrity_report.clone();
    let mut cleanup_outcome: Option<sprint_integrity::SprintCleanupOutcome> = None;

    if cleanup_missing && !integrity_report.missing_sprints.is_empty() {
        match sprint_integrity::cleanup_missing_sprint_refs(&mut storage, &mut records, None) {
            Ok(outcome) => {
                integrity_report = sprint_integrity::detect_missing_sprints(&storage, &records);
                cleanup_outcome = Some(outcome);
            }
            Err(error) => {
                return err(
                    req.id,
                    -32603,
                    "Internal error",
                    Some(json!({"message": format!(
                        "Failed to clean up sprint references: {}",
                        error
                    )})),
                );
            }
        }
    }

    let raw_tasks = req.params.get("tasks");
    if raw_tasks.is_none() {
        return err(req.id, -32602, "Missing required field: tasks", None);
    }
    let tasks: Vec<String> = match raw_tasks.unwrap() {
        Value::String(value) => {
            let trimmed = value.trim();
            if trimmed.is_empty() {
                Vec::new()
            } else {
                vec![trimmed.to_string()]
            }
        }
        Value::Array(values) => values
            .iter()
            .filter_map(|v| v.as_str())
            .map(|s| s.trim())
            .filter(|s| !s.is_empty())
            .map(|s| s.to_string())
            .collect(),
        _ => {
            return err(
                req.id,
                -32602,
                "tasks must be a string or array of strings",
                None,
            );
        }
    };

    let sprint_ref = req.params.get("sprint").map(|value| match value {
        Value::Number(num) => num.to_string(),
        Value::String(text) => text.trim().to_string(),
        _ => String::new(),
    });

    if sprint_ref
        .as_ref()
        .is_some_and(|reference| reference.is_empty())
    {
        return err(
            req.id,
            -32602,
            "sprint must be a numeric id or keyword when provided",
            None,
        );
    }

    let outcome = match sprint_assignment::remove_tasks(
        &mut storage,
        &records,
        &tasks,
        sprint_ref.as_deref(),
    ) {
        Ok(outcome) => outcome,
        Err(msg) => return err(req.id, -32602, msg.as_str(), None),
    };

    let mut payload = serde_json::Map::new();
    payload.insert("status".to_string(), Value::String("ok".to_string()));
    payload.insert(
        "action".to_string(),
        Value::String(outcome.action.as_str().to_string()),
    );
    payload.insert("sprint_id".to_string(), Value::from(outcome.sprint_id));
    if let Some(label) = outcome.sprint_label.clone() {
        payload.insert("sprint_label".to_string(), Value::String(label));
    } else {
        payload.insert("sprint_label".to_string(), Value::Null);
    }
    payload.insert(
        "modified".to_string(),
        Value::Array(
            outcome
                .modified
                .iter()
                .map(|id| Value::String(id.clone()))
                .collect(),
        ),
    );
    payload.insert(
        "unchanged".to_string(),
        Value::Array(
            outcome
                .unchanged
                .iter()
                .map(|id| Value::String(id.clone()))
                .collect(),
        ),
    );
    payload.insert(
        "missing_sprints".to_string(),
        Value::Array(
            integrity_report
                .missing_sprints
                .iter()
                .map(|id| Value::from(*id))
                .collect(),
        ),
    );
    if let Some(integrity) = make_mcp_integrity_payload(
        &baseline_report,
        &integrity_report,
        cleanup_outcome.as_ref(),
    ) {
        payload.insert("integrity".to_string(), integrity);
    }
    let payload = Value::Object(payload);

    ok(
        req.id,
        json!({
            "content": [ { "type": "text", "text": serde_json::to_string_pretty(&payload).unwrap_or_else(|_| "{}".into()) } ]
        }),
    )
}

pub(crate) fn handle_sprint_delete(req: JsonRpcRequest) -> JsonRpcResponse {
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

    let sprint_val = req.params.get("sprint");
    let sprint_id = match sprint_val {
        Some(Value::Number(num)) => num.as_u64().and_then(|value| u32::try_from(value).ok()),
        Some(Value::String(text)) => text.trim().parse::<u32>().ok(),
        _ => None,
    };
    let sprint_id = match sprint_id {
        Some(id) => id,
        None => return err(req.id, -32602, "Missing or invalid sprint id", None),
    };

    let cleanup_missing = req
        .params
        .get("cleanup_missing")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);

    let record = match SprintService::get(&storage, sprint_id) {
        Ok(record) => record,
        Err(crate::errors::LoTaRError::SprintNotFound(_)) => {
            return err(
                req.id,
                -32004,
                &format!("Sprint #{} not found", sprint_id),
                None,
            );
        }
        Err(error) => {
            return err(
                req.id,
                -32603,
                "Internal error",
                Some(json!({"message": error.to_string()})),
            );
        }
    };

    let display_name = sprint_assignment::sprint_display_name(&record);
    let sprint_label = record
        .sprint
        .plan
        .as_ref()
        .and_then(|plan| plan.label.clone());

    match SprintService::delete(&mut storage, sprint_id) {
        Ok(true) => {}
        Ok(false) => {
            return err(
                req.id,
                -32004,
                &format!("Sprint #{} not found", sprint_id),
                None,
            );
        }
        Err(error) => {
            return err(
                req.id,
                -32603,
                "Failed to delete sprint",
                Some(json!({"message": error.to_string()})),
            );
        }
    }

    let mut records = match SprintService::list(&storage) {
        Ok(records) => records,
        Err(error) => {
            return err(
                req.id,
                -32603,
                "Internal error",
                Some(json!({"message": error.to_string()})),
            );
        }
    };

    let mut integrity_report = sprint_integrity::detect_missing_sprints(&storage, &records);
    let baseline_report = integrity_report.clone();
    let mut cleanup_outcome: Option<sprint_integrity::SprintCleanupOutcome> = None;

    if cleanup_missing {
        match sprint_integrity::cleanup_missing_sprint_refs(
            &mut storage,
            &mut records,
            Some(sprint_id),
        ) {
            Ok(outcome) => {
                integrity_report = sprint_integrity::detect_missing_sprints(&storage, &records);
                cleanup_outcome = Some(outcome);
            }
            Err(error) => {
                return err(
                    req.id,
                    -32603,
                    "Failed to clean sprint references",
                    Some(json!({"message": error.to_string()})),
                );
            }
        }
    }

    let removed_references = cleanup_outcome
        .as_ref()
        .map(|outcome| outcome.removed_references)
        .unwrap_or(0);
    let updated_tasks = cleanup_outcome
        .as_ref()
        .map(|outcome| outcome.updated_tasks)
        .unwrap_or(0);

    let mut summary = format!("Deleted {}.", display_name);

    if cleanup_missing {
        if removed_references > 0 {
            let _ = write!(
                summary,
                " Removed {} dangling sprint reference(s) across {} task(s).",
                removed_references, updated_tasks
            );
        } else {
            summary.push_str(" No dangling sprint references required cleanup.");
        }
    } else if !integrity_report.missing_sprints.is_empty() {
        let missing = integrity_report
            .missing_sprints
            .iter()
            .map(|id| format!("#{}", id))
            .collect::<Vec<_>>()
            .join(", ");
        let _ = write!(
            summary,
            " Missing sprint references detected: {}. Re-run with cleanup_missing=true to remove them.",
            missing
        );
    }

    let mut payload = serde_json::Map::new();
    payload.insert("status".to_string(), Value::String("ok".to_string()));
    payload.insert("deleted".to_string(), Value::Bool(true));
    payload.insert("sprint_id".to_string(), Value::from(sprint_id));
    if let Some(label) = sprint_label.clone() {
        payload.insert("sprint_label".to_string(), Value::String(label));
    } else {
        payload.insert("sprint_label".to_string(), Value::Null);
    }
    payload.insert(
        "removed_references".to_string(),
        Value::from(removed_references as u64),
    );
    payload.insert(
        "updated_tasks".to_string(),
        Value::from(updated_tasks as u64),
    );
    if let Some(integrity) = make_mcp_integrity_payload(
        &baseline_report,
        &integrity_report,
        cleanup_outcome.as_ref(),
    ) {
        payload.insert("integrity".to_string(), integrity);
    }

    let payload = Value::Object(payload);

    ok(
        req.id,
        json!({
            "content": [
                { "type": "text", "text": format!("{}", summary) },
                { "type": "text", "text": serde_json::to_string_pretty(&payload).unwrap_or_else(|_| "{}".into()) }
            ]
        }),
    )
}

pub(crate) fn handle_sprint_backlog(req: JsonRpcRequest) -> JsonRpcResponse {
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

    let project = req
        .params
        .get("project")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());
    let project_scope: Vec<String> = project.iter().cloned().collect();
    let enum_hints = ConfigManager::new_manager_with_tasks_dir_readonly(&resolver.path)
        .ok()
        .and_then(|mgr| {
            let cfg = mgr.get_resolved_config();
            EnumHints::from_resolved_config(cfg, &project_scope)
        });

    let mut storage = match Storage::try_open(resolver.path.clone()) {
        Some(storage) => storage,
        None => {
            let mut payload = serde_json::Map::new();
            payload.insert("status".to_string(), Value::String("ok".to_string()));
            payload.insert("count".to_string(), Value::from(0u64));
            payload.insert("truncated".to_string(), Value::Bool(false));
            payload.insert("tasks".to_string(), Value::Array(Vec::new()));
            if let Some(hints) = enum_hints.as_ref() {
                payload.insert("enumHints".to_string(), enum_hints_to_value(hints));
            }
            let payload = Value::Object(payload);
            return ok(
                req.id,
                json!({
                    "content": [ { "type": "text", "text": serde_json::to_string_pretty(&payload).unwrap_or_else(|_| "{}".into()) } ]
                }),
            );
        }
    };

    let limit = match parse_limit_value(req.params.get("limit"), MCP_DEFAULT_BACKLOG_LIMIT) {
        Ok(value) if (1..=MCP_MAX_BACKLOG_LIMIT).contains(&value) => value,
        Ok(_) => {
            return err(
                req.id,
                -32602,
                &format!("limit must be between 1 and {}", MCP_MAX_BACKLOG_LIMIT),
                None,
            );
        }
        Err(msg) => return err(req.id, -32602, msg, None),
    };
    let cursor = match parse_cursor_value(req.params.get("cursor")) {
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

    let cleanup_missing = req
        .params
        .get("cleanup_missing")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);

    fn parse_string_vec(value: Option<&Value>) -> Vec<String> {
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

    let status_tokens = parse_string_vec(req.params.get("status"));
    let statuses: Vec<TaskStatus> = status_tokens
        .iter()
        .map(|token| TaskStatus::from(token.as_str()))
        .collect();

    let tags = parse_string_vec(req.params.get("tag"));

    let options = SprintBacklogOptions {
        project: project.clone(),
        tags,
        statuses,
        assignee: req
            .params
            .get("assignee")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string()),
        limit: 0,
    };

    let mut records = match SprintService::list(&storage) {
        Ok(records) => records,
        Err(e) => {
            return err(
                req.id,
                -32603,
                "Internal error",
                Some(json!({"message": format!("Failed to load sprints: {}", e)})),
            );
        }
    };

    let mut integrity_report = sprint_integrity::detect_missing_sprints(&storage, &records);
    let baseline_report = integrity_report.clone();
    let mut cleanup_outcome: Option<sprint_integrity::SprintCleanupOutcome> = None;

    if cleanup_missing && !integrity_report.missing_sprints.is_empty() {
        match sprint_integrity::cleanup_missing_sprint_refs(&mut storage, &mut records, None) {
            Ok(outcome) => {
                integrity_report = sprint_integrity::detect_missing_sprints(&storage, &records);
                cleanup_outcome = Some(outcome);
            }
            Err(error) => {
                return err(
                    req.id,
                    -32603,
                    "Internal error",
                    Some(json!({"message": format!(
                        "Failed to clean up sprint references: {}",
                        error
                    )})),
                );
            }
        }
    }

    let result = match sprint_assignment::fetch_backlog(&storage, options) {
        Ok(result) => result,
        Err(msg) => return err(req.id, -32602, msg.as_str(), None),
    };
    let total_entries = result.entries.len();
    let start = cursor.min(total_entries);
    let end = (start + limit).min(total_entries);
    let page_entries = result.entries[start..end].to_vec();
    let has_more = end < total_entries;
    let next_cursor = if has_more { Some(end) } else { None };

    let mut payload = serde_json::Map::new();
    payload.insert("status".to_string(), Value::String("ok".to_string()));
    payload.insert("count".to_string(), Value::from(page_entries.len() as u64));
    payload.insert("total".to_string(), Value::from(total_entries as u64));
    payload.insert("truncated".to_string(), Value::Bool(has_more));
    payload.insert("hasMore".to_string(), Value::Bool(has_more));
    payload.insert("cursor".to_string(), Value::from(start as u64));
    payload.insert(
        "nextCursor".to_string(),
        next_cursor
            .map(|pos| Value::String(pos.to_string()))
            .unwrap_or(Value::Null),
    );
    payload.insert(
        "tasks".to_string(),
        serde_json::to_value(&page_entries).unwrap_or_else(|_| Value::Array(Vec::new())),
    );
    payload.insert(
        "missing_sprints".to_string(),
        Value::Array(
            integrity_report
                .missing_sprints
                .iter()
                .map(|id| Value::from(*id))
                .collect(),
        ),
    );
    if let Some(integrity) = make_mcp_integrity_payload(
        &baseline_report,
        &integrity_report,
        cleanup_outcome.as_ref(),
    ) {
        payload.insert("integrity".to_string(), integrity);
    }
    if let Some(hints) = enum_hints.as_ref() {
        payload.insert("enumHints".to_string(), enum_hints_to_value(hints));
    }

    let payload = Value::Object(payload);

    ok(
        req.id,
        json!({
            "content": [ { "type": "text", "text": serde_json::to_string_pretty(&payload).unwrap_or_else(|_| "{}".into()) } ]
        }),
    )
}

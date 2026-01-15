use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::io::{self, BufRead, Read, Write};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, OnceLock, RwLock, mpsc};
use std::time::Duration;

mod handlers;
mod hints;
mod tools;
mod watchers;

#[cfg(test)]
mod mcp_server_tests;

use handlers::{
    handle_config_set, handle_config_show, handle_project_list, handle_project_stats,
    handle_sprint_add, handle_sprint_backlog, handle_sprint_delete, handle_sprint_remove,
    handle_task_create, handle_task_delete, handle_task_get, handle_task_list,
    handle_task_reference_add, handle_task_reference_remove, handle_task_update,
};
use hints::gather_enum_hints;
use tools::build_tool_definitions;
#[cfg(test)]
pub(crate) use watchers::event_affects_tooling;
use watchers::{ServerEvent, spawn_event_dispatcher, start_tools_change_notifier};

#[derive(Debug, Serialize, Deserialize)]
struct JsonRpcRequest {
    jsonrpc: String,
    id: Option<Value>,
    method: String,
    #[serde(default)]
    params: Value,
}

#[derive(Debug, Serialize, Deserialize)]
struct JsonRpcResponse {
    jsonrpc: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    id: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    result: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<JsonRpcError>,
}

#[derive(Debug, Serialize, Deserialize)]
struct JsonRpcError {
    code: i64,
    message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    data: Option<Value>,
}

const MCP_DEFAULT_TASK_LIST_LIMIT: usize = 50;
const MCP_MAX_TASK_LIST_LIMIT: usize = 200;
const MCP_DEFAULT_PROJECT_LIST_LIMIT: usize = 50;
const MCP_MAX_PROJECT_LIST_LIMIT: usize = 200;
const MCP_DEFAULT_BACKLOG_LIMIT: usize = 20;
const MCP_MAX_BACKLOG_LIMIT: usize = 100;
const MCP_MAX_CURSOR: usize = 5000;

fn ok(id: Option<Value>, v: Value) -> JsonRpcResponse {
    JsonRpcResponse {
        jsonrpc: "2.0".into(),
        id,
        result: Some(v),
        error: None,
    }
}

fn err(id: Option<Value>, code: i64, message: &str, data: Option<Value>) -> JsonRpcResponse {
    JsonRpcResponse {
        jsonrpc: "2.0".into(),
        id,
        result: None,
        error: Some(JsonRpcError {
            code,
            message: message.into(),
            data,
        }),
    }
}

fn normalize_method(name: &str) -> String {
    if name.contains('/') {
        name.to_string()
    } else if let Some(idx) = name.find('_') {
        let (left, right) = name.split_at(idx);
        format!("{}/{}", left, &right[1..])
    } else {
        name.to_string()
    }
}

static LOG_LEVEL: OnceLock<RwLock<String>> = OnceLock::new();
static USE_FRAMED_OUTPUT: AtomicBool = AtomicBool::new(false);
static SESSION_INITIALIZED: AtomicBool = AtomicBool::new(false);
fn set_log_level(level: &str) {
    let lvl = level.to_ascii_lowercase();
    let valid = matches!(
        lvl.as_str(),
        "trace" | "debug" | "info" | "warn" | "error" | "off"
    );
    let final_level = if valid { lvl } else { "info".to_string() };
    let cell = LOG_LEVEL.get_or_init(|| RwLock::new("info".to_string()));
    if let Ok(mut guard) = cell.write() {
        *guard = final_level;
    }
}

fn make_mcp_cleanup_summary(
    outcome: &crate::services::sprint_integrity::SprintCleanupOutcome,
) -> Value {
    let mut payload = serde_json::Map::new();
    payload.insert(
        "removed_references".to_string(),
        Value::from(outcome.removed_references as u64),
    );
    payload.insert(
        "updated_tasks".to_string(),
        Value::from(outcome.updated_tasks as u64),
    );
    let removed: Vec<Value> = outcome
        .removed_by_sprint
        .iter()
        .map(|metric| {
            let mut item = serde_json::Map::new();
            item.insert("sprint_id".to_string(), Value::from(metric.sprint_id));
            item.insert("count".to_string(), Value::from(metric.count as u64));
            Value::Object(item)
        })
        .collect();
    payload.insert("removed_by_sprint".to_string(), Value::Array(removed));
    payload.insert(
        "remaining_missing".to_string(),
        Value::Array(
            outcome
                .remaining_missing
                .iter()
                .map(|id| Value::from(*id))
                .collect(),
        ),
    );
    Value::Object(payload)
}

fn make_mcp_integrity_payload(
    baseline: &crate::services::sprint_integrity::MissingSprintReport,
    current: &crate::services::sprint_integrity::MissingSprintReport,
    cleanup: Option<&crate::services::sprint_integrity::SprintCleanupOutcome>,
) -> Option<Value> {
    if baseline.missing_sprints.is_empty() && cleanup.is_none() {
        return None;
    }

    let mut payload = serde_json::Map::new();
    payload.insert(
        "missing_sprints".to_string(),
        Value::Array(
            current
                .missing_sprints
                .iter()
                .map(|id| Value::from(*id))
                .collect(),
        ),
    );
    if baseline.tasks_with_missing > 0 {
        payload.insert(
            "tasks_with_missing".to_string(),
            Value::from(baseline.tasks_with_missing as u64),
        );
    }
    if let Some(outcome) = cleanup {
        payload.insert(
            "auto_cleanup".to_string(),
            make_mcp_cleanup_summary(outcome),
        );
    }

    Some(Value::Object(payload))
}

fn enable_framed_output() {
    USE_FRAMED_OUTPUT.store(true, Ordering::Relaxed);
}

fn write_json_message(stdout: &Arc<Mutex<io::Stdout>>, payload: &str) {
    if USE_FRAMED_OUTPUT.load(Ordering::Relaxed) {
        write_framed_json(stdout, payload);
    } else {
        write_raw_json(stdout, payload);
    }
}

fn respond_parse_error(stdout: &Arc<Mutex<io::Stdout>>, details: &str) {
    let response = err(
        Some(Value::Null),
        -32700,
        "Parse error",
        Some(json!({"details": details})),
    );
    if let Ok(encoded) = serde_json::to_string(&response) {
        write_json_message(stdout, &encoded);
    }
}

fn mark_session_initialized() {
    SESSION_INITIALIZED.store(true, Ordering::Relaxed);
}

fn session_initialized() -> bool {
    SESSION_INITIALIZED.load(Ordering::Relaxed)
}

pub fn run_stdio_server() {
    let autoreload_enabled = std::env::var("LOTAR_MCP_AUTORELOAD")
        .ok()
        .map(|v| v != "0")
        .unwrap_or(true);
    if autoreload_enabled
        && let Ok(exe_path) = std::env::current_exe()
        && let Ok(meta) = std::fs::metadata(&exe_path)
        && let Ok(modified) = meta.modified()
    {
        let initial = modified;
        std::thread::spawn(move || {
            loop {
                std::thread::sleep(Duration::from_secs(2));
                if let Ok(Ok(m)) = std::fs::metadata(&exe_path).map(|m| m.modified())
                    && m > initial
                {
                    std::process::exit(0);
                }
            }
        });
    }

    let stdin = io::stdin();
    let stdout = Arc::new(Mutex::new(io::stdout()));
    let (event_tx, event_rx) = mpsc::channel::<ServerEvent>();
    start_tools_change_notifier(event_tx);
    spawn_event_dispatcher(event_rx, stdout.clone());
    let mut reader = io::BufReader::new(stdin.lock());

    loop {
        let mut first_line = String::new();
        if reader
            .read_line(&mut first_line)
            .ok()
            .filter(|&n| n > 0)
            .is_none()
        {
            break;
        }
        let trimmed = first_line.trim_end_matches(['\r', '\n']);
        if trimmed.is_empty() {
            continue;
        }

        if trimmed.to_ascii_lowercase().starts_with("content-length:") {
            enable_framed_output();
            let mut content_length: Option<usize> = None;
            if let Some(v) = trimmed.split(':').nth(1) {
                content_length = v.trim().parse::<usize>().ok();
            }
            loop {
                let mut line = String::new();
                if reader
                    .read_line(&mut line)
                    .ok()
                    .filter(|&n| n > 0)
                    .is_none()
                {
                    break;
                }
                let l = line.trim_end_matches(['\r', '\n']);
                if l.is_empty() {
                    break;
                }
                if l.to_ascii_lowercase().starts_with("content-length:")
                    && let Some(v) = l.split(':').nth(1)
                {
                    content_length = v.trim().parse::<usize>().ok();
                }
            }
            if let Some(len) = content_length {
                let mut buf = vec![0u8; len];
                if let Err(e) = reader.read_exact(&mut buf) {
                    let detail = format!("body read failed: {}", e);
                    respond_parse_error(&stdout, &detail);
                    continue;
                }
                let body = match String::from_utf8(buf) {
                    Ok(s) => s,
                    Err(e) => {
                        let detail = format!("utf8 error: {}", e);
                        respond_parse_error(&stdout, &detail);
                        continue;
                    }
                };
                let req: Result<JsonRpcRequest, _> = serde_json::from_str(&body);
                match req {
                    Ok(r) => {
                        let should_respond = r.id.is_some();
                        let response = dispatch(r);
                        if should_respond {
                            let payload = match serde_json::to_string(&response) {
                                Ok(s) => s,
                                Err(e) => format!(
                                    "{{\"jsonrpc\":\"2.0\",\"error\":{{\"code\":-32603,\"message\":\"Serialization error: {}\"}},\"id\":null}}",
                                    e
                                ),
                            };
                            write_framed_json(&stdout, &payload);
                        }
                    }
                    Err(e) => {
                        respond_parse_error(&stdout, &e.to_string());
                    }
                }
                continue;
            } else {
                respond_parse_error(&stdout, "missing Content-Length");
                continue;
            }
        }

        let req: Result<JsonRpcRequest, _> = serde_json::from_str(trimmed);
        match req {
            Ok(r) => {
                let should_respond = r.id.is_some();
                let response = dispatch(r);
                if should_respond {
                    match serde_json::to_string(&response) {
                        Ok(s) => write_json_message(&stdout, &s),
                        Err(e) => {
                            let fallback = json!({
                                "jsonrpc": "2.0",
                                "error": {
                                    "code": -32603,
                                    "message": format!("Serialization error: {}", e)
                                },
                                "id": Value::Null
                            })
                            .to_string();
                            write_json_message(&stdout, &fallback);
                        }
                    }
                }
            }
            Err(e) => respond_parse_error(&stdout, &e.to_string()),
        }
    }
}

fn dispatch(req: JsonRpcRequest) -> JsonRpcResponse {
    let method = normalize_method(req.method.as_str());
    match method.as_str() {
        // Minimal MCP handshake
        // initialize -> return protocol and capabilities
        "initialize" => {
            // Negotiate protocol version per MCP 2025-06-18
            let client_version = req
                .params
                .get("protocolVersion")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            // We support only the latest spec we target
            let server_version = "2025-06-18";
            let negotiated = if client_version.is_empty() {
                server_version
            } else {
                // If client asks for something else, respond with our supported version per spec
                server_version
            };

            mark_session_initialized();
            ok(
                req.id,
                json!({
                    "protocolVersion": negotiated,
                    "capabilities": {
                        // We expose tools and emit listChanged notifications when config/project metadata updates
                        "tools": { "listChanged": true },
                        // Optionally declare logging support so hosts can subscribe if desired
                        "logging": {}
                    },
                    "serverInfo": {
                        "name": "lotar-mcp",
                        "version": env!("CARGO_PKG_VERSION")
                    },
                    "instructions": "Lotar MCP server exposes task, project, and config tools."
                }),
            )
        }
        // logging/setLevel -> accept the requested level and ack
        "logging/setLevel" => {
            let level = req
                .params
                .get("level")
                .and_then(|v| v.as_str())
                .unwrap_or("info");
            set_log_level(level);
            ok(req.id, json!({}))
        }
        // tools/list -> return available tool definitions with input schemas
        "tools/list" => {
            let enum_hints = gather_enum_hints();
            ok(
                req.id,
                json!({
                    "tools": build_tool_definitions(enum_hints.as_ref())
                }),
            )
        }
        "schema/discover" => {
            let enum_hints = gather_enum_hints();
            let mut tools = build_tool_definitions(enum_hints.as_ref());
            if let Some(filter) = req
                .params
                .get("tool")
                .and_then(|v| v.as_str())
                .map(|s| s.to_ascii_lowercase())
            {
                tools.retain(|tool| {
                    tool.get("name")
                        .and_then(|v| v.as_str())
                        .map(|name| name.to_ascii_lowercase() == filter)
                        .unwrap_or(false)
                });
            }

            let payload = json!({
                "status": "ok",
                "toolCount": tools.len(),
                "tools": tools,
            });

            ok(
                req.id,
                json!({
                    "content": [
                        {
                            "type": "text",
                            "text": serde_json::to_string_pretty(&payload).unwrap_or_else(|_| "{}".into())
                        }
                    ]
                }),
            )
        }
        // tools/call -> forward to specific method name in params.name
        "tools/call" => {
            let tool_name = match req.params.get("name").and_then(|v| v.as_str()) {
                Some(name) => name.to_string(),
                None => return err(req.id, -32602, "Missing tool name", None),
            };
            let arguments = req.params.get("arguments").cloned().unwrap_or(json!({}));
            let inner_req = JsonRpcRequest {
                jsonrpc: "2.0".into(),
                id: req.id.clone(),
                method: tool_name.clone(),
                params: arguments,
            };
            let mut response = dispatch(inner_req);
            if response.error.is_none() {
                let already_wrapped = response
                    .result
                    .as_ref()
                    .and_then(|value| value.get("functionResponse"))
                    .is_some();
                if !already_wrapped {
                    // VS Code MCP expects tool responses to expose `result.content` as an array.
                    // Gemini CLI expects `result.functionResponse.response` to contain the tool payload.
                    // Provide both shapes to maximize compatibility.
                    let inner_result = response.result.take().unwrap_or_else(|| json!({}));
                    match inner_result {
                        Value::Object(mut obj) => {
                            let cloned = Value::Object(obj.clone());
                            obj.insert(
                                "functionResponse".to_string(),
                                json!({
                                    "name": tool_name,
                                    "response": cloned,
                                }),
                            );
                            response.result = Some(Value::Object(obj));
                        }
                        other => {
                            response.result = Some(json!({
                                "content": [
                                    {
                                        "type": "text",
                                        "text": other.to_string()
                                    }
                                ],
                                "functionResponse": {
                                    "name": tool_name,
                                    "response": other
                                }
                            }));
                        }
                    }
                }
            }
            response
        }
        // task/create(params: TaskCreate) -> { task }
        "task/create" => handle_task_create(req),
        // task/get({ id, project? }) -> { task }
        "task/get" => handle_task_get(req),
        // config/show({ global?, project? }) -> { config }
        "config/show" => handle_config_show(req),
        // config/set({ global?, project?, values }) -> { updated }
        "config/set" => handle_config_set(req),
        // task/update({ id, patch }) -> { task }
        "task/update" => handle_task_update(req),
        // task/reference_add({ id, project?, kind, value }) -> { task, changed }
        "task/reference_add" => handle_task_reference_add(req),
        // task/reference_remove({ id, project?, kind, value }) -> { task, changed }
        "task/reference_remove" => handle_task_reference_remove(req),
        // task/delete({ id, project? }) -> { deleted }
        "task/delete" => handle_task_delete(req),
        // task/list(params: TaskListFilter) -> { tasks }
        "task/list" => handle_task_list(req),
        "sprint/add" => handle_sprint_add(req),
        "sprint/remove" => handle_sprint_remove(req),
        "sprint/delete" => handle_sprint_delete(req),
        "sprint/backlog" => handle_sprint_backlog(req),
        // project/list({}) -> { projects }
        "project/list" => handle_project_list(req),
        // project/stats({ name }) -> { stats }
        "project/stats" => handle_project_stats(req),
        _ => err(req.id, -32601, "Method not found", None),
    }
}

fn parse_limit_value(value: Option<&Value>, default: usize) -> Result<usize, &'static str> {
    match value {
        None | Some(Value::Null) => Ok(default),
        Some(Value::Number(num)) if num.is_u64() => Ok(num.as_u64().unwrap() as usize),
        Some(Value::String(text)) => text
            .trim()
            .parse::<usize>()
            .map_err(|_| "limit must be a positive integer"),
        _ => Err("limit must be a positive integer"),
    }
}

fn parse_cursor_value(value: Option<&Value>) -> Result<usize, &'static str> {
    match value {
        None | Some(Value::Null) => Ok(0),
        Some(Value::Number(num)) if num.is_u64() => Ok(num.as_u64().unwrap() as usize),
        Some(Value::String(text)) => text
            .trim()
            .parse::<usize>()
            .map_err(|_| "cursor must be a positive integer"),
        _ => Err("cursor must be a positive integer"),
    }
}

fn write_raw_json(stdout: &Arc<Mutex<io::Stdout>>, line: &str) {
    if let Ok(mut guard) = stdout.lock() {
        let _ = writeln!(&mut *guard, "{}", line);
        let _ = guard.flush();
    }
}

fn write_framed_json(stdout: &Arc<Mutex<io::Stdout>>, payload: &str) {
    if let Ok(mut guard) = stdout.lock() {
        let _ = write!(
            &mut *guard,
            "Content-Length: {}\r\n\r\n{}",
            payload.len(),
            payload
        );
        let _ = guard.flush();
    }
}

// inline tests moved to tests/mcp_server_unit_test.rs

// Helper for tests and simple harnesses: process one line and return response line
pub fn handle_json_line(line: &str) -> String {
    let req: Result<JsonRpcRequest, _> = serde_json::from_str(line);
    let response = match req {
        Ok(r) => dispatch(r),
        Err(e) => err(
            None,
            -32700,
            "Parse error",
            Some(json!({"details": e.to_string()})),
        ),
    };
    serde_json::to_string(&response).unwrap_or_else(|_| {
        // Fall back to a minimal, valid JSON-RPC error line
        "{\"jsonrpc\":\"2.0\",\"error\":{\"code\":-32603,\"message\":\"Serialization error\"}}"
            .to_string()
    })
}

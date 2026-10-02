use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::io::{self, Read, Write};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, OnceLock, RwLock, mpsc};
use std::time::Duration;

mod handlers;
mod hints;
mod registry;
mod schema;
mod session;
mod tools;
mod transport;
mod watchers;

#[cfg(test)]
mod mcp_server_tests;

use hints::gather_enum_hints;
use session::McpSession;
use tools::build_tool_definitions;
#[cfg(test)]
pub(crate) use watchers::event_affects_tooling;
use watchers::{ServerEvent, spawn_event_dispatcher, start_tools_change_notifier};

/// MCP protocol revision this server implements and always negotiates to.
const MCP_PROTOCOL_VERSION: &str = "2025-06-18";

/// JSON-RPC error code rejecting operational requests that arrive before the
/// session finished the initialize lifecycle.
const MCP_NOT_INITIALIZED: i64 = -32002;

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
const MAX_MCP_FRAME_BYTES: usize = 10 * 1024 * 1024;

/// Discard an oversized framed body byte-by-byte so the stream stays in sync
/// without ever allocating the announced size.
fn drain_framed_body<R: Read + ?Sized>(reader: &mut R, mut remaining: usize) {
    let mut sink = [0u8; 8192];
    while remaining > 0 {
        let want = remaining.min(sink.len());
        match reader.read(&mut sink[..want]) {
            Ok(0) => break,
            Ok(n) => remaining -= n,
            Err(_) => break,
        }
    }
}
const MCP_DEFAULT_PROJECT_LIST_LIMIT: usize = 50;
const MCP_MAX_PROJECT_LIST_LIMIT: usize = 200;
const MCP_DEFAULT_SPRINT_LIST_LIMIT: usize = 50;
const MCP_MAX_SPRINT_LIST_LIMIT: usize = 200;
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

/// Map a raw method spelling to its canonical wire method: snake_case tool
/// names (`task_create`) normalize to their direct slash form
/// (`task/create`); methods already containing `/` pass through unchanged.
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

/// Classify a domain-service error for MCP tool responses: expected domain
/// failures (validation, not-found, invalid references) use the custom
/// -32000 code the registry converts into `isError` tool results, while
/// genuine internal failures (I/O, serialization) keep the reserved
/// -32603 protocol error.
fn domain_service_error(
    id: Option<Value>,
    envelope: &str,
    error: &crate::errors::LoTaRError,
) -> JsonRpcResponse {
    use crate::errors::LoTaRError;
    match error {
        LoTaRError::ValidationError(_)
        | LoTaRError::TaskNotFound(_)
        | LoTaRError::SprintNotFound(_)
        | LoTaRError::InvalidTaskId(_)
        | LoTaRError::ProjectNotFound(_)
        | LoTaRError::IndexError(_) => err(
            id,
            -32000,
            envelope,
            Some(json!({ "message": error.to_string() })),
        ),
        _ => err(
            id,
            -32603,
            envelope,
            Some(json!({ "message": error.to_string() })),
        ),
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
    let response = parse_error_response(details);
    if let Ok(encoded) = serde_json::to_string(&response) {
        write_json_message(stdout, &encoded);
    }
}

fn parse_error_response(details: &str) -> JsonRpcResponse {
    err(
        Some(Value::Null),
        -32700,
        "Parse error",
        Some(json!({ "details": details })),
    )
}

fn invalid_request_response(id: Value, message: &str) -> JsonRpcResponse {
    err(Some(id), -32600, message, None)
}

fn write_serialized_response(stdout: &Arc<Mutex<io::Stdout>>, response: &JsonRpcResponse) {
    match serde_json::to_string(response) {
        Ok(payload) => write_json_message(stdout, &payload),
        Err(error) => {
            let fallback = json!({
                "jsonrpc": "2.0",
                "error": {
                    "code": -32603,
                    "message": format!("Serialization error: {error}")
                },
                "id": Value::Null
            })
            .to_string();
            write_json_message(stdout, &fallback);
        }
    }
}

/// What the wire loop should do after routing one incoming message.
enum WireOutcome {
    /// Respond to a request.
    Respond(JsonRpcResponse),
    /// A lifecycle transition happened; emit the coalesced deferred
    /// tools-list change if one is pending.
    FlushDeferred,
    /// Notification or ignored message: no response is ever written.
    Silent,
}

/// A syntactically valid JSON-RPC message with a validated envelope.
enum Incoming {
    Request {
        id: Value,
        method: String,
        params: Value,
    },
    Notification {
        method: String,
    },
}

/// Parse and validate one raw message (NDJSON line or framed body).
///
/// Error classification:
/// - syntactically malformed JSON -> -32700 Parse error (id null);
/// - structurally invalid envelopes (non-object payload, wrong `jsonrpc`,
///   non-string `method`, non-string/number `id`, non-object `params` on a
///   request) -> -32600 Invalid Request (request id echoed when valid).
fn parse_incoming(raw: &str) -> Result<Incoming, JsonRpcResponse> {
    let value: Value = match serde_json::from_str(raw) {
        Ok(value) => value,
        Err(error) => return Err(parse_error_response(&error.to_string())),
    };

    let object = match value {
        Value::Object(object) => object,
        _ => {
            return Err(invalid_request_response(
                Value::Null,
                "request must be a JSON object",
            ));
        }
    };

    match object.get("jsonrpc") {
        Some(Value::String(version)) if version == "2.0" => {}
        _ => {
            return Err(invalid_request_response(
                Value::Null,
                "jsonrpc must be exactly \"2.0\"",
            ));
        }
    }

    let method = match object.get("method") {
        Some(Value::String(method)) => method.clone(),
        _ => {
            return Err(invalid_request_response(
                Value::Null,
                "method must be a string",
            ));
        }
    };

    let id = match object.get("id") {
        None => None,
        Some(id) if is_valid_request_id(id) => Some(id.clone()),
        Some(_) => {
            return Err(invalid_request_response(
                Value::Null,
                "id must be a string or a number",
            ));
        }
    };

    let params = match object.get("params") {
        None | Some(Value::Null) => Value::Object(serde_json::Map::new()),
        Some(params @ Value::Object(_)) => params.clone(),
        Some(_) => match id {
            Some(id) => {
                return Err(invalid_request_response(id, "params must be an object"));
            }
            // Malformed notifications are dropped without a response, per
            // JSON-RPC: servers never reply to a message carrying no id.
            None => Value::Object(serde_json::Map::new()),
        },
    };

    match id {
        Some(id) => Ok(Incoming::Request { id, method, params }),
        None => Ok(Incoming::Notification { method }),
    }
}

fn is_valid_request_id(id: &Value) -> bool {
    match id {
        Value::String(_) => true,
        Value::Number(number) => number.is_u64() || number.is_i64(),
        _ => false,
    }
}

/// Route one parsed message. `session` is `None` for the stateless
/// in-process dispatch surface (treated as fully initialized); the wire
/// server always passes its own cold session.
fn route_message(session: Option<&McpSession>, message: Incoming) -> WireOutcome {
    match message {
        Incoming::Notification { method } => {
            // Notifications never receive responses and never execute tools.
            if normalize_method(&method) == "notifications/initialized"
                && let Some(session) = session
                && session.complete_initialize()
            {
                return WireOutcome::FlushDeferred;
            }
            WireOutcome::Silent
        }
        Incoming::Request { id, method, params } => {
            WireOutcome::Respond(route_request(session, id, &method, params))
        }
    }
}

fn route_request(
    session: Option<&McpSession>,
    id: Value,
    method: &str,
    params: Value,
) -> JsonRpcResponse {
    let method_key = normalize_method(method);

    // Lifecycle-exempt methods: initialize and ping answer in every state.
    match method_key.as_str() {
        "initialize" => {
            if let Err(issues) = validate_initialize_params(&params) {
                return err(
                    Some(id),
                    -32602,
                    "Invalid params",
                    Some(json!({ "tool": "initialize", "issues": issues })),
                );
            }
            if let Some(session) = session {
                // A second initialize never resets a live session.
                session.begin_initialize();
            }
            return initialize_response(id);
        }
        "ping" => return ok(Some(id), json!({})),
        _ => {}
    }

    // Everything else requires a Ready session on the wire; the stateless
    // surface (session == None) is dispatched as initialized by contract.
    if let Some(session) = session
        && !session.is_ready()
    {
        return err(
            Some(id),
            MCP_NOT_INITIALIZED,
            "Server not initialized",
            None,
        );
    }

    match method_key.as_str() {
        // tools/list -> return available tool definitions with input schemas
        "tools/list" => {
            let enum_hints = gather_enum_hints();
            ok(
                Some(id),
                json!({
                    "tools": build_tool_definitions(enum_hints.as_ref())
                }),
            )
        }
        // tools/call -> resolve the tool in the registry, validate arguments
        // against the advertised schema, then invoke its handler.
        "tools/call" => handle_tools_call(id, &params),
        // logging/setLevel -> accept the requested level and ack
        "logging/setLevel" => {
            let level = params
                .get("level")
                .and_then(|v| v.as_str())
                .unwrap_or("info");
            set_log_level(level);
            ok(Some(id), json!({}))
        }
        "schema/discover" => handle_schema_discover(id, &params),
        _ => match registry::find_tool_by_method(&method_key) {
            Some(spec) => {
                let (response, _domain_error) = registry::invoke_tool(spec, Some(id), params);
                response
            }
            None => err(Some(id), -32601, "Method not found", None),
        },
    }
}

/// Validate `initialize` params against the MCP 2025-06-18 request shape:
/// `protocolVersion` string, `capabilities` object, and `clientInfo` with
/// `name`/`version` strings. Unsupported version strings still negotiate to
/// the supported revision; a missing or malformed shape is invalid params.
fn validate_initialize_params(params: &Value) -> Result<(), Vec<String>> {
    let mut issues = Vec::new();
    match params.get("protocolVersion") {
        Some(Value::String(_)) => {}
        _ => issues.push("protocolVersion must be a string".to_string()),
    }
    match params.get("capabilities") {
        Some(Value::Object(_)) => {}
        _ => issues.push("capabilities must be an object".to_string()),
    }
    match params.get("clientInfo") {
        Some(Value::Object(client_info)) => {
            if !matches!(client_info.get("name"), Some(Value::String(_))) {
                issues.push("clientInfo.name must be a string".to_string());
            }
            if !matches!(client_info.get("version"), Some(Value::String(_))) {
                issues.push("clientInfo.version must be a string".to_string());
            }
        }
        _ => issues.push("clientInfo must be an object with name and version strings".to_string()),
    }
    if issues.is_empty() {
        Ok(())
    } else {
        Err(issues)
    }
}

fn initialize_response(id: Value) -> JsonRpcResponse {
    ok(
        Some(id),
        json!({
            // Whatever version the client offered, the server responds with
            // the revision it supports per the MCP negotiation rules.
            "protocolVersion": MCP_PROTOCOL_VERSION,
            "capabilities": {
                // Tools with listChanged notifications when config/project
                // metadata updates.
                "tools": { "listChanged": true },
                // Logging support so hosts can subscribe if desired.
                "logging": {}
            },
            "serverInfo": {
                "name": "lotar-mcp",
                "version": env!("CARGO_PKG_VERSION")
            },
            "instructions": "Lotar MCP server exposes task, project, config, and agent tools."
        }),
    )
}

fn handle_tools_call(id: Value, params: &Value) -> JsonRpcResponse {
    let Some(name) = params.get("name").and_then(|v| v.as_str()) else {
        return err(
            Some(id),
            -32602,
            "Invalid params",
            Some(json!({ "tool": null, "issues": ["params.name must be a string"] })),
        );
    };
    let spec = match registry::find_tool_for_call(name) {
        Ok(spec) => spec,
        Err(message) => return err(Some(id), -32602, &message, None),
    };
    let arguments = match params.get("arguments") {
        None | Some(Value::Null) => json!({}),
        Some(value @ Value::Object(_)) => value.clone(),
        Some(_) => {
            return err(
                Some(id),
                -32602,
                "Invalid params",
                Some(json!({
                    "tool": spec.name,
                    "issues": ["params.arguments must be an object"]
                })),
            );
        }
    };
    let (response, domain_error) = registry::invoke_tool(spec, Some(id), arguments);
    registry::wrap_for_tools_call(response, domain_error, spec.name)
}

fn handle_schema_discover(id: Value, params: &Value) -> JsonRpcResponse {
    let enum_hints = gather_enum_hints();
    let mut tools = build_tool_definitions(enum_hints.as_ref());
    if let Some(filter) = params
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
        Some(id),
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

/// Registry handler adapter: `schema_discover` is an advertised tool and this
/// exposes the control implementation under the tool-handler signature.
fn handle_schema_discover_request(req: JsonRpcRequest) -> JsonRpcResponse {
    handle_schema_discover(req.id.unwrap_or(Value::Null), &req.params)
}

/// Stateless single-request dispatch for the in-source test suite: identical
/// to [`handle_json_line`] after envelope parsing, routing through the
/// initialized (session-less) surface.
#[cfg(test)]
fn dispatch(req: JsonRpcRequest) -> JsonRpcResponse {
    let id = req.id.unwrap_or(Value::Null);
    route_request(None, id, &req.method, req.params)
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
    let session = Arc::new(McpSession::new());
    let (event_tx, event_rx) = mpsc::channel::<ServerEvent>();
    start_tools_change_notifier(event_tx);
    spawn_event_dispatcher(event_rx, stdout.clone(), Some(session.clone()));
    let mut reader = io::BufReader::new(stdin.lock());

    loop {
        // NDJSON reads are bounded to the same 10 MiB ceiling as framed
        // bodies; an overlong line is answered with a parse error and the
        // stream resynchronizes at the next line.
        let first_line = match transport::read_ndjson_line(&mut reader) {
            transport::NdjsonLineOutcome::Line(line) => line,
            transport::NdjsonLineOutcome::Overlong => {
                respond_parse_error(&stdout, "line exceeds maximum line size");
                continue;
            }
            transport::NdjsonLineOutcome::Eof | transport::NdjsonLineOutcome::InvalidUtf8 => break,
        };
        let trimmed = first_line.trim_end_matches(['\r', '\n']);
        if trimmed.is_empty() {
            continue;
        }

        if trimmed.to_ascii_lowercase().starts_with("content-length:") {
            enable_framed_output();
            match transport::read_framed_message(&mut reader, trimmed) {
                transport::FramedReadOutcome::Body(body) => {
                    let outcome = process_wire_text(&session, &body);
                    write_wire_outcome(&stdout, &session, outcome);
                }
                transport::FramedReadOutcome::Malformed(details) => {
                    respond_parse_error(&stdout, &details);
                }
            }
            continue;
        }

        let outcome = process_wire_text(&session, trimmed);
        write_wire_outcome(&stdout, &session, outcome);
    }
}

fn process_wire_text(session: &McpSession, raw: &str) -> WireOutcome {
    match parse_incoming(raw) {
        Ok(message) => route_message(Some(session), message),
        Err(error_response) => WireOutcome::Respond(error_response),
    }
}

fn write_wire_outcome(stdout: &Arc<Mutex<io::Stdout>>, session: &McpSession, outcome: WireOutcome) {
    match outcome {
        WireOutcome::Respond(response) => write_serialized_response(stdout, &response),
        WireOutcome::FlushDeferred => {
            if session.take_deferred_list_changed()
                && let Ok(payload) =
                    serde_json::to_string(&watchers::build_tools_changed_notification(&[]))
            {
                write_json_message(stdout, &payload);
            }
        }
        WireOutcome::Silent => {}
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

/// Stateless in-process dispatch helper for tests and simple harnesses.
///
/// This function parses ONE newline-delimited JSON-RPC message and dispatches
/// it as if the session were already fully initialized. It applies the same
/// envelope validation (JSON-RPC version, id type, params shape), tool-input
/// schema enforcement, and domain-error -> `isError` conversion as the wire
/// server, but it deliberately owns NO wire lifecycle:
///
/// - `initialize` returns the standard result without creating readiness;
/// - `notifications/initialized` (or any notification) is ignored and
///   produces no output line;
/// - no readiness state exists to leak: the wire server uses its own
///   [`McpSession`] exclusively, and pre-initialize rejection, notification
///   suppression, and deferred list-changed emission live only there.
///
/// Returns the serialized response line, or an empty string for
/// notifications and other messages that never receive responses.
pub fn handle_json_line(line: &str) -> String {
    match parse_incoming(line) {
        Err(error_response) => serde_json::to_string(&error_response).unwrap_or_else(|_| {
            // Fall back to a minimal, valid JSON-RPC error line
            "{\"jsonrpc\":\"2.0\",\"error\":{\"code\":-32603,\"message\":\"Serialization error\"},\"id\":null}"
                .to_string()
        }),
        Ok(message) => match route_message(None, message) {
            WireOutcome::Respond(response) => serde_json::to_string(&response).unwrap_or_else(
                |_| {
                    "{\"jsonrpc\":\"2.0\",\"error\":{\"code\":-32603,\"message\":\"Serialization error\"},\"id\":null}"
                        .to_string()
                },
            ),
            WireOutcome::FlushDeferred | WireOutcome::Silent => String::new(),
        },
    }
}

/// Shared test-only environment helpers for the MCP in-source suites, so the
/// per-variable mutex discipline is identical across modules.
#[cfg(test)]
pub(crate) mod test_env {
    use std::collections::HashMap;
    use std::path::Path;
    use std::sync::{LazyLock, Mutex, MutexGuard};

    static ENV_LOCKS: LazyLock<Mutex<HashMap<&'static str, &'static Mutex<()>>>> =
        LazyLock::new(|| Mutex::new(HashMap::new()));

    pub(crate) fn lock_var(var: &'static str) -> MutexGuard<'static, ()> {
        let lock: &'static Mutex<()> = {
            let mut map = ENV_LOCKS.lock().unwrap();
            if let Some(existing) = map.get(var) {
                existing
            } else {
                let leaked: &'static Mutex<()> = Box::leak(Box::new(Mutex::new(())));
                map.insert(var, leaked);
                leaked
            }
        };
        lock.lock().unwrap()
    }

    /// Guard that clears the LOTAR_TASKS_DIR test variables when dropped.
    pub(crate) struct TasksDirEnvGuard(MutexGuard<'static, ()>);

    impl Drop for TasksDirEnvGuard {
        fn drop(&mut self) {
            clear_tasks_dir();
            // Hold the per-variable lock until the environment is cleared.
            let _ = &self.0;
        }
    }

    pub(crate) fn lock_tasks_dir() -> TasksDirEnvGuard {
        TasksDirEnvGuard(lock_var("LOTAR_TASKS_DIR"))
    }

    pub(crate) fn set_tasks_dir(tasks_dir: &Path) {
        unsafe {
            std::env::remove_var("LOTAR_IGNORE_ENV_TASKS_DIR");
            std::env::remove_var("LOTAR_TEST_MODE");
            std::env::set_var("LOTAR_TASKS_DIR", tasks_dir);
        }
    }

    pub(crate) fn clear_tasks_dir() {
        unsafe {
            std::env::remove_var("LOTAR_TASKS_DIR");
            std::env::remove_var("LOTAR_IGNORE_ENV_TASKS_DIR");
            std::env::remove_var("LOTAR_TEST_MODE");
        }
    }
}

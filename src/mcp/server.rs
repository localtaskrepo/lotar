#[cfg(test)]
mod mcp_server_tests {
    use super::*;
    // Minimal per-variable lock for this test to avoid env races
    use std::collections::HashMap;
    use std::sync::LazyLock;
    use std::sync::{Mutex, MutexGuard};
    static ENV_LOCKS: LazyLock<Mutex<HashMap<&'static str, &'static Mutex<()>>>> =
        LazyLock::new(|| Mutex::new(HashMap::new()));
    fn lock_var(var: &'static str) -> MutexGuard<'static, ()> {
        let mtx: &'static Mutex<()> = {
            let mut map = ENV_LOCKS.lock().unwrap();
            if let Some(m) = map.get(var) {
                m
            } else {
                let boxed: Box<Mutex<()>> = Box::new(Mutex::new(()));
                let leaked: &'static Mutex<()> = Box::leak(boxed);
                map.insert(var, leaked);
                leaked
            }
        };
        mtx.lock().unwrap()
    }

    #[test]
    fn tools_call_update_delete_list_and_invalid_enum() {
        let _lock = lock_var("LOTAR_TASKS_DIR");
        let tmp = tempfile::tempdir().unwrap();
        let tasks_dir = tmp.path().join(".tasks");
        std::fs::create_dir_all(&tasks_dir).unwrap();
        // Set test-specific LOTAR_TASKS_DIR
        unsafe {
            std::env::set_var("LOTAR_TASKS_DIR", &tasks_dir);
        }

        // Create a task
        let create_args = json!({
            "name": "task_create",
            "arguments": { "title": "MCP Update", "project": "MCP" }
        });
        let create_req = JsonRpcRequest {
            jsonrpc: "2.0".into(),
            id: Some(json!(10)),
            method: "tools/call".into(),
            params: create_args,
        };
        let create_resp = dispatch(create_req);
        assert!(create_resp.error.is_none(), "task_create failed");
        let content = create_resp
            .result
            .as_ref()
            .unwrap()
            .get("content")
            .and_then(|v| v.as_array())
            .unwrap();
        let text = content[0].get("text").and_then(|v| v.as_str()).unwrap();
        let task_json: serde_json::Value = serde_json::from_str(text).unwrap();

        let id = task_json
            .get("id")
            .and_then(|v| v.as_str())
            .unwrap()
            .to_string();

        // Update the task
        let update_args = json!({
            "name": "task_update",
            "arguments": { "id": id, "project": "MCP", "patch": { "title": "Updated Title" } }
        });
        let update_req = JsonRpcRequest {
            jsonrpc: "2.0".into(),
            id: Some(json!(11)),
            method: "tools/call".into(),
            params: update_args,
        };
        let update_resp = dispatch(update_req);
        if update_resp.error.is_some() {
            // Print tree to help debug
            fn print_dir_tree<P: AsRef<std::path::Path>>(path: P, indent: usize) {
                let path = path.as_ref();
                if let Ok(entries) = std::fs::read_dir(path) {
                    for entry in entries.flatten() {
                        let p = entry.path();
                        let name = p.file_name().and_then(|n| n.to_str()).unwrap_or("");
                        eprintln!("{0:indent$}{1}", "", name, indent = indent);
                        if p.is_dir() {
                            print_dir_tree(&p, indent + 2);
                        }
                    }
                }
            }
            eprintln!("Update failed; .tasks tree:");
            print_dir_tree(&tasks_dir, 2);
        }
        assert!(
            update_resp.error.is_none(),
            "task_update failed: {:?}",
            update_resp.error
        );
        let update_content = update_resp
            .result
            .as_ref()
            .unwrap()
            .get("content")
            .and_then(|v| v.as_array())
            .unwrap();
        let update_text = update_content[0]
            .get("text")
            .and_then(|v| v.as_str())
            .unwrap();
        let updated_json: serde_json::Value = serde_json::from_str(update_text).unwrap();
        assert_eq!(updated_json.get("title").unwrap(), "Updated Title");

        // List tasks
        let list_args = json!({ "name": "task_list", "arguments": { "project": "MCP" } });
        let list_req = JsonRpcRequest {
            jsonrpc: "2.0".into(),
            id: Some(json!(12)),
            method: "tools/call".into(),
            params: list_args,
        };
        let list_resp = dispatch(list_req);
        assert!(list_resp.error.is_none(), "task_list failed");
        let list_content = list_resp
            .result
            .as_ref()
            .unwrap()
            .get("content")
            .and_then(|v| v.as_array())
            .unwrap();
        let list_text = list_content[0]
            .get("text")
            .and_then(|v| v.as_str())
            .unwrap();
        let tasks: serde_json::Value = serde_json::from_str(list_text).unwrap();
        assert!(
            tasks
                .as_array()
                .unwrap()
                .iter()
                .any(|t| t.get("id").unwrap() == &serde_json::Value::String(id.clone()))
        );

        // Delete the task
        let delete_args =
            json!({ "name": "task_delete", "arguments": { "id": id, "project": "MCP" } });
        let delete_req = JsonRpcRequest {
            jsonrpc: "2.0".into(),
            id: Some(json!(13)),
            method: "tools/call".into(),
            params: delete_args,
        };
        let delete_resp = dispatch(delete_req);
        assert!(delete_resp.error.is_none(), "task_delete failed");
        let del_content = delete_resp
            .result
            .as_ref()
            .unwrap()
            .get("content")
            .and_then(|v| v.as_array())
            .unwrap();
        let del_text = del_content[0].get("text").and_then(|v| v.as_str()).unwrap();
        assert!(del_text.contains("deleted=true"));

        // Negative test: invalid priority
        let bad_args = json!({ "name": "task_create", "arguments": { "title": "bad", "project": "MCP", "priority": "NOT_A_PRIORITY" } });
        let bad_req = JsonRpcRequest {
            jsonrpc: "2.0".into(),
            id: Some(json!(14)),
            method: "tools/call".into(),
            params: bad_args,
        };
        let bad_resp = dispatch(bad_req);
        assert!(
            bad_resp.error.is_some(),
            "Expected error for invalid priority"
        );
        let msg = bad_resp.error.as_ref().unwrap().message.to_lowercase();
        assert!(msg.contains("priority"));

        // Negative test: invalid type
        let bad_type_args = json!({ "name": "task_create", "arguments": { "title": "bad", "project": "MCP", "type": "NOT_A_TYPE" } });
        let bad_type_req = JsonRpcRequest {
            jsonrpc: "2.0".into(),
            id: Some(json!(15)),
            method: "tools/call".into(),
            params: bad_type_args,
        };
        let bad_type_resp = dispatch(bad_type_req);
        assert!(
            bad_type_resp.error.is_some(),
            "Expected error for invalid type"
        );
        let msg = bad_type_resp.error.as_ref().unwrap().message.to_lowercase();
        assert!(msg.contains("type"));

        unsafe {
            std::env::remove_var("LOTAR_TASKS_DIR");
        }
    }
}
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::io::{self, BufRead, Read, Write};
use std::sync::{OnceLock, RwLock};

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

// Normalize method/tool names so callers can use either "group/op" or "group_op".
// If a slash is present, we keep it. If only underscores, replace the first underscore with a slash.
// Otherwise, return as-is (e.g., "initialize").
fn normalize_method(name: &str) -> String {
    if name.contains('/') {
        name.to_string()
    } else if let Some(idx) = name.find('_') {
        let (left, right) = name.split_at(idx);
        // right starts with '_', so skip it when joining
        format!("{}/{}", left, &right[1..])
    } else {
        name.to_string()
    }
}

// Simple in-process log level storage for MCP logging capability.
// This is intentionally minimal; the server avoids noisy stdout and doesn't stream logs.
static LOG_LEVEL: OnceLock<RwLock<String>> = OnceLock::new();
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

pub fn run_stdio_server() {
    // Optional auto-reload: exit when the binary is rebuilt so the host can restart us
    let autoreload_enabled = std::env::var("LOTAR_MCP_AUTORELOAD")
        .ok()
        .map(|v| v != "0")
        .unwrap_or(true);
    if autoreload_enabled {
        if let Ok(exe_path) = std::env::current_exe() {
            if let Ok(meta) = std::fs::metadata(&exe_path) {
                if let Ok(modified) = meta.modified() {
                    let initial = modified;
                    std::thread::spawn(move || {
                        use std::time::Duration;
                        loop {
                            std::thread::sleep(Duration::from_secs(2));
                            if let Ok(Ok(m)) = std::fs::metadata(&exe_path).map(|m| m.modified()) {
                                if m > initial {
                                    // Binary updated; exit to allow host to restart with new build
                                    std::process::exit(0);
                                }
                            }
                        }
                    });
                }
            }
        }
    }
    let stdin = io::stdin();
    let mut stdout = io::stdout();
    let mut reader = io::BufReader::new(stdin.lock());

    loop {
        // Try to read either an LSP-style header-framed message or a single-line JSON
        let mut first_line = String::new();
        if reader
            .read_line(&mut first_line)
            .ok()
            .filter(|&n| n > 0)
            .is_none()
        {
            break; // EOF
        }
        let trimmed = first_line.trim_end_matches(['\r', '\n']);
        if trimmed.is_empty() {
            continue;
        }

        if trimmed.to_ascii_lowercase().starts_with("content-length:") {
            // Accumulate headers until blank line
            let mut content_length: Option<usize> = None;
            if let Some(v) = trimmed.split(':').nth(1) {
                content_length = v.trim().parse::<usize>().ok();
            }
            // Read the rest of the headers
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
                if l.to_ascii_lowercase().starts_with("content-length:") {
                    if let Some(v) = l.split(':').nth(1) {
                        content_length = v.trim().parse::<usize>().ok();
                    }
                }
            }
            if let Some(len) = content_length {
                let mut buf = vec![0u8; len];
                if let Err(e) = reader.read_exact(&mut buf) {
                    let resp = err(
                        None,
                        -32700,
                        "Parse error",
                        Some(json!({"details": format!("body read failed: {}", e)})),
                    );
                    if let Ok(s) = serde_json::to_string(&resp) {
                        let _ = writeln!(stdout, "{}", s);
                    }
                    let _ = stdout.flush();
                    continue;
                }
                let body = match String::from_utf8(buf) {
                    Ok(s) => s,
                    Err(e) => {
                        let resp = err(
                            None,
                            -32700,
                            "Parse error",
                            Some(json!({"details": format!("utf8 error: {}", e)})),
                        );
                        if let Ok(s) = serde_json::to_string(&resp) {
                            let _ = writeln!(stdout, "{}", s);
                        }
                        let _ = stdout.flush();
                        continue;
                    }
                };
                let req: Result<JsonRpcRequest, _> = serde_json::from_str(&body);
                let response = match req {
                    Ok(r) => dispatch(r),
                    Err(e) => err(
                        None,
                        -32700,
                        "Parse error",
                        Some(json!({"details": e.to_string()})),
                    ),
                };
                // Respond using the same framing style (Content-Length)
                let payload = match serde_json::to_string(&response) {
                    Ok(s) => s,
                    Err(e) => format!(
                        "{{\"jsonrpc\":\"2.0\",\"error\":{{\"code\":-32603,\"message\":\"Serialization error: {}\"}},\"id\":null}}",
                        e
                    ),
                };
                let _ = write!(
                    stdout,
                    "Content-Length: {}\r\n\r\n{}",
                    payload.len(),
                    payload
                );
                let _ = stdout.flush();
                continue;
            } else {
                // No length found; skip
                let resp = err(
                    None,
                    -32700,
                    "Parse error",
                    Some(json!({"details": "missing Content-Length"})),
                );
                if let Ok(s) = serde_json::to_string(&resp) {
                    let _ = writeln!(stdout, "{}", s);
                }
                let _ = stdout.flush();
                continue;
            }
        }

        // Fallback: treat the line as a full JSON-RPC object (line-delimited mode)
        let req: Result<JsonRpcRequest, _> = serde_json::from_str(trimmed);
        let response = match req {
            Ok(r) => dispatch(r),
            Err(e) => err(
                None,
                -32700,
                "Parse error",
                Some(json!({"details": e.to_string()})),
            ),
        };
        if let Ok(s) = serde_json::to_string(&response) {
            let _ = writeln!(stdout, "{}", s);
        }
        let _ = stdout.flush();
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

            ok(
                req.id,
                json!({
                    "protocolVersion": negotiated,
                    "capabilities": {
                        // We currently only expose tools; listChanged notifies are not implemented
                        "tools": { "listChanged": false },
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
        "tools/list" => ok(
            req.id,
            json!({
                "tools": [
                    {
                        "name": "task_create",
                        "description": "Create a new task. Note: priority and type are project-configured strings; call config/show for allowed values.",
                        "inputSchema": {
                            "type": "object",
                            "properties": {
                                "title": {"type": "string"},
                                "description": {"type": ["string", "null"]},
                                "project": {"type": ["string", "null"]},
                                "priority": {"type": ["string", "null"]},
                                "type": {"type": ["string", "null"]},
                                "assignee": {"type": ["string", "null"]},
                                "due_date": {"type": ["string", "null"]},
                                "effort": {"type": ["string", "null"]},
                                "category": {"type": ["string", "null"]},
                                "tags": {"type": "array", "items": {"type": "string"}},
                                "custom_fields": {"type": "object"}
                            },
                            "required": ["title"],
                            "additionalProperties": false
                        }
                    },
                    {
                        "name": "task_get",
                        "description": "Get a task by id",
                        "inputSchema": {
                            "type": "object",
                            "properties": {
                                "id": {"type": "string"},
                                "project": {"type": ["string", "null"]}
                            },
                            "required": ["id"],
                            "additionalProperties": false
                        }
                    },
                    {
                        "name": "task_update",
                        "description": "Update a task by id. Note: status, priority, and type are project-configured strings; call config/show for allowed values.",
                        "inputSchema": {
                            "type": "object",
                            "properties": {
                                "id": {"type": "string"},
                                "patch": {
                                    "type": "object",
                                    "properties": {
                                        "title": {"type": ["string", "null"]},
                                        "description": {"type": ["string", "null"]},
                                        "status": {"type": ["string", "null"]},
                                        "priority": {"type": ["string", "null"]},
                                        "type": {"type": ["string", "null"]},
                                        "assignee": {"type": ["string", "null"]},
                                        "due_date": {"type": ["string", "null"]},
                                        "effort": {"type": ["string", "null"]},
                                        "category": {"type": ["string", "null"]},
                                        "tags": {"type": "array", "items": {"type": "string"}},
                                        "custom_fields": {"type": "object"}
                                    },
                                    "additionalProperties": false
                                }
                            },
                            "required": ["id"],
                            "additionalProperties": false
                        }
                    },
                    {
                        "name": "task_delete",
                        "description": "Delete a task by id",
                        "inputSchema": {
                            "type": "object",
                            "properties": {
                                "id": {"type": "string"},
                                "project": {"type": ["string", "null"]}
                            },
                            "required": ["id"],
                            "additionalProperties": false
                        }
                    },
                    {
                        "name": "task_list",
            "description": "List tasks with optional filters. Note: status, priority, and type are project-configured strings; call config/show for allowed values.",
                        "inputSchema": {
                            "type": "object",
                            "properties": {
                                "project": {"type": ["string", "null"]},
                "status": {"oneOf": [ {"type": "string"}, {"type": "array", "items": {"type": "string"}} ]},
                                "assignee": {"type": ["string", "null"]},
                "priority": {"oneOf": [ {"type": "string"}, {"type": "array", "items": {"type": "string"}} ]},
                "type": {"oneOf": [ {"type": "string"}, {"type": "array", "items": {"type": "string"}} ]},
                                "tag": {"type": ["string", "null"]},
                                "search": {"type": ["string", "null"]}
                            },
                            "additionalProperties": false
                        }
                    },
                    {
                        "name": "project_list",
                        "description": "List projects",
                        "inputSchema": {"type": "object", "properties": {}, "additionalProperties": false}
                    },
                    {
                        "name": "project_stats",
                        "description": "Get project statistics",
                        "inputSchema": {
                            "type": "object",
                            "properties": {"name": {"type": "string"}},
                            "required": ["name"],
                            "additionalProperties": false
                        }
                    },
                    {
                        "name": "config_show",
                        "description": "Show effective configuration",
                        "inputSchema": {
                            "type": "object",
                            "properties": {
                                "global": {"type": ["boolean", "null"]},
                                "project": {"type": ["string", "null"]}
                            },
                            "additionalProperties": false
                        }
                    },
                    {
                        "name": "config_set",
                        "description": "Set configuration values",
                        "inputSchema": {
                            "type": "object",
                            "properties": {
                                "global": {"type": ["boolean", "null"]},
                                "project": {"type": ["string", "null"]},
                                "values": {"type": "object", "additionalProperties": {"type": "string"}}
                            },
                            "required": ["values"],
                            "additionalProperties": false
                        }
                    }
                ]
            }),
        ),
        // tools/call -> forward to specific method name in params.name
        "tools/call" => {
            let name = req.params.get("name").and_then(|v| v.as_str());
            let arguments = req.params.get("arguments").cloned().unwrap_or(json!({}));
            if name.is_none() {
                return err(req.id, -32602, "Missing tool name", None);
            }
            let inner_req = JsonRpcRequest {
                jsonrpc: "2.0".into(),
                id: req.id.clone(),
                method: name.unwrap().to_string(),
                params: arguments,
            };
            dispatch(inner_req)
        }
        // task/create(params: TaskCreate) -> { task }
        "task/create" => {
            // Build DTO manually so we can accept strings and validate via config
            let resolver = match crate::workspace::TasksDirectoryResolver::resolve(None, None) {
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
            let cfg_mgr =
                match crate::config::manager::ConfigManager::new_manager_with_tasks_dir_readonly(
                    &resolver.path,
                ) {
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
            let validator = crate::cli::validation::CliValidator::new(cfg);

            let title = match req.params.get("title").and_then(|v| v.as_str()) {
                Some(s) if !s.is_empty() => s.to_string(),
                _ => return err(req.id, -32602, "Missing required field: title", None),
            };
            let project = req
                .params
                .get("project")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string());
            let priority = if let Some(s) = req.params.get("priority").and_then(|v| v.as_str()) {
                match validator.validate_priority(s) {
                    Ok(v) => Some(v),
                    Err(e) => {
                        return err(
                            req.id,
                            -32602,
                            &format!("Priority validation failed: {}", e),
                            None,
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
                        return err(
                            req.id,
                            -32602,
                            &format!("Type validation failed: {}", e),
                            None,
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
                .and_then(|s| {
                    crate::utils::identity::resolve_me_alias(s, Some(resolver.path.as_path()))
                });
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
            let category = req
                .params
                .get("category")
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
            // helper to convert JSON -> feature-aware custom value type
            fn json_to_custom(val: &serde_json::Value) -> crate::types::CustomFieldValue {
                #[cfg(feature = "schema")]
                {
                    val.clone()
                }
                #[cfg(not(feature = "schema"))]
                {
                    serde_yaml::to_value(val).unwrap_or(serde_yaml::Value::Null)
                }
            }
            let custom_fields = req
                .params
                .get("custom_fields")
                .and_then(|v| v.as_object())
                .map(|o| {
                    let mut m: std::collections::HashMap<String, crate::types::CustomFieldValue> =
                        std::collections::HashMap::new();
                    for (k, v) in o.iter() {
                        m.insert(k.clone(), json_to_custom(v));
                    }
                    m
                });

            // Assemble DTO
            let dto = crate::api_types::TaskCreate {
                title,
                project,
                priority,
                task_type,
                reporter: req
                    .params
                    .get("reporter")
                    .and_then(|v| v.as_str())
                    .and_then(|s| {
                        crate::utils::identity::resolve_me_alias(s, Some(resolver.path.as_path()))
                    }),
                assignee,
                due_date,
                effort,
                description,
                category,
                tags,
                custom_fields,
            };

            let mut storage = crate::storage::manager::Storage::new(resolver.path);
            match crate::services::task_service::TaskService::create(&mut storage, dto) {
                Ok(task) => ok(
                    req.id,
                    json!({
                        "content": [ { "type": "text", "text": serde_json::to_string_pretty(&task).unwrap_or_else(|_| "{}".into()) } ]
                    }),
                ),
                Err(e) => err(
                    req.id,
                    -32000,
                    "Task create failed",
                    Some(json!({"message": e.to_string()})),
                ),
            }
        }
        // task/get({ id, project? }) -> { task }
        "task/get" => {
            let id = req
                .params
                .get("id")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string());
            if id.is_none() {
                return err(req.id, -32602, "Missing id", None);
            }
            let project = req.params.get("project").and_then(|v| v.as_str());
            let resolver = match crate::workspace::TasksDirectoryResolver::resolve(None, None) {
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
            match crate::services::task_service::TaskService::get(&storage, &id.unwrap(), project) {
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
        // config/show({ global?, project? }) -> { config }
        "config/show" => {
            let global = req
                .params
                .get("global")
                .and_then(|v| v.as_bool())
                .unwrap_or(false);
            let project = req.params.get("project").and_then(|v| v.as_str());
            let resolver = match crate::workspace::TasksDirectoryResolver::resolve(None, None) {
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
            match crate::services::config_service::ConfigService::show(&resolver, scope) {
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
        // config/set({ global?, project?, values }) -> { updated }
        // values is an object of string->string (use same behavior as REST)
        "config/set" => {
            let resolver = match crate::workspace::TasksDirectoryResolver::resolve(None, None) {
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
            let mut map = std::collections::BTreeMap::new();
            for (k, v) in values.iter() {
                map.insert(k.clone(), v.as_str().unwrap_or(&v.to_string()).to_string());
            }
            let global = req
                .params
                .get("global")
                .and_then(|v| v.as_bool())
                .unwrap_or(false);
            let project = req.params.get("project").and_then(|v| v.as_str());
            match crate::services::config_service::ConfigService::set(
                &resolver, &map, global, project,
            ) {
                Ok(_) => ok(
                    req.id,
                    json!({
                        "content": [ { "type": "text", "text": "Configuration updated" } ]
                    }),
                ),
                Err(e) => err(
                    req.id,
                    -32002,
                    "Config set failed",
                    Some(json!({"message": e.to_string()})),
                ),
            }
        }
        // task/update({ id, patch }) -> { task }
        "task/update" => {
            let id = req
                .params
                .get("id")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string());
            if id.is_none() {
                return err(req.id, -32602, "Missing id", None);
            }
            let resolver = match crate::workspace::TasksDirectoryResolver::resolve(None, None) {
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
            let cfg_mgr =
                match crate::config::manager::ConfigManager::new_manager_with_tasks_dir_readonly(
                    &resolver.path,
                ) {
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
            let validator = crate::cli::validation::CliValidator::new(cfg);
            let patch_val = req.params.get("patch").cloned().unwrap_or(json!({}));
            if !patch_val.is_object() {
                return err(req.id, -32602, "Invalid patch (expected object)", None);
            }
            let mut patch = crate::api_types::TaskUpdate::default();
            if let Some(s) = patch_val.get("title").and_then(|v| v.as_str()) {
                patch.title = Some(s.to_string());
            }
            if let Some(s) = patch_val.get("status").and_then(|v| v.as_str()) {
                match validator.validate_status(s) {
                    Ok(v) => patch.status = Some(v),
                    Err(e) => {
                        return err(
                            req.id,
                            -32602,
                            &format!("Status validation failed: {}", e),
                            None,
                        );
                    }
                }
            }
            if let Some(s) = patch_val.get("priority").and_then(|v| v.as_str()) {
                match validator.validate_priority(s) {
                    Ok(v) => patch.priority = Some(v),
                    Err(e) => {
                        return err(
                            req.id,
                            -32602,
                            &format!("Priority validation failed: {}", e),
                            None,
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
                        return err(
                            req.id,
                            -32602,
                            &format!("Type validation failed: {}", e),
                            None,
                        );
                    }
                }
            }
            if let Some(s) = patch_val.get("reporter").and_then(|v| v.as_str()) {
                patch.reporter =
                    crate::utils::identity::resolve_me_alias(s, Some(resolver.path.as_path()));
            }
            if let Some(s) = patch_val.get("assignee").and_then(|v| v.as_str()) {
                patch.assignee =
                    crate::utils::identity::resolve_me_alias(s, Some(resolver.path.as_path()));
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
            if let Some(s) = patch_val.get("category").and_then(|v| v.as_str()) {
                patch.category = Some(s.to_string());
            }
            if let Some(arr) = patch_val.get("tags").and_then(|v| v.as_array()) {
                patch.tags = Some(
                    arr.iter()
                        .filter_map(|x| x.as_str().map(|s| s.to_string()))
                        .collect(),
                );
            }
            if let Some(obj) = patch_val.get("custom_fields").and_then(|v| v.as_object()) {
                fn json_to_custom(val: &serde_json::Value) -> crate::types::CustomFieldValue {
                    #[cfg(feature = "schema")]
                    {
                        val.clone()
                    }
                    #[cfg(not(feature = "schema"))]
                    {
                        serde_yaml::to_value(val).unwrap_or(serde_yaml::Value::Null)
                    }
                }
                let mut m: std::collections::HashMap<String, crate::types::CustomFieldValue> =
                    std::collections::HashMap::new();
                for (k, v) in obj.iter() {
                    m.insert(k.clone(), json_to_custom(v));
                }
                patch.custom_fields = Some(m);
            }
            let mut storage = crate::storage::manager::Storage::new(resolver.path);
            match crate::services::task_service::TaskService::update(
                &mut storage,
                &id.unwrap(),
                patch,
            ) {
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
        // task/delete({ id, project? }) -> { deleted }
        "task/delete" => {
            let id = req.params.get("id").and_then(|v| v.as_str());
            if id.is_none() {
                return err(req.id, -32602, "Missing id", None);
            }
            let project = req.params.get("project").and_then(|v| v.as_str());
            let resolver = match crate::workspace::TasksDirectoryResolver::resolve(None, None) {
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
            let mut storage = crate::storage::manager::Storage::new(resolver.path);
            match crate::services::task_service::TaskService::delete(
                &mut storage,
                id.unwrap(),
                project,
            ) {
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
        // task/list(params: TaskListFilter) -> { tasks }
        "task/list" => {
            // Accept string or array for status/priority/type and validate via config
            let resolver = match crate::workspace::TasksDirectoryResolver::resolve(None, None) {
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
            let cfg_mgr =
                match crate::config::manager::ConfigManager::new_manager_with_tasks_dir_readonly(
                    &resolver.path,
                ) {
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
            let validator = crate::cli::validation::CliValidator::new(cfg);

            // Helper to parse list or single
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
            let category = req
                .params
                .get("category")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string());
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

            let filter = crate::api_types::TaskListFilter {
                status,
                priority,
                task_type,
                project,
                category,
                tags,
                text_query,
            };
            let storage = crate::storage::manager::Storage::new(resolver.path);
            let tasks = crate::services::task_service::TaskService::list(&storage, &filter)
                .into_iter()
                .map(|(_, t)| t)
                .collect::<Vec<_>>();
            ok(
                req.id,
                json!({
                    "content": [ { "type": "text", "text": serde_json::to_string_pretty(&tasks).unwrap_or_else(|_| "[]".into()) } ]
                }),
            )
        }
        // project/list({}) -> { projects }
        "project/list" => {
            let resolver = match crate::workspace::TasksDirectoryResolver::resolve(None, None) {
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
            let projects = crate::services::project_service::ProjectService::list(&storage);
            ok(
                req.id,
                json!({
                    "content": [ { "type": "text", "text": serde_json::to_string_pretty(&projects).unwrap_or_else(|_| "[]".into()) } ]
                }),
            )
        }
        // project/stats({ name }) -> { stats }
        "project/stats" => {
            let name = req.params.get("name").and_then(|v| v.as_str());
            if name.is_none() {
                return err(req.id, -32602, "Missing name", None);
            }
            let resolver = match crate::workspace::TasksDirectoryResolver::resolve(None, None) {
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
            let stats =
                crate::services::project_service::ProjectService::stats(&storage, name.unwrap());
            ok(
                req.id,
                json!({
                    "content": [ { "type": "text", "text": serde_json::to_string_pretty(&stats).unwrap_or_else(|_| "{}".into()) } ]
                }),
            )
        }
        _ => err(req.id, -32601, "Method not found", None),
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

//! Real-process MCP stdio lifecycle coverage (DEV-62).
//!
//! These tests drive an actual `lotar mcp` child over NDJSON (and framed
//! compatibility) so the wire contract is exercised end to end: cold-session
//! pre-initialize rejection, the initialize -> notifications/initialized ->
//! ready state machine, notification suppression, envelope validation, and
//! deferred tools-list notifications. The stateless `handle_json_line`
//! helper is never used as a readiness shortcut.

mod common;

use std::io::{BufRead, BufReader, Read, Write};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::mpsc::{Receiver, RecvTimeoutError, channel};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use serde_json::{Value, json};

/// One decoded unit from the server's stdout: a bare NDJSON line or the body
/// of a framed message.
#[derive(Debug, Clone)]
enum Wire {
    Line(String),
    Frame(String),
}

impl Wire {
    fn parse_json(&self) -> Value {
        let text = match self {
            Wire::Line(line) => line.as_str(),
            Wire::Frame(body) => body.as_str(),
        };
        serde_json::from_str(text)
            .unwrap_or_else(|error| panic!("non-JSON output {text:?}: {error}"))
    }

    fn is_framed(&self) -> bool {
        matches!(self, Wire::Frame(_))
    }
}

struct McpChild {
    child: Child,
    stdin: ChildStdin,
    messages: Receiver<Wire>,
    stderr_log: Arc<Mutex<Vec<String>>>,
}

impl Drop for McpChild {
    fn drop(&mut self) {
        let _ = self.stdin.flush();
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

impl McpChild {
    fn spawn(tasks_dir: &std::path::Path) -> Self {
        // Same child-scoped hygiene as common::lotar_cmd (isolated home
        // config, no RUST_TEST_THREADS), but as a std Command so stdin and
        // stdout pipes can be owned directly.
        let mut command = Command::new(env!("CARGO_BIN_EXE_lotar"));
        command
            .arg("mcp")
            .env_remove("RUST_TEST_THREADS")
            .env("LOTAR_IGNORE_HOME_CONFIG", "1")
            .env("LOTAR_TASKS_DIR", tasks_dir)
            .env("LOTAR_MCP_AUTORELOAD", "0")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        let mut child = command.spawn().expect("spawn lotar mcp");
        let stdin = child.stdin.take().expect("stdin");
        let stdout = child.stdout.take().expect("stdout");
        let stderr = child.stderr.take().expect("stderr");

        // The reader understands BOTH transports: NDJSON lines and
        // Content-Length framed bodies (a framed response has no trailing
        // newline, so a plain line reader would block forever).
        let (tx, rx) = channel::<Wire>();
        std::thread::spawn(move || {
            let mut reader = BufReader::new(stdout);
            'outer: loop {
                let mut header = String::new();
                match reader.read_line(&mut header) {
                    Ok(0) => break,
                    Ok(_) => {}
                    Err(_) => break,
                }
                let trimmed = header.trim_end_matches(['\r', '\n']);
                if trimmed.is_empty() {
                    continue;
                }
                if trimmed.to_ascii_lowercase().starts_with("content-length:") {
                    let length = trimmed
                        .split(':')
                        .nth(1)
                        .and_then(|value| value.trim().parse::<usize>().ok())
                        .unwrap_or(0);
                    loop {
                        let mut extra = String::new();
                        match reader.read_line(&mut extra) {
                            Ok(0) => break 'outer,
                            Ok(_) => {}
                            Err(_) => break 'outer,
                        }
                        if extra.trim_end_matches(['\r', '\n']).is_empty() {
                            break;
                        }
                    }
                    let mut body = vec![0u8; length];
                    if reader.read_exact(&mut body).is_err() {
                        break;
                    }
                    if tx
                        .send(Wire::Frame(String::from_utf8_lossy(&body).to_string()))
                        .is_err()
                    {
                        break;
                    }
                } else if tx.send(Wire::Line(trimmed.to_string())).is_err() {
                    break;
                }
            }
        });

        let stderr_log = Arc::new(Mutex::new(Vec::new()));
        let stderr_sink = stderr_log.clone();
        std::thread::spawn(move || {
            let reader = BufReader::new(stderr);
            for line in reader.lines() {
                match line {
                    Ok(line) => {
                        let mut log = stderr_sink.lock().unwrap();
                        if log.len() < 200 {
                            log.push(line);
                        }
                    }
                    Err(_) => break,
                }
            }
        });

        Self {
            child,
            stdin,
            messages: rx,
            stderr_log,
        }
    }

    fn send(&mut self, value: &Value) {
        let line = serde_json::to_string(value).unwrap();
        writeln!(self.stdin, "{line}").expect("write to mcp stdin");
        self.stdin.flush().expect("flush mcp stdin");
    }

    fn send_raw(&mut self, raw: &str) {
        writeln!(self.stdin, "{raw}").expect("write raw to mcp stdin");
        self.stdin.flush().expect("flush mcp stdin");
    }

    fn send_frame(&mut self, body: &str) {
        write!(self.stdin, "Content-Length: {}\r\n\r\n{}", body.len(), body)
            .expect("write frame to mcp stdin");
        self.stdin.flush().expect("flush mcp stdin");
    }

    fn next_wire(&mut self, what: &str) -> Wire {
        match self.messages.recv_timeout(Duration::from_secs(10)) {
            Ok(wire) => wire,
            Err(RecvTimeoutError::Timeout) => panic!(
                "timed out waiting for {what}; stderr: {:?}",
                self.stderr_log.lock().unwrap()
            ),
            Err(RecvTimeoutError::Disconnected) => panic!(
                "server exited while waiting for {what}; stderr: {:?}",
                self.stderr_log.lock().unwrap()
            ),
        }
    }

    fn next_json(&mut self, what: &str) -> Value {
        self.next_wire(what).parse_json()
    }

    fn expect_silence(&mut self, what: &str) {
        match self.messages.recv_timeout(Duration::from_millis(600)) {
            Ok(wire) => panic!("unexpected output while {what}: {wire:?}"),
            Err(RecvTimeoutError::Disconnected) => panic!(
                "server exited unexpectedly while {what}; stderr: {:?}",
                self.stderr_log.lock().unwrap()
            ),
            Err(RecvTimeoutError::Timeout) => {}
        }
    }

    fn request(&mut self, id: i64, method: &str, params: Value) -> Value {
        self.send(&json!({"jsonrpc": "2.0", "id": id, "method": method, "params": params}));
        self.next_json(&format!("response to {method}"))
    }
}

fn valid_initialize_params() -> Value {
    json!({
        "protocolVersion": "2025-06-18",
        "capabilities": {},
        "clientInfo": {"name": "lotar-wire-test", "version": "1.0.0"}
    })
}

fn workspace() -> (tempfile::TempDir, std::path::PathBuf) {
    let tmp = tempfile::tempdir().unwrap();
    let tasks_dir = tmp.path().join(".tasks");
    std::fs::create_dir_all(&tasks_dir).unwrap();
    (tmp, tasks_dir)
}

fn workspace_with_config() -> (tempfile::TempDir, std::path::PathBuf) {
    let (tmp, tasks_dir) = workspace();
    std::fs::write(
        tasks_dir.join("config.yml"),
        "default:\n  project: MCP\nissue:\n  tags:\n    - alpha\n",
    )
    .unwrap();
    (tmp, tasks_dir)
}

fn error_code(response: &Value) -> i64 {
    response
        .get("error")
        .and_then(|error| error.get("code"))
        .and_then(|code| code.as_i64())
        .unwrap_or(0)
}

fn error_message(response: &Value) -> String {
    response
        .get("error")
        .and_then(|error| error.get("message"))
        .and_then(|message| message.as_str())
        .unwrap_or_default()
        .to_string()
}

fn ready_session(tasks_dir: &std::path::Path) -> McpChild {
    let mut child = McpChild::spawn(tasks_dir);
    let initialize = child.request(1, "initialize", valid_initialize_params());
    assert!(initialize.get("error").is_none(), "initialize failed");
    child.send(&json!({"jsonrpc": "2.0", "method": "notifications/initialized"}));
    child
}

fn direct_tool_payload(response: &Value) -> Value {
    let text = response
        .get("result")
        .and_then(|result| result.get("content"))
        .and_then(|content| content.as_array())
        .and_then(|entries| entries.first())
        .and_then(|entry| entry.get("text"))
        .and_then(|text| text.as_str())
        .unwrap_or("{}");
    serde_json::from_str(text).unwrap_or(json!({}))
}

#[test]
fn premature_requests_rejected_with_not_initialized_and_zero_writes() {
    let (_tmp, tasks_dir) = workspace();
    let mut child = McpChild::spawn(&tasks_dir);

    for (id, method, params) in [
        (1, "tools/list", json!({})),
        (
            2,
            "tools/call",
            json!({"name": "task_list", "arguments": {}}),
        ),
        (3, "schema/discover", json!({})),
        (4, "logging/setLevel", json!({"level": "debug"})),
        (5, "task/list", json!({})),
        (6, "task/create", json!({"title": "Premature"})),
        (7, "sync/push", json!({"remote": "origin"})),
    ] {
        let response = child.request(id, method, params);
        assert_eq!(
            error_code(&response),
            -32002,
            "{method} before initialize must be rejected: {response}"
        );
        assert_eq!(
            error_message(&response),
            "Server not initialized",
            "{method}"
        );
        assert_eq!(
            response.get("id").and_then(|v| v.as_i64()),
            Some(id),
            "id must be echoed for {method}"
        );
    }

    // ping is exempt before initialization and returns an empty result.
    let ping = child.request(10, "ping", json!({}));
    assert!(ping.get("error").is_none(), "ping must be exempt: {ping}");
    assert_eq!(ping.get("result"), Some(&json!({})));

    // Zero task/config writes happened.
    let entries: Vec<_> = std::fs::read_dir(&tasks_dir)
        .unwrap()
        .filter_map(|entry| entry.ok())
        .map(|entry| entry.file_name().to_string_lossy().to_string())
        .collect();
    assert!(
        entries.is_empty(),
        "premature requests must not write anything: {entries:?}"
    );
}

#[test]
fn initialize_response_and_await_window_before_initialized_notification() {
    let (_tmp, tasks_dir) = workspace();
    let mut child = McpChild::spawn(&tasks_dir);

    let initialize = child.request(1, "initialize", valid_initialize_params());
    assert!(initialize.get("error").is_none(), "initialize failed");
    let result = initialize.get("result").expect("initialize result");
    assert_eq!(
        result.get("protocolVersion").and_then(|v| v.as_str()),
        Some("2025-06-18")
    );
    assert!(
        result
            .get("capabilities")
            .and_then(|v| v.as_object())
            .is_some(),
        "capabilities object required"
    );
    let server_info = result.get("serverInfo").expect("serverInfo");
    assert!(server_info.get("name").and_then(|v| v.as_str()).is_some());
    assert!(
        server_info
            .get("version")
            .and_then(|v| v.as_str())
            .is_some()
    );
    assert!(
        result
            .get("capabilities")
            .and_then(|c| c.get("tools"))
            .and_then(|tools| tools.get("listChanged"))
            .and_then(|flag| flag.as_bool())
            == Some(true),
        "tools.listChanged capability stays camelCase"
    );

    // Awaiting window: operational requests stay rejected between the
    // initialize response and notifications/initialized.
    let awaiting = child.request(2, "tools/list", json!({}));
    assert_eq!(error_code(&awaiting), -32002, "awaiting window: {awaiting}");

    // initialized is a notification: no response is ever written for it.
    child.send(&json!({"jsonrpc": "2.0", "method": "notifications/initialized"}));
    child.expect_silence("awaiting the reply-free initialized notification");

    // ping still answers {} and tools/list now works.
    let ping = child.request(3, "ping", json!({}));
    assert_eq!(ping.get("result"), Some(&json!({})));
    let list = child.request(4, "tools/list", json!({}));
    assert!(
        list.get("error").is_none(),
        "tools/list after ready: {list}"
    );
    let tools = list
        .get("result")
        .and_then(|result| result.get("tools"))
        .and_then(|tools| tools.as_array())
        .expect("tools array");
    assert_eq!(tools.len(), 37, "advertised tool count");
}

#[test]
fn tool_methods_sent_as_notifications_never_execute_or_reply() {
    let (_tmp, tasks_dir) = workspace();
    let mut child = ready_session(&tasks_dir);

    // Mutating tool methods without request ids: no response, no execution.
    child.send(&json!({
        "jsonrpc": "2.0",
        "method": "task/create",
        "params": {"title": "Ghost task", "project": "MCP"}
    }));
    child.send(&json!({
        "jsonrpc": "2.0",
        "method": "task_create",
        "params": {"title": "Ghost task 2", "project": "MCP"}
    }));
    child.send(&json!({"jsonrpc": "2.0", "method": "some/unknown/notification"}));
    child.expect_silence("notifications never receive responses");

    let ping = child.request(50, "ping", json!({}));
    assert_eq!(ping.get("result"), Some(&json!({})));

    let list = child.request(51, "task/list", json!({}));
    assert!(list.get("error").is_none(), "task/list must work: {list}");
    let payload = direct_tool_payload(&list);
    assert_eq!(
        payload.get("total").and_then(|v| v.as_u64()),
        Some(0),
        "notification-sent tool calls must not execute"
    );
}

#[test]
fn out_of_order_initialized_never_grants_readiness() {
    let (_tmp, tasks_dir) = workspace();
    let mut child = McpChild::spawn(&tasks_dir);

    // initialized before any initialize request: ignored, grants nothing.
    child.send(&json!({"jsonrpc": "2.0", "method": "notifications/initialized"}));
    child.expect_silence("out-of-order initialized notification");
    let premature = child.request(1, "tools/list", json!({}));
    assert_eq!(error_code(&premature), -32002);

    // initialize moves to the awaiting window only.
    let initialize = child.request(2, "initialize", valid_initialize_params());
    assert!(initialize.get("error").is_none());
    let still_awaiting = child.request(3, "tools/list", json!({}));
    assert_eq!(
        error_code(&still_awaiting),
        -32002,
        "stale initialized must not grant readiness: {still_awaiting}"
    );

    // The real initialized notification completes the lifecycle.
    child.send(&json!({"jsonrpc": "2.0", "method": "notifications/initialized"}));
    let list = child.request(4, "tools/list", json!({}));
    assert!(
        list.get("error").is_none(),
        "ready after initialized: {list}"
    );
}

#[test]
fn second_initialize_does_not_reset_live_session() {
    let (_tmp, tasks_dir) = workspace();
    let mut child = ready_session(&tasks_dir);

    let reinitialize = child.request(2, "initialize", valid_initialize_params());
    assert!(
        reinitialize.get("error").is_none(),
        "repeated initialize answers without error: {reinitialize}"
    );

    // The session stayed ready: operational requests still work.
    let list = child.request(3, "tools/list", json!({}));
    assert!(
        list.get("error").is_none(),
        "session must stay ready: {list}"
    );
}

#[test]
fn envelope_validation_rejects_invalid_versions_and_id_types() {
    let (_tmp, tasks_dir) = workspace();
    let mut child = ready_session(&tasks_dir);

    // Malformed JSON -> parse error with null id.
    child.send_raw("{this is not json");
    let parse_error = child.next_json("parse error");
    assert_eq!(error_code(&parse_error), -32700);
    assert_eq!(parse_error.get("id"), Some(&Value::Null));

    // Wrong jsonrpc version.
    child.send_raw("{\"jsonrpc\": \"1.0\", \"id\": 60, \"method\": \"ping\"}");
    let wrong_version = child.next_json("jsonrpc version");
    assert_eq!(error_code(&wrong_version), -32600);
    assert_eq!(wrong_version.get("id"), Some(&Value::Null));

    // Invalid id types.
    for raw in [
        r#"{"jsonrpc": "2.0", "id": true, "method": "ping"}"#,
        r#"{"jsonrpc": "2.0", "id": [], "method": "ping"}"#,
        r#"{"jsonrpc": "2.0", "id": {}, "method": "ping"}"#,
        r#"{"jsonrpc": "2.0", "id": null, "method": "ping"}"#,
        r#"{"jsonrpc": "2.0", "id": 1.5, "method": "ping"}"#,
    ] {
        child.send_raw(raw);
        let invalid = child.next_json("invalid id");
        assert_eq!(error_code(&invalid), -32600, "raw: {raw}");
        assert_eq!(invalid.get("id"), Some(&Value::Null), "raw: {raw}");
    }

    // Non-string method and non-object params on a request.
    child.send_raw(r#"{"jsonrpc": "2.0", "id": 61, "method": 42}"#);
    let bad_method = child.next_json("method type");
    assert_eq!(error_code(&bad_method), -32600);
    child.send_raw(r#"{"jsonrpc": "2.0", "id": 62, "method": "ping", "params": [1, 2]}"#);
    let bad_params = child.next_json("params type");
    assert_eq!(error_code(&bad_params), -32600);
    assert_eq!(bad_params.get("id").and_then(|v| v.as_i64()), Some(62));

    // A valid id survives on the wire afterwards.
    let ping = child.request(63, "ping", json!({}));
    assert_eq!(ping.get("result"), Some(&json!({})));
}

#[test]
fn initialize_params_shape_is_enforced_and_versions_negotiate() {
    let (_tmp, tasks_dir) = workspace();
    let mut child = McpChild::spawn(&tasks_dir);

    for (label, params) in [
        ("empty", json!({})),
        (
            "missing clientInfo",
            json!({"protocolVersion": "2025-06-18", "capabilities": {}}),
        ),
        (
            "missing capabilities",
            json!({"protocolVersion": "2025-06-18", "clientInfo": {"name": "x", "version": "1"}}),
        ),
        (
            "missing protocolVersion",
            json!({"capabilities": {}, "clientInfo": {"name": "x", "version": "1"}}),
        ),
        (
            "non-string protocolVersion",
            json!({"protocolVersion": 2025, "capabilities": {}, "clientInfo": {"name": "x", "version": "1"}}),
        ),
        (
            "clientInfo missing version",
            json!({"protocolVersion": "2025-06-18", "capabilities": {}, "clientInfo": {"name": "x"}}),
        ),
    ] {
        let response = child.request(1, "initialize", params);
        assert_eq!(
            error_code(&response),
            -32602,
            "initialize params ({label}) must be invalid: {response}"
        );
        // Session stays uninitialized.
        let premature = child.request(2, "tools/list", json!({}));
        assert_eq!(error_code(&premature), -32002, "after {label}");
    }

    // Unsupported version strings negotiate to the supported revision.
    let negotiate = child.request(
        3,
        "initialize",
        json!({
            "protocolVersion": "1999-01-01",
            "capabilities": {},
            "clientInfo": {"name": "old-client", "version": "0.1"}
        }),
    );
    assert!(negotiate.get("error").is_none(), "negotiation failed");
    assert_eq!(
        negotiate
            .get("result")
            .and_then(|result| result.get("protocolVersion"))
            .and_then(|v| v.as_str()),
        Some("2025-06-18")
    );
}

#[test]
fn framed_transport_roundtrips_and_recovers_from_oversize_frames() {
    let (_tmp, tasks_dir) = workspace();
    let mut child = McpChild::spawn(&tasks_dir);

    let initialize_body = serde_json::to_string(&json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "initialize",
        "params": valid_initialize_params()
    }))
    .unwrap();
    child.send_frame(&initialize_body);

    // Framed input switches output framing.
    let initialize = child.next_wire("framed initialize");
    assert!(initialize.is_framed(), "expected framed output");
    let initialize = initialize.parse_json();
    assert!(
        initialize.get("error").is_none(),
        "framed initialize failed"
    );

    child.send(&json!({"jsonrpc": "2.0", "method": "notifications/initialized"}));
    child.expect_silence("framed session ready");

    // Oversize frame: header claims more than 10 MiB; the server must answer
    // with a parse error and stay in sync for a follow-up framed request.
    let junk_size = 10 * 1024 * 1024 + 1;
    write!(child.stdin, "Content-Length: {junk_size}\r\n\r\n").unwrap();
    let junk = vec![b'x'; junk_size];
    child.stdin.write_all(&junk).unwrap();
    child.stdin.flush().unwrap();

    let oversize = child.next_wire("oversize parse error");
    assert!(oversize.is_framed(), "parse error must use framed output");
    let oversize = oversize.parse_json();
    assert_eq!(error_code(&oversize), -32700);
    assert!(
        oversize
            .get("error")
            .and_then(|error| error.get("data"))
            .and_then(|data| data.get("details"))
            .and_then(|v| v.as_str())
            .unwrap_or_default()
            .contains("exceeds maximum frame size")
    );

    // Framed follow-up still works after the drain.
    let ping_body = serde_json::to_string(&json!({
        "jsonrpc": "2.0", "id": 7, "method": "ping"
    }))
    .unwrap();
    child.send_frame(&ping_body);
    let ping = child.next_wire("framed ping");
    assert!(ping.is_framed());
    assert_eq!(ping.parse_json().get("result"), Some(&json!({})));
}

#[test]
fn oversize_ndjson_line_answers_parse_error_and_server_survives() {
    let (_tmp, tasks_dir) = workspace();
    let mut child = ready_session(&tasks_dir);

    // A single NDJSON line larger than the 10 MiB ceiling (matching the
    // framed bound): the server must answer a parse error, keep its memory
    // bounded, and resynchronize at the next line.
    let huge = "a".repeat(10 * 1024 * 1024 + 1);
    let payload = format!(
        "{{\"jsonrpc\":\"2.0\",\"id\":80,\"method\":\"tools/call\",\"params\":{{\"name\":\"task_list\",\"arguments\":{{\"search\":\"{huge}\"}}}}}}"
    );
    child.send_raw(&payload);
    let overlong = child.next_json("oversize line parse error");
    assert_eq!(error_code(&overlong), -32700);
    assert!(
        overlong
            .get("error")
            .and_then(|error| error.get("data"))
            .and_then(|data| data.get("details"))
            .and_then(|v| v.as_str())
            .unwrap_or_default()
            .contains("maximum line size"),
        "parse error must name the line ceiling: {overlong}"
    );

    // The stream resynchronizes: a normal request still works.
    let ping = child.request(81, "ping", json!({}));
    assert_eq!(ping.get("result"), Some(&json!({})));
}

#[test]
fn long_multibyte_enum_rejection_is_an_error_not_a_crash() {
    let (_tmp, tasks_dir) = workspace();
    let mut child = ready_session(&tasks_dir);

    // Reviewer repro: 81-byte multi-byte value whose byte 80 sits mid
    // character on the enum-rejection preview path. The server must answer
    // a normal -32602 and keep serving.
    let repro = "研".repeat(27);
    let request = json!({
        "jsonrpc": "2.0",
        "id": 90,
        "method": "tools/call",
        "params": {"name": "task_list", "arguments": {"due": repro}}
    });
    child.send(&request);
    let rejection = child.next_json("multibyte enum rejection");
    assert_eq!(error_code(&rejection), -32602, "got: {rejection}");
    assert!(
        rejection
            .get("error")
            .and_then(|error| error.get("data"))
            .and_then(|data| data.get("issues"))
            .and_then(|v| v.as_array())
            .map(|issues| !issues.is_empty())
            .unwrap_or(false),
        "issues must carry the truncated preview: {rejection}"
    );

    // Direct-method surface too, then prove the server still answers.
    child.send(&json!({
        "jsonrpc": "2.0",
        "id": 91,
        "method": "task/list",
        "params": {"due": repro}
    }));
    let direct = child.next_json("direct multibyte rejection");
    assert_eq!(error_code(&direct), -32602);
    let ping = child.request(92, "ping", json!({}));
    assert_eq!(ping.get("result"), Some(&json!({})));
}

#[test]
fn deferred_list_changed_is_coalesced_and_emitted_after_ready() {
    // Timing note: the tools watcher polls on an intentional 2s cadence (the
    // kernel watcher is best-effort under sandboxed file-watch syscalls), so
    // this test uses generous bounds. By construction nothing can be emitted
    // before notifications/initialized: the dispatcher defers while the
    // session is not ready, whether or not the poll has observed the change.
    let (_tmp, tasks_dir) = workspace_with_config();
    let mut child = McpChild::spawn(&tasks_dir);

    let initialize = child.request(1, "initialize", valid_initialize_params());
    assert!(initialize.get("error").is_none());

    // Let the watcher take its baseline snapshot (one poll period) BEFORE
    // the change, so the diff below is deterministic.
    std::thread::sleep(Duration::from_millis(2300));

    // Trigger a tools-hint change (tags list grows) while only initialized,
    // not ready. Give the poller ample time to observe it.
    std::fs::write(
        tasks_dir.join("config.yml"),
        "default:\n  project: MCP\nissue:\n  tags:\n    - alpha\n    - beta\n",
    )
    .unwrap();
    std::thread::sleep(Duration::from_millis(2600));

    // Nothing may be emitted pre-ready.
    child.expect_silence("pre-ready window must stay silent");

    // The ready transition flushes the coalesced change (or the watcher emits
    // directly now that the session is ready); either way the notification
    // arrives only after notifications/initialized.
    child.send(&json!({"jsonrpc": "2.0", "method": "notifications/initialized"}));
    let notification = child.next_json("deferred list-changed notification");
    assert_eq!(
        notification.get("method").and_then(|v| v.as_str()),
        Some("notifications/tools/list_changed"),
        "notification method must use the notifications namespace: {notification}"
    );
    assert!(
        notification.get("id").is_none(),
        "notifications never carry an id"
    );

    // The session is ready and operational afterwards.
    let ping = child.request(2, "ping", json!({}));
    assert_eq!(ping.get("result"), Some(&json!({})));
}

use crate::api_server::{self, HttpRequest};
use crate::output::{LogLevel, OutputFormat, OutputRenderer};
use include_dir::{Dir, include_dir};
use std::collections::HashMap;
use std::fs;
use std::io::{Read, Write};
use std::net::TcpListener;
use std::path::Path;
use std::sync::{LazyLock, Mutex};

// Embed built web assets. In dev/CI, we use repo's tmp/web snapshot to avoid build-time panics.
// To re-embed production assets, point this to target/web after running the web build.
static STATIC_FILES: Dir<'_> = include_dir!("$CARGO_MANIFEST_DIR/tmp/web");

pub fn serve(api_server: &api_server::ApiServer, port: u16) {
    let listener = match TcpListener::bind(format!("127.0.0.1:{}", port)) {
        Ok(l) => l,
        Err(e) => {
            OutputRenderer::new(OutputFormat::Text, LogLevel::Error)
                .log_error(&format!("Failed to bind to port {}: {}", port, e));
            return;
        }
    };
    // Register this server instance in a global stop registry (used only by tests)
    static STOP_FLAGS: LazyLock<Mutex<HashMap<u16, bool>>> =
        LazyLock::new(|| Mutex::new(HashMap::new()));
    {
        let mut map = STOP_FLAGS.lock().unwrap();
        map.insert(port, false);
    }

    // Suppress startup info logs to keep tests and consumers' output clean.
    for stream in listener.incoming() {
        // Check for test-initiated shutdown before handling the next connection
        {
            if STOP_FLAGS
                .lock()
                .ok()
                .and_then(|m| m.get(&port).cloned())
                .unwrap_or(false)
            {
                break;
            }
        }
        match stream {
            Ok(mut stream) => {
                // Read headers fully first
                let mut head_buf: Vec<u8> = Vec::new();
                let mut tmp = [0u8; 1024];
                loop {
                    let n: usize = stream.read(&mut tmp).unwrap_or_default();
                    if n == 0 {
                        break;
                    }
                    head_buf.extend_from_slice(&tmp[..n]);
                    if head_buf.windows(4).any(|w| w == b"\r\n\r\n") {
                        break;
                    }
                    if head_buf.len() > 32 * 1024 {
                        break;
                    } // cap headers
                }
                let header_end = head_buf
                    .windows(4)
                    .position(|w| w == b"\r\n\r\n")
                    .map(|i| i + 4)
                    .unwrap_or(head_buf.len());
                let (head_part, body_bytes) = head_buf.split_at(header_end);
                let request_head = String::from_utf8_lossy(head_part);
                let request_line = match request_head.lines().next() {
                    Some(line) => line,
                    None => {
                        continue;
                    }
                };
                let request_parts: Vec<&str> = request_line.split(" ").collect();
                let method = request_parts.first().cloned().unwrap_or("GET").to_string();
                let path_full = request_parts.get(1).cloned().unwrap_or("/");
                let (path, query) = parse_path_and_query(path_full);
                // basic header parse
                let mut headers = HashMap::new();
                for line in request_head.lines().skip(1) {
                    if line.trim().is_empty() {
                        break;
                    }
                    if let Some((k, v)) = line.split_once(":") {
                        headers.insert(k.trim().to_string(), v.trim().to_string());
                    }
                }
                // Read remaining body based on Content-Length
                let mut body: Vec<u8> = body_bytes.to_vec();
                if let Some(cl) = headers
                    .get("Content-Length")
                    .and_then(|v| v.parse::<usize>().ok())
                {
                    while body.len() < cl {
                        let n: usize = stream.read(&mut tmp).unwrap_or_default();
                        if n == 0 {
                            break;
                        }
                        body.extend_from_slice(&tmp[..n]);
                    }
                    body.truncate(cl);
                }

                // SSE endpoints (simple line-delimited JSON, with optional debounce and filters)
                if path == "/api/events" || path == "/api/tasks/stream" {
                    use std::time::{Duration, Instant};

                    // Per-connection settings
                    let debounce_ms = query
                        .get("debounce_ms")
                        .and_then(|s| s.parse::<u64>().ok())
                        .or_else(|| {
                            std::env::var("LOTAR_SSE_DEBOUNCE_MS")
                                .ok()
                                .and_then(|s| s.parse::<u64>().ok())
                        })
                        .unwrap_or(100);
                    let kinds_filter: Option<Vec<String>> =
                        query.get("kinds").or_else(|| query.get("topic")).map(|s| {
                            s.split(',')
                                .filter(|p| !p.is_empty())
                                .map(|p| p.trim().to_string())
                                .collect()
                        });
                    let project_filter: Option<String> = query.get("project").cloned();

                    let rx = crate::api_events::subscribe();
                    // Test acceleration: allow LOTAR_TEST_FAST_IO=1 to lower latencies
                    let fast = std::env::var("LOTAR_TEST_FAST_IO").ok().as_deref() == Some("1");
                    let headers = "HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nCache-Control: no-cache\r\nConnection: keep-alive\r\nAccess-Control-Allow-Origin: *\r\n\r\n";
                    // Send headers and initial retry hint together
                    let mut initial = String::with_capacity(headers.len() + 14);
                    initial.push_str(headers);
                    initial.push_str("retry: 1000\n\n");
                    let _ = stream.write_all(initial.as_bytes());
                    let _ = stream.flush();
                    // Optional: emit an immediate readiness event for clients/tests to sync on
                    // Minimized in production: requires LOTAR_SSE_READY=1 to be set.
                    let allow_ready = std::env::var("LOTAR_SSE_READY").ok().as_deref() == Some("1");
                    if allow_ready
                        && query
                            .get("ready")
                            .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
                            .unwrap_or(false)
                    {
                        let _ = stream.write_all(b"event: ready\ndata: {}\n\n");
                        let _ = stream.flush();
                    }
                    // Spawn a thread to forward events to this client with debounce and filtering
                    let mut stream_clone = stream.try_clone().unwrap();
                    std::thread::spawn(move || {
                        let mut buffer: Vec<crate::api_events::ApiEvent> = Vec::new();
                        let mut deadline: Option<Instant> = None;
                        let debounce = Duration::from_millis(if fast {
                            debounce_ms.min(20)
                        } else {
                            debounce_ms
                        });
                        let heartbeat_every = if fast {
                            Duration::from_secs(2)
                        } else {
                            Duration::from_secs(15)
                        };
                        loop {
                            // Determine timeout for recv
                            let timeout = match deadline {
                                Some(d) => {
                                    let now = Instant::now();
                                    if d <= now {
                                        Duration::from_millis(0)
                                    } else {
                                        d - now
                                    }
                                }
                                None => heartbeat_every, // send heartbeat if idle
                            };

                            match rx.recv_timeout(timeout) {
                                Ok(evt) => {
                                    // Apply filters before buffering
                                    if let Some(ref kinds) = kinds_filter {
                                        if !kinds.iter().any(|k| k.eq_ignore_ascii_case(&evt.kind))
                                        {
                                            continue;
                                        }
                                    }
                                    if let Some(ref pf) = project_filter {
                                        // Extract id string from event data
                                        let id_opt = if evt.kind == "task_deleted" {
                                            evt.data.get("id").and_then(|v| v.as_str())
                                        } else {
                                            evt.data.get("id").and_then(|v| v.as_str())
                                        };
                                        if let Some(id_str) = id_opt {
                                            let prefix = id_str.split('-').next().unwrap_or("");
                                            if prefix != pf {
                                                continue;
                                            }
                                        } else {
                                            // If we cannot determine, conservatively skip
                                            continue;
                                        }
                                    }
                                    buffer.push(evt);
                                    // Set or extend deadline
                                    let now = Instant::now();
                                    deadline = Some(now + debounce);
                                }
                                Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {
                                    // Time to flush if we have buffered events, otherwise heartbeat
                                    if !buffer.is_empty() {
                                        for evt in buffer.drain(..) {
                                            let line = format!(
                                                "event: {}\ndata: {}\n\n",
                                                evt.kind,
                                                serde_json::to_string(&evt.data)
                                                    .unwrap_or("null".to_string())
                                            );
                                            if stream_clone.write_all(line.as_bytes()).is_err() {
                                                return; // client gone
                                            }
                                        }
                                        let _ = stream_clone.flush();
                                        deadline = None; // reset deadline until next event arrives
                                    } else {
                                        // Idle: keep connection alive
                                        if stream_clone.write_all(b":heartbeat\n\n").is_err() {
                                            return; // client gone
                                        }
                                        let _ = stream_clone.flush();
                                    }
                                }
                                Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => {
                                    return; // broadcaster dropped
                                }
                            }
                        }
                    });
                    continue;
                } else if path == "/__test/stop" {
                    // Test-only endpoint to allow clean shutdown of the server loop
                    if let Ok(mut map) = STOP_FLAGS.lock() {
                        map.insert(port, true);
                    }
                    let response = "HTTP/1.1 200 OK\r\nContent-Type: text/plain\r\nContent-Length: 8\r\n\r\nstopping";
                    let _ = stream.write_all(response.as_bytes());
                    let _ = stream.flush();
                    // Loop will observe the flag on next iteration and break
                    continue;
                } else if path.starts_with("/api") {
                    // Serve static OpenAPI if requested
                    if path == "/api/openapi.json" {
                        let spec = include_str!("../docs/openapi.json");
                        let response = format!(
                            "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nAccess-Control-Allow-Origin: *\r\nContent-Length: {}\r\n\r\n{}",
                            spec.len(),
                            spec
                        );
                        let _ = stream.write_all(response.as_bytes());
                        let _ = stream.flush();
                        continue;
                    }
                    // CORS preflight for API
                    if method.eq_ignore_ascii_case("OPTIONS") {
                        let preflight = "HTTP/1.1 204 No Content\r\nAccess-Control-Allow-Origin: *\r\nAccess-Control-Allow-Methods: GET,POST,OPTIONS\r\nAccess-Control-Allow-Headers: Content-Type\r\nContent-Length: 0\r\n\r\n";
                        let _ = stream.write_all(preflight.as_bytes());
                        let _ = stream.flush();
                        continue;
                    }
                    // Execute the appropriate rust code to handle the API request
                    let req = HttpRequest {
                        method,
                        path: path.clone(),
                        query,
                        headers,
                        body,
                    };
                    let mut resp = api_server.handle_request(&req);
                    // CORS permissive defaults
                    resp.headers
                        .push(("Access-Control-Allow-Origin".into(), "*".into()));
                    resp.headers.push((
                        "Access-Control-Allow-Methods".into(),
                        "GET,POST,OPTIONS".into(),
                    ));
                    resp.headers
                        .push(("Access-Control-Allow-Headers".into(), "Content-Type".into()));
                    if !resp
                        .headers
                        .iter()
                        .any(|(k, _)| k.eq_ignore_ascii_case("Content-Type"))
                    {
                        resp.headers
                            .push(("Content-Type".into(), "application/json".into()));
                    }
                    let headers_str = resp
                        .headers
                        .iter()
                        .map(|(k, v)| format!("{}: {}\r\n", k, v))
                        .collect::<String>();
                    let status_line = match resp.status {
                        200 => "200 OK",
                        201 => "201 Created",
                        404 => "404 Not Found",
                        400 => "400 Bad Request",
                        500 => "500 Internal Server Error",
                        _ => "200 OK",
                    };
                    let response = format!(
                        "HTTP/1.1 {}\r\n{}Content-Length: {}\r\n\r\n",
                        status_line,
                        headers_str,
                        resp.body.len()
                    );
                    let _ = stream.write_all(response.as_bytes());
                    let _ = stream.write_all(&resp.body);
                    let _ = stream.flush();
                } else {
                    // Serve embedded static files first, then fallback to filesystem
                    let request_path = if path == "/" { "/index.html" } else { &path };
                    let rel_path = request_path.trim_start_matches('/');

                    // Try embedded
                    if let Some(file) = STATIC_FILES.get_file(rel_path) {
                        let data = file.contents();
                        let ext = Path::new(rel_path)
                            .extension()
                            .and_then(|e| e.to_str())
                            .unwrap_or("");
                        let content_type = match ext {
                            "html" => "text/html",
                            "jpg" | "jpeg" => "image/jpeg",
                            "png" => "image/png",
                            "css" => "text/css",
                            "js" => "application/javascript",
                            "svg" => "image/svg+xml",
                            _ => "application/octet-stream",
                        };
                        let header = format!(
                            "HTTP/1.1 200 OK\r\nContent-Type: {}\r\nContent-Length: {}\r\n\r\n",
                            content_type,
                            data.len()
                        );
                        let _ = stream.write_all(header.as_bytes());
                        let _ = stream.write_all(data);
                        let _ = stream.flush();
                    } else {
                        // Fallback to filesystem (useful in dev)
                        let fs_path = Path::new("target/web").join(rel_path);
                        match fs::read(&fs_path) {
                            Ok(bytes) => {
                                let ext =
                                    fs_path.extension().and_then(|e| e.to_str()).unwrap_or("");
                                let content_type = match ext {
                                    "html" => "text/html",
                                    "jpg" | "jpeg" => "image/jpeg",
                                    "png" => "image/png",
                                    "css" => "text/css",
                                    "js" => "application/javascript",
                                    "svg" => "image/svg+xml",
                                    _ => "application/octet-stream",
                                };
                                let header = format!(
                                    "HTTP/1.1 200 OK\r\nContent-Type: {}\r\nContent-Length: {}\r\n\r\n",
                                    content_type,
                                    bytes.len()
                                );
                                let _ = stream.write_all(header.as_bytes());
                                let _ = stream.write_all(&bytes);
                                let _ = stream.flush();
                            }
                            Err(_) => {
                                let response =
                                    "HTTP/1.1 404 NOT FOUND\r\n\r\n404 - Page not found.";
                                let _ = stream.write_all(response.as_bytes());
                                let _ = stream.flush();
                            }
                        }
                    }
                }
            }
            Err(e) => {
                // Log the error; avoid panicking on transient failures
                OutputRenderer::new(OutputFormat::Text, LogLevel::Warn)
                    .log_warn(&format!("Connection error: {}", e));
            }
        }
    }

    // Cleanup entry in stop registry when server exits
    let _ = STOP_FLAGS.lock().map(|mut m| m.remove(&port));
}

fn parse_path_and_query(path_full: &str) -> (String, HashMap<String, String>) {
    let mut out = HashMap::new();
    if let Some((p, q)) = path_full.split_once('?') {
        for part in q.split('&') {
            if part.is_empty() {
                continue;
            }
            let (k, v) = part.split_once('=').unwrap_or((part, ""));
            out.insert(url_decode(k), url_decode(v));
        }
        (p.to_string(), out)
    } else {
        (path_full.to_string(), out)
    }
}

fn url_decode(s: &str) -> String {
    // Minimal percent-decoder for application/x-www-form-urlencoded-like strings
    let mut out = String::with_capacity(s.len());
    let bytes = s.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'+' => {
                out.push(' ');
                i += 1;
            }
            b'%' if i + 2 < bytes.len() => {
                let hi = bytes[i + 1];
                let lo = bytes[i + 2];
                let hex = |c: u8| -> Option<u8> {
                    match c {
                        b'0'..=b'9' => Some(c - b'0'),
                        b'a'..=b'f' => Some(10 + c - b'a'),
                        b'A'..=b'F' => Some(10 + c - b'A'),
                        _ => None,
                    }
                };
                if let (Some(h), Some(l)) = (hex(hi), hex(lo)) {
                    out.push((h << 4 | l) as char);
                    i += 3;
                } else {
                    out.push('%');
                    i += 1;
                }
            }
            c => {
                out.push(c as char);
                i += 1;
            }
        }
    }
    out
}

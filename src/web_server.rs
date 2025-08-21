use crate::api_server::{self, HttpRequest};
use crate::output::{LogLevel, OutputFormat, OutputRenderer};
use include_dir::{Dir, include_dir};
use notify::{Config as NotifyConfig, EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use std::collections::{HashMap, HashSet};
use std::fs;
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::Path;
use std::sync::{LazyLock, Mutex};

static STATIC_FILES: Dir<'_> = include_dir!("$CARGO_MANIFEST_DIR/target/web");
static STOP_FLAGS: LazyLock<Mutex<HashMap<u16, bool>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

pub fn serve_with_host(api_server: &api_server::ApiServer, host: &str, port: u16) {
    let addr = format!("{}:{}", host, port);
    let listener = match TcpListener::bind(&addr) {
        Ok(l) => l,
        Err(e) => {
            OutputRenderer::new(OutputFormat::Text, LogLevel::Error)
                .log_error(&format!("Failed to bind to {}: {}", addr, e));
            return;
        }
    };

    // Register this server instance in a global stop registry (used only by tests)
    if let Ok(mut map) = STOP_FLAGS.lock() {
        map.insert(port, false);
    }

    // Best-effort: start a filesystem watcher thread to emit SSE events on changes under .tasks
    start_tasks_watcher();

    for stream in listener.incoming() {
        // Check for test-initiated shutdown before handling the next connection
        if STOP_FLAGS
            .lock()
            .ok()
            .and_then(|m| m.get(&port).cloned())
            .unwrap_or(false)
        {
            break;
        }

        match stream {
            Ok(mut stream) => {
                // Read a single HTTP request (headers + body)
                let (method, path, query, headers, body) = {
                    let mut head_buf: Vec<u8> = Vec::new();
                    let mut buf = [0u8; 1024];
                    loop {
                        let n: usize = stream.read(&mut buf).unwrap_or_default();
                        if n == 0 {
                            break;
                        }
                        head_buf.extend_from_slice(&buf[..n]);
                        if head_buf.windows(4).any(|w| w == b"\r\n\r\n") {
                            break;
                        }
                        if head_buf.len() > 32 * 1024 {
                            break;
                        }
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
                    let parts: Vec<&str> = request_line.split(' ').collect();
                    let method = parts.first().cloned().unwrap_or("GET").to_string();
                    let path_full = parts.get(1).cloned().unwrap_or("/");
                    let (path, query) = parse_path_and_query(path_full);
                    let mut headers = HashMap::new();
                    for line in request_head.lines().skip(1) {
                        if line.trim().is_empty() {
                            break;
                        }
                        if let Some((k, v)) = line.split_once(":") {
                            headers.insert(k.trim().to_string(), v.trim().to_string());
                        }
                    }
                    let mut body: Vec<u8> = body_bytes.to_vec();
                    if let Some(cl) = headers
                        .get("Content-Length")
                        .and_then(|v| v.parse::<usize>().ok())
                    {
                        while body.len() < cl {
                            let n: usize = stream.read(&mut buf).unwrap_or_default();
                            if n == 0 {
                                break;
                            }
                            body.extend_from_slice(&buf[..n]);
                        }
                        body.truncate(cl);
                    }
                    (method, path, query, headers, body)
                };

                // SSE endpoints
                if path == "/api/events" || path == "/api/tasks/stream" {
                    handle_sse_connection(stream, &query);
                    continue;
                } else if path == "/__test/stop" || path == "/shutdown" {
                    if let Ok(mut map) = STOP_FLAGS.lock() {
                        map.insert(port, true);
                    }
                    let response = "HTTP/1.1 200 OK\r\nContent-Type: text/plain\r\nContent-Length: 8\r\n\r\nstopping";
                    let _ = stream.write_all(response.as_bytes());
                    let _ = stream.flush();
                    continue;
                } else if path.starts_with("/api") {
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
                    if method.eq_ignore_ascii_case("OPTIONS") {
                        let preflight = "HTTP/1.1 204 No Content\r\nAccess-Control-Allow-Origin: *\r\nAccess-Control-Allow-Methods: GET,POST,OPTIONS\r\nAccess-Control-Allow-Headers: Content-Type\r\nContent-Length: 0\r\n\r\n";
                        let _ = stream.write_all(preflight.as_bytes());
                        let _ = stream.flush();
                        continue;
                    }

                    let req = HttpRequest {
                        method,
                        path: path.clone(),
                        query,
                        headers,
                        body,
                    };
                    let mut resp = api_server.handle_request(&req);
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
                    let request_path = if path == "/" { "/index.html" } else { &path };
                    let rel_path = request_path.trim_start_matches('/');
                    if !try_serve_static(rel_path, &mut stream) {
                        let response = "HTTP/1.1 404 NOT FOUND\r\n\r\n404 - Page not found.";
                        let _ = stream.write_all(response.as_bytes());
                        let _ = stream.flush();
                    }
                }
            }
            Err(e) => {
                OutputRenderer::new(OutputFormat::Text, LogLevel::Warn)
                    .log_warn(&format!("Connection error: {}", e));
            }
        }
    }

    let _ = STOP_FLAGS.lock().map(|mut m| m.remove(&port));
}

pub fn serve(api_server: &api_server::ApiServer, port: u16) {
    serve_with_host(api_server, "127.0.0.1", port)
}

fn try_serve_static(rel_path: &str, stream: &mut TcpStream) -> bool {
    // Try embedded first
    if let Some(file) = STATIC_FILES.get_file(rel_path) {
        let data = file.contents();
        let content_type = content_type_for(rel_path);
        let header = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: {}\r\nContent-Length: {}\r\n\r\n",
            content_type,
            data.len()
        );
        let _ = stream.write_all(header.as_bytes());
        let _ = stream.write_all(data);
        let _ = stream.flush();
        return true;
    }

    // Fallback to filesystem (useful during development)
    let fs_path = Path::new("target/web").join(rel_path);
    match fs::read(&fs_path) {
        Ok(bytes) => {
            let content_type = content_type_for(rel_path);
            let header = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: {}\r\nContent-Length: {}\r\n\r\n",
                content_type,
                bytes.len()
            );
            let _ = stream.write_all(header.as_bytes());
            let _ = stream.write_all(&bytes);
            let _ = stream.flush();
            true
        }
        Err(_) => false,
    }
}

fn content_type_for(path: &str) -> &'static str {
    match Path::new(path)
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
    {
        "html" => "text/html",
        "jpg" | "jpeg" => "image/jpeg",
        "png" => "image/png",
        "css" => "text/css",
        "js" => "application/javascript",
        "svg" => "image/svg+xml",
        _ => "application/octet-stream",
    }
}

fn handle_sse_connection(mut stream: TcpStream, query: &HashMap<String, String>) {
    use std::time::{Duration, Instant};

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
    let fast = std::env::var("LOTAR_TEST_FAST_IO").ok().as_deref() == Some("1");
    let headers = "HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nCache-Control: no-cache\r\nConnection: keep-alive\r\nAccess-Control-Allow-Origin: *\r\n\r\n";

    // Send headers and initial retry hint
    let mut initial = String::with_capacity(headers.len() + 14);
    initial.push_str(headers);
    initial.push_str("retry: 1000\n\n");
    let _ = stream.write_all(initial.as_bytes());
    let _ = stream.flush();

    // Optional readiness event for tests
    let allow_ready = std::env::var("LOTAR_SSE_READY").ok().as_deref() == Some("1");
    if allow_ready
        && query
            .get("ready")
            .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
            .unwrap_or(false)
    {
        let _ = stream.write_all(b"event: ready\ndata: {}\n\n");
        let _ = stream.flush();

        // If a project filter is active and client is interested in project_changed,
        // emit a synthetic snapshot event to avoid races with watcher startup.
        if let Some(proj_name) = project_filter.as_ref() {
            let wants_project_changed = match &kinds_filter {
                None => true,
                Some(kinds) => kinds
                    .iter()
                    .any(|k| k.eq_ignore_ascii_case("project_changed")),
            };
            if wants_project_changed {
                let cwd_ok = std::env::current_dir().ok();
                if let Some(cwd) = cwd_ok {
                    let proj_dir = cwd.join(".tasks").join(proj_name);
                    if proj_dir.exists() {
                        // Emit via bus (picked up by forwarder) and also write one immediate event inline
                        crate::api_events::emit(crate::api_events::ApiEvent {
                            kind: "project_changed".to_string(),
                            data: serde_json::json!({ "name": proj_name }),
                        });
                        let inline = format!(
                            "event: project_changed\ndata: {{\"name\":\"{}\"}}\n\n",
                            proj_name
                        );
                        let _ = stream.write_all(inline.as_bytes());
                        let _ = stream.flush();
                    }
                }
            }
        }
    }

    // Spawn a thread to forward events
    let Ok(mut stream_clone) = stream.try_clone() else {
        return;
    };
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
            let timeout = match deadline {
                Some(d) => {
                    let now = Instant::now();
                    if d <= now {
                        Duration::from_millis(0)
                    } else {
                        d - now
                    }
                }
                None => heartbeat_every,
            };

            match rx.recv_timeout(timeout) {
                Ok(evt) => {
                    if let Some(ref kinds) = kinds_filter {
                        if !kinds.iter().any(|k| k.eq_ignore_ascii_case(&evt.kind)) {
                            continue;
                        }
                    }
                    if let Some(ref pf) = project_filter {
                        let matches_project = match evt.kind.as_str() {
                            // Filesystem watcher emits { name: <PROJECT> }
                            "project_changed" => evt
                                .data
                                .get("name")
                                .and_then(|v| v.as_str())
                                .map(|name| name == pf)
                                .unwrap_or(false),
                            // Task events include { id: "PREFIX-N" }
                            _ => evt
                                .data
                                .get("id")
                                .and_then(|v| v.as_str())
                                .map(|id_str| id_str.split('-').next().unwrap_or("") == pf)
                                .unwrap_or(false),
                        };
                        if !matches_project {
                            continue;
                        }
                    }
                    buffer.push(evt);
                    let now = Instant::now();
                    deadline = Some(now + debounce);
                }
                Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {
                    if !buffer.is_empty() {
                        for evt in buffer.drain(..) {
                            let line = format!(
                                "event: {}\ndata: {}\n\n",
                                evt.kind,
                                serde_json::to_string(&evt.data).unwrap_or("null".to_string())
                            );
                            if stream_clone.write_all(line.as_bytes()).is_err() {
                                return;
                            }
                        }
                        let _ = stream_clone.flush();
                        deadline = None;
                    } else {
                        if stream_clone.write_all(b":heartbeat\n\n").is_err() {
                            return;
                        }
                        let _ = stream_clone.flush();
                    }
                }
                Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => return,
            }
        }
    });
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

// Lightweight watcher that monitors the nearest .tasks directory under CWD and emits project_changed
fn start_tasks_watcher() {
    // Don't crash server if watcher setup fails
    let cwd = match std::env::current_dir() {
        Ok(c) => c,
        Err(_) => return,
    };
    let tasks_dir = cwd.join(".tasks");
    if !tasks_dir.exists() {
        return;
    }
    // Spawn a detached thread that owns the watcher
    std::thread::spawn(move || {
        // Channel for notify events
        let (tx, rx) = std::sync::mpsc::channel();
        let mut watcher = match RecommendedWatcher::new(tx, NotifyConfig::default()) {
            Ok(w) => w,
            Err(_) => return,
        };
        if watcher.watch(&tasks_dir, RecursiveMode::Recursive).is_err() {
            return;
        }
        // Simple loop: for any modify/create/remove, emit project_changed events.
        while let Ok(res) = rx.recv() {
            let Ok(event) = res else {
                continue;
            };
            match event.kind {
                EventKind::Create(_) | EventKind::Modify(_) | EventKind::Remove(_) => {
                    // Derive project(s) from paths by finding the component under ".tasks"
                    if let Some(paths) = (!event.paths.is_empty()).then_some(event.paths) {
                        let mut emitted: HashSet<String> = HashSet::new();
                        for p in paths {
                            // Walk ancestors to locate the ".tasks" directory and take the next component as project
                            let mut proj: Option<String> = None;
                            for anc in p.ancestors() {
                                if let Some(name) = anc.file_name().and_then(|s| s.to_str()) {
                                    if name == ".tasks" {
                                        // The path immediately under .tasks is the project directory
                                        if let Some(project) = p
                                            .strip_prefix(anc)
                                            .ok()
                                            .and_then(|rest| rest.components().next())
                                            .and_then(|c| match c {
                                                std::path::Component::Normal(os) => os.to_str(),
                                                _ => None,
                                            })
                                        {
                                            proj = Some(project.to_string());
                                        }
                                        break;
                                    }
                                }
                            }
                            if let Some(project) = proj {
                                if emitted.insert(project.clone()) {
                                    crate::api_events::emit(crate::api_events::ApiEvent {
                                        kind: "project_changed".to_string(),
                                        data: serde_json::json!({ "name": project }),
                                    });
                                }
                            }
                        }
                    }
                }
                _ => {}
            }
        }
    });
}

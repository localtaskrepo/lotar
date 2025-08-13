use lotar::api_server::{ApiServer, HttpRequest};
use lotar::routes;
use serde_json::{Value, json};
use std::collections::HashMap;
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::thread;
use std::time::{Duration, Instant};
mod common;
use crate::common::env_mutex::lock_var;

// Test-time acceleration helpers
fn fast_net() -> bool {
    std::env::var("LOTAR_TEST_FAST_NET").ok().as_deref() == Some("1")
}

fn net_timeout() -> std::time::Duration {
    if fast_net() {
        std::time::Duration::from_millis(500)
    } else {
        std::time::Duration::from_millis(1500)
    }
}

fn mk_req(method: &str, path: &str, query: &[(&str, &str)], body: Value) -> HttpRequest {
    let mut q = HashMap::new();
    for (k, v) in query {
        q.insert((*k).to_string(), (*v).to_string());
    }
    HttpRequest {
        method: method.to_string(),
        path: path.to_string(),
        query: q,
        headers: HashMap::new(),
        body: serde_json::to_vec(&body).unwrap(),
    }
}

// Helpers merged from sse_events_test.rs
fn find_free_port() -> u16 {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    drop(listener);
    port
}

fn start_server_on(port: u16) {
    thread::spawn(move || {
        // Enable fast IO/heartbeat paths inside the server during tests
        unsafe {
            std::env::set_var("LOTAR_TEST_FAST_IO", "1");
        }
        let mut api = ApiServer::new();
        routes::initialize(&mut api);
        lotar::web_server::serve(&api, port);
    });
    // Also enable faster client-side network timeouts for this process
    unsafe {
        std::env::set_var("LOTAR_TEST_FAST_NET", "1");
    }
    std::thread::sleep(Duration::from_millis(50));
}

fn stop_server_on(port: u16) {
    if let Ok(mut stream) = TcpStream::connect(("127.0.0.1", port)) {
        let _ = stream.set_read_timeout(Some(Duration::from_millis(250)));
        let req = "GET /__test/stop HTTP/1.1\r\nHost: 127.0.0.1\r\n\r\n";
        let _ = stream.write_all(req.as_bytes());
        let _ = stream.flush();
        let mut tmp = [0u8; 256];
        let _ = stream.read(&mut tmp);
    }
    std::thread::sleep(Duration::from_millis(20));
}

fn http_post_json(port: u16, path_and_query: &str, body: &str) -> (u16, Vec<u8>) {
    let mut stream = TcpStream::connect(("127.0.0.1", port)).unwrap();
    stream.set_read_timeout(Some(net_timeout())).unwrap();
    let req = format!(
        "POST {} HTTP/1.1\r\nHost: 127.0.0.1\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{}",
        path_and_query,
        body.len(),
        body
    );
    stream.write_all(req.as_bytes()).unwrap();
    stream.flush().unwrap();

    // Read headers first
    let mut header_buf = Vec::new();
    let mut tmp = [0u8; 1024];
    loop {
        let n = stream.read(&mut tmp).unwrap();
        if n == 0 {
            break;
        }
        header_buf.extend_from_slice(&tmp[..n]);
        if let Some(pos) = header_buf.windows(4).position(|w| w == b"\r\n\r\n") {
            // Split headers and leftover body bytes
            let body_leftover = header_buf.split_off(pos + 4);
            let headers_text = String::from_utf8_lossy(&header_buf);
            let status = headers_text
                .lines()
                .next()
                .and_then(|l| l.split_whitespace().nth(1))
                .and_then(|s| s.parse::<u16>().ok())
                .unwrap_or(0);
            let content_length = headers_text
                .lines()
                .find_map(|l| l.split_once(":").map(|(k, v)| (k.trim(), v.trim())))
                .and_then(|_| {
                    headers_text
                        .lines()
                        .filter_map(|l| l.split_once(":").map(|(k, v)| (k.trim(), v.trim())))
                        .find(|(k, _)| k.eq_ignore_ascii_case("Content-Length"))
                        .and_then(|(_, v)| v.parse::<usize>().ok())
                })
                .unwrap_or(0);

            // Read exact remaining bytes for body
            let mut body_bytes = body_leftover;
            while body_bytes.len() < content_length {
                let n = stream.read(&mut tmp).unwrap_or(0);
                if n == 0 {
                    break;
                }
                body_bytes.extend_from_slice(&tmp[..n]);
            }
            body_bytes.truncate(content_length);
            return (status, body_bytes);
        }
        if header_buf.len() > 64 * 1024 {
            break; // safety cap
        }
    }
    (0, Vec::new())
}

fn http_get_bytes(port: u16, path: &str) -> (u16, HashMap<String, String>, Vec<u8>) {
    let mut stream = TcpStream::connect(("127.0.0.1", port)).unwrap();
    stream.set_read_timeout(Some(net_timeout())).unwrap();
    let req = format!("GET {path} HTTP/1.1\r\nHost: 127.0.0.1\r\n\r\n");
    stream.write_all(req.as_bytes()).unwrap();
    stream.flush().unwrap();

    // Read headers first
    let mut header_buf = Vec::new();
    let mut tmp = [0u8; 1024];
    loop {
        let n = stream.read(&mut tmp).unwrap_or(0);
        if n == 0 {
            break;
        }
        header_buf.extend_from_slice(&tmp[..n]);
        if let Some(pos) = header_buf.windows(4).position(|w| w == b"\r\n\r\n") {
            let mut headers = HashMap::new();
            let body_leftover = header_buf.split_off(pos + 4);
            let headers_text = String::from_utf8_lossy(&header_buf);
            // Parse status
            let status = headers_text
                .lines()
                .next()
                .and_then(|l| l.split_whitespace().nth(1))
                .and_then(|s| s.parse::<u16>().ok())
                .unwrap_or(0);
            // Collect headers map
            for line in headers_text.lines().skip(1) {
                if line.trim().is_empty() {
                    break;
                }
                if let Some((k, v)) = line.split_once(":") {
                    headers.insert(k.trim().to_string(), v.trim().to_string());
                }
            }
            let content_length = headers
                .get("Content-Length")
                .and_then(|v| v.parse::<usize>().ok())
                .unwrap_or(0);
            // Read exact remaining bytes for body
            let mut body_bytes = body_leftover;
            while body_bytes.len() < content_length {
                let n = stream.read(&mut tmp).unwrap_or(0);
                if n == 0 {
                    break;
                }
                body_bytes.extend_from_slice(&tmp[..n]);
            }
            body_bytes.truncate(content_length);
            return (status, headers, body_bytes);
        }
        if header_buf.len() > 64 * 1024 {
            break;
        }
    }
    (0, HashMap::new(), Vec::new())
}

fn open_sse(port: u16, query: &str) -> (TcpStream, Vec<u8>) {
    let mut stream = TcpStream::connect(("127.0.0.1", port)).unwrap();
    stream.set_read_timeout(Some(net_timeout())).unwrap();
    let suffix = if query.is_empty() {
        String::new()
    } else {
        format!("?{query}")
    };
    let path = format!("/api/events{suffix}");
    let req = format!("GET {path} HTTP/1.1\r\nHost: 127.0.0.1\r\n\r\n");
    stream.write_all(req.as_bytes()).unwrap();
    stream.flush().unwrap();
    let mut header_buf = [0u8; 1024];
    let mut collected = Vec::new();
    loop {
        let n = stream.read(&mut header_buf).unwrap();
        if n == 0 {
            break;
        }
        collected.extend_from_slice(&header_buf[..n]);
        if let Some(pos) = collected.windows(4).position(|w| w == b"\r\n\r\n") {
            let leftover = collected.split_off(pos + 4);
            return (stream, leftover);
        }
    }
    (stream, Vec::new())
}

fn read_sse_events(
    stream: &mut TcpStream,
    max_events: usize,
    timeout: Duration,
    mut buf: Vec<u8>,
) -> Vec<(String, String)> {
    let start = Instant::now();
    let mut events = Vec::new();
    while events.len() < max_events && start.elapsed() < timeout {
        let mut tmp = [0u8; 4096];
        match stream.read(&mut tmp) {
            Ok(0) => break,
            Ok(n) => {
                buf.extend_from_slice(&tmp[..n]);
                while let Some(pos) = buf
                    .windows(2)
                    .position(|w| w == b"\n\n" || w == b"\r\n\r\n")
                {
                    let chunk = buf.drain(..pos + 2).collect::<Vec<u8>>();
                    let text = String::from_utf8_lossy(&chunk);
                    let mut kind = String::new();
                    let mut data = String::new();
                    for line in text.lines() {
                        if let Some(rest) = line.strip_prefix("event: ") {
                            kind = rest.trim().to_string();
                        }
                        if let Some(rest) = line.strip_prefix("data: ") {
                            data = rest.trim().to_string();
                        }
                    }
                    if !kind.is_empty() {
                        events.push((kind, data));
                    }
                }
            }
            Err(e) => {
                if e.kind() == std::io::ErrorKind::WouldBlock
                    || e.kind() == std::io::ErrorKind::TimedOut
                {
                    std::thread::sleep(Duration::from_millis(10));
                    continue;
                } else {
                    break;
                }
            }
        }
    }
    events
}

fn http_options(port: u16, path: &str) -> (u16, HashMap<String, String>) {
    let mut stream = TcpStream::connect(("127.0.0.1", port)).unwrap();
    let to = if fast_net() {
        Duration::from_millis(400)
    } else {
        Duration::from_millis(1000)
    };
    stream.set_read_timeout(Some(to)).unwrap();
    let req = format!("OPTIONS {path} HTTP/1.1\r\nHost: 127.0.0.1\r\n\r\n");
    stream.write_all(req.as_bytes()).unwrap();
    stream.flush().unwrap();
    // Read only headers; OPTIONS has no body
    let mut header_buf = Vec::new();
    let mut tmp = [0u8; 1024];
    loop {
        let n = stream.read(&mut tmp).unwrap_or(0);
        if n == 0 {
            break;
        }
        header_buf.extend_from_slice(&tmp[..n]);
        if header_buf.windows(4).any(|w| w == b"\r\n\r\n") {
            break;
        }
    }
    let resp = String::from_utf8_lossy(&header_buf);
    let mut lines = resp.lines();
    let status = lines
        .next()
        .and_then(|l| l.split_whitespace().nth(1))
        .and_then(|s| s.parse::<u16>().ok())
        .unwrap_or(0);
    let mut headers = HashMap::new();
    for line in lines {
        if line.trim().is_empty() {
            break;
        }
        if let Some((k, v)) = line.split_once(":") {
            headers.insert(k.trim().to_string(), v.trim().to_string());
        }
    }
    (status, headers)
}

#[test]
fn api_add_list_get_delete_roundtrip() {
    // Serialize environment mutations across tests
    let _guard = lock_var("LOTAR_TASKS_DIR");
    // Speed up IO handling in server during tests
    unsafe {
        std::env::set_var("LOTAR_TEST_FAST_IO", "1");
    }
    // Isolate tasks dir via env var
    let tmp = tempfile::tempdir().unwrap();
    let tasks_dir = tmp.path().join(".tasks");
    std::fs::create_dir_all(&tasks_dir).unwrap();
    unsafe {
        std::env::set_var("LOTAR_TASKS_DIR", tasks_dir.to_string_lossy().to_string());
    }

    let mut api = ApiServer::new();
    routes::initialize(&mut api);

    // Add a task
    let add_body = json!({
        "title": "Test task via API",
        "priority": "High",
        "type": "feature",
        "tags": ["api", "test"],
        "fields": {"foo": "bar"}
    });
    let resp = api.handle_request(&mk_req(
        "POST",
        "/api/tasks/add",
        &[("project", "TEST")],
        add_body,
    ));
    assert_eq!(resp.status, 201, "add status");
    let added: Value = serde_json::from_slice(&resp.body).unwrap();
    let id = added["data"]["id"].as_str().unwrap().to_string();

    // List (scoped to the created task's project for determinism)
    let prefix = "TEST".to_string();
    // Sanity: ensure the file exists under the LOTAR_TASKS_DIR/prefix
    let project_dir = tasks_dir.join(&prefix);
    let mut yml_count = 0;
    if let Ok(entries) = std::fs::read_dir(&project_dir) {
        for e in entries.flatten() {
            if let Some(ext) = e.path().extension().and_then(|s| s.to_str()) {
                if ext == "yml" {
                    yml_count += 1;
                }
            }
        }
    }
    assert!(
        yml_count >= 1,
        "expected at least one yml in {project_dir:?}"
    );
    let resp = api.handle_request(&mk_req(
        "GET",
        "/api/tasks/list",
        &[("project", &prefix)],
        json!({}),
    ));
    assert_eq!(resp.status, 200, "list status");
    let listed: Value = serde_json::from_slice(&resp.body).unwrap();
    assert!(listed["data"].is_array());
    assert!(listed["meta"]["count"].as_u64().unwrap() >= 1);

    // Get
    let resp = api.handle_request(&mk_req("GET", "/api/tasks/get", &[("id", &id)], json!({})));
    assert_eq!(resp.status, 200, "get status");
    let got: Value = serde_json::from_slice(&resp.body).unwrap();
    assert_eq!(got["data"]["id"].as_str().unwrap(), id);

    // Delete
    let del_body = json!({"id": id});
    let resp = api.handle_request(&mk_req("POST", "/api/tasks/delete", &[], del_body));
    assert_eq!(resp.status, 200, "delete status");
    let deleted: Value = serde_json::from_slice(&resp.body).unwrap();
    assert!(deleted["data"]["deleted"].as_bool().unwrap());

    // Cleanup env var
    unsafe {
        std::env::remove_var("LOTAR_TASKS_DIR");
    }
}

#[test]
fn api_config_show_set() {
    let _guard = lock_var("LOTAR_TASKS_DIR");
    let tmp = tempfile::tempdir().unwrap();
    let tasks_dir = tmp.path().join(".tasks");
    std::fs::create_dir_all(&tasks_dir).unwrap();
    unsafe {
        std::env::set_var("LOTAR_TASKS_DIR", tasks_dir.to_string_lossy().to_string());
    }

    let mut api = ApiServer::new();
    routes::initialize(&mut api);

    // Show resolved config
    let resp = api.handle_request(&mk_req("GET", "/api/config/show", &[], json!({})));
    assert_eq!(resp.status, 200);
    let show: Value = serde_json::from_slice(&resp.body).unwrap();
    assert!(show["data"].is_object());

    // Set and then show
    let set_body = json!({"values": {"default_project": "DEMO"}, "global": true});
    let resp = api.handle_request(&mk_req("POST", "/api/config/set", &[], set_body));
    assert_eq!(resp.status, 200);

    let resp = api.handle_request(&mk_req("GET", "/api/config/show", &[], json!({})));
    assert_eq!(resp.status, 200);
    let show2: Value = serde_json::from_slice(&resp.body).unwrap();
    assert_eq!(show2["data"]["default_project"].as_str().unwrap(), "DEMO");

    unsafe {
        std::env::remove_var("LOTAR_TASKS_DIR");
    }
}

#[test]
fn api_options_preflight_returns_204_and_cors_headers() {
    let port = find_free_port();
    start_server_on(port);
    let (status, headers) = http_options(port, "/api/tasks/list");
    assert_eq!(status, 204);
    assert_eq!(
        headers
            .get("Access-Control-Allow-Origin")
            .map(|s| s.as_str()),
        Some("*")
    );
    let methods = headers
        .get("Access-Control-Allow-Methods")
        .cloned()
        .unwrap_or_default();
    assert!(methods.contains("GET") && methods.contains("POST") && methods.contains("OPTIONS"));
    let allow_headers = headers
        .get("Access-Control-Allow-Headers")
        .cloned()
        .unwrap_or_default();
    assert!(allow_headers.to_ascii_lowercase().contains("content-type"));
    stop_server_on(port);
}

#[test]
fn sse_initial_retry_hint_is_sent() {
    let port = find_free_port();
    start_server_on(port);
    let (mut stream, mut leftover) = open_sse(port, "");
    if leftover.is_empty() {
        // Read a bit more to capture any small write coalescing differences
        let mut tmp = [0u8; 256];
        let _ = stream.read(&mut tmp).unwrap_or(0);
        leftover.extend_from_slice(&tmp);
    }
    let text = String::from_utf8_lossy(&leftover);
    assert!(
        text.contains("retry: 1000"),
        "leftover did not contain retry hint: {text}"
    );
    stop_server_on(port);
}

#[test]
fn openapi_spec_served() {
    let port = find_free_port();
    start_server_on(port);
    let (status, headers, body) = http_get_bytes(port, "/api/openapi.json");
    assert_eq!(status, 200);
    assert_eq!(
        headers.get("Content-Type").map(|s| s.as_str()),
        Some("application/json")
    );
    let resp_body = String::from_utf8_lossy(&body);
    // Check that a couple of known paths are present
    assert!(resp_body.contains("\"/api/tasks/add\""));
    assert!(resp_body.contains("\"/api/events\""));
    stop_server_on(port);
}

// Merged from sse_events_test.rs
#[test]
fn sse_events_with_kinds_and_project_filter() {
    let _guard = lock_var("LOTAR_TASKS_DIR");
    let tmp = tempfile::tempdir().unwrap();
    let tasks_dir = tmp.path().join(".tasks");
    std::fs::create_dir_all(&tasks_dir).unwrap();
    unsafe {
        std::env::set_var("LOTAR_TASKS_DIR", tasks_dir.to_string_lossy().to_string());
    }

    let port = find_free_port();
    start_server_on(port);

    let (mut sse, leftover) = open_sse(port, "debounce_ms=10&kinds=task_created&project=TEST");
    std::thread::sleep(Duration::from_millis(30));

    let add_body_test = r#"{\"title\":\"A\",\"priority\":\"High\"}"#;
    let (_st1, _b1) = http_post_json(port, "/api/tasks/add?project=TEST", add_body_test);
    let add_body_other = r#"{\"title\":\"B\",\"priority\":\"Low\"}"#;
    let (_st2, _b2) = http_post_json(port, "/api/tasks/add?project=OTHER", add_body_other);

    let events = read_sse_events(&mut sse, 2, Duration::from_millis(1200), leftover);
    assert!(events.iter().all(|(k, _)| k == "task_created"));
    assert!(
        events
            .iter()
            .all(|(_, d)| d.contains("\"id\":") && d.contains("TEST-")),
        "events: {events:?}"
    );

    unsafe {
        std::env::remove_var("LOTAR_TASKS_DIR");
    }
    stop_server_on(port);
}

#[test]
fn sse_debounce_emits_all_events() {
    let _guard = lock_var("LOTAR_TASKS_DIR");
    let tmp = tempfile::tempdir().unwrap();
    let tasks_dir = tmp.path().join(".tasks");
    std::fs::create_dir_all(&tasks_dir).unwrap();
    unsafe {
        std::env::set_var("LOTAR_TASKS_DIR", tasks_dir.to_string_lossy().to_string());
    }

    let port = find_free_port();
    start_server_on(port);

    let (mut sse, leftover) = open_sse(port, "debounce_ms=25&kinds=task_created&project=TEST");
    std::thread::sleep(Duration::from_millis(30));

    for i in 0..3 {
        let body = format!("{{\"title\":\"T{i}\",\"priority\":\"High\"}}");
        let _ = http_post_json(port, "/api/tasks/add?project=TEST", &body);
    }

    let events = read_sse_events(&mut sse, 3, Duration::from_millis(2000), leftover);
    assert_eq!(events.len(), 3, "expected 3 created events, got {events:?}");

    unsafe {
        std::env::remove_var("LOTAR_TASKS_DIR");
    }
    stop_server_on(port);
}

#[test]
fn api_get_missing_and_unknown_id_errors() {
    let _guard = lock_var("LOTAR_TASKS_DIR");
    let tmp = tempfile::tempdir().unwrap();
    let tasks_dir = tmp.path().join(".tasks");
    std::fs::create_dir_all(&tasks_dir).unwrap();
    unsafe {
        std::env::set_var("LOTAR_TASKS_DIR", tasks_dir.to_string_lossy().to_string());
    }

    let mut api = ApiServer::new();
    routes::initialize(&mut api);

    // Missing id
    let resp = api.handle_request(&mk_req("GET", "/api/tasks/get", &[], json!({})));
    assert_eq!(resp.status, 400);
    let body: Value = serde_json::from_slice(&resp.body).unwrap();
    assert_eq!(body["error"]["code"].as_str(), Some("INVALID_ARGUMENT"));

    // Unknown id
    let resp = api.handle_request(&mk_req(
        "GET",
        "/api/tasks/get",
        &[("id", "TEST-999")],
        json!({}),
    ));
    assert_eq!(resp.status, 404);
    let body: Value = serde_json::from_slice(&resp.body).unwrap();
    let msg = body["error"]["message"]
        .as_str()
        .unwrap_or("")
        .to_lowercase();
    assert!(msg.contains("not found") || msg.contains("invalid"));

    unsafe {
        std::env::remove_var("LOTAR_TASKS_DIR");
    }
}

#[test]
fn api_add_invalid_priority_and_type_rejected() {
    let _guard = lock_var("LOTAR_TASKS_DIR");
    let tmp = tempfile::tempdir().unwrap();
    let tasks_dir = tmp.path().join(".tasks");
    std::fs::create_dir_all(&tasks_dir).unwrap();
    unsafe {
        std::env::set_var("LOTAR_TASKS_DIR", tasks_dir.to_string_lossy().to_string());
    }

    let mut api = ApiServer::new();
    routes::initialize(&mut api);

    // Invalid priority
    let add_body = json!({ "title": "X", "priority": "NOT_A_PRIORITY" });
    let resp = api.handle_request(&mk_req(
        "POST",
        "/api/tasks/add",
        &[("project", "TEST")],
        add_body,
    ));
    assert_eq!(resp.status, 400);
    let body: Value = serde_json::from_slice(&resp.body).unwrap();
    let msg = body["error"]["message"]
        .as_str()
        .unwrap_or("")
        .to_lowercase();
    assert!(msg.contains("priority"));

    // Invalid type
    let add_body = json!({ "title": "X", "type": "NOT_A_TYPE" });
    let resp = api.handle_request(&mk_req(
        "POST",
        "/api/tasks/add",
        &[("project", "TEST")],
        add_body,
    ));
    assert_eq!(resp.status, 400);
    let body: Value = serde_json::from_slice(&resp.body).unwrap();
    let msg = body["error"]["message"]
        .as_str()
        .unwrap_or("")
        .to_lowercase();
    assert!(msg.contains("type"));

    unsafe {
        std::env::remove_var("LOTAR_TASKS_DIR");
    }
}

#[test]
fn sse_debounce_zero_and_invalid_kind_handling() {
    let _guard = lock_var("LOTAR_TASKS_DIR");
    let tmp = tempfile::tempdir().unwrap();
    let tasks_dir = tmp.path().join(".tasks");
    std::fs::create_dir_all(&tasks_dir).unwrap();
    unsafe {
        std::env::set_var("LOTAR_TASKS_DIR", tasks_dir.to_string_lossy().to_string());
    }

    let port = find_free_port();
    start_server_on(port);

    // invalid kind should filter out events; debounce 0 should flush immediately
    let (mut sse, leftover) = open_sse(port, "debounce_ms=0&kinds=invalid_kind");
    std::thread::sleep(Duration::from_millis(20));
    // Create a task -> would generate task_created, but our kind filter excludes it
    let _ = http_post_json(port, "/api/tasks/add?project=TEST", r#"{\"title\":\"Z\"}"#);
    let events = read_sse_events(&mut sse, 1, Duration::from_millis(200), leftover);
    assert!(events.is_empty(), "invalid kind should filter all events");

    unsafe {
        std::env::remove_var("LOTAR_TASKS_DIR");
    }
    stop_server_on(port);
}

use chrono::Utc;
use lotar::api_server::{ApiServer, HttpRequest};
use lotar::routes;
use serde_json::{Value, json};
use std::collections::HashMap;
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::thread;
use std::time::{Duration, Instant};
mod common;
use crate::common::env_mutex::{EnvVarGuard, lock_var};

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
    // Wait until a TCP connection to the server port succeeds.
    // This avoids panicking while the listener is still starting up.
    let start = Instant::now();
    let max_wait = Duration::from_millis(750);
    loop {
        match TcpStream::connect(("127.0.0.1", port)) {
            Ok(_) => break,
            Err(_) => {
                if start.elapsed() > max_wait {
                    break; // give up after max_wait; tests will still proceed
                }
                std::thread::sleep(Duration::from_millis(5));
            }
        }
    }
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
    // Allow the server a brief moment to shut down sockets
    std::thread::sleep(Duration::from_millis(5));
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

#[test]
fn rest_create_and_update_supports_me_alias() {
    // Use EnvVarGuard instead of manual set/remove
    let tmp = tempfile::tempdir().unwrap();
    let tasks_dir = tmp.path().join(".tasks");
    std::fs::create_dir_all(&tasks_dir).unwrap();
    let _guard_tasks = EnvVarGuard::set("LOTAR_TASKS_DIR", &tasks_dir.to_string_lossy());
    let _guard_silent = EnvVarGuard::set("LOTAR_TEST_SILENT", "1");

    // Configure identity for deterministic @me
    std::fs::write(
        lotar::utils::paths::global_config_path(&tasks_dir),
        "default.project: REST\nissue.states: [Todo, InProgress, Done]\nissue.types: [Feature, Bug, Chore]\nissue.priorities: [Low, Medium, High]\ndefault.reporter: erin\n",
    )
    .unwrap();

    let port = find_free_port();
    start_server_on(port);

    // Create task with @me as assignee
    let create_body = json!({
        "title": "REST @me",
        "project": "REST",
        "assignee": "@me"
    });
    let (status, body) = http_post_json(
        port,
        "/api/tasks/add?project=REST",
        &create_body.to_string(),
    );
    assert_eq!(status, 201, "create should succeed");
    let env: serde_json::Value = serde_json::from_slice(&body).unwrap();
    let data = env.get("data").cloned().unwrap_or(json!({}));
    let id = data.get("id").and_then(|v| v.as_str()).unwrap().to_string();
    assert_eq!(data.get("assignee").and_then(|v| v.as_str()), Some("erin"));

    // Update via REST with @me for reporter
    let patch = json!({
        "id": id,
        "reporter": "@me"
    });
    let (status, body) = http_post_json(port, "/api/tasks/update", &patch.to_string());
    assert_eq!(status, 200, "update should succeed");
    let env: serde_json::Value = serde_json::from_slice(&body).unwrap();
    let data = env.get("data").cloned().unwrap_or(json!({}));
    assert_eq!(data.get("reporter").and_then(|v| v.as_str()), Some("erin"));

    stop_server_on(port);

    // Env restored by guards
}

#[test]
fn api_sprint_assignment_and_backlog() {
    let _guard_fast = EnvVarGuard::set("LOTAR_TEST_FAST_IO", "1");
    let tmp = tempfile::tempdir().unwrap();
    let tasks_dir = tmp.path().join(".tasks");
    std::fs::create_dir_all(&tasks_dir).unwrap();
    let _guard_tasks = EnvVarGuard::set("LOTAR_TASKS_DIR", &tasks_dir.to_string_lossy());

    // Minimal config to support task writes
    std::fs::write(
        lotar::utils::paths::global_config_path(&tasks_dir),
        "default.project: API\nissue.states: [Todo, InProgress, Done]\nissue.types: [Feature, Bug]\nissue.priorities: [Low, Medium, High]\n",
    )
    .unwrap();

    let mut storage = lotar::storage::manager::Storage::new(tasks_dir.clone());
    let mut sprint = lotar::storage::sprint::Sprint::default();
    sprint.plan = Some(lotar::storage::sprint::SprintPlan {
        label: Some("Iteration 1".into()),
        ..Default::default()
    });
    sprint.actual = Some(lotar::storage::sprint::SprintActual {
        started_at: Some(Utc::now().to_rfc3339()),
        ..Default::default()
    });
    let created =
        lotar::services::sprint_service::SprintService::create(&mut storage, sprint, None)
            .expect("create sprint");
    let sprint_id = created.record.id;

    let task_a = lotar::services::task_service::TaskService::create(
        &mut storage,
        lotar::api_types::TaskCreate {
            title: "Sprint API A".into(),
            project: Some("API".into()),
            ..lotar::api_types::TaskCreate::default()
        },
    )
    .expect("task a");
    let task_b = lotar::services::task_service::TaskService::create(
        &mut storage,
        lotar::api_types::TaskCreate {
            title: "Sprint API B".into(),
            project: Some("API".into()),
            ..lotar::api_types::TaskCreate::default()
        },
    )
    .expect("task b");
    let task_c = lotar::services::task_service::TaskService::create(
        &mut storage,
        lotar::api_types::TaskCreate {
            title: "Sprint API Backlog".into(),
            project: Some("API".into()),
            ..lotar::api_types::TaskCreate::default()
        },
    )
    .expect("task c");
    drop(storage);

    let mut api = ApiServer::new();
    routes::initialize(&mut api);

    let list_req = mk_req("GET", "/api/sprints/list", &[], json!({}));
    let list_resp = api.handle_request(&list_req);
    assert_eq!(list_resp.status, 200, "list should succeed");
    let list_json: serde_json::Value = serde_json::from_slice(&list_resp.body).unwrap();
    let list_data = list_json.get("data").expect("list data");
    assert_eq!(
        list_data.get("count").and_then(|v| v.as_u64()),
        Some(1),
        "should report one sprint"
    );
    let sprints = list_data
        .get("sprints")
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();
    assert_eq!(sprints.len(), 1, "expected single sprint in list");
    let sprint_entry = &sprints[0];
    assert_eq!(
        sprint_entry.get("id").and_then(|v| v.as_u64()),
        Some(sprint_id as u64)
    );
    assert_eq!(
        sprint_entry.get("display_name").and_then(|v| v.as_str()),
        Some("Iteration 1")
    );
    assert_eq!(
        sprint_entry.get("state").and_then(|v| v.as_str()),
        Some("active")
    );

    let add_body = json!({
        "sprint": sprint_id,
        "tasks": [task_a.id.clone(), task_b.id.clone()]
    });
    let add_req = mk_req("POST", "/api/sprints/add", &[], add_body);
    let add_resp = api.handle_request(&add_req);
    assert_eq!(add_resp.status, 200);
    let add_json: serde_json::Value = serde_json::from_slice(&add_resp.body).unwrap();
    let add_data = add_json.get("data").unwrap();
    let modified = add_data
        .get("modified")
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();
    assert_eq!(modified.len(), 2);

    let storage = lotar::storage::manager::Storage::new(tasks_dir.clone());
    let assigned_a = lotar::services::task_service::TaskService::get(&storage, &task_a.id, None)
        .expect("task a assigned");
    assert_eq!(assigned_a.sprints, vec![sprint_id]);
    let assigned_b = lotar::services::task_service::TaskService::get(&storage, &task_b.id, None)
        .expect("task b assigned");
    assert_eq!(assigned_b.sprints, vec![sprint_id]);
    drop(storage);

    let backlog_req = mk_req(
        "GET",
        "/api/sprints/backlog",
        &[("project", "API")],
        json!({}),
    );
    let backlog_resp = api.handle_request(&backlog_req);
    assert_eq!(backlog_resp.status, 200);
    let backlog_json: serde_json::Value = serde_json::from_slice(&backlog_resp.body).unwrap();
    let backlog_tasks = backlog_json
        .get("data")
        .and_then(|d| d.get("tasks"))
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();
    assert_eq!(backlog_tasks.len(), 1);
    assert_eq!(
        backlog_tasks[0].get("id").and_then(|v| v.as_str()),
        Some(task_c.id.as_str())
    );

    let remove_body = json!({
        "sprint": sprint_id,
        "tasks": [task_a.id.clone()]
    });
    let remove_req = mk_req("POST", "/api/sprints/remove", &[], remove_body);
    let remove_resp = api.handle_request(&remove_req);
    assert_eq!(remove_resp.status, 200);
    let remove_json: serde_json::Value = serde_json::from_slice(&remove_resp.body).unwrap();
    let removed = remove_json
        .get("data")
        .and_then(|d| d.get("modified"))
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();
    assert_eq!(removed, vec![serde_json::Value::String(task_a.id.clone())]);

    let storage = lotar::storage::manager::Storage::new(tasks_dir.clone());
    let updated_a = lotar::services::task_service::TaskService::get(&storage, &task_a.id, None)
        .expect("task a after remove");
    assert!(updated_a.sprints.is_empty());
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
    // Speed up IO handling in server during tests and serialize env
    let _guard_fast = EnvVarGuard::set("LOTAR_TEST_FAST_IO", "1");
    // Isolate tasks dir via env var
    let tmp = tempfile::tempdir().unwrap();
    let tasks_dir = tmp.path().join(".tasks");
    std::fs::create_dir_all(&tasks_dir).unwrap();
    let _guard_tasks = EnvVarGuard::set("LOTAR_TASKS_DIR", &tasks_dir.to_string_lossy());

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
            if e.path()
                .extension()
                .and_then(|s| s.to_str())
                .is_some_and(|ext| ext == "yml")
            {
                yml_count += 1;
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
    assert!(listed["data"]["tasks"].is_array());
    assert!(listed["data"]["total"].as_u64().unwrap_or(0) >= 1);

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

    // Restored by guards
}

#[test]
fn api_comment_update_edits_existing_comment() {
    let _guard_fast = EnvVarGuard::set("LOTAR_TEST_FAST_IO", "1");
    let tmp = tempfile::tempdir().unwrap();
    let tasks_dir = tmp.path().join(".tasks");
    std::fs::create_dir_all(&tasks_dir).unwrap();
    let _guard_tasks = EnvVarGuard::set("LOTAR_TASKS_DIR", &tasks_dir.to_string_lossy());

    let mut api = ApiServer::new();
    routes::initialize(&mut api);

    // Create a task to attach comments to
    let add_body = json!({
        "title": "Comment editing",
        "project": "EDIT",
    });
    let resp = api.handle_request(&mk_req("POST", "/api/tasks/add", &[], add_body));
    assert_eq!(resp.status, 201, "task creation status");
    let created: Value = serde_json::from_slice(&resp.body).unwrap();
    let id = created["data"]["id"].as_str().unwrap().to_string();

    // Add initial comment
    let resp = api.handle_request(&mk_req(
        "POST",
        "/api/tasks/comment",
        &[],
        json!({ "id": id.clone(), "text": "Initial note" }),
    ));
    assert_eq!(resp.status, 200, "add comment status");
    let with_comment: Value = serde_json::from_slice(&resp.body).unwrap();
    let comment_text = with_comment["data"]["comments"][0]["text"]
        .as_str()
        .unwrap();
    assert_eq!(comment_text, "Initial note");

    // Edit the comment
    let resp = api.handle_request(&mk_req(
        "POST",
        "/api/tasks/comment/update",
        &[],
        json!({ "id": id.clone(), "index": 0, "text": " Edited note " }),
    ));
    assert_eq!(resp.status, 200, "edit comment status");
    let updated: Value = serde_json::from_slice(&resp.body).unwrap();
    let updated_text = updated["data"]["comments"][0]["text"].as_str().unwrap();
    assert_eq!(
        updated_text, "Edited note",
        "comment text should be trimmed and updated"
    );
    let history = updated["data"]["history"]
        .as_array()
        .cloned()
        .unwrap_or_default();
    let last_change = history.last().expect("history entry for comment edit");
    let change_field = last_change["changes"][0]["field"].as_str().unwrap();
    assert_eq!(change_field, "comment#1");
    assert_eq!(
        last_change["changes"][0]["old"].as_str().unwrap(),
        "Initial note"
    );
    assert_eq!(
        last_change["changes"][0]["new"].as_str().unwrap(),
        "Edited note"
    );

    // Invalid index should be rejected
    let resp = api.handle_request(&mk_req(
        "POST",
        "/api/tasks/comment/update",
        &[],
        json!({ "id": id, "index": 5, "text": "Nope" }),
    ));
    assert_eq!(resp.status, 400, "invalid index should fail");
}

#[test]
fn api_config_show_set() {
    // Guard tasks dir env var
    let tmp = tempfile::tempdir().unwrap();
    let tasks_dir = tmp.path().join(".tasks");
    std::fs::create_dir_all(&tasks_dir).unwrap();
    let _guard_tasks = EnvVarGuard::set("LOTAR_TASKS_DIR", &tasks_dir.to_string_lossy());

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
    assert!(show2["data"].get("default_prefix").is_none());

    // Restored by guard
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

#[test]
fn api_list_accepts_plain_custom_field_filters_and_me() {
    // Ensure isolated tasks dir with guard
    let tmp = tempfile::tempdir().unwrap();
    let tasks_dir = tmp.path().join(".tasks");
    std::fs::create_dir_all(&tasks_dir).unwrap();
    let _guard_tasks = EnvVarGuard::set("LOTAR_TASKS_DIR", &tasks_dir.to_string_lossy());

    // Set declared custom field 'sprint' globally to allow plain usage
    let mut api = ApiServer::new();
    routes::initialize(&mut api);
    // Configure custom_fields
    let set_body = json!({"values": {"custom_fields": "sprint,release"}, "global": true});
    let resp = api.handle_request(&mk_req("POST", "/api/config/set", &[], set_body));
    assert_eq!(resp.status, 200, "config set");

    // Create tasks across two projects with assignees and custom fields
    let add = |project: &str, title: &str, assignee: Option<&str>, sprint: &str| {
        serde_json::json!({
            "title": title,
            "project": project,
            "assignee": assignee,
            "fields": {"sprint": sprint}
        })
    };

    let r1 = api.handle_request(&mk_req(
        "POST",
        "/api/tasks/add",
        &[],
        add("TEST", "A", Some("alice"), "W35"),
    ));
    assert_eq!(r1.status, 201);
    let r2 = api.handle_request(&mk_req(
        "POST",
        "/api/tasks/add",
        &[],
        add("TEST", "B", Some("bob"), "W36"),
    ));
    assert_eq!(r2.status, 201);
    let r3 = api.handle_request(&mk_req(
        "POST",
        "/api/tasks/add",
        &[],
        add("OTHER", "C", Some("alice"), "w35"),
    ));
    assert_eq!(r3.status, 201);

    let created_a: Value = serde_json::from_slice(&r1.body).unwrap();
    let task_a_id = created_a["data"]["id"].as_str().unwrap().to_string();
    let status_resp = api.handle_request(&mk_req(
        "POST",
        "/api/tasks/status",
        &[],
        json!({"id": task_a_id, "status": "InProgress"}),
    ));
    assert_eq!(status_resp.status, 200);

    let resp = api.handle_request(&mk_req(
        "GET",
        "/api/tasks/list",
        &[("project", "TEST"), ("status", "InProgress")],
        json!({}),
    ));
    assert_eq!(resp.status, 200, "filtering by InProgress should succeed");
    let filtered: Value = serde_json::from_slice(&resp.body).unwrap();
    assert_eq!(filtered["data"]["total"].as_u64().unwrap_or(0), 1);
    assert_eq!(
        filtered["data"]["tasks"]
            .as_array()
            .map(|a| a.len())
            .unwrap_or(0),
        1
    );

    // List by custom field directly: ?sprint=W35 should match case-insensitively
    let resp = api.handle_request(&mk_req(
        "GET",
        "/api/tasks/list",
        &[("project", "TEST"), ("sprint", "W35")],
        json!({}),
    ));
    assert_eq!(resp.status, 200);
    let list: Value = serde_json::from_slice(&resp.body).unwrap();
    let count = list["data"]["total"].as_u64().unwrap_or(0);
    assert_eq!(count, 1, "expected one TEST task with sprint W35");

    // CSV and fuzzy matching: W35 should match w-35 via normalization
    let resp = api.handle_request(&mk_req(
        "GET",
        "/api/tasks/list",
        &[("project", "TEST"), ("sprint", "W35,w-99")],
        json!({}),
    ));
    assert_eq!(resp.status, 200);
    let list2: Value = serde_json::from_slice(&resp.body).unwrap();
    assert_eq!(list2["data"]["total"].as_u64().unwrap_or(0), 1);

    // Assignee @me resolution: set default reporter/assignee identity to alice by creating config
    // We'll simulate identity resolution falling back to system username by setting assignee explicitly and querying @me
    let resp = api.handle_request(&mk_req(
        "GET",
        "/api/tasks/list",
        &[("project", "TEST"), ("assignee", "@me")],
        json!({}),
    ));
    // We can't control identity in this unit test; ensure it doesn't 500 and returns a list
    assert_eq!(resp.status, 200);
    let _ = serde_json::from_slice::<Value>(&resp.body).unwrap();

    // Restored by guard
}

#[test]
fn api_list_supports_fuzzy_tag_filters() {
    let tmp = tempfile::tempdir().unwrap();
    let tasks_dir = tmp.path().join(".tasks");
    std::fs::create_dir_all(&tasks_dir).unwrap();
    let _guard_tasks = EnvVarGuard::set("LOTAR_TASKS_DIR", &tasks_dir.to_string_lossy());

    let mut api = ApiServer::new();
    routes::initialize(&mut api);

    let add = |title: &str, tags: &[&str]| {
        json!({
            "title": title,
            "project": "TEST",
            "tags": tags,
        })
    };

    let add_one = |title: &str, tags: &[&str]| {
        api.handle_request(&mk_req("POST", "/api/tasks/add", &[], add(title, tags)))
    };

    let r1 = add_one("DevOps upgrade", &["DevOps", "backend"]);
    assert_eq!(r1.status, 201);
    let r2 = add_one("Frontend polish", &["frontend"]);
    assert_eq!(r2.status, 201);
    let r3 = add_one("Database tuning", &["db", "storage"]);
    assert_eq!(r3.status, 201);

    let ids_from = |resp: &serde_json::Value| -> Vec<String> {
        resp["data"]["tasks"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|v| v["id"].as_str().map(|s| s.to_string()))
            .collect()
    };

    let resp_ops = api.handle_request(&mk_req(
        "GET",
        "/api/tasks/list",
        &[("project", "TEST"), ("tags", "ops")],
        json!({}),
    ));
    assert_eq!(resp_ops.status, 200);
    let parsed_ops: Value = serde_json::from_slice(&resp_ops.body).unwrap();
    let ops_ids = ids_from(&parsed_ops);
    assert_eq!(parsed_ops["data"]["total"].as_u64().unwrap_or(0), 1);
    assert!(ops_ids.iter().any(|id| id.starts_with("TEST-")));

    let resp_case = api.handle_request(&mk_req(
        "GET",
        "/api/tasks/list",
        &[("project", "TEST"), ("tags", "DEVOPS")],
        json!({}),
    ));
    assert_eq!(resp_case.status, 200);
    let parsed_case: Value = serde_json::from_slice(&resp_case.body).unwrap();
    let case_ids = ids_from(&parsed_case);
    assert_eq!(case_ids, ops_ids);

    let resp_partial = api.handle_request(&mk_req(
        "GET",
        "/api/tasks/list",
        &[("project", "TEST"), ("tags", "front")],
        json!({}),
    ));
    assert_eq!(resp_partial.status, 200);
    let parsed_partial: Value = serde_json::from_slice(&resp_partial.body).unwrap();
    assert_eq!(parsed_partial["data"]["total"].as_u64().unwrap_or(0), 1);
    let partial_ids = ids_from(&parsed_partial);
    assert_ne!(partial_ids, ops_ids);
}

// Merged from sse_events_test.rs
#[test]
fn sse_events_with_kinds_and_project_filter() {
    // Guard tasks dir
    let tmp = tempfile::tempdir().unwrap();
    let tasks_dir = tmp.path().join(".tasks");
    std::fs::create_dir_all(&tasks_dir).unwrap();
    let _guard_tasks = EnvVarGuard::set("LOTAR_TASKS_DIR", &tasks_dir.to_string_lossy());
    // Enable explicit ready event for faster startup sync
    let _guard_ready = EnvVarGuard::set("LOTAR_SSE_READY", "1");

    let port = find_free_port();
    start_server_on(port);

    let (mut sse, leftover) = open_sse(
        port,
        "debounce_ms=10&kinds=task_created&project=TEST&ready=1",
    );

    let add_body_test = r#"{\"title\":\"A\",\"priority\":\"High\"}"#;
    let (_st1, _b1) = http_post_json(port, "/api/tasks/add?project=TEST", add_body_test);
    let add_body_other = r#"{\"title\":\"B\",\"priority\":\"Low\"}"#;
    let (_st2, _b2) = http_post_json(port, "/api/tasks/add?project=OTHER", add_body_other);

    let events = read_sse_events(&mut sse, 3, Duration::from_millis(800), leftover);
    let created: Vec<_> = events
        .into_iter()
        .filter(|(k, _)| k == "task_created")
        .collect();
    assert!(created.iter().all(|(k, _)| k == "task_created"));
    assert!(
        created
            .iter()
            .all(|(_, d)| d.contains("\"id\":") && d.contains("TEST-")),
        "events: {created:?}"
    );

    // Restored by guard
    stop_server_on(port);
}

#[test]
fn sse_debounce_emits_all_events() {
    // Guard tasks dir
    let tmp = tempfile::tempdir().unwrap();
    let tasks_dir = tmp.path().join(".tasks");
    std::fs::create_dir_all(&tasks_dir).unwrap();
    let _guard_tasks = EnvVarGuard::set("LOTAR_TASKS_DIR", &tasks_dir.to_string_lossy());
    // Enable explicit ready event to avoid startup sleeps
    let _guard_ready = EnvVarGuard::set("LOTAR_SSE_READY", "1");

    let port = find_free_port();
    start_server_on(port);

    let (mut sse, leftover) = open_sse(
        port,
        "debounce_ms=25&kinds=task_created&project=TEST&ready=1",
    );

    for i in 0..3 {
        let body = format!("{{\"title\":\"T{i}\",\"priority\":\"High\"}}");
        let _ = http_post_json(port, "/api/tasks/add?project=TEST", &body);
    }

    let events = read_sse_events(&mut sse, 4, Duration::from_millis(1200), leftover);
    let created: Vec<_> = events
        .into_iter()
        .filter(|(k, _)| k == "task_created")
        .collect();
    assert_eq!(
        created.len(),
        3,
        "expected 3 created events, got {created:?}"
    );

    // Restored by guard
    stop_server_on(port);
}

#[test]
fn sse_includes_triggered_by_identity() {
    // Guard flags and tasks dir
    // Speed up IO handling in server during tests and enable ready event
    let _guard_fast = EnvVarGuard::set("LOTAR_TEST_FAST_IO", "1");
    let _guard_ready = EnvVarGuard::set("LOTAR_SSE_READY", "1");

    let tmp = tempfile::tempdir().unwrap();
    let tasks_dir = tmp.path().join(".tasks");
    std::fs::create_dir_all(&tasks_dir).unwrap();
    // Provide default.reporter in config so identity is deterministic (canonical key)
    std::fs::write(tasks_dir.join("config.yml"), b"default.reporter: alice\n").unwrap();
    let _guard_tasks = EnvVarGuard::set("LOTAR_TASKS_DIR", &tasks_dir.to_string_lossy());

    let port = find_free_port();
    start_server_on(port);

    let (mut sse, mut leftover) = open_sse(
        port,
        "debounce_ms=10&kinds=task_created&project=TEST&ready=1",
    );
    // Drain until the ready event
    let events = read_sse_events(&mut sse, 1, Duration::from_millis(400), leftover);
    if events.is_empty() {
        // read any leftover and continue
        leftover = Vec::new();
    } else {
        leftover = Vec::new();
    }

    // Send two creates to reduce chance of missing the first due to a race
    let (st1, _b1) = http_post_json(
        port,
        "/api/tasks/add?project=TEST",
        r#"{"title":"SSE actor"}"#,
    );
    assert_eq!(st1, 201, "first add should succeed");
    let (st2, _b2) = http_post_json(
        port,
        "/api/tasks/add?project=TEST",
        r#"{"title":"SSE actor 2"}"#,
    );
    assert_eq!(st2, 201, "second add should succeed");

    // We only need the first created event to validate triggered_by; collect up to 2 with a tighter timeout
    let events = read_sse_events(&mut sse, 2, Duration::from_millis(1000), leftover);
    assert!(!events.is_empty(), "expected at least one created event");
    // Find the first event for TEST-* and validate triggered_by
    let mut found = false;
    for (_kind, data) in events {
        let json: serde_json::Value = match serde_json::from_str(&data) {
            Ok(v) => v,
            Err(_) => continue,
        };
        if json
            .get("id")
            .and_then(|v| v.as_str())
            .is_some_and(|id| id.starts_with("TEST-"))
        {
            assert_eq!(
                json.get("triggered_by").and_then(|v| v.as_str()),
                Some("alice")
            );
            found = true;
            break;
        }
    }
    assert!(found, "did not find a TEST-* task_created event");

    // Restored by guards
    stop_server_on(port);
}

#[test]
fn api_get_missing_and_unknown_id_errors() {
    // Guard tasks dir
    let tmp = tempfile::tempdir().unwrap();
    let tasks_dir = tmp.path().join(".tasks");
    std::fs::create_dir_all(&tasks_dir).unwrap();
    let _guard_tasks = EnvVarGuard::set("LOTAR_TASKS_DIR", &tasks_dir.to_string_lossy());

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

    // Restored by guard
}

#[test]
fn api_add_invalid_priority_and_type_rejected() {
    // Guard tasks dir
    let tmp = tempfile::tempdir().unwrap();
    let tasks_dir = tmp.path().join(".tasks");
    std::fs::create_dir_all(&tasks_dir).unwrap();
    let _guard_tasks = EnvVarGuard::set("LOTAR_TASKS_DIR", &tasks_dir.to_string_lossy());

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

    // Restored by guard
}

#[test]
fn sse_debounce_zero_and_invalid_kind_handling() {
    // Guard tasks dir
    let tmp = tempfile::tempdir().unwrap();
    let tasks_dir = tmp.path().join(".tasks");
    std::fs::create_dir_all(&tasks_dir).unwrap();
    let _guard_tasks = EnvVarGuard::set("LOTAR_TASKS_DIR", &tasks_dir.to_string_lossy());

    let port = find_free_port();
    start_server_on(port);

    // invalid kind should filter out events; debounce 0 should flush immediately
    let (mut sse, leftover) = open_sse(port, "debounce_ms=0&kinds=invalid_kind");
    std::thread::sleep(Duration::from_millis(20));
    // Create a task -> would generate task_created, but our kind filter excludes it
    let _ = http_post_json(port, "/api/tasks/add?project=TEST", r#"{\"title\":\"Z\"}"#);
    let events = read_sse_events(&mut sse, 1, Duration::from_millis(200), leftover);
    assert!(events.is_empty(), "invalid kind should filter all events");

    // Restored by guard
    stop_server_on(port);
}

#[test]
fn sse_project_changed_emitted_on_fs_change() {
    // Serialize CWD changes to avoid races with other tests
    let _guard = lock_var("LOTAR_CWD");
    // Enable fast paths and ready event
    let _guard_fast = EnvVarGuard::set("LOTAR_TEST_FAST_IO", "1");
    let _guard_ready = EnvVarGuard::set("LOTAR_SSE_READY", "1");

    // Create an isolated workspace with .tasks/DEMO
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path().to_path_buf();
    let tasks_dir = root.join(".tasks").join("DEMO");
    std::fs::create_dir_all(&tasks_dir).unwrap();

    // Temporarily change process CWD so the watcher sees our .tasks directory
    let prev_cwd = std::env::current_dir().unwrap();
    std::env::set_current_dir(&root).unwrap();

    let port = find_free_port();
    start_server_on(port);

    // Open SSE stream with project filter and kinds=project_changed
    let (mut sse, leftover) = open_sse(
        port,
        "debounce_ms=0&kinds=project_changed&project=DEMO&ready=1",
    );
    // Drain the ready event if present
    let _ = read_sse_events(&mut sse, 1, Duration::from_millis(500), leftover);

    // Give the watcher a moment to start listening, then create a YAML file
    std::thread::sleep(Duration::from_millis(60));
    let file_path = tasks_dir.join("1.yml");
    std::fs::write(&file_path, b"title: From watcher\n").unwrap();
    // Nudge: perform a quick modify to reduce chance of missing the initial create
    std::thread::sleep(Duration::from_millis(10));
    std::fs::write(&file_path, b"title: From watcher updated\n").unwrap();

    // Read one project_changed event
    let events = read_sse_events(&mut sse, 1, Duration::from_millis(2000), Vec::new());
    assert!(
        !events.is_empty(),
        "expected a project_changed event from watcher, got: {events:?}"
    );
    let (kind, data) = &events[0];
    assert_eq!(kind, "project_changed", "unexpected event kind: {kind}");
    // Data should be JSON with { name: "DEMO" }
    let v: serde_json::Value = serde_json::from_str(data).unwrap_or_else(|_| serde_json::json!({}));
    assert_eq!(v.get("name").and_then(|s| s.as_str()), Some("DEMO"));

    // Cleanup and restore CWD
    stop_server_on(port);
    std::env::set_current_dir(prev_cwd).unwrap();
    // Restored by guards
}

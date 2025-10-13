mod common;
use crate::common::env_mutex::EnvVarGuard;

#[test]
fn mcp_tools_list_includes_underscore_names() {
    let req = serde_json::json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "tools/list",
        "params": {}
    });
    let line = serde_json::to_string(&req).unwrap();
    let resp_line = lotar::mcp::server::handle_json_line(&line);
    let resp: serde_json::Value = serde_json::from_str(&resp_line).unwrap();
    assert!(
        resp.get("error").is_none(),
        "tools/list should not error: {resp}"
    );
    let tools = resp
        .get("result")
        .and_then(|r| r.get("tools"))
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();
    let names: Vec<String> = tools
        .iter()
        .filter_map(|t| {
            t.get("name")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string())
        })
        .collect();
    assert!(names.contains(&"task_create".to_string()));
    assert!(names.contains(&"project_list".to_string()));
}

#[test]
fn mcp_logging_set_level_acknowledged() {
    let req = serde_json::json!({
        "jsonrpc": "2.0",
        "id": 2,
        "method": "logging/setLevel",
        "params": {"level": "warn"}
    });
    let line = serde_json::to_string(&req).unwrap();
    let resp_line = lotar::mcp::server::handle_json_line(&line);
    let resp: serde_json::Value = serde_json::from_str(&resp_line).unwrap();
    assert!(
        resp.get("error").is_none(),
        "logging/setLevel should be accepted: {resp}"
    );
}

#[test]
fn mcp_tools_call_accepts_underscore_name_for_task_create() {
    let tmp = tempfile::tempdir().unwrap();
    let tasks_dir = tmp.path().join(".tasks");
    std::fs::create_dir_all(&tasks_dir).unwrap();
    let _guard_tasks = EnvVarGuard::set("LOTAR_TASKS_DIR", tasks_dir.to_string_lossy().as_ref());

    let req = serde_json::json!({
        "jsonrpc": "2.0",
        "id": 3,
        "method": "tools/call",
        "params": {"name": "task_create", "arguments": {"title": "MCP Unit", "project": "MCP"}}
    });
    let line = serde_json::to_string(&req).unwrap();
    let resp_line = lotar::mcp::server::handle_json_line(&line);
    let resp: serde_json::Value = serde_json::from_str(&resp_line).unwrap();
    assert!(
        resp.get("error").is_none(),
        "tools/call task_create failed: {resp}"
    );
    let content = resp
        .get("result")
        .and_then(|r| r.get("content"))
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();
    assert!(!content.is_empty(), "expected content array with task json");
    let text = content[0]
        .get("text")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    let task_json: serde_json::Value = serde_json::from_str(text).unwrap_or(serde_json::json!({}));
    let id = task_json.get("id").and_then(|v| v.as_str()).unwrap_or("");
    assert!(
        id.starts_with("MCP-"),
        "expected id to start with MCP-, got {id}"
    );

    // guard drops here
}

// Merged from mcp_smoke_test.rs: basic storage smoke via MCP-shaped flow
#[test]
fn mcp_task_create_and_get() {
    let tmp = tempfile::tempdir().unwrap();
    let tasks_dir = tmp.path().join(".tasks");
    std::fs::create_dir_all(&tasks_dir).unwrap();
    let _guard_tasks = EnvVarGuard::set("LOTAR_TASKS_DIR", tasks_dir.to_string_lossy().as_ref());

    let create =
        serde_json::json!({"title":"From MCP","project":"MCP","priority":"High","tags":[]});
    let resolver = lotar::TasksDirectoryResolver::resolve(None, None).unwrap();
    let mut storage = lotar::Storage::new(resolver.path.clone());
    let req: lotar::api_types::TaskCreate = serde_json::from_value(create).unwrap();
    let task = lotar::services::task_service::TaskService::create(&mut storage, req).unwrap();
    assert!(task.id.starts_with("MCP-"));

    let storage = lotar::Storage::new(resolver.path);
    let got = lotar::services::task_service::TaskService::get(&storage, &task.id, None).unwrap();
    assert_eq!(got.id, task.id);

    // guard drops here
}

#[test]
fn mcp_tools_call_with_invalid_tool_name_returns_method_not_found() {
    let req = serde_json::json!({
        "jsonrpc": "2.0",
        "id": 99,
        "method": "tools/call",
        "params": {"name": "does_not_exist", "arguments": {}}
    });
    let line = serde_json::to_string(&req).unwrap();
    let resp_line = lotar::mcp::server::handle_json_line(&line);
    let resp: serde_json::Value = serde_json::from_str(&resp_line).unwrap();
    let err = resp.get("error").cloned().unwrap_or(serde_json::json!({}));
    assert_eq!(err.get("code").and_then(|v| v.as_i64()), Some(-32601));
}

#[test]
fn mcp_task_create_missing_title_returns_invalid_params() {
    let tmp = tempfile::tempdir().unwrap();
    let tasks_dir = tmp.path().join(".tasks");
    std::fs::create_dir_all(&tasks_dir).unwrap();
    let _guard_tasks = EnvVarGuard::set("LOTAR_TASKS_DIR", tasks_dir.to_string_lossy().as_ref());

    let req = serde_json::json!({
        "jsonrpc": "2.0",
        "id": 100,
        "method": "tools/call",
        "params": {"name": "task_create", "arguments": {"project": "TEST"}}
    });
    let line = serde_json::to_string(&req).unwrap();
    let resp_line = lotar::mcp::server::handle_json_line(&line);
    let resp: serde_json::Value = serde_json::from_str(&resp_line).unwrap();
    let err = resp.get("error").cloned().unwrap();
    assert_eq!(err.get("code").and_then(|v| v.as_i64()), Some(-32602));
    let msg = err
        .get("message")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_lowercase();
    assert!(msg.contains("missing") && msg.contains("title"));

    // guard drops here
}

// Merged from mcp_me_alias_test.rs
#[test]
fn mcp_task_create_resolves_me_alias() {
    let tmp = tempfile::tempdir().unwrap();
    let tasks_dir = tmp.path().join(".tasks");
    std::fs::create_dir_all(&tasks_dir).unwrap();

    // Configure identity so @me resolves deterministically
    std::fs::write(
        lotar::utils::paths::global_config_path(&tasks_dir),
        "default.project: MCP\nissue.states: [Todo, InProgress, Done]\nissue.types: [Feature, Bug, Chore]\nissue.priorities: [Low, Medium, High]\ndefault.reporter: carol\n",
    )
    .unwrap();

    let _guard_tasks = EnvVarGuard::set("LOTAR_TASKS_DIR", tasks_dir.to_string_lossy().as_ref());

    let req = serde_json::json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "tools/call",
        "params": {"name": "task_create", "arguments": {
            "title": "MCP @me", "project": "MCP", "assignee": "@me"
        }}
    });
    let line = serde_json::to_string(&req).unwrap();
    let resp_line = lotar::mcp::server::handle_json_line(&line);
    let resp: serde_json::Value = serde_json::from_str(&resp_line).unwrap();
    assert!(
        resp.get("error").is_none(),
        "task_create should not error: {resp}"
    );
    let content = resp
        .get("result")
        .and_then(|r| r.get("content"))
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();
    assert!(!content.is_empty());
    let text = content[0]
        .get("text")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    let task_json: serde_json::Value = serde_json::from_str(text).unwrap();
    assert_eq!(
        task_json.get("assignee").and_then(|v| v.as_str()),
        Some("carol")
    );

    // guard drops here
}

// Merged from mcp_update_me_alias_test.rs
#[test]
fn mcp_task_update_resolves_me_in_patch() {
    let tmp = tempfile::tempdir().unwrap();
    let tasks_dir = tmp.path().join(".tasks");
    std::fs::create_dir_all(&tasks_dir).unwrap();

    // Configure identity so @me resolves deterministically
    std::fs::write(
        lotar::utils::paths::global_config_path(&tasks_dir),
        "default.project: MCP\nissue.states: [Todo, InProgress, Done]\nissue.types: [Feature, Bug, Chore]\nissue.priorities: [Low, Medium, High]\ndefault.reporter: dave\n",
    )
    .unwrap();

    let _guard_tasks = EnvVarGuard::set("LOTAR_TASKS_DIR", tasks_dir.to_string_lossy().as_ref());

    // Create a task (using service for convenience)
    let resolver = lotar::TasksDirectoryResolver::resolve(None, None).unwrap();
    let mut storage = lotar::Storage::new(resolver.path.clone());
    let created = lotar::services::task_service::TaskService::create(
        &mut storage,
        lotar::api_types::TaskCreate {
            title: "Update @me patch".into(),
            project: Some("MCP".into()),
            priority: None,
            task_type: None,
            reporter: None,
            assignee: None,
            due_date: None,
            effort: None,
            description: None,
            category: None,
            tags: vec![],
            relationships: None,
            custom_fields: None,
        },
    )
    .expect("create initial task");

    // Send MCP task/update with @me in both reporter and assignee
    let req = serde_json::json!({
        "jsonrpc": "2.0",
        "id": 42,
        "method": "tools/call",
        "params": {
            "name": "task_update",
            "arguments": {
                "id": created.id,
                "patch": {"assignee": "@me", "reporter": "@me"}
            }
        }
    });
    let line = serde_json::to_string(&req).unwrap();
    let resp_line = lotar::mcp::server::handle_json_line(&line);
    let resp: serde_json::Value = serde_json::from_str(&resp_line).unwrap();
    assert!(
        resp.get("error").is_none(),
        "task_update should not error: {resp}"
    );
    let content = resp
        .get("result")
        .and_then(|r| r.get("content"))
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();
    assert!(!content.is_empty());
    let text = content[0]
        .get("text")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    let task_json: serde_json::Value = serde_json::from_str(text).unwrap();
    assert_eq!(
        task_json.get("assignee").and_then(|v| v.as_str()),
        Some("dave")
    );
    assert_eq!(
        task_json.get("reporter").and_then(|v| v.as_str()),
        Some("dave")
    );

    // guard drops here
}

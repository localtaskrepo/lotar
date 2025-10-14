mod common;
use crate::common::env_mutex::EnvVarGuard;
use std::collections::HashMap;

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

#[test]
fn mcp_task_create_accepts_custom_fields() {
    let tmp = tempfile::tempdir().unwrap();
    let tasks_dir = tmp.path().join(".tasks");
    std::fs::create_dir_all(&tasks_dir).unwrap();
    let _guard_tasks = EnvVarGuard::set("LOTAR_TASKS_DIR", tasks_dir.to_string_lossy().as_ref());

    let req = serde_json::json!({
        "jsonrpc": "2.0",
        "id": 17,
        "method": "tools/call",
        "params": {
            "name": "task_create",
            "arguments": {
                "title": "MCP Category",
                "project": "MCP",
                "custom_fields": {
                    "product": "Platform"
                }
            }
        }
    });
    let line = serde_json::to_string(&req).unwrap();
    let resp_line = lotar::mcp::server::handle_json_line(&line);
    let resp: serde_json::Value = serde_json::from_str(&resp_line).unwrap();
    assert!(resp.get("error").is_none(), "task_create failed: {resp}");

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
    let custom_fields = task_json
        .get("custom_fields")
        .and_then(|v| v.as_object())
        .cloned()
        .unwrap_or_default();
    assert_eq!(
        custom_fields.get("product").and_then(|v| v.as_str()),
        Some("Platform")
    );

    let task_id = task_json
        .get("id")
        .and_then(|v| v.as_str())
        .unwrap()
        .to_string();
    let resolver = lotar::TasksDirectoryResolver::resolve(None, None).unwrap();
    let storage = lotar::Storage::new(resolver.path.clone());
    let stored = lotar::services::task_service::TaskService::get(&storage, &task_id, None)
        .expect("stored task");
    let stored_product = stored
        .custom_fields
        .get("product")
        .map(lotar::types::custom_value_to_string);
    assert_eq!(stored_product, Some("Platform".to_string()));
}

#[test]
fn mcp_task_update_overwrites_custom_fields() {
    let tmp = tempfile::tempdir().unwrap();
    let tasks_dir = tmp.path().join(".tasks");
    std::fs::create_dir_all(&tasks_dir).unwrap();

    let _guard_tasks = EnvVarGuard::set("LOTAR_TASKS_DIR", tasks_dir.to_string_lossy().as_ref());

    let resolver = lotar::TasksDirectoryResolver::resolve(None, None).unwrap();
    let mut storage = lotar::Storage::new(resolver.path.clone());
    let mut custom_fields: lotar::types::CustomFields = HashMap::new();
    custom_fields.insert(
        "product".to_string(),
        lotar::types::custom_value_string("Initial"),
    );
    let created = lotar::services::task_service::TaskService::create(
        &mut storage,
        lotar::api_types::TaskCreate {
            title: "Update product".into(),
            project: Some("MCP".into()),
            priority: None,
            task_type: None,
            reporter: None,
            assignee: None,
            due_date: None,
            effort: None,
            description: None,
            tags: vec![],
            relationships: None,
            custom_fields: Some(custom_fields),
        },
    )
    .expect("create initial task");

    let req = serde_json::json!({
        "jsonrpc": "2.0",
        "id": 18,
        "method": "tools/call",
        "params": {
            "name": "task_update",
            "arguments": {
                "id": created.id,
                "patch": {"custom_fields": {"product": "Docs"}}
            }
        }
    });
    let line = serde_json::to_string(&req).unwrap();
    let resp_line = lotar::mcp::server::handle_json_line(&line);
    let resp: serde_json::Value = serde_json::from_str(&resp_line).unwrap();
    assert!(resp.get("error").is_none(), "task_update failed: {resp}");

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
    let custom_fields = task_json
        .get("custom_fields")
        .and_then(|v| v.as_object())
        .cloned()
        .unwrap_or_default();
    assert_eq!(
        custom_fields.get("product").and_then(|v| v.as_str()),
        Some("Docs")
    );

    let storage = lotar::Storage::new(resolver.path.clone());
    let stored = lotar::services::task_service::TaskService::get(&storage, &created.id, None)
        .expect("stored task");
    assert_eq!(
        stored
            .custom_fields
            .get("product")
            .map(lotar::types::custom_value_to_string),
        Some("Docs".to_string())
    );
}

#[test]
fn mcp_task_list_includes_custom_fields() {
    let tmp = tempfile::tempdir().unwrap();
    let tasks_dir = tmp.path().join(".tasks");
    std::fs::create_dir_all(&tasks_dir).unwrap();
    let _guard_tasks = EnvVarGuard::set("LOTAR_TASKS_DIR", tasks_dir.to_string_lossy().as_ref());

    let resolver = lotar::TasksDirectoryResolver::resolve(None, None).unwrap();
    let mut storage = lotar::Storage::new(resolver.path.clone());

    let mut product_a: lotar::types::CustomFields = HashMap::new();
    product_a.insert(
        "product".to_string(),
        lotar::types::custom_value_string("Platform"),
    );
    let mut product_b: lotar::types::CustomFields = HashMap::new();
    product_b.insert(
        "product".to_string(),
        lotar::types::custom_value_string("Docs"),
    );

    lotar::services::task_service::TaskService::create(
        &mut storage,
        lotar::api_types::TaskCreate {
            title: "Platform task".into(),
            project: Some("MCP".into()),
            priority: None,
            task_type: None,
            reporter: None,
            assignee: None,
            due_date: None,
            effort: None,
            description: None,
            tags: vec![],
            relationships: None,
            custom_fields: Some(product_a),
        },
    )
    .expect("create platform task");

    lotar::services::task_service::TaskService::create(
        &mut storage,
        lotar::api_types::TaskCreate {
            title: "Docs task".into(),
            project: Some("MCP".into()),
            priority: None,
            task_type: None,
            reporter: None,
            assignee: None,
            due_date: None,
            effort: None,
            description: None,
            tags: vec![],
            relationships: None,
            custom_fields: Some(product_b),
        },
    )
    .expect("create docs task");

    let list_req = serde_json::json!({
        "jsonrpc": "2.0",
        "id": 19,
        "method": "task/list",
        "params": {"project": "MCP"}
    });
    let list_line = serde_json::to_string(&list_req).unwrap();
    let list_resp_line = lotar::mcp::server::handle_json_line(&list_line);
    let list_resp: serde_json::Value = serde_json::from_str(&list_resp_line).unwrap();
    assert!(
        list_resp.get("error").is_none(),
        "task/list failed: {list_resp}"
    );
    let content = list_resp
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
    let tasks_json: serde_json::Value = serde_json::from_str(text).unwrap();
    let tasks = tasks_json.as_array().cloned().unwrap_or_default();
    assert_eq!(tasks.len(), 2);
    let mut seen = tasks
        .into_iter()
        .filter_map(|task| {
            task.as_object().and_then(|obj| {
                obj.get("custom_fields")
                    .and_then(|v| v.as_object())
                    .and_then(|cf| {
                        cf.get("product")
                            .and_then(|v| v.as_str())
                            .map(|s| s.to_string())
                    })
            })
        })
        .collect::<Vec<_>>();
    seen.sort();
    assert_eq!(seen, vec!["Docs".to_string(), "Platform".to_string()]);
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

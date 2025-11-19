mod common;
use crate::common::env_mutex::EnvVarGuard;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

struct CwdGuard(PathBuf);

impl CwdGuard {
    fn enter(target: &Path) -> std::io::Result<Self> {
        let original = std::env::current_dir()?;
        std::env::set_current_dir(target)?;
        Ok(CwdGuard(original))
    }
}

impl Drop for CwdGuard {
    fn drop(&mut self) {
        let _ = std::env::set_current_dir(&self.0);
    }
}

fn task_from_payload(payload: &serde_json::Value) -> serde_json::Value {
    payload
        .get("task")
        .cloned()
        .unwrap_or_else(|| payload.clone())
}

fn task_from_text(text: &str) -> serde_json::Value {
    let payload = serde_json::from_str::<serde_json::Value>(text).expect("valid task payload");
    task_from_payload(&payload)
}

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
fn mcp_initialize_advertises_list_changed_capability() {
    let req = serde_json::json!({
        "jsonrpc": "2.0",
        "id": 42,
        "method": "initialize",
        "params": {}
    });
    let line = serde_json::to_string(&req).unwrap();
    let resp_line = lotar::mcp::server::handle_json_line(&line);
    let resp: serde_json::Value = serde_json::from_str(&resp_line).unwrap();
    assert!(resp.get("error").is_none(), "initialize failed: {resp}");
    let capabilities = resp
        .get("result")
        .and_then(|r| r.get("capabilities"))
        .cloned()
        .expect("capabilities present");
    let list_changed = capabilities
        .get("tools")
        .and_then(|tools| tools.get("listChanged"))
        .and_then(|flag| flag.as_bool());
    assert_eq!(list_changed, Some(true));
}

#[test]
fn mcp_schema_discover_lists_tools() {
    let req = serde_json::json!({
        "jsonrpc": "2.0",
        "id": 43,
        "method": "schema/discover",
        "params": {}
    });
    let line = serde_json::to_string(&req).unwrap();
    let resp_line = lotar::mcp::server::handle_json_line(&line);
    let resp: serde_json::Value = serde_json::from_str(&resp_line).unwrap();
    assert!(
        resp.get("error").is_none(),
        "schema/discover failed: {resp}"
    );
    let payload = resp
        .get("result")
        .and_then(|r| r.get("content"))
        .and_then(|v| v.as_array())
        .and_then(|arr| arr.first())
        .and_then(|entry| entry.get("text"))
        .and_then(|v| v.as_str())
        .map(|text| serde_json::from_str::<serde_json::Value>(text).unwrap())
        .unwrap();
    assert!(
        payload
            .get("toolCount")
            .and_then(|v| v.as_u64())
            .unwrap_or(0)
            > 0
    );
    let filtered_req = serde_json::json!({
        "jsonrpc": "2.0",
        "id": 44,
        "method": "schema/discover",
        "params": {"tool": "task_create"}
    });
    let filtered_line = serde_json::to_string(&filtered_req).unwrap();
    let filtered_resp_line = lotar::mcp::server::handle_json_line(&filtered_line);
    let filtered_resp: serde_json::Value = serde_json::from_str(&filtered_resp_line).unwrap();
    assert!(
        filtered_resp.get("error").is_none(),
        "filtered schema/discover failed: {filtered_resp}"
    );
    let filtered_payload = filtered_resp
        .get("result")
        .and_then(|r| r.get("content"))
        .and_then(|v| v.as_array())
        .and_then(|arr| arr.first())
        .and_then(|entry| entry.get("text"))
        .and_then(|v| v.as_str())
        .map(|text| serde_json::from_str::<serde_json::Value>(text).unwrap())
        .unwrap();
    assert_eq!(
        filtered_payload.get("toolCount").and_then(|v| v.as_u64()),
        Some(1)
    );
    let names: Vec<String> = filtered_payload
        .get("tools")
        .and_then(|v| v.as_array())
        .unwrap()
        .iter()
        .filter_map(|tool| {
            tool.get("name")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string())
        })
        .collect();
    assert_eq!(names, vec!["task_create".to_string()]);
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
    let task_json = task_from_text(text);
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
    let task_json = task_from_text(text);
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
            custom_fields: Some(custom_fields),
            ..lotar::api_types::TaskCreate::default()
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
            custom_fields: Some(product_a),
            ..lotar::api_types::TaskCreate::default()
        },
    )
    .expect("create platform task");

    lotar::services::task_service::TaskService::create(
        &mut storage,
        lotar::api_types::TaskCreate {
            title: "Docs task".into(),
            project: Some("MCP".into()),
            custom_fields: Some(product_b),
            ..lotar::api_types::TaskCreate::default()
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
    let payload: serde_json::Value = serde_json::from_str(text).unwrap();
    let tasks = payload
        .get("tasks")
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();
    assert_eq!(
        payload.get("hasMore").and_then(|v| v.as_bool()),
        Some(false)
    );
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

#[test]
fn mcp_task_list_filters_by_custom_fields() {
    let tmp = tempfile::tempdir().unwrap();
    let tasks_dir = tmp.path().join(".tasks");
    std::fs::create_dir_all(&tasks_dir).unwrap();
    let _guard_tasks = EnvVarGuard::set("LOTAR_TASKS_DIR", tasks_dir.to_string_lossy().as_ref());

    let resolver = lotar::TasksDirectoryResolver::resolve(None, None).unwrap();
    let mut storage = lotar::Storage::new(resolver.path.clone());

    for (title, product) in [("Platform", "Platform"), ("Docs", "Docs")] {
        let mut fields: lotar::types::CustomFields = HashMap::new();
        fields.insert(
            "product".to_string(),
            lotar::types::custom_value_string(product),
        );
        lotar::services::task_service::TaskService::create(
            &mut storage,
            lotar::api_types::TaskCreate {
                title: format!("{title} task"),
                project: Some("MCP".into()),
                custom_fields: Some(fields),
                ..lotar::api_types::TaskCreate::default()
            },
        )
        .expect("create task");
    }

    let list_req = serde_json::json!({
        "jsonrpc": "2.0",
        "id": 191,
        "method": "task/list",
        "params": {
            "project": "MCP",
            "custom_fields": {"product": "Docs"}
        }
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
    let payload: serde_json::Value = serde_json::from_str(text).unwrap();
    let tasks = payload
        .get("tasks")
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();
    assert_eq!(tasks.len(), 1, "expected only docs task in response");
    let product = tasks[0]
        .get("custom_fields")
        .and_then(|v| v.as_object())
        .and_then(|cf| cf.get("product"))
        .and_then(|v| v.as_str())
        .unwrap_or("");
    assert_eq!(product, "Docs");
}

#[test]
fn mcp_task_list_supports_pagination() {
    let tmp = tempfile::tempdir().unwrap();
    let tasks_dir = tmp.path().join(".tasks");
    std::fs::create_dir_all(&tasks_dir).unwrap();
    let _guard_tasks = EnvVarGuard::set("LOTAR_TASKS_DIR", tasks_dir.to_string_lossy().as_ref());

    let resolver = lotar::TasksDirectoryResolver::resolve(None, None).unwrap();
    let mut storage = lotar::Storage::new(resolver.path.clone());
    for idx in 0..3 {
        lotar::services::task_service::TaskService::create(
            &mut storage,
            lotar::api_types::TaskCreate {
                title: format!("Paginate {idx}"),
                project: Some("MCP".into()),
                ..lotar::api_types::TaskCreate::default()
            },
        )
        .expect("create task");
    }

    let first_req = serde_json::json!({
        "jsonrpc": "2.0",
        "id": 201,
        "method": "task/list",
        "params": {"project": "MCP", "limit": 2}
    });
    let first_resp_line =
        lotar::mcp::server::handle_json_line(&serde_json::to_string(&first_req).unwrap());
    let first_resp: serde_json::Value = serde_json::from_str(&first_resp_line).unwrap();
    assert!(
        first_resp.get("error").is_none(),
        "first page failed: {first_resp}"
    );
    let first_payload = first_resp
        .get("result")
        .and_then(|r| r.get("content"))
        .and_then(|v| v.as_array())
        .and_then(|arr| arr.first())
        .and_then(|entry| entry.get("text"))
        .and_then(|v| v.as_str())
        .map(|text| serde_json::from_str::<serde_json::Value>(text).unwrap())
        .unwrap();
    assert_eq!(first_payload.get("count").and_then(|v| v.as_u64()), Some(2));
    assert_eq!(
        first_payload.get("hasMore").and_then(|v| v.as_bool()),
        Some(true)
    );
    let cursor = first_payload
        .get("nextCursor")
        .and_then(|v| v.as_str())
        .expect("next cursor present")
        .to_string();

    let second_req = serde_json::json!({
        "jsonrpc": "2.0",
        "id": 202,
        "method": "task/list",
        "params": {"project": "MCP", "cursor": cursor, "limit": 2}
    });
    let second_resp_line =
        lotar::mcp::server::handle_json_line(&serde_json::to_string(&second_req).unwrap());
    let second_resp: serde_json::Value = serde_json::from_str(&second_resp_line).unwrap();
    assert!(
        second_resp.get("error").is_none(),
        "second page failed: {second_resp}"
    );
    let second_payload = second_resp
        .get("result")
        .and_then(|r| r.get("content"))
        .and_then(|v| v.as_array())
        .and_then(|arr| arr.first())
        .and_then(|entry| entry.get("text"))
        .and_then(|v| v.as_str())
        .map(|text| serde_json::from_str::<serde_json::Value>(text).unwrap())
        .unwrap();
    assert_eq!(
        second_payload.get("count").and_then(|v| v.as_u64()),
        Some(1)
    );
    assert_eq!(
        second_payload.get("hasMore").and_then(|v| v.as_bool()),
        Some(false)
    );
}

#[test]
fn mcp_task_create_and_list_resolve_me_aliases() {
    let tmp = tempfile::tempdir().unwrap();
    let tasks_dir = tmp.path().join(".tasks");
    std::fs::create_dir_all(&tasks_dir).unwrap();
    let _guard_tasks = EnvVarGuard::set("LOTAR_TASKS_DIR", tasks_dir.to_string_lossy().as_ref());
    let _guard_user = EnvVarGuard::set("USER", "mcp-user");

    lotar::utils::identity::invalidate_identity_cache(Some(tasks_dir.as_path()));
    lotar::utils::identity::invalidate_identity_explain_cache();

    let create_req = serde_json::json!({
        "jsonrpc": "2.0",
        "id": 27,
        "method": "tools/call",
        "params": {
            "name": "task_create",
            "arguments": {
                "title": "Alias parity",
                "project": "MCP",
                "assignee": "@me",
                "reporter": "@me"
            }
        }
    });
    let create_line = serde_json::to_string(&create_req).unwrap();
    let create_resp_line = lotar::mcp::server::handle_json_line(&create_line);
    let create_resp: serde_json::Value = serde_json::from_str(&create_resp_line).unwrap();
    assert!(
        create_resp.get("error").is_none(),
        "task_create failed: {create_resp}"
    );
    let create_content = create_resp
        .get("result")
        .and_then(|r| r.get("content"))
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();
    assert!(!create_content.is_empty());
    let create_text = create_content[0]
        .get("text")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    let created_task = task_from_text(create_text);
    let created_id = created_task
        .get("id")
        .and_then(|v| v.as_str())
        .expect("task id present")
        .to_string();
    assert_eq!(
        created_task.get("assignee").and_then(|v| v.as_str()),
        Some("mcp-user")
    );
    assert_eq!(
        created_task.get("reporter").and_then(|v| v.as_str()),
        Some("mcp-user")
    );

    let list_req = serde_json::json!({
        "jsonrpc": "2.0",
        "id": 28,
        "method": "task/list",
        "params": {"project": "MCP", "assignee": "@me"}
    });
    let list_line = serde_json::to_string(&list_req).unwrap();
    let list_resp_line = lotar::mcp::server::handle_json_line(&list_line);
    let list_resp: serde_json::Value = serde_json::from_str(&list_resp_line).unwrap();
    assert!(
        list_resp.get("error").is_none(),
        "task/list failed: {list_resp}"
    );
    let list_content = list_resp
        .get("result")
        .and_then(|r| r.get("content"))
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();
    assert!(!list_content.is_empty());
    let list_text = list_content[0]
        .get("text")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    let payload: serde_json::Value = serde_json::from_str(list_text).unwrap();
    let tasks = payload
        .get("tasks")
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();
    assert_eq!(tasks.len(), 1, "expected single task filtered by @me");
    assert_eq!(
        tasks[0]
            .get("id")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string()),
        Some(created_id)
    );
}

#[test]
fn mcp_sprint_tools_assign_and_backlog() {
    let tmp = tempfile::tempdir().unwrap();
    let tasks_dir = tmp.path().join(".tasks");
    std::fs::create_dir_all(&tasks_dir).unwrap();
    let _guard_tasks = EnvVarGuard::set("LOTAR_TASKS_DIR", tasks_dir.to_string_lossy().as_ref());

    std::fs::write(
        lotar::utils::paths::global_config_path(&tasks_dir),
        "default.project: MCP\nissue.states: [Todo, InProgress, Done]\nissue.types: [Feature, Bug]\nissue.priorities: [Low, Medium, High]\n",
    )
    .unwrap();

    let mut storage = lotar::storage::manager::Storage::new(tasks_dir.clone());
    let mut sprint = lotar::storage::sprint::Sprint::default();
    sprint.plan = Some(lotar::storage::sprint::SprintPlan {
        label: Some("Iteration".into()),
        ..Default::default()
    });
    sprint.actual = Some(lotar::storage::sprint::SprintActual {
        started_at: Some(chrono::Utc::now().to_rfc3339()),
        ..Default::default()
    });
    let created =
        lotar::services::sprint_service::SprintService::create(&mut storage, sprint, None)
            .expect("create sprint");
    let sprint_id = created.record.id;

    let task_a = lotar::services::task_service::TaskService::create(
        &mut storage,
        lotar::api_types::TaskCreate {
            title: "MCP Sprint A".into(),
            project: Some("MCP".into()),
            ..lotar::api_types::TaskCreate::default()
        },
    )
    .expect("task a");
    let task_b = lotar::services::task_service::TaskService::create(
        &mut storage,
        lotar::api_types::TaskCreate {
            title: "MCP Sprint B".into(),
            project: Some("MCP".into()),
            ..lotar::api_types::TaskCreate::default()
        },
    )
    .expect("task b");
    let task_c = lotar::services::task_service::TaskService::create(
        &mut storage,
        lotar::api_types::TaskCreate {
            title: "MCP Backlog".into(),
            project: Some("MCP".into()),
            ..lotar::api_types::TaskCreate::default()
        },
    )
    .expect("task c");
    drop(storage);

    let add_req = serde_json::json!({
        "jsonrpc": "2.0",
        "id": 101,
        "method": "tools/call",
        "params": {
            "name": "sprint_add",
            "arguments": {
                "sprint": sprint_id,
                "tasks": [task_a.id.clone(), task_b.id.clone()]
            }
        }
    });
    let add_line = serde_json::to_string(&add_req).unwrap();
    let add_resp_line = lotar::mcp::server::handle_json_line(&add_line);
    let add_resp: serde_json::Value = serde_json::from_str(&add_resp_line).unwrap();
    assert!(
        add_resp.get("error").is_none(),
        "sprint_add failed: {add_resp:?}"
    );
    let add_payload = add_resp
        .get("result")
        .and_then(|r| r.get("content"))
        .and_then(|v| v.as_array())
        .and_then(|arr| arr.first())
        .and_then(|entry| entry.get("text"))
        .and_then(|v| v.as_str())
        .map(|text| serde_json::from_str::<serde_json::Value>(text).unwrap())
        .unwrap();
    let modified = add_payload
        .get("modified")
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();
    assert_eq!(modified.len(), 2);

    let backlog_req = serde_json::json!({
        "jsonrpc": "2.0",
        "id": 102,
        "method": "tools/call",
        "params": {
            "name": "sprint_backlog",
            "arguments": {"project": "MCP"}
        }
    });
    let backlog_line = serde_json::to_string(&backlog_req).unwrap();
    let backlog_resp_line = lotar::mcp::server::handle_json_line(&backlog_line);
    let backlog_resp: serde_json::Value = serde_json::from_str(&backlog_resp_line).unwrap();
    assert!(
        backlog_resp.get("error").is_none(),
        "sprint_backlog failed: {backlog_resp:?}"
    );
    let backlog_payload = backlog_resp
        .get("result")
        .and_then(|r| r.get("content"))
        .and_then(|v| v.as_array())
        .and_then(|arr| arr.first())
        .and_then(|entry| entry.get("text"))
        .and_then(|v| v.as_str())
        .map(|text| serde_json::from_str::<serde_json::Value>(text).unwrap())
        .unwrap();
    assert_eq!(
        backlog_payload.get("cursor").and_then(|v| v.as_u64()),
        Some(0)
    );
    assert_eq!(
        backlog_payload.get("hasMore").and_then(|v| v.as_bool()),
        Some(false)
    );
    let backlog_tasks = backlog_payload
        .get("tasks")
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();
    assert_eq!(backlog_tasks.len(), 1);
    assert_eq!(
        backlog_tasks[0].get("id").and_then(|v| v.as_str()),
        Some(task_c.id.as_str())
    );

    let remove_req = serde_json::json!({
        "jsonrpc": "2.0",
        "id": 103,
        "method": "tools/call",
        "params": {
            "name": "sprint_remove",
            "arguments": {
                "sprint": sprint_id,
                "tasks": [task_a.id.clone()]
            }
        }
    });
    let remove_line = serde_json::to_string(&remove_req).unwrap();
    let remove_resp_line = lotar::mcp::server::handle_json_line(&remove_line);
    let remove_resp: serde_json::Value = serde_json::from_str(&remove_resp_line).unwrap();
    assert!(
        remove_resp.get("error").is_none(),
        "sprint_remove failed: {remove_resp:?}"
    );

    let storage = lotar::storage::manager::Storage::new(tasks_dir.clone());
    let task_a_post = lotar::services::task_service::TaskService::get(&storage, &task_a.id, None)
        .expect("task a post remove");
    let task_b_post = lotar::services::task_service::TaskService::get(&storage, &task_b.id, None)
        .expect("task b post add");
    assert!(task_a_post.sprints.is_empty());
    assert_eq!(task_b_post.sprints, vec![sprint_id]);
}

#[test]
fn mcp_sprint_backlog_reports_missing_integrity_and_cleanup() {
    let tmp = tempfile::tempdir().unwrap();
    let tasks_dir = tmp.path().join(".tasks");
    std::fs::create_dir_all(&tasks_dir).unwrap();
    let _guard_tasks = EnvVarGuard::set("LOTAR_TASKS_DIR", tasks_dir.to_string_lossy().as_ref());

    let resolver = lotar::TasksDirectoryResolver::resolve(None, None).unwrap();
    let mut storage = lotar::Storage::new(resolver.path.clone());
    let ghost = lotar::services::task_service::TaskService::create(
        &mut storage,
        lotar::api_types::TaskCreate {
            title: "Ghost sprint member".into(),
            project: Some("MCP".into()),
            ..lotar::api_types::TaskCreate::default()
        },
    )
    .expect("create ghost task");
    let project_prefix = ghost.id.split('-').next().unwrap_or_default().to_string();
    let mut ghost_record = storage
        .get(&ghost.id, project_prefix.clone())
        .expect("ghost record on disk");
    ghost_record.sprints = vec![42];
    storage
        .edit(&ghost.id, &ghost_record)
        .expect("failed to persist ghost task");
    drop(storage);

    let backlog_req = serde_json::json!({
        "jsonrpc": "2.0",
        "id": 140,
        "method": "tools/call",
        "params": {"name": "sprint_backlog", "arguments": {}}
    });
    let backlog_line = serde_json::to_string(&backlog_req).unwrap();
    let backlog_resp_line = lotar::mcp::server::handle_json_line(&backlog_line);
    let backlog_resp: serde_json::Value = serde_json::from_str(&backlog_resp_line).unwrap();
    assert!(
        backlog_resp.get("error").is_none(),
        "sprint_backlog failed: {backlog_resp:?}"
    );
    let backlog_payload = backlog_resp
        .get("result")
        .and_then(|r| r.get("content"))
        .and_then(|v| v.as_array())
        .and_then(|arr| arr.first())
        .and_then(|entry| entry.get("text"))
        .and_then(|v| v.as_str())
        .map(|text| serde_json::from_str::<serde_json::Value>(text).unwrap())
        .unwrap();
    let missing_initial = backlog_payload
        .get("missing_sprints")
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();
    assert_eq!(
        missing_initial
            .iter()
            .map(|v| v.as_u64().unwrap())
            .collect::<Vec<_>>(),
        vec![42]
    );
    let integrity_initial = backlog_payload
        .get("integrity")
        .and_then(|v| v.as_object())
        .expect("integrity diagnostics present");
    let integrity_missing = integrity_initial
        .get("missing_sprints")
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();
    assert_eq!(
        integrity_missing
            .iter()
            .map(|v| v.as_u64().unwrap())
            .collect::<Vec<_>>(),
        vec![42]
    );
    assert_eq!(
        integrity_initial
            .get("tasks_with_missing")
            .and_then(|v| v.as_u64()),
        Some(1)
    );
    assert!(integrity_initial.get("auto_cleanup").is_none());

    let storage = lotar::Storage::new(tasks_dir.clone());
    let ghost_before = storage
        .get(&ghost.id, project_prefix.clone())
        .expect("ghost before cleanup");
    assert_eq!(ghost_before.sprints, vec![42]);
    drop(storage);

    let cleanup_req = serde_json::json!({
        "jsonrpc": "2.0",
        "id": 141,
        "method": "tools/call",
        "params": {"name": "sprint_backlog", "arguments": {"cleanup_missing": true}}
    });
    let cleanup_line = serde_json::to_string(&cleanup_req).unwrap();
    let cleanup_resp_line = lotar::mcp::server::handle_json_line(&cleanup_line);
    let cleanup_resp: serde_json::Value = serde_json::from_str(&cleanup_resp_line).unwrap();
    assert!(
        cleanup_resp.get("error").is_none(),
        "sprint_backlog cleanup failed: {cleanup_resp:?}"
    );
    let cleanup_payload = cleanup_resp
        .get("result")
        .and_then(|r| r.get("content"))
        .and_then(|v| v.as_array())
        .and_then(|arr| arr.first())
        .and_then(|entry| entry.get("text"))
        .and_then(|v| v.as_str())
        .map(|text| serde_json::from_str::<serde_json::Value>(text).unwrap())
        .unwrap();
    let missing_after = cleanup_payload
        .get("missing_sprints")
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();
    assert!(missing_after.is_empty());
    let cleanup_integrity = cleanup_payload
        .get("integrity")
        .and_then(|v| v.as_object())
        .expect("cleanup integrity diagnostics present");
    let auto_cleanup = cleanup_integrity
        .get("auto_cleanup")
        .and_then(|v| v.as_object())
        .expect("auto cleanup summary present");
    assert_eq!(
        auto_cleanup
            .get("removed_references")
            .and_then(|v| v.as_u64()),
        Some(1)
    );
    assert_eq!(
        auto_cleanup.get("updated_tasks").and_then(|v| v.as_u64()),
        Some(1)
    );

    let storage = lotar::Storage::new(tasks_dir);
    let ghost_after = storage
        .get(&ghost.id, project_prefix)
        .expect("ghost after cleanup");
    assert!(ghost_after.sprints.is_empty());
}

#[test]
fn mcp_sprint_backlog_supports_pagination() {
    let tmp = tempfile::tempdir().unwrap();
    let tasks_dir = tmp.path().join(".tasks");
    std::fs::create_dir_all(&tasks_dir).unwrap();
    let _guard_tasks = EnvVarGuard::set("LOTAR_TASKS_DIR", tasks_dir.to_string_lossy().as_ref());

    let resolver = lotar::TasksDirectoryResolver::resolve(None, None).unwrap();
    let mut storage = lotar::Storage::new(resolver.path.clone());
    for idx in 0..3 {
        lotar::services::task_service::TaskService::create(
            &mut storage,
            lotar::api_types::TaskCreate {
                title: format!("Backlog {idx}"),
                project: Some("MCP".into()),
                ..lotar::api_types::TaskCreate::default()
            },
        )
        .expect("create task");
    }
    drop(storage);

    let first_req = serde_json::json!({
        "jsonrpc": "2.0",
        "id": 301,
        "method": "tools/call",
        "params": {
            "name": "sprint_backlog",
            "arguments": {"project": "MCP", "limit": 1}
        }
    });
    let first_resp_line =
        lotar::mcp::server::handle_json_line(&serde_json::to_string(&first_req).unwrap());
    let first_resp: serde_json::Value = serde_json::from_str(&first_resp_line).unwrap();
    assert!(
        first_resp.get("error").is_none(),
        "first backlog page failed: {first_resp}"
    );
    let first_payload = first_resp
        .get("result")
        .and_then(|r| r.get("content"))
        .and_then(|v| v.as_array())
        .and_then(|arr| arr.first())
        .and_then(|entry| entry.get("text"))
        .and_then(|v| v.as_str())
        .map(|text| serde_json::from_str::<serde_json::Value>(text).unwrap())
        .unwrap();
    assert_eq!(first_payload.get("count").and_then(|v| v.as_u64()), Some(1));
    assert_eq!(
        first_payload.get("hasMore").and_then(|v| v.as_bool()),
        Some(true)
    );
    let cursor = first_payload
        .get("nextCursor")
        .and_then(|v| v.as_str())
        .expect("next cursor present")
        .to_string();

    let second_req = serde_json::json!({
        "jsonrpc": "2.0",
        "id": 302,
        "method": "tools/call",
        "params": {
            "name": "sprint_backlog",
            "arguments": {"project": "MCP", "limit": 2, "cursor": cursor}
        }
    });
    let second_resp_line =
        lotar::mcp::server::handle_json_line(&serde_json::to_string(&second_req).unwrap());
    let second_resp: serde_json::Value = serde_json::from_str(&second_resp_line).unwrap();
    assert!(
        second_resp.get("error").is_none(),
        "second backlog page failed: {second_resp}"
    );
    let second_payload = second_resp
        .get("result")
        .and_then(|r| r.get("content"))
        .and_then(|v| v.as_array())
        .and_then(|arr| arr.first())
        .and_then(|entry| entry.get("text"))
        .and_then(|v| v.as_str())
        .map(|text| serde_json::from_str::<serde_json::Value>(text).unwrap())
        .unwrap();
    assert_eq!(
        second_payload.get("count").and_then(|v| v.as_u64()),
        Some(2)
    );
    assert_eq!(
        second_payload.get("hasMore").and_then(|v| v.as_bool()),
        Some(false)
    );
}

#[test]
fn mcp_task_create_honors_default_assignee() {
    let tmp = tempfile::tempdir().unwrap();
    let tasks_dir = tmp.path().join(".tasks");
    std::fs::create_dir_all(&tasks_dir).unwrap();
    let _guard_tasks = EnvVarGuard::set("LOTAR_TASKS_DIR", tasks_dir.to_string_lossy().as_ref());

    std::fs::write(
        tasks_dir.join("config.yml"),
        "default.project: MCP\nissue.states: [Todo, InProgress, Done]\nissue.types: [Feature, Bug, Chore]\nissue.priorities: [Low, Medium, High]\ndefault.assignee: default-user@example.com\n",
    )
    .unwrap();

    let create_req = serde_json::json!({
        "jsonrpc": "2.0",
        "id": 29,
        "method": "tools/call",
        "params": {
            "name": "task_create",
            "arguments": {
                "title": "Default assignee",
                "project": "MCP"
            }
        }
    });
    let create_line = serde_json::to_string(&create_req).unwrap();
    let create_resp_line = lotar::mcp::server::handle_json_line(&create_line);
    let create_resp: serde_json::Value = serde_json::from_str(&create_resp_line).unwrap();
    assert!(
        create_resp.get("error").is_none(),
        "task_create failed: {create_resp}"
    );
    let content = create_resp
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
    let task_json = task_from_text(text);
    assert_eq!(
        task_json.get("assignee").and_then(|v| v.as_str()),
        Some("default-user@example.com")
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
    assert_eq!(stored.assignee.as_deref(), Some("default-user@example.com"));
}

#[test]
fn mcp_task_create_infers_branch_defaults() {
    let tmp = tempfile::tempdir().unwrap();
    let repo_root = tmp.path();
    let tasks_dir = repo_root.join(".tasks");
    std::fs::create_dir_all(&tasks_dir).unwrap();
    let git_dir = repo_root.join(".git");
    std::fs::create_dir_all(&git_dir).unwrap();
    std::fs::write(git_dir.join("HEAD"), "ref: refs/heads/feat/new-ui\n").unwrap();
    let _guard_tasks = EnvVarGuard::set("LOTAR_TASKS_DIR", tasks_dir.to_string_lossy().as_ref());

    std::fs::write(
        tasks_dir.join("config.yml"),
        "default.project: MCP\nissue.states: [Todo, InProgress, Done]\nissue.types: [Feature, Bug, Chore]\nissue.priorities: [Low, Medium, High]\nauto.branch_infer_priority: true\nauto.branch_infer_status: true\nauto.branch_infer_type: true\nbranch.priority_aliases:\n  feat: High\nbranch.status_aliases:\n  feat: InProgress\n",
    )
    .unwrap();

    let _cwd = CwdGuard::enter(repo_root).unwrap();

    let create_req = serde_json::json!({
        "jsonrpc": "2.0",
        "id": 30,
        "method": "tools/call",
        "params": {
            "name": "task_create",
            "arguments": {
                "title": "Branch inferred",
                "project": "MCP"
            }
        }
    });
    let create_line = serde_json::to_string(&create_req).unwrap();
    let create_resp_line = lotar::mcp::server::handle_json_line(&create_line);
    let create_resp: serde_json::Value = serde_json::from_str(&create_resp_line).unwrap();
    assert!(
        create_resp.get("error").is_none(),
        "task_create failed: {create_resp}"
    );
    let content = create_resp
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
    let task_json = task_from_text(text);
    assert_eq!(
        task_json.get("priority").and_then(|v| v.as_str()),
        Some("High")
    );
    assert_eq!(
        task_json.get("status").and_then(|v| v.as_str()),
        Some("InProgress")
    );
    assert_eq!(
        task_json.get("task_type").and_then(|v| v.as_str()),
        Some("Feature")
    );
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
    let task_json = task_from_text(text);
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
            ..lotar::api_types::TaskCreate::default()
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

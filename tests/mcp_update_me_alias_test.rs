mod common;
use crate::common::env_mutex::lock_var;

#[test]
fn mcp_task_update_resolves_me_in_patch() {
    let _env = lock_var("LOTAR_TASKS_DIR");
    let tmp = tempfile::tempdir().unwrap();
    let tasks_dir = tmp.path().join(".tasks");
    std::fs::create_dir_all(&tasks_dir).unwrap();

    // Configure identity so @me resolves deterministically
    std::fs::write(
        lotar::utils::paths::global_config_path(&tasks_dir),
        "default_project: MCP\nissue_states: [Todo, InProgress, Done]\nissue_types: [Feature, Bug, Chore]\nissue_priorities: [Low, Medium, High]\ndefault_reporter: dave\n",
    )
    .unwrap();

    unsafe {
        std::env::set_var("LOTAR_TASKS_DIR", &tasks_dir);
    }

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

    unsafe {
        std::env::remove_var("LOTAR_TASKS_DIR");
    }
}

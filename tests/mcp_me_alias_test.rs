mod common;
use crate::common::env_mutex::lock_var;
use lotar::utils::paths;

#[test]
fn mcp_task_create_resolves_me_alias() {
    let _env = lock_var("LOTAR_TASKS_DIR");
    let tmp = tempfile::tempdir().unwrap();
    let tasks_dir = tmp.path().join(".tasks");
    std::fs::create_dir_all(&tasks_dir).unwrap();

    // Configure identity so @me resolves deterministically
    std::fs::write(
        paths::global_config_path(&tasks_dir),
        "default_project: MCP\nissue_states: [Todo, InProgress, Done]\nissue_types: [Feature, Bug, Chore]\nissue_priorities: [Low, Medium, High]\ndefault_reporter: carol\n",
    )
    .unwrap();

    unsafe {
        std::env::set_var("LOTAR_TASKS_DIR", &tasks_dir);
    }

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

    unsafe {
        std::env::remove_var("LOTAR_TASKS_DIR");
    }
}

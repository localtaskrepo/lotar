use super::*;
// Minimal per-variable lock for this test to avoid env races
use std::collections::HashMap;
use std::path::Path;
use std::sync::LazyLock;
use std::sync::{Mutex, MutexGuard};
static ENV_LOCKS: LazyLock<Mutex<HashMap<&'static str, &'static Mutex<()>>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));
fn lock_var(var: &'static str) -> MutexGuard<'static, ()> {
    let mtx: &'static Mutex<()> = {
        let mut map = ENV_LOCKS.lock().unwrap();
        if let Some(m) = map.get(var) {
            m
        } else {
            let boxed: Box<Mutex<()>> = Box::new(Mutex::new(()));
            let leaked: &'static Mutex<()> = Box::leak(boxed);
            map.insert(var, leaked);
            leaked
        }
    };
    mtx.lock().unwrap()
}

fn seed_single_project_config(tasks_dir: &Path) {
    std::fs::create_dir_all(tasks_dir).unwrap();
    let config_yaml = r#"
default:
    project: MCP
members:
    - alice
    - bob
issue:
    tags:
        - cli
        - infra
custom:
    fields:
        - severity
        - impact
"#;
    std::fs::write(tasks_dir.join("config.yml"), config_yaml).unwrap();
}

fn field_hint<'a>(tool: &'a Value, field: &str) -> &'a Value {
    tool.get("hints")
        .and_then(|h| h.get("fields"))
        .and_then(|fields| fields.get(field))
        .unwrap_or_else(|| panic!("missing hint for field {field}"))
}

fn field_values(tool: &Value, field: &str) -> Vec<String> {
    field_hint(tool, field)
        .get("values")
        .and_then(|values| values.as_array())
        .unwrap_or_else(|| panic!("missing values array for field {field}"))
        .iter()
        .map(|value| value.as_str().unwrap().to_string())
        .collect()
}

fn accepts_multiple(tool: &Value, field: &str) -> Option<bool> {
    field_hint(tool, field)
        .get("acceptsMultiple")
        .and_then(|flag| flag.as_bool())
}

fn tool_by_name<'a>(tools: &'a [Value], name: &str) -> &'a Value {
    tools
        .iter()
        .find(|tool| tool.get("name").and_then(|v| v.as_str()) == Some(name))
        .unwrap_or_else(|| panic!("Missing tool {}", name))
}

fn tool_description(tool: &Value) -> &str {
    tool.get("description")
        .and_then(|v| v.as_str())
        .unwrap_or_else(|| panic!("Missing description for tool"))
}

fn tool_response_payload(resp: &JsonRpcResponse) -> &Value {
    resp.result
        .as_ref()
        .unwrap_or_else(|| panic!("missing result payload"))
        .get("functionResponse")
        .and_then(|fr| fr.get("response"))
        .unwrap_or_else(|| panic!("missing functionResponse response"))
}

fn tool_content_entries(resp: &JsonRpcResponse) -> &[Value] {
    tool_response_payload(resp)
        .get("content")
        .and_then(|v| v.as_array())
        .unwrap_or_else(|| panic!("missing content array"))
}

fn first_tool_text(resp: &JsonRpcResponse) -> &str {
    tool_content_entries(resp)
        .first()
        .and_then(|entry| entry.get("text"))
        .and_then(|v| v.as_str())
        .unwrap_or_else(|| panic!("missing text entry"))
}

fn parse_tool_payload(resp: &JsonRpcResponse) -> serde_json::Value {
    serde_json::from_str(first_tool_text(resp)).expect("tool payload should be valid json")
}

#[test]
fn tools_call_reference_add_and_remove() {
    let _lock = lock_var("LOTAR_TASKS_DIR");
    let tmp = tempfile::tempdir().unwrap();
    let tasks_dir = tmp.path().join(".tasks");
    std::fs::create_dir_all(&tasks_dir).unwrap();
    seed_single_project_config(&tasks_dir);

    // Make repo root discoverable for file/code references.
    std::fs::create_dir_all(tmp.path().join(".git")).unwrap();
    std::fs::create_dir_all(tmp.path().join("src")).unwrap();
    std::fs::write(tmp.path().join("src/example.rs"), "fn main() {}\n").unwrap();

    unsafe {
        std::env::set_var("LOTAR_TASKS_DIR", &tasks_dir);
    }

    // Create a task
    let create_args = json!({
        "name": "task_create",
        "arguments": { "title": "MCP References", "project": "MCP" }
    });
    let create_req = JsonRpcRequest {
        jsonrpc: "2.0".into(),
        id: Some(json!(30)),
        method: "tools/call".into(),
        params: create_args,
    };
    let create_resp = dispatch(create_req);
    assert!(create_resp.error.is_none(), "task_create failed");
    let task_json = parse_tool_payload(&create_resp);
    let id = task_json
        .get("task")
        .and_then(|task| task.get("id"))
        .and_then(|v| v.as_str())
        .unwrap()
        .to_string();

    // Add link reference
    let add_link_args = json!({
        "name": "task_reference_add",
        "arguments": { "id": id, "kind": "link", "value": "https://example.com" }
    });
    let add_link_req = JsonRpcRequest {
        jsonrpc: "2.0".into(),
        id: Some(json!(31)),
        method: "tools/call".into(),
        params: add_link_args,
    };
    let add_link_resp = dispatch(add_link_req);
    assert!(
        add_link_resp.error.is_none(),
        "task_reference_add link failed"
    );
    let payload = parse_tool_payload(&add_link_resp);
    assert!(payload.get("changed").and_then(|v| v.as_bool()).unwrap());

    // Add file reference
    let add_file_args = json!({
        "name": "task_reference_add",
        "arguments": { "id": payload.get("id").and_then(|v| v.as_str()).unwrap(), "kind": "file", "value": "src/example.rs" }
    });
    let add_file_req = JsonRpcRequest {
        jsonrpc: "2.0".into(),
        id: Some(json!(32)),
        method: "tools/call".into(),
        params: add_file_args,
    };
    let add_file_resp = dispatch(add_file_req);
    assert!(
        add_file_resp.error.is_none(),
        "task_reference_add file failed"
    );

    // Add code reference
    let add_code_args = json!({
        "name": "task_reference_add",
        "arguments": { "id": payload.get("id").and_then(|v| v.as_str()).unwrap(), "kind": "code", "value": "src/example.rs#1" }
    });
    let add_code_req = JsonRpcRequest {
        jsonrpc: "2.0".into(),
        id: Some(json!(33)),
        method: "tools/call".into(),
        params: add_code_args,
    };
    let add_code_resp = dispatch(add_code_req);
    assert!(
        add_code_resp.error.is_none(),
        "task_reference_add code failed"
    );
    let payload = parse_tool_payload(&add_code_resp);
    let refs = payload
        .get("task")
        .and_then(|t| t.get("references"))
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();
    assert!(refs.iter().any(|r| {
        r.get("link")
            .and_then(|v| v.as_str())
            .is_some_and(|s| s == "https://example.com")
    }));
    assert!(refs.iter().any(|r| {
        r.get("file")
            .and_then(|v| v.as_str())
            .is_some_and(|s| s == "src/example.rs")
    }));
    assert!(refs.iter().any(|r| {
        r.get("code")
            .and_then(|v| v.as_str())
            .is_some_and(|s| s == "src/example.rs#1")
    }));

    // Remove code reference
    let remove_code_args = json!({
        "name": "task_reference_remove",
        "arguments": { "id": payload.get("id").and_then(|v| v.as_str()).unwrap(), "kind": "code", "value": "src/example.rs#1" }
    });
    let remove_code_req = JsonRpcRequest {
        jsonrpc: "2.0".into(),
        id: Some(json!(34)),
        method: "tools/call".into(),
        params: remove_code_args,
    };
    let remove_code_resp = dispatch(remove_code_req);
    assert!(
        remove_code_resp.error.is_none(),
        "task_reference_remove code failed"
    );
    let payload = parse_tool_payload(&remove_code_resp);
    let refs = payload
        .get("task")
        .and_then(|t| t.get("references"))
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();
    assert!(!refs.iter().any(|r| {
        r.get("code")
            .and_then(|v| v.as_str())
            .is_some_and(|s| s == "src/example.rs#1")
    }));
}

#[test]
fn tools_call_update_delete_list_and_invalid_enum() {
    let _lock = lock_var("LOTAR_TASKS_DIR");
    let tmp = tempfile::tempdir().unwrap();
    let tasks_dir = tmp.path().join(".tasks");
    std::fs::create_dir_all(&tasks_dir).unwrap();
    // Set test-specific LOTAR_TASKS_DIR
    unsafe {
        std::env::set_var("LOTAR_TASKS_DIR", &tasks_dir);
    }

    // Create a task
    let create_args = json!({
        "name": "task_create",
        "arguments": { "title": "MCP Update", "project": "MCP" }
    });
    let create_req = JsonRpcRequest {
        jsonrpc: "2.0".into(),
        id: Some(json!(10)),
        method: "tools/call".into(),
        params: create_args,
    };
    let create_resp = dispatch(create_req);
    assert!(create_resp.error.is_none(), "task_create failed");
    let task_json = parse_tool_payload(&create_resp);
    let id = task_json
        .get("task")
        .and_then(|task| task.get("id"))
        .and_then(|v| v.as_str())
        .unwrap()
        .to_string();

    // Update the task
    let update_args = json!({
        "name": "task_update",
        "arguments": { "id": id, "project": "MCP", "patch": { "title": "Updated Title" } }
    });
    let update_req = JsonRpcRequest {
        jsonrpc: "2.0".into(),
        id: Some(json!(11)),
        method: "tools/call".into(),
        params: update_args,
    };
    let update_resp = dispatch(update_req);
    if update_resp.error.is_some() {
        // Print tree to help debug
        fn print_dir_tree<P: AsRef<std::path::Path>>(path: P, indent: usize) {
            let path = path.as_ref();
            if let Ok(entries) = std::fs::read_dir(path) {
                for entry in entries.flatten() {
                    let p = entry.path();
                    let name = p.file_name().and_then(|n| n.to_str()).unwrap_or("");
                    eprintln!("{0:indent$}{1}", "", name, indent = indent);
                    if p.is_dir() {
                        print_dir_tree(&p, indent + 2);
                    }
                }
            }
        }
        eprintln!("Update failed; .tasks tree:");
        print_dir_tree(&tasks_dir, 2);
    }
    assert!(
        update_resp.error.is_none(),
        "task_update failed: {:?}",
        update_resp.error
    );
    let updated_json = parse_tool_payload(&update_resp);
    assert_eq!(updated_json.get("title").unwrap(), "Updated Title");

    // List tasks
    let list_args = json!({ "name": "task_list", "arguments": { "project": "MCP" } });
    let list_req = JsonRpcRequest {
        jsonrpc: "2.0".into(),
        id: Some(json!(12)),
        method: "tools/call".into(),
        params: list_args,
    };
    let list_resp = dispatch(list_req);
    assert!(list_resp.error.is_none(), "task_list failed");
    let payload = parse_tool_payload(&list_resp);
    assert!(payload.get("hasMore").and_then(|v| v.as_bool()).is_some());
    let listed_tasks = payload
        .get("tasks")
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();
    assert!(
        listed_tasks
            .iter()
            .any(|t| t.get("id").unwrap() == &serde_json::Value::String(id.clone()))
    );

    // Delete the task
    let delete_args = json!({ "name": "task_delete", "arguments": { "id": id, "project": "MCP" } });
    let delete_req = JsonRpcRequest {
        jsonrpc: "2.0".into(),
        id: Some(json!(13)),
        method: "tools/call".into(),
        params: delete_args,
    };
    let delete_resp = dispatch(delete_req);
    assert!(delete_resp.error.is_none(), "task_delete failed");
    let del_text = first_tool_text(&delete_resp);
    assert!(del_text.contains("deleted=true"));

    // Negative test: invalid priority
    let bad_args = json!({ "name": "task_create", "arguments": { "title": "bad", "project": "MCP", "priority": "NOT_A_PRIORITY" } });
    let bad_req = JsonRpcRequest {
        jsonrpc: "2.0".into(),
        id: Some(json!(14)),
        method: "tools/call".into(),
        params: bad_args,
    };
    let bad_resp = dispatch(bad_req);
    let error = bad_resp
        .error
        .as_ref()
        .expect("Expected error for invalid priority");
    let msg = error.message.to_lowercase();
    assert!(msg.contains("priority"));
    let data = error.data.as_ref().expect("priority error data");
    assert_eq!(data.get("field").and_then(|v| v.as_str()), Some("priority"));
    let suggestions = data
        .get("suggestions")
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();
    assert!(
        suggestions.iter().any(|val| val.as_str() == Some("Low")),
        "expected priority suggestions"
    );

    // Negative test: invalid type
    let bad_type_args = json!({ "name": "task_create", "arguments": { "title": "bad", "project": "MCP", "type": "NOT_A_TYPE" } });
    let bad_type_req = JsonRpcRequest {
        jsonrpc: "2.0".into(),
        id: Some(json!(15)),
        method: "tools/call".into(),
        params: bad_type_args,
    };
    let bad_type_resp = dispatch(bad_type_req);
    let error = bad_type_resp
        .error
        .as_ref()
        .expect("Expected error for invalid type");
    let msg = error.message.to_lowercase();
    assert!(msg.contains("type"));
    let data = error.data.as_ref().expect("type error data");
    assert_eq!(data.get("field").and_then(|v| v.as_str()), Some("type"));

    // Negative test: invalid status during task_update
    let recreate_req = JsonRpcRequest {
        jsonrpc: "2.0".into(),
        id: Some(json!(16)),
        method: "tools/call".into(),
        params: json!({
            "name": "task_create",
            "arguments": {"title": "Status repro", "project": "MCP"}
        }),
    };
    let recreate_resp = dispatch(recreate_req);
    assert!(recreate_resp.error.is_none(), "recreate task failed");
    let recreate_json = parse_tool_payload(&recreate_resp);
    let recreated_id = recreate_json
        .get("task")
        .and_then(|task| task.get("id"))
        .and_then(|v| v.as_str())
        .expect("recreated id");
    let bad_status_req = JsonRpcRequest {
        jsonrpc: "2.0".into(),
        id: Some(json!(17)),
        method: "task/update".into(),
        params: json!({"id": recreated_id, "patch": {"status": "NOT_A_STATUS"}}),
    };
    let bad_status_resp = dispatch(bad_status_req);
    let error = bad_status_resp
        .error
        .as_ref()
        .expect("Expected status validation error");
    assert!(error.message.to_ascii_lowercase().contains("status"));
    let data = error.data.as_ref().expect("status error data");
    assert_eq!(data.get("field").and_then(|v| v.as_str()), Some("status"));

    unsafe {
        std::env::remove_var("LOTAR_TASKS_DIR");
    }
}

#[test]
fn tools_list_reports_enum_hints_with_single_project() {
    let _lock = lock_var("LOTAR_TASKS_DIR");
    let tmp = tempfile::tempdir().unwrap();
    let tasks_dir = tmp.path().join(".tasks");
    std::fs::create_dir_all(&tasks_dir).unwrap();
    std::fs::create_dir_all(tasks_dir.join("MCP")).unwrap();
    let config_yaml = r#"
default:
    project: MCP
members:
    - alice
    - bob
issue:
    tags:
        - cli
        - infra
custom:
    fields:
        - severity
        - impact
"#;
    std::fs::write(tasks_dir.join("config.yml"), config_yaml).unwrap();
    unsafe {
        std::env::set_var("LOTAR_TASKS_DIR", &tasks_dir);
    }

    let req = JsonRpcRequest {
        jsonrpc: "2.0".into(),
        id: Some(json!(99)),
        method: "tools/list".into(),
        params: json!({}),
    };
    let resp = dispatch(req);
    assert!(resp.error.is_none(), "tools/list failed: {:?}", resp.error);
    let tools = resp
        .result
        .as_ref()
        .and_then(|value| value.get("tools"))
        .and_then(|value| value.as_array())
        .expect("tools array available");

    let task_create = tools
        .iter()
        .find(|tool| tool.get("name").and_then(|v| v.as_str()) == Some("task_create"))
        .expect("task_create tool present");
    assert_eq!(
        field_values(task_create, "priority"),
        vec!["Low", "Medium", "High"]
    );
    assert_eq!(
        field_values(task_create, "type"),
        vec!["Feature", "Bug", "Chore"]
    );
    assert_eq!(field_values(task_create, "project"), vec!["MCP"]);
    assert_eq!(field_values(task_create, "reporter"), vec!["alice", "bob"]);
    assert_eq!(field_values(task_create, "assignee"), vec!["alice", "bob"]);
    assert_eq!(field_values(task_create, "tags"), vec!["cli", "infra"]);
    assert_eq!(accepts_multiple(task_create, "tags"), Some(true));
    assert_eq!(
        field_values(task_create, "custom_fields"),
        vec!["severity", "impact"]
    );

    let task_list = tools
        .iter()
        .find(|tool| tool.get("name").and_then(|v| v.as_str()) == Some("task_list"))
        .expect("task_list tool present");
    assert_eq!(
        field_values(task_list, "status"),
        vec!["Todo", "InProgress", "Done"]
    );
    assert_eq!(accepts_multiple(task_list, "status"), Some(true));
    assert_eq!(field_values(task_list, "assignee"), vec!["alice", "bob"]);
    assert_eq!(field_values(task_list, "project"), vec!["MCP"]);

    let sprint_backlog = tools
        .iter()
        .find(|tool| tool.get("name").and_then(|v| v.as_str()) == Some("sprint_backlog"))
        .expect("sprint_backlog tool present");
    assert_eq!(
        field_values(sprint_backlog, "assignee"),
        vec!["alice", "bob"]
    );

    let config_show = tools
        .iter()
        .find(|tool| tool.get("name").and_then(|v| v.as_str()) == Some("config_show"))
        .expect("config_show tool present");
    assert_eq!(field_values(config_show, "project"), vec!["MCP"]);

    unsafe {
        std::env::remove_var("LOTAR_TASKS_DIR");
    }
}

#[test]
fn tools_list_skips_enum_hints_with_multiple_projects() {
    let _lock = lock_var("LOTAR_TASKS_DIR");
    let tmp = tempfile::tempdir().unwrap();
    let tasks_dir = tmp.path().join(".tasks");
    std::fs::create_dir_all(tasks_dir.join("AAA")).unwrap();
    std::fs::create_dir_all(tasks_dir.join("BBB")).unwrap();
    unsafe {
        std::env::set_var("LOTAR_TASKS_DIR", &tasks_dir);
    }

    let req = JsonRpcRequest {
        jsonrpc: "2.0".into(),
        id: Some(json!(100)),
        method: "tools/list".into(),
        params: json!({}),
    };
    let resp = dispatch(req);
    assert!(resp.error.is_none(), "tools/list failed: {:?}", resp.error);
    let tools = resp
        .result
        .as_ref()
        .and_then(|value| value.get("tools"))
        .and_then(|value| value.as_array())
        .expect("tools array available");

    let task_create = tools
        .iter()
        .find(|tool| tool.get("name").and_then(|v| v.as_str()) == Some("task_create"))
        .expect("task_create tool present");
    assert!(
        task_create.get("hints").is_none(),
        "hints should be absent for task_create when multiple projects exist"
    );

    let task_list = tools
        .iter()
        .find(|tool| tool.get("name").and_then(|v| v.as_str()) == Some("task_list"))
        .expect("task_list tool present");
    assert!(
        task_list.get("hints").is_none(),
        "task_list hints should be absent when multiple projects"
    );

    unsafe {
        std::env::remove_var("LOTAR_TASKS_DIR");
    }
}

#[test]
fn tool_descriptions_list_available_values_when_unambiguous() {
    let _lock = lock_var("LOTAR_TASKS_DIR");
    let tmp = tempfile::tempdir().unwrap();
    let tasks_dir = tmp.path().join(".tasks");
    std::fs::create_dir_all(&tasks_dir).unwrap();
    std::fs::create_dir_all(tasks_dir.join("MCP")).unwrap();
    let config_yaml = r#"
default:
  project: MCP
members:
  - alice
  - bob
issue:
  tags:
    - cli
    - infra
custom:
  fields:
    - severity
    - impact
"#;
    std::fs::write(tasks_dir.join("config.yml"), config_yaml).unwrap();
    unsafe {
        std::env::set_var("LOTAR_TASKS_DIR", &tasks_dir);
    }

    let req = JsonRpcRequest {
        jsonrpc: "2.0".into(),
        id: Some(json!(321)),
        method: "tools/list".into(),
        params: json!({}),
    };
    let resp = dispatch(req);
    assert!(resp.error.is_none(), "tools/list failed: {:?}", resp.error);
    let tools = resp
        .result
        .as_ref()
        .and_then(|value| value.get("tools"))
        .and_then(|value| value.as_array())
        .expect("tools array available");

    let task_create_desc = tool_description(tool_by_name(tools, "task_create"));
    assert!(task_create_desc.contains("Available projects: MCP."));
    assert!(task_create_desc.contains("Available priorities: Low, Medium, High."));
    assert!(task_create_desc.contains("Available members: alice, bob."));
    assert!(task_create_desc.contains("Available tags: cli, infra."));

    let task_list_desc = tool_description(tool_by_name(tools, "task_list"));
    assert!(task_list_desc.contains("Available statuses: Todo, InProgress, Done."));

    let config_show_desc = tool_description(tool_by_name(tools, "config_show"));
    assert!(config_show_desc.contains("Available projects: MCP."));

    unsafe {
        std::env::remove_var("LOTAR_TASKS_DIR");
    }
}

#[test]
fn tool_descriptions_remain_static_with_multiple_projects() {
    let _lock = lock_var("LOTAR_TASKS_DIR");
    let tmp = tempfile::tempdir().unwrap();
    let tasks_dir = tmp.path().join(".tasks");
    std::fs::create_dir_all(tasks_dir.join("AAA")).unwrap();
    std::fs::create_dir_all(tasks_dir.join("BBB")).unwrap();
    unsafe {
        std::env::set_var("LOTAR_TASKS_DIR", &tasks_dir);
    }

    let req = JsonRpcRequest {
        jsonrpc: "2.0".into(),
        id: Some(json!(322)),
        method: "tools/list".into(),
        params: json!({}),
    };
    let resp = dispatch(req);
    assert!(resp.error.is_none(), "tools/list failed: {:?}", resp.error);
    let tools = resp
        .result
        .as_ref()
        .and_then(|value| value.get("tools"))
        .and_then(|value| value.as_array())
        .expect("tools array available");

    let task_create_desc = tool_description(tool_by_name(tools, "task_create"));
    assert!(!task_create_desc.contains("Available"));

    let project_list_desc = tool_description(tool_by_name(tools, "project_list"));
    assert!(!project_list_desc.contains("Available"));

    unsafe {
        std::env::remove_var("LOTAR_TASKS_DIR");
    }
}

#[test]
fn task_create_response_includes_metadata_and_enum_hints() {
    let _lock = lock_var("LOTAR_TASKS_DIR");
    let tmp = tempfile::tempdir().unwrap();
    let tasks_dir = tmp.path().join(".tasks");
    seed_single_project_config(&tasks_dir);
    std::fs::create_dir_all(tasks_dir.join("MCP")).unwrap();
    unsafe {
        std::env::set_var("LOTAR_TASKS_DIR", &tasks_dir);
    }

    let req = JsonRpcRequest {
        jsonrpc: "2.0".into(),
        id: Some(json!(400)),
        method: "tools/call".into(),
        params: json!({
            "name": "task_create",
            "arguments": {
                "title": "Metadata",
                "project": "MCP"
            }
        }),
    };
    let resp = dispatch(req);
    assert!(resp.error.is_none(), "task_create failed: {:?}", resp.error);
    let payload = parse_tool_payload(&resp);
    let metadata = payload
        .get("metadata")
        .and_then(|value| value.as_object())
        .expect("metadata present");
    let applied = metadata
        .get("appliedDefaults")
        .and_then(|value| value.as_array())
        .expect("appliedDefaults array");
    let fields: Vec<&str> = applied
        .iter()
        .filter_map(|entry| entry.get("field").and_then(|value| value.as_str()))
        .collect();
    assert!(fields.contains(&"priority"));
    assert!(fields.contains(&"status"));
    assert!(fields.contains(&"type"));
    let enum_hints = metadata
        .get("enumHints")
        .and_then(|value| value.as_object())
        .expect("enumHints present");
    let projects = enum_hints
        .get("projects")
        .and_then(|value| value.as_array())
        .expect("projects array present");
    assert_eq!(projects[0].as_str(), Some("MCP"));

    unsafe {
        std::env::remove_var("LOTAR_TASKS_DIR");
    }
}

#[test]
fn task_list_payload_includes_enum_hints() {
    let _lock = lock_var("LOTAR_TASKS_DIR");
    let tmp = tempfile::tempdir().unwrap();
    let tasks_dir = tmp.path().join(".tasks");
    seed_single_project_config(&tasks_dir);
    std::fs::create_dir_all(tasks_dir.join("MCP")).unwrap();
    unsafe {
        std::env::set_var("LOTAR_TASKS_DIR", &tasks_dir);
    }

    // Ensure storage initialized by creating one task
    let create_req = JsonRpcRequest {
        jsonrpc: "2.0".into(),
        id: Some(json!(401)),
        method: "tools/call".into(),
        params: json!({
            "name": "task_create",
            "arguments": {
                "title": "List Hints",
                "project": "MCP"
            }
        }),
    };
    let create_resp = dispatch(create_req);
    assert!(create_resp.error.is_none(), "seed create failed");

    let list_req = JsonRpcRequest {
        jsonrpc: "2.0".into(),
        id: Some(json!(402)),
        method: "task/list".into(),
        params: json!({ "project": "MCP" }),
    };
    let list_resp = dispatch(list_req);
    assert!(list_resp.error.is_none(), "task/list failed");
    let content = list_resp
        .result
        .as_ref()
        .and_then(|value| value.get("content"))
        .and_then(|value| value.as_array())
        .expect("content array present");
    let payload_text = content[0]
        .get("text")
        .and_then(|value| value.as_str())
        .expect("text payload available");
    let payload: serde_json::Value = serde_json::from_str(payload_text).unwrap();
    let enum_hints = payload
        .get("enumHints")
        .and_then(|value| value.as_object())
        .expect("enumHints present");
    assert!(enum_hints.get("statuses").is_some());
    let projects = enum_hints
        .get("projects")
        .and_then(|value| value.as_array())
        .expect("projects array present");
    assert_eq!(projects[0].as_str(), Some("MCP"));

    unsafe {
        std::env::remove_var("LOTAR_TASKS_DIR");
    }
}

#[test]
fn sprint_backlog_payload_includes_enum_hints() {
    let _lock = lock_var("LOTAR_TASKS_DIR");
    let tmp = tempfile::tempdir().unwrap();
    let tasks_dir = tmp.path().join(".tasks");
    seed_single_project_config(&tasks_dir);
    std::fs::create_dir_all(tasks_dir.join("MCP")).unwrap();
    unsafe {
        std::env::set_var("LOTAR_TASKS_DIR", &tasks_dir);
    }

    let req = JsonRpcRequest {
        jsonrpc: "2.0".into(),
        id: Some(json!(403)),
        method: "sprint/backlog".into(),
        params: json!({ "project": "MCP" }),
    };
    let resp = dispatch(req);
    assert!(resp.error.is_none(), "sprint/backlog failed");
    let content = resp
        .result
        .as_ref()
        .and_then(|value| value.get("content"))
        .and_then(|value| value.as_array())
        .expect("content array present");
    let payload_text = content[0]
        .get("text")
        .and_then(|value| value.as_str())
        .expect("text payload available");
    let payload: serde_json::Value = serde_json::from_str(payload_text).unwrap();
    let enum_hints = payload
        .get("enumHints")
        .and_then(|value| value.as_object())
        .expect("enumHints present");
    assert!(enum_hints.get("priorities").is_some());
    let projects = enum_hints
        .get("projects")
        .and_then(|value| value.as_array())
        .expect("projects array present");
    assert_eq!(projects[0].as_str(), Some("MCP"));

    unsafe {
        std::env::remove_var("LOTAR_TASKS_DIR");
    }
}

#[test]
fn schema_discover_filters_tools_case_insensitively() {
    let _lock = lock_var("LOTAR_TASKS_DIR");
    let tmp = tempfile::tempdir().unwrap();
    let tasks_dir = tmp.path().join(".tasks");
    std::fs::create_dir_all(&tasks_dir).unwrap();
    std::fs::write(tasks_dir.join("config.yml"), "default:\n  project: Demo\n").unwrap();
    unsafe {
        std::env::set_var("LOTAR_TASKS_DIR", &tasks_dir);
    }

    let req = JsonRpcRequest {
        jsonrpc: "2.0".into(),
        id: Some(json!(201)),
        method: "schema/discover".into(),
        params: json!({ "tool": "TASK_CREATE" }),
    };
    let resp = dispatch(req);
    assert!(
        resp.error.is_none(),
        "schema/discover failed: {:?}",
        resp.error
    );
    let content = resp
        .result
        .as_ref()
        .and_then(|value| value.get("content"))
        .and_then(|value| value.as_array())
        .expect("content array present");
    let text = content[0]
        .get("text")
        .and_then(|value| value.as_str())
        .expect("text payload present");
    let payload: serde_json::Value = serde_json::from_str(text).expect("valid payload json");
    assert_eq!(payload.get("toolCount").and_then(|v| v.as_u64()), Some(1));
    let tools = payload
        .get("tools")
        .and_then(|value| value.as_array())
        .expect("tools array present");
    assert_eq!(tools.len(), 1, "expected only filtered tool");
    assert_eq!(
        tools[0].get("name").and_then(|value| value.as_str()),
        Some("task_create")
    );

    unsafe {
        std::env::remove_var("LOTAR_TASKS_DIR");
    }
}

#[test]
fn normalize_method_supports_underscore_aliases() {
    assert_eq!(normalize_method("tools/call"), "tools/call");
    assert_eq!(normalize_method("tools_call"), "tools/call");
    assert_eq!(normalize_method("initialize"), "initialize");
    assert_eq!(normalize_method("task_create"), "task/create");
}

#[test]
fn event_affects_tooling_triggers_for_root_and_config() {
    let tasks_dir = std::path::PathBuf::from("/tmp/lotar/.tasks");
    let root_event = vec![tasks_dir.clone()];
    assert!(event_affects_tooling(&root_event, tasks_dir.as_path()));

    let config_event = vec![tasks_dir.join("MCP").join("config.yml")];
    assert!(event_affects_tooling(&config_event, tasks_dir.as_path()));

    let project_create = vec![tasks_dir.join("DOCS")];
    assert!(event_affects_tooling(&project_create, tasks_dir.as_path()));

    assert!(event_affects_tooling(&[], tasks_dir.as_path()));
}

#[test]
fn event_affects_tooling_ignores_nested_task_files() {
    let tasks_dir = std::path::PathBuf::from("/tmp/lotar/.tasks");
    let nested_task = vec![tasks_dir.join("MCP").join("tasks").join("123.yml")];
    assert!(!event_affects_tooling(&nested_task, tasks_dir.as_path()));
}

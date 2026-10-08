//! DEV-64: MCP catalog <-> handler/shared-DTO contract tests.
//!
//! Everything runs through the real public dispatch boundary
//! (`lotar::mcp::server::handle_json_line`) against isolated temporary
//! workspaces, covering:
//! - the whole advertised catalog (every tool, every advertised nullable
//!   field accepts explicit null at the schema layer),
//! - shared-DTO parity for task create/update null semantics,
//! - targeted sync `task_id` forwarding on both sync tools,
//! - the `type`/`task_type` alias precedence contract,
//! - the positive-integer sprint id schema contract,
//! - task patch/bulk sprint clear semantics,
//! - create project-only enum validation with project-scoped suggestions,
//! - soft/hard delete semantics on the task tools.

mod common;

use crate::common::env_mutex::EnvVarGuard;
use serde_json::{Value, json};
use std::path::Path;

fn mcp(method: &str, params: Value) -> Value {
    let req = json!({
        "jsonrpc": "2.0",
        "id": 64,
        "method": method,
        "params": params
    });
    let line = serde_json::to_string(&req).unwrap();
    let resp_line = lotar::mcp::server::handle_json_line(&line);
    serde_json::from_str(&resp_line).unwrap()
}

fn call_tool(name: &str, arguments: Value) -> Value {
    mcp("tools/call", json!({"name": name, "arguments": arguments}))
}

fn result_text(resp: &Value) -> Option<String> {
    resp.get("result")
        .and_then(|r| r.get("content"))
        .and_then(|v| v.as_array())
        .and_then(|arr| arr.first())
        .and_then(|entry| entry.get("text"))
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
}

fn result_payload(resp: &Value) -> Value {
    let text = result_text(resp).unwrap_or_default();
    serde_json::from_str(&text).unwrap_or(Value::Null)
}

fn is_error_payload(resp: &Value) -> bool {
    resp.get("result")
        .and_then(|r| r.get("isError"))
        .and_then(|v| v.as_bool())
        .unwrap_or(false)
}

fn error_code(resp: &Value) -> Option<i64> {
    resp.get("error")
        .and_then(|e| e.get("code"))
        .and_then(|v| v.as_i64())
}

fn schema_issues(resp: &Value) -> Vec<String> {
    resp.get("error")
        .and_then(|e| e.get("data"))
        .and_then(|d| d.get("issues"))
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|i| i.as_str().map(str::to_string))
                .collect()
        })
        .unwrap_or_default()
}

struct Workspace {
    tasks_dir: std::path::PathBuf,
    _tmp: tempfile::TempDir,
    _guard: EnvVarGuard,
}

fn workspace(config: Option<&str>) -> Workspace {
    let tmp = tempfile::tempdir().unwrap();
    let tasks_dir = tmp.path().join(".tasks");
    std::fs::create_dir_all(&tasks_dir).unwrap();
    if let Some(config) = config {
        std::fs::write(lotar::utils::paths::global_config_path(&tasks_dir), config).unwrap();
    }
    let guard = EnvVarGuard::set("LOTAR_TASKS_DIR", tasks_dir.to_string_lossy().as_ref());
    Workspace {
        tasks_dir,
        _tmp: tmp,
        _guard: guard,
    }
}

fn task_yaml_count(root: &Path) -> usize {
    fn walk(dir: &Path, count: &mut usize) {
        let Ok(entries) = std::fs::read_dir(dir) else {
            return;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                walk(&path, count);
            } else if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                let is_task = name
                    .strip_suffix(".yml")
                    .is_some_and(|stem| stem.chars().all(|c| c.is_ascii_digit()))
                    && !name.starts_with('@');
                if is_task {
                    *count += 1;
                }
            }
        }
    }
    let mut count = 0;
    walk(root, &mut count);
    count
}

fn create_task(args: Value) -> Value {
    let resp = call_tool("task_create", args);
    assert!(resp.get("error").is_none(), "task_create failed: {resp}");
    result_payload(&resp)
        .get("task")
        .cloned()
        .unwrap_or(Value::Null)
}

fn advertised_tools() -> Vec<Value> {
    let resp = mcp("tools/list", json!({}));
    assert!(resp.get("error").is_none(), "tools/list failed: {resp}");
    resp.get("result")
        .and_then(|r| r.get("tools"))
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default()
}

fn tool_schema(name: &str) -> Value {
    advertised_tools()
        .into_iter()
        .find(|tool| tool.get("name").and_then(|v| v.as_str()) == Some(name))
        .and_then(|tool| tool.get("inputSchema").cloned())
        .unwrap_or_else(|| panic!("tool {name} must be advertised"))
}

fn property_allows_null(property: &Value) -> bool {
    if let Some(types) = property.get("type").and_then(|v| v.as_array())
        && types.iter().any(|t| t.as_str() == Some("null"))
    {
        return true;
    }
    property
        .get("oneOf")
        .and_then(|v| v.as_array())
        .map(|branches| {
            branches
                .iter()
                .any(|branch| branch.get("type").and_then(|t| t.as_str()) == Some("null"))
        })
        .unwrap_or(false)
}

#[test]
fn advertised_catalog_covers_every_registered_tool_with_schemas() {
    let tools = advertised_tools();
    assert_eq!(tools.len(), 38, "advertised tool count changed");
    let mut names: Vec<&str> = tools
        .iter()
        .filter_map(|tool| tool.get("name").and_then(|v| v.as_str()))
        .collect();
    let total = names.len();
    names.sort_unstable();
    names.dedup();
    assert_eq!(total, names.len(), "duplicate advertised tool names");

    for tool in &tools {
        let schema = tool
            .get("inputSchema")
            .expect("every tool advertises an inputSchema");
        assert_eq!(
            schema.get("type").and_then(|v| v.as_str()),
            Some("object"),
            "{}: root schema must be an object",
            tool.get("name").unwrap()
        );
        assert!(
            schema
                .get("properties")
                .and_then(|v| v.as_object())
                .is_some(),
            "{}: root schema must declare properties",
            tool.get("name").unwrap()
        );
    }

    // The task tools the registry pins must all stay advertised.
    for expected in [
        "whoami",
        "task_create",
        "task_update",
        "task_bulk_update",
        "task_delete",
        "task_restore",
        "task_list",
        "sprint_add",
        "sprint_remove",
        "sync_pull",
        "sync_push",
        "schema_discover",
    ] {
        assert!(names.contains(&expected), "{expected} must stay advertised");
    }
}

#[test]
fn every_advertised_nullable_field_accepts_explicit_null() {
    // Isolated workspace: probes on tools without required fields reach
    // handlers (reads or harmless creations) and must never touch the real
    // repository.
    let _ws = workspace(None);
    let tools = advertised_tools();
    assert!(!tools.is_empty());

    // (tool name, schema pointer, probe arguments): the owning tool is
    // carried with each probe because schemas for a shared property name
    // (e.g. `tags`) can differ between tools.
    let mut probes: Vec<(String, String, Value)> = Vec::new();
    for tool in &tools {
        let tool_name = tool.get("name").and_then(|v| v.as_str()).unwrap();
        let properties = tool
            .get("inputSchema")
            .and_then(|s| s.get("properties"))
            .and_then(|p| p.as_object())
            .cloned()
            .unwrap_or_default();
        for (prop, property_schema) in properties {
            if property_allows_null(&property_schema) {
                let mut probe = serde_json::Map::new();
                probe.insert(prop.clone(), Value::Null);
                probes.push((
                    tool_name.to_string(),
                    format!("/{prop}"),
                    Value::Object(probe),
                ));
            }
            // Nested object properties (task_update/task_bulk_update `patch`)
            // advertise their own nullable subfields.
            if let Some(sub_properties) = property_schema
                .get("properties")
                .and_then(|p| p.as_object())
            {
                for (sub, sub_schema) in sub_properties {
                    if property_allows_null(sub_schema) {
                        let mut nested = serde_json::Map::new();
                        nested.insert(sub.clone(), Value::Null);
                        let mut probe = serde_json::Map::new();
                        probe.insert(prop.clone(), Value::Object(nested));
                        probes.push((
                            tool_name.to_string(),
                            format!("/{prop}/{sub}"),
                            Value::Object(probe),
                        ));
                    }
                }
            }
        }
    }

    assert!(
        probes.len() >= 60,
        "nullable sweep must cover the catalog, got {} probes",
        probes.len()
    );

    for (tool_name, pointer, probe) in &probes {
        let resp = call_tool(tool_name, probe.clone());
        let violations: Vec<String> = schema_issues(&resp)
            .into_iter()
            .filter(|issue| {
                issue.contains(pointer.as_str())
                    && (issue.contains("expected type")
                        || issue.contains("allowed enum")
                        || issue.contains("unknown property"))
            })
            .collect();
        assert!(
            violations.is_empty(),
            "null must satisfy {tool_name}'s advertised contract for {pointer}: {violations:?} (response: {resp})"
        );
    }
}

#[test]
fn sync_tools_advertise_and_forward_targeted_task_id() {
    let _ws = workspace(Some(
        "default.project: MCP\nremotes:\n  origin:\n    provider: github\n    repo: org/repo\n",
    ));

    // Advertised as a nullable string on both sync tools.
    for tool in ["sync_pull", "sync_push"] {
        let schema = tool_schema(tool);
        let task_id = schema
            .get("properties")
            .and_then(|p| p.get("task_id"))
            .cloned()
            .unwrap_or_else(|| panic!("{tool} must advertise task_id"));
        let types = task_id.get("type").and_then(|v| v.as_array()).cloned();
        assert_eq!(
            types,
            Some(vec![json!("string"), json!("null")]),
            "{tool}.task_id must be a nullable string"
        );
    }

    // The schema layer must accept task_id (the old catalog rejected it as
    // an unknown property); with no side-effect-free way to run a real
    // remote, a missing remote exercises handler reachability.
    let resp = call_tool(
        "sync_pull",
        json!({"remote": "missing", "task_id": "MCP-1", "dry_run": true}),
    );
    assert!(
        resp.get("error").is_none()
            || error_code(&resp) != Some(-32602)
            || schema_issues(&resp)
                .iter()
                .all(|issue| !issue.contains("task_id")),
        "task_id must pass the schema layer: {resp}"
    );
    assert!(
        is_error_payload(&resp) || resp.get("error").is_some(),
        "unknown remote must surface as a domain failure, not success: {resp}"
    );

    // Targeted dry-run push scopes the run to exactly the named task.
    let first = create_task(json!({"title": "targeted one", "project": "MCP"}));
    let first_id = first
        .get("id")
        .and_then(|v| v.as_str())
        .unwrap()
        .to_string();
    create_task(json!({"title": "targeted two", "project": "MCP"}));

    let targeted = call_tool(
        "sync_push",
        json!({"remote": "origin", "task_id": first_id, "dry_run": true, "include_report": true}),
    );
    assert!(
        resp_ok(&targeted),
        "targeted dry-run push must succeed offline: {targeted}"
    );
    let payload = result_payload(&targeted);
    assert_eq!(
        payload
            .get("summary")
            .and_then(|s| s.get("created"))
            .and_then(|v| v.as_u64()),
        Some(1),
        "task_id must scope the push to one task: {payload}"
    );
    let entries = payload
        .get("report_entries")
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();
    let entry_ids: Vec<&str> = entries
        .iter()
        .filter_map(|e| e.get("task_id").and_then(|v| v.as_str()))
        .collect();
    assert_eq!(entry_ids, vec![first_id.as_str()]);

    // Without task_id (and with explicit null) the whole project is in
    // scope, proving the parameter is authoritative rather than ignored.
    for arguments in [
        json!({"remote": "origin", "dry_run": true, "include_report": true}),
        json!({"remote": "origin", "task_id": null, "dry_run": true, "include_report": true}),
    ] {
        let unscoped = call_tool("sync_push", arguments);
        assert!(
            resp_ok(&unscoped),
            "unscoped dry-run push failed: {unscoped}"
        );
        let payload = result_payload(&unscoped);
        assert_eq!(
            payload
                .get("summary")
                .and_then(|s| s.get("created"))
                .and_then(|v| v.as_u64()),
            Some(2),
            "unscoped push covers both tasks: {payload}"
        );
    }

    // A task_id outside the explicit project is rejected before any sync.
    let mismatch = call_tool(
        "sync_push",
        json!({"remote": "origin", "project": "MCP", "task_id": "ZZZ-9", "dry_run": true}),
    );
    let text = serde_json::to_string(&mismatch).unwrap();
    assert!(
        text.contains("does not belong to project"),
        "cross-project task_id must be rejected: {mismatch}"
    );
}

fn resp_ok(resp: &Value) -> bool {
    resp.get("error").is_none() && !is_error_payload(resp)
}

#[test]
fn task_create_accepts_null_relationships_matching_shared_dto() {
    let ws = workspace(Some("default.project: MCP\n"));

    // Shared-DTO parity: TaskCreate deserializes explicit null to None
    // (`tags` is a required Vec on the DTO wire shape).
    let dto: lotar::api_types::TaskCreate =
        serde_json::from_value(json!({"title": "dto parity", "tags": [], "relationships": null}))
            .unwrap();
    assert!(dto.relationships.is_none());

    // The MCP surface honors the same contract.
    let task = create_task(json!({
        "title": "null relationships",
        "project": "MCP",
        "relationships": null
    }));
    assert!(
        task.get("relationships")
            .map(|r| r.as_object().map(|o| o.is_empty()).unwrap_or(true))
            .unwrap_or(true),
        "null relationships must create a task without relationships: {task}"
    );

    // {} matches the DTO's empty -> None behavior.
    let task = create_task(json!({
        "title": "empty relationships",
        "project": "MCP",
        "relationships": {}
    }));
    assert!(
        task.get("relationships").is_none()
            || task["relationships"].as_object().unwrap().is_empty()
    );

    // Malformed payloads still fail before any side effect.
    let before = task_yaml_count(&ws.tasks_dir);
    let resp = call_tool(
        "task_create",
        json!({
            "title": "bad relationships",
            "project": "MCP",
            "relationships": {"depends_on": "not-an-array"}
        }),
    );
    assert_eq!(
        error_code(&resp),
        Some(-32602),
        "malformed relationships rejected: {resp}"
    );
    assert_eq!(
        task_yaml_count(&ws.tasks_dir),
        before,
        "rejected create must not write a task file"
    );
}

#[test]
fn type_alias_takes_precedence_and_null_falls_through_to_task_type() {
    let _ws = workspace(Some(
        "default.project: MCP\nissue.types: [Feature, Bug, Task]\n",
    ));

    // Canonical `type` wins when both aliases are provided.
    let task = create_task(json!({
        "title": "alias precedence",
        "project": "MCP",
        "type": "Feature",
        "task_type": "Bug"
    }));
    assert_eq!(
        task.get("task_type").and_then(|v| v.as_str()),
        Some("Feature"),
        "type must take precedence over the task_type alias: {task}"
    );

    // The alias alone still applies.
    let task = create_task(json!({
        "title": "alias alone",
        "project": "MCP",
        "task_type": "Bug"
    }));
    assert_eq!(task.get("task_type").and_then(|v| v.as_str()), Some("Bug"));

    // Update: a null `type` is "treated as omitted", so the alias applies.
    let created = create_task(json!({"title": "update alias", "project": "MCP"}));
    let id = created
        .get("id")
        .and_then(|v| v.as_str())
        .unwrap()
        .to_string();
    let resp = call_tool(
        "task_update",
        json!({"id": id, "patch": {"type": null, "task_type": "Bug"}}),
    );
    assert!(resp_ok(&resp), "null type + task_type patch failed: {resp}");
    let updated = result_payload(&resp);
    assert_eq!(
        updated.get("task_type").and_then(|v| v.as_str()),
        Some("Bug"),
        "null type must fall through to the task_type alias: {updated}"
    );

    // Create: same null fall-through for the pre-validation and DTO paths.
    let task = create_task(json!({
        "title": "create null fallthrough",
        "project": "MCP",
        "type": null,
        "task_type": "Task"
    }));
    assert_eq!(
        task.get("task_type").and_then(|v| v.as_str()),
        Some("Task"),
        "null type must fall through to the task_type alias on create: {task}"
    );
}

#[test]
fn sprint_identifiers_advertise_the_positive_integer_contract() {
    let _ws = workspace(Some("default.project: MCP\n"));

    // task_create: non-integer and non-positive entries are rejected by the
    // SCHEMA layer (before any handler/side effect), not just the handler.
    for sprints in [json!([1.5]), json!([0]), json!([-1])] {
        let resp = call_tool("task_create", json!({"title": "s", "sprints": sprints}));
        assert_eq!(
            error_code(&resp),
            Some(-32602),
            "sprints {sprints} must be invalid params: {resp}"
        );
        let issues = schema_issues(&resp).join("; ");
        assert!(
            issues.contains("integer") || issues.contains("minimum"),
            "schema issues must name the integer contract: {issues}"
        );
    }

    // task_update patch.sprints carries the same contract.
    for sprints in [json!([1.5]), json!([0])] {
        let resp = call_tool(
            "task_update",
            json!({"id": "MCP-1", "patch": {"sprints": sprints}}),
        );
        assert_eq!(error_code(&resp), Some(-32602), "{resp}");
        let issues = schema_issues(&resp).join("; ");
        assert!(
            issues.contains("integer") || issues.contains("minimum"),
            "patch.sprints issues must name the integer contract: {issues}"
        );
    }

    // sprint_get sprint_id: fractional values are schema-level rejections.
    let resp = call_tool("sprint_get", json!({"sprint_id": 1.5}));
    assert_eq!(error_code(&resp), Some(-32602), "{resp}");
    assert!(
        schema_issues(&resp)
            .iter()
            .any(|issue| issue.contains("integer")),
        "sprint_id must advertise the integer contract: {resp}"
    );

    // task_list keeps its flexible filter grammar: '#3' and 3 pass the
    // schema, while 0 and 1.5 do not.
    for sprints in [json!("#3"), json!(3), json!([1, "#2"])] {
        let resp = call_tool("task_list", json!({"sprints": sprints}));
        assert!(
            !schema_issues(&resp)
                .iter()
                .any(|issue| issue.contains("/sprints")),
            "flexible sprints filter {sprints} must pass the schema: {resp}"
        );
    }
    for sprints in [json!(0), json!(1.5), json!([0]), json!([1.5])] {
        let resp = call_tool("task_list", json!({"sprints": sprints}));
        let issues = schema_issues(&resp).join("; ");
        assert!(
            issues.contains("minimum") || issues.contains("integer"),
            "sprints filter {sprints} must be schema-rejected: {issues}"
        );
    }
}

#[test]
fn task_list_null_cursor_falls_through_to_offset_alias() {
    let _ws = workspace(Some("default.project: MCP\n"));
    create_task(json!({"title": "cursor one", "project": "MCP"}));
    create_task(json!({"title": "cursor two", "project": "MCP"}));

    // cursor=null is omitted and must not shadow the offset alias.
    let resp = call_tool(
        "task_list",
        json!({"limit": 1, "cursor": null, "offset": 1}),
    );
    assert!(resp_ok(&resp), "{resp}");
    let payload = result_payload(&resp);
    assert_eq!(payload.get("cursor").and_then(|v| v.as_u64()), Some(1));
    assert_eq!(payload.get("count").and_then(|v| v.as_u64()), Some(1));
    assert_eq!(
        payload.get("hasMore").and_then(|v| v.as_bool()),
        Some(false)
    );
}

#[test]
fn task_patch_and_bulk_update_clear_sprint_memberships() {
    let _ws = workspace(Some("default.project: MCP\n"));

    let created = call_tool("sprint_create", json!({"label": "Sprint One"}));
    assert!(resp_ok(&created), "sprint_create failed: {created}");

    let task = create_task(json!({
        "title": "sprint membership",
        "project": "MCP",
        "sprints": [1]
    }));
    assert_eq!(
        task.get("sprints")
            .and_then(|v| v.as_array())
            .map(|a| a.len()),
        Some(1),
        "create must seed sprint membership: {task}"
    );
    let id = task.get("id").and_then(|v| v.as_str()).unwrap().to_string();

    // patch.sprints = null clears.
    let resp = call_tool("task_update", json!({"id": id, "patch": {"sprints": null}}));
    assert!(resp_ok(&resp), "null sprints patch failed: {resp}");
    let updated = result_payload(&resp);
    assert!(
        updated
            .get("sprints")
            .map(|s| s.as_array().map(|a| a.is_empty()).unwrap_or(true))
            .unwrap_or(true),
        "null sprints must clear memberships: {updated}"
    );

    // Re-attach, then bulk clear with [].
    let resp = call_tool("task_update", json!({"id": id, "patch": {"sprints": [1]}}));
    assert!(resp_ok(&resp), "{resp}");
    let resp = call_tool(
        "task_bulk_update",
        json!({"ids": [id], "patch": {"sprints": []}}),
    );
    assert!(resp_ok(&resp), "bulk sprint clear failed: {resp}");
    let payload = result_payload(&resp);
    let updated = payload
        .get("updated")
        .and_then(|v| v.as_array())
        .and_then(|a| a.first())
        .cloned()
        .unwrap_or(Value::Null);
    assert!(
        updated
            .get("sprints")
            .map(|s| s.as_array().map(|a| a.is_empty()).unwrap_or(true))
            .unwrap_or(true),
        "bulk [] must clear sprint memberships: {payload}"
    );
}

#[test]
fn task_create_validates_enums_against_the_target_project_only() {
    let ws = workspace(Some("default.project: MCP\nissue.states: [Todo, Done]\n"));
    let project_dir = ws.tasks_dir.join("MCP");
    std::fs::create_dir_all(&project_dir).unwrap();
    // Project-only vocabulary: Alpha/Beta exist ONLY for MCP.
    std::fs::write(
        project_dir.join("config.yml"),
        "issue.states: [Alpha, Beta]\nissue.types: [Feature]\n",
    )
    .unwrap();

    // A globally valid status is invalid for the target project, and the
    // error suggests the project's own values.
    let resp = call_tool(
        "task_create",
        json!({"title": "project enum", "project": "MCP", "status": "Todo"}),
    );
    assert_eq!(error_code(&resp), Some(-32602), "{resp}");
    let data = resp
        .get("error")
        .and_then(|e| e.get("data"))
        .cloned()
        .unwrap_or(Value::Null);
    assert_eq!(data.get("field").and_then(|v| v.as_str()), Some("status"));
    let suggestions: Vec<&str> = data
        .get("suggestions")
        .and_then(|v| v.as_array())
        .map(|arr| arr.iter().filter_map(|s| s.as_str()).collect())
        .unwrap_or_default();
    assert_eq!(
        suggestions,
        vec!["Alpha", "Beta"],
        "suggestions must come from the project's config: {data}"
    );

    // Project-only values are accepted for that project.
    let task = create_task(json!({"title": "project alpha", "project": "MCP", "status": "Alpha"}));
    assert_eq!(task.get("status").and_then(|v| v.as_str()), Some("Alpha"));
}

#[test]
fn task_delete_keeps_soft_default_hard_opt_in_and_rejects_force() {
    let _ws = workspace(Some("default.project: MCP\n"));

    // CLI-style force is not an MCP concept and stays rejected.
    let resp = call_tool("task_delete", json!({"id": "MCP-1", "force": true}));
    assert_eq!(error_code(&resp), Some(-32602), "{resp}");
    assert!(
        schema_issues(&resp)
            .iter()
            .any(|issue| issue.contains("unknown property 'force'")),
        "force must stay unadvertised: {resp}"
    );

    let task = create_task(json!({"title": "delete me", "project": "MCP"}));
    let id = task.get("id").and_then(|v| v.as_str()).unwrap().to_string();

    // Default is a soft delete: hidden from task_get, restorable.
    let resp = call_tool("task_delete", json!({"id": id}));
    assert!(resp_ok(&resp), "{resp}");
    let payload = result_payload(&resp);
    assert_eq!(payload.get("deleted").and_then(|v| v.as_bool()), Some(true));
    assert_eq!(payload.get("hard").and_then(|v| v.as_bool()), Some(false));

    let resp = call_tool("task_get", json!({"id": id}));
    assert!(
        is_error_payload(&resp),
        "soft-deleted task must be hidden: {resp}"
    );
    let resp = call_tool("task_get", json!({"id": id, "include_deleted": true}));
    assert!(resp_ok(&resp), "include_deleted must resolve it: {resp}");

    let resp = call_tool("task_restore", json!({"id": id}));
    assert!(resp_ok(&resp), "restore failed: {resp}");

    // hard=true physically removes the file; restore can no longer work.
    let resp = call_tool("task_delete", json!({"id": id, "hard": true}));
    assert!(resp_ok(&resp), "{resp}");
    let payload = result_payload(&resp);
    assert_eq!(payload.get("hard").and_then(|v| v.as_bool()), Some(true));

    let resp = call_tool("task_restore", json!({"id": id}));
    assert!(
        is_error_payload(&resp) || resp.get("error").is_some(),
        "hard-deleted task must not be restorable: {resp}"
    );
}

#[test]
fn create_hints_and_validation_use_the_service_repository_project() {
    let tmp = tempfile::tempdir().unwrap();
    let tasks_dir = tmp.path().join("Fallback Repo/.tasks");
    std::fs::create_dir_all(tasks_dir.join("FR")).unwrap();
    std::fs::write(
        tasks_dir.join("config.yml"),
        "default.project: MCP\nissue.states: [Todo, Done]\n",
    )
    .unwrap();
    std::fs::write(
        tasks_dir.join("FR/config.yml"),
        "issue.states: [Ready, Closed]\n",
    )
    .unwrap();
    let _guard = EnvVarGuard::set("LOTAR_TASKS_DIR", tasks_dir.to_string_lossy().as_ref());
    let response = call_tool(
        "task_create",
        json!({"title": "Repository project", "status": "Ready"}),
    );
    assert!(resp_ok(&response), "{response}");
    let payload = result_payload(&response);
    assert_eq!(payload["task"]["id"], "FR-1");
    assert_eq!(payload["metadata"]["enumHints"]["projects"], json!(["FR"]));
    assert_eq!(
        payload["metadata"]["enumHints"]["statuses"],
        json!(["Ready", "Closed"])
    );
}

#[test]
fn sprint_create_schema_matches_nonnullable_boolean_dto() {
    let _ws = workspace(None);
    assert!(
        serde_json::from_value::<lotar::api_types::SprintCreateRequest>(
            json!({"skip_defaults": null})
        )
        .is_err()
    );
    let response = call_tool("sprint_create", json!({"skip_defaults": null}));
    assert!(
        schema_issues(&response)
            .iter()
            .any(|issue| issue.contains("/skip_defaults")),
        "{response}"
    );
    for value in [true, false] {
        assert!(resp_ok(&call_tool(
            "sprint_create",
            json!({"skip_defaults": value})
        )));
    }
}

#[test]
fn sprint_velocity_rejects_invalid_explicit_window() {
    let _ws = workspace(None);
    for limit in [json!(0), json!(-1), json!(1.5)] {
        let response = call_tool("sprint_velocity", json!({"limit": limit}));
        assert_eq!(error_code(&response), Some(-32602), "{response}");
    }
}

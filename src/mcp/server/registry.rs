//! Authoritative MCP tool registry (DEV-63).
//!
//! One table binds every advertised tool definition (the 38 entries emitted by
//! `tools::build_tool_definitions`) to its handler and both documented direct
//! wire aliases: the canonical snake_case tool name (`task_create`) and the
//! slash method (`task/create`).
//!
//! Both dispatch surfaces route through [`invoke_tool`]:
//!
//! 1. tool arguments are validated against the SAME advertised
//!    `inputSchema` before any handler runs (fail-closed validator in
//!    [`super::schema`]);
//! 2. recoverable tool/domain execution failures are converted into
//!    successful JSON-RPC responses with `result.isError = true` and
//!    explanatory content; reserved JSON-RPC protocol codes
//!    (-32700..-32600) stay protocol errors.
//!
//! Control-plane methods (`initialize`, `ping`, `tools/list`, `tools/call`,
//! `logging/setLevel`) are never reachable through `tools/call`; the
//! introspection tool `schema_discover` is a registered tool and remains
//! callable both directly and through `tools/call`.

use std::collections::HashMap;
use std::sync::OnceLock;

use serde_json::{Value, json};

use super::handlers::{
    handle_agent_cancel, handle_agent_list_jobs, handle_agent_run, handle_agent_send_message,
    handle_agent_status, handle_config_set, handle_config_show, handle_project_list,
    handle_project_stats, handle_sprint_add, handle_sprint_backlog, handle_sprint_burndown,
    handle_sprint_create, handle_sprint_delete, handle_sprint_get, handle_sprint_list,
    handle_sprint_remove, handle_sprint_summary, handle_sprint_update, handle_sprint_velocity,
    handle_sync_pull, handle_sync_push, handle_task_bulk_comment_add,
    handle_task_bulk_reference_add, handle_task_bulk_reference_remove, handle_task_bulk_update,
    handle_task_comment_add, handle_task_comment_update, handle_task_create, handle_task_delete,
    handle_task_get, handle_task_list, handle_task_reference_add, handle_task_reference_remove,
    handle_task_restore, handle_task_update, handle_whoami,
};
use super::schema::{unsupported_keywords, validate_instance};
use super::tools::build_tool_definitions;
use super::{
    JsonRpcRequest, JsonRpcResponse, err, handle_schema_discover_request, normalize_method,
};

pub(crate) type ToolHandler = fn(JsonRpcRequest) -> JsonRpcResponse;

pub(crate) struct ToolSpec {
    /// Canonical advertised tool name (snake_case, as returned by tools/list).
    pub(crate) name: &'static str,
    /// Direct wire method (slash form).
    pub(crate) method: &'static str,
    pub(crate) handler: ToolHandler,
}

/// Methods that belong to the JSON-RPC/MCP control plane and must never be
/// invocable as tools. `schema/discover` is deliberately absent: it is a
/// registered, read-only introspection tool.
const CONTROL_PLANE_METHODS: &[&str] = &[
    "initialize",
    "initialized",
    "notifications/initialized",
    "ping",
    "tools/list",
    "tools/listChanged",
    "notifications/tools/list_changed",
    "tools/call",
    "logging/setLevel",
];

pub(crate) fn is_control_plane_method(method: &str) -> bool {
    CONTROL_PLANE_METHODS.contains(&method)
}

pub(crate) fn tool_specs() -> &'static [ToolSpec] {
    &[
        ToolSpec {
            name: "whoami",
            method: "whoami",
            handler: handle_whoami,
        },
        ToolSpec {
            name: "task_create",
            method: "task/create",
            handler: handle_task_create,
        },
        ToolSpec {
            name: "task_get",
            method: "task/get",
            handler: handle_task_get,
        },
        ToolSpec {
            name: "task_update",
            method: "task/update",
            handler: handle_task_update,
        },
        ToolSpec {
            name: "task_comment_add",
            method: "task/comment_add",
            handler: handle_task_comment_add,
        },
        ToolSpec {
            name: "task_comment_update",
            method: "task/comment_update",
            handler: handle_task_comment_update,
        },
        ToolSpec {
            name: "task_bulk_update",
            method: "task/bulk_update",
            handler: handle_task_bulk_update,
        },
        ToolSpec {
            name: "task_bulk_comment_add",
            method: "task/bulk_comment_add",
            handler: handle_task_bulk_comment_add,
        },
        ToolSpec {
            name: "task_bulk_reference_add",
            method: "task/bulk_reference_add",
            handler: handle_task_bulk_reference_add,
        },
        ToolSpec {
            name: "task_bulk_reference_remove",
            method: "task/bulk_reference_remove",
            handler: handle_task_bulk_reference_remove,
        },
        ToolSpec {
            name: "task_reference_add",
            method: "task/reference_add",
            handler: handle_task_reference_add,
        },
        ToolSpec {
            name: "task_reference_remove",
            method: "task/reference_remove",
            handler: handle_task_reference_remove,
        },
        ToolSpec {
            name: "task_delete",
            method: "task/delete",
            handler: handle_task_delete,
        },
        ToolSpec {
            name: "task_restore",
            method: "task/restore",
            handler: handle_task_restore,
        },
        ToolSpec {
            name: "task_list",
            method: "task/list",
            handler: handle_task_list,
        },
        ToolSpec {
            name: "sprint_list",
            method: "sprint/list",
            handler: handle_sprint_list,
        },
        ToolSpec {
            name: "sprint_get",
            method: "sprint/get",
            handler: handle_sprint_get,
        },
        ToolSpec {
            name: "sprint_create",
            method: "sprint/create",
            handler: handle_sprint_create,
        },
        ToolSpec {
            name: "sprint_update",
            method: "sprint/update",
            handler: handle_sprint_update,
        },
        ToolSpec {
            name: "sprint_summary",
            method: "sprint/summary",
            handler: handle_sprint_summary,
        },
        ToolSpec {
            name: "sprint_burndown",
            method: "sprint/burndown",
            handler: handle_sprint_burndown,
        },
        ToolSpec {
            name: "sprint_velocity",
            method: "sprint/velocity",
            handler: handle_sprint_velocity,
        },
        ToolSpec {
            name: "sprint_add",
            method: "sprint/add",
            handler: handle_sprint_add,
        },
        ToolSpec {
            name: "sprint_remove",
            method: "sprint/remove",
            handler: handle_sprint_remove,
        },
        ToolSpec {
            name: "sprint_delete",
            method: "sprint/delete",
            handler: handle_sprint_delete,
        },
        ToolSpec {
            name: "sprint_backlog",
            method: "sprint/backlog",
            handler: handle_sprint_backlog,
        },
        ToolSpec {
            name: "project_list",
            method: "project/list",
            handler: handle_project_list,
        },
        ToolSpec {
            name: "project_stats",
            method: "project/stats",
            handler: handle_project_stats,
        },
        ToolSpec {
            name: "config_show",
            method: "config/show",
            handler: handle_config_show,
        },
        ToolSpec {
            name: "config_set",
            method: "config/set",
            handler: handle_config_set,
        },
        ToolSpec {
            name: "sync_pull",
            method: "sync/pull",
            handler: handle_sync_pull,
        },
        ToolSpec {
            name: "sync_push",
            method: "sync/push",
            handler: handle_sync_push,
        },
        ToolSpec {
            name: "schema_discover",
            method: "schema/discover",
            handler: handle_schema_discover_request,
        },
        ToolSpec {
            name: "agent_run",
            method: "agent/run",
            handler: handle_agent_run,
        },
        ToolSpec {
            name: "agent_status",
            method: "agent/status",
            handler: handle_agent_status,
        },
        ToolSpec {
            name: "agent_list_jobs",
            method: "agent/list_jobs",
            handler: handle_agent_list_jobs,
        },
        ToolSpec {
            name: "agent_cancel",
            method: "agent/cancel",
            handler: handle_agent_cancel,
        },
        ToolSpec {
            name: "agent_send_message",
            method: "agent/send_message",
            handler: handle_agent_send_message,
        },
    ]
}

/// Resolve a direct wire method (slash form; snake_case names normalize to
/// it) to its registry entry.
pub(crate) fn find_tool_by_method(method: &str) -> Option<&'static ToolSpec> {
    let normalized = normalize_method(method);
    tool_specs().iter().find(|spec| spec.method == normalized)
}

/// Resolve a `tools/call` tool name: the canonical advertised name, or the
/// documented slash alias. Control-plane names never resolve.
pub(crate) fn find_tool_for_call(name: &str) -> Result<&'static ToolSpec, String> {
    let normalized = normalize_method(name);
    if is_control_plane_method(&normalized) {
        return Err(format!("Unknown tool: {name}"));
    }
    tool_specs()
        .iter()
        .find(|spec| spec.name == name || spec.method == normalized)
        .ok_or_else(|| format!("Unknown tool: {name}"))
}

/// The advertised `inputSchema` for a tool. Enum hints never alter
/// inputSchemas, so the hint-less definitions are the canonical source.
pub(crate) fn schema_for_tool(name: &str) -> Option<Value> {
    static SCHEMAS: OnceLock<HashMap<String, Value>> = OnceLock::new();
    let schemas = SCHEMAS.get_or_init(|| {
        build_tool_definitions(None)
            .into_iter()
            .filter_map(|tool| {
                let name = tool.get("name")?.as_str()?.to_string();
                let schema = tool.get("inputSchema").cloned()?;
                Some((name, schema))
            })
            .collect()
    });
    schemas.get(name).cloned()
}

fn invalid_params(id: Option<Value>, tool: &str, issues: &[String]) -> JsonRpcResponse {
    err(
        id,
        -32602,
        "Invalid params",
        Some(json!({ "tool": tool, "issues": issues })),
    )
}

/// Structured form of a converted domain execution failure.
#[derive(Clone)]
pub(crate) struct DomainError {
    pub(crate) message: String,
    pub(crate) data: Option<Value>,
}

fn is_reserved_code(code: i64) -> bool {
    (-32700..=-32600).contains(&code)
}

/// Validate the argument object against the tool's advertised schema.
fn validate_arguments(spec: &ToolSpec, arguments: &Value) -> Result<(), Vec<String>> {
    let Some(schema) = schema_for_tool(spec.name) else {
        return Err(Vec::new());
    };
    let unsupported = unsupported_keywords(&schema);
    if !unsupported.is_empty() {
        // Fail closed: never run a handler under a schema we cannot enforce.
        return Err(unsupported);
    }
    let issues = validate_instance(&schema, arguments);
    if issues.is_empty() {
        Ok(())
    } else {
        Err(issues)
    }
}

/// Invoke a registered tool: schema-validate first, run the handler, then
/// convert recoverable domain failures into `isError` tool results.
///
/// Returns the response plus the structured domain error when one was
/// converted (used by the `tools/call` wrapper).
pub(crate) fn invoke_tool(
    spec: &ToolSpec,
    id: Option<Value>,
    arguments: Value,
) -> (JsonRpcResponse, Option<DomainError>) {
    if !arguments.is_object() {
        return (
            invalid_params(id, spec.name, &["arguments must be an object".to_string()]),
            None,
        );
    }
    if let Err(issues) = validate_arguments(spec, &arguments) {
        return (invalid_params(id, spec.name, &issues), None);
    }

    let request = JsonRpcRequest {
        jsonrpc: "2.0".into(),
        id,
        method: spec.method.into(),
        params: arguments,
    };
    let mut response = (spec.handler)(request);

    if let Some(error) = response.error.take() {
        if is_reserved_code(error.code) {
            response.error = Some(error);
            return (response, None);
        }
        let mut text = error.message.clone();
        if let Some(data) = &error.data
            && let Ok(pretty) = serde_json::to_string_pretty(data)
        {
            text.push('\n');
            text.push_str(&pretty);
        }
        let domain = DomainError {
            message: error.message,
            data: error.data,
        };
        response.result = Some(json!({
            "content": [ { "type": "text", "text": text } ],
            "isError": true
        }));
        return (response, Some(domain));
    }

    (response, None)
}

/// Wrap a tool outcome for the `tools/call` surface. Successful results keep
/// the historical dual shape (`result.content` array + Gemini-style
/// `result.functionResponse`); converted domain failures carry the same
/// `content`/`isError` plus a structured `functionResponse.response`.
pub(crate) fn wrap_for_tools_call(
    mut response: JsonRpcResponse,
    domain_error: Option<DomainError>,
    tool_name: &str,
) -> JsonRpcResponse {
    if response.error.is_some() {
        return response;
    }
    let already_wrapped = response
        .result
        .as_ref()
        .and_then(|value| value.get("functionResponse"))
        .is_some();
    if already_wrapped {
        return response;
    }

    if let Some(domain) = domain_error
        && let Some(Value::Object(result)) = response.result.as_mut()
    {
        result.insert(
            "functionResponse".to_string(),
            json!({
                "name": tool_name,
                "response": {
                    "isError": true,
                    "message": domain.message,
                    "data": domain.data.unwrap_or(Value::Null)
                }
            }),
        );
        return response;
    }

    let inner_result = response.result.take().unwrap_or_else(|| json!({}));
    match inner_result {
        Value::Object(mut obj) => {
            let cloned = Value::Object(obj.clone());
            obj.insert(
                "functionResponse".to_string(),
                json!({
                    "name": tool_name,
                    "response": cloned,
                }),
            );
            response.result = Some(Value::Object(obj));
        }
        other => {
            response.result = Some(json!({
                "content": [
                    {
                        "type": "text",
                        "text": other.to_string()
                    }
                ],
                "functionResponse": {
                    "name": tool_name,
                    "response": other
                }
            }));
        }
    }
    response
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn dispatch_tool(tool: &str, arguments: Value, wrapped: bool) -> Value {
        let spec = find_tool_for_call(tool).unwrap();
        let (method, params) = if wrapped {
            ("tools/call", json!({"name": tool, "arguments": arguments}))
        } else {
            (spec.method, arguments)
        };
        let request = json!({"jsonrpc": "2.0", "id": 21, "method": method, "params": params});
        serde_json::from_str(&super::super::handle_json_line(&request.to_string())).unwrap()
    }

    #[test]
    fn registry_binds_every_advertised_tool_exactly_once() {
        let definitions = build_tool_definitions(None);
        let mut advertised: Vec<String> = definitions
            .iter()
            .map(|tool| {
                tool.get("name")
                    .and_then(|v| v.as_str())
                    .unwrap()
                    .to_string()
            })
            .collect();
        advertised.sort();
        assert_eq!(advertised.len(), 38, "advertised tool count changed");

        let mut registered: Vec<&str> = tool_specs().iter().map(|spec| spec.name).collect();
        registered.sort_unstable();
        let mut registered_unique = registered.clone();
        registered_unique.dedup();
        assert_eq!(
            registered.len(),
            registered_unique.len(),
            "duplicate registry names"
        );
        let mut registered_owned: Vec<String> = registered
            .into_iter()
            .map(|name| name.to_string())
            .collect();
        registered_owned.sort();
        assert_eq!(
            advertised, registered_owned,
            "registry must match tools/list"
        );

        let mut methods: Vec<&str> = tool_specs().iter().map(|spec| spec.method).collect();
        methods.sort_unstable();
        let methods_len = methods.len();
        methods.dedup();
        assert_eq!(methods_len, methods.len(), "duplicate direct methods");

        // Every registry entry resolves through both aliases.
        for spec in tool_specs() {
            assert_eq!(
                find_tool_by_method(spec.method).map(|found| found.name),
                Some(spec.name),
                "method {} must resolve",
                spec.method
            );
            assert_eq!(
                find_tool_by_method(spec.name).map(|found| found.name),
                Some(spec.name),
                "snake_case alias {} must resolve",
                spec.name
            );
        }
    }

    #[test]
    fn every_advertised_schema_is_regression_compiled() {
        for spec in tool_specs() {
            let schema =
                schema_for_tool(spec.name).unwrap_or_else(|| panic!("schema for {}", spec.name));
            assert!(
                unsupported_keywords(&schema).is_empty(),
                "schema for {} uses keywords the validator fails closed on: {:?}",
                spec.name,
                unsupported_keywords(&schema)
            );
            // {} must be classifiable (valid or with concrete issues), never
            // silently skipped.
            let _ = validate_instance(&schema, &json!({}));
        }
    }

    #[test]
    fn control_plane_methods_are_never_tools() {
        for method in [
            "initialize",
            "initialized",
            "notifications/initialized",
            "ping",
            "tools/list",
            "tools_list",
            "tools/call",
            "tools_call",
            "logging/setLevel",
            "logging_setLevel",
        ] {
            assert!(
                is_control_plane_method(&normalize_method(method)),
                "{method} must be control-plane"
            );
            let resolved = find_tool_for_call(method);
            assert!(resolved.is_err(), "{method} must not resolve as a tool");
        }

        // schema_discover stays a callable tool through both surfaces.
        assert_eq!(
            find_tool_for_call("schema_discover").unwrap().name,
            "schema_discover"
        );
        assert_eq!(
            find_tool_for_call("schema/discover").unwrap().name,
            "schema_discover"
        );
    }

    #[test]
    fn schema_validation_rejects_unknown_and_mistyped_arguments_before_handlers() {
        let spec = find_tool_by_method("sync/pull").unwrap();

        // "dryrun" typo must be rejected as an unknown property; dry_run
        // defaults to false inside handlers, so executing would perform a
        // live sync.
        let (response, domain) = invoke_tool(
            spec,
            Some(json!(1)),
            json!({"remote": "origin", "dryrun": true}),
        );
        assert!(domain.is_none());
        let error = response.error.expect("dryrun typo must be invalid params");
        assert_eq!(error.code, -32602);
        assert_eq!(error.message, "Invalid params");
        let issues = error
            .data
            .and_then(|data| data.get("issues").and_then(|v| v.as_array()).cloned())
            .unwrap_or_default();
        assert!(
            issues.iter().any(|issue| issue
                .as_str()
                .unwrap_or("")
                .contains("unknown property 'dryrun'")),
            "issues must name the typo: {issues:?}"
        );

        // Missing required remote.
        let (response, _) = invoke_tool(spec, Some(json!(2)), json!({}));
        assert_eq!(response.error.unwrap().code, -32602);

        // Nested type violations: task_update patch with a non-string tag.
        let update = find_tool_by_method("task/update").unwrap();
        let (response, _) = invoke_tool(
            update,
            Some(json!(3)),
            json!({"id": "MCP-1", "patch": {"tags": ["a", 5]}}),
        );
        let error = response.error.expect("nested type violation rejected");
        assert_eq!(error.code, -32602);
        let issues = error.data.unwrap();
        let issues = issues.get("issues").and_then(|v| v.as_array()).unwrap();
        assert!(
            issues
                .iter()
                .any(|issue| issue.as_str().unwrap_or("").contains("/patch/tags/1"))
        );

        // Enum enforcement on advertised enums.
        let list = find_tool_by_method("task/list").unwrap();
        let (response, _) = invoke_tool(list, Some(json!(4)), json!({"order": "diagonal"}));
        assert_eq!(response.error.unwrap().code, -32602);

        // Null is an advertised variant of the multi-value filters
        // (documented null-clear semantics) and must pass.
        let (response, _) = invoke_tool(list, Some(json!(5)), json!({"status": null}));
        assert!(
            response.error.is_none(),
            "null filter must stay valid: {:?}",
            response.error
        );
        // A non-string filter value still violates the oneOf branches.
        let (response, _) = invoke_tool(list, Some(json!(6)), json!({"status": 5}));
        assert_eq!(response.error.unwrap().code, -32602);
    }

    #[test]
    fn typo_dryrun_rejection_executes_nothing_and_direct_call_matches() {
        let guard = super::super::test_env::lock_tasks_dir();
        let tmp = tempfile::tempdir().unwrap();
        let tasks_dir = tmp.path().join(".tasks");
        std::fs::create_dir_all(&tasks_dir).unwrap();
        super::super::test_env::set_tasks_dir(&tasks_dir);

        // tools/call surface: the "dryrun" typo is rejected by the schema
        // layer naming the unknown property; the handler never runs, so no
        // remote request or local write can happen (dry_run defaults false
        // inside the handler, meaning a missed rejection would perform a
        // live sync).
        let spec = find_tool_by_method("sync/pull").unwrap();
        let (response, domain) = invoke_tool(
            spec,
            Some(json!(11)),
            json!({"remote": "origin", "dryrun": true}),
        );
        assert!(domain.is_none());
        let error = response.error.expect("dryrun typo must not execute");
        assert_eq!(error.code, -32602);
        let issues = error.data.unwrap();
        assert!(
            issues
                .get("issues")
                .and_then(|v| v.as_array())
                .unwrap()
                .iter()
                .any(|issue| issue
                    .as_str()
                    .unwrap_or("")
                    .contains("unknown property 'dryrun'"))
        );

        // Direct method dispatch enforces the identical contract.
        let (direct, domain) = invoke_tool(
            find_tool_by_method("sync_pull").unwrap(),
            Some(json!(12)),
            json!({"remote": "origin", "dryrun": true}),
        );
        assert!(domain.is_none());
        let direct_error = direct.error.expect("direct dispatch must match");
        assert_eq!(direct_error.code, -32602);
        assert_eq!(
            direct_error
                .data
                .as_ref()
                .and_then(|data| data.get("tool"))
                .and_then(|v| v.as_str()),
            Some("sync_pull")
        );

        // Belt and suspenders: nothing was written to the workspace.
        let entries: Vec<_> = std::fs::read_dir(&tasks_dir)
            .unwrap()
            .filter_map(|entry| entry.ok())
            .map(|entry| entry.file_name().to_string_lossy().to_string())
            .collect();
        assert!(entries.is_empty(), "no side effects: {entries:?}");

        drop(guard);
    }

    #[test]
    fn hinted_definitions_keep_identical_inputschemas() {
        use super::super::hints::EnumHints;

        // The registry caches hint-less schemas as canonical; hints must
        // only ever touch descriptions/hint metadata, never validation.
        let hints = EnumHints {
            statuses: vec!["Todo".to_string(), "Done".to_string()],
            priorities: vec!["Low".to_string()],
            types: vec!["Feature".to_string()],
            projects: vec!["MCP".to_string()],
            members: vec!["alice".to_string()],
            tags: vec!["cli".to_string()],
            custom_fields: vec!["severity".to_string()],
        };
        let hinted = build_tool_definitions(Some(&hints));
        let hintless = build_tool_definitions(None);
        assert_eq!(hinted.len(), hintless.len());
        for (with_hints, without) in hinted.iter().zip(hintless.iter()) {
            assert_eq!(with_hints.get("name"), without.get("name"));
            assert_eq!(
                with_hints.get("inputSchema"),
                without.get("inputSchema"),
                "hints altered inputSchema for {:?}",
                with_hints.get("name")
            );
        }
    }

    #[test]
    fn strict_rejections_are_pinned_on_both_surfaces() {
        let guard = super::super::test_env::lock_tasks_dir();
        let tmp = tempfile::tempdir().unwrap();
        let tasks_dir = tmp.path().join(".tasks");
        std::fs::create_dir_all(&tasks_dir).unwrap();
        std::fs::write(
            tasks_dir.join("config.yml"),
            "default:\n  project: MCP\nmembers:\n  - alice\n",
        )
        .unwrap();
        super::super::test_env::set_tasks_dir(&tasks_dir);

        let config_before = std::fs::read(tasks_dir.join("config.yml")).unwrap();
        let pins = [
            // sprint_delete advertised force historically but the handler
            // ignored it; the flag is intentionally rejected now.
            (
                "sprint_delete",
                json!({"sprint": "#1", "force": true}),
                "unknown property 'force'",
            ),
            // MCP task_delete has no confirmation concept; CLI-style force
            // must be rejected instead of silently ignored.
            (
                "task_delete",
                json!({"id": "MCP-1", "force": true}),
                "unknown property 'force'",
            ),
            // task_restore has no hard/dry-run flags on the MCP surface.
            (
                "task_restore",
                json!({"id": "MCP-1", "dry_run": true}),
                "unknown property 'dry_run'",
            ),
            (
                "config_set",
                json!({"values": {"default.project": 42}}),
                "/values/default.project: expected type string, found number",
            ),
        ];
        for (tool, arguments, expected_issue) in pins {
            for wrapped in [false, true] {
                let response = dispatch_tool(tool, arguments.clone(), wrapped);
                assert_eq!(
                    response["error"]["code"], -32602,
                    "{tool}, wrapped={wrapped}: {response}"
                );
                assert!(response.get("result").is_none());
                assert_eq!(response["error"]["data"]["tool"], tool);
                let issues = response["error"]["data"]["issues"].as_array().unwrap();
                assert!(
                    issues
                        .iter()
                        .any(|issue| issue.as_str().unwrap().contains(expected_issue)),
                    "{tool}: {issues:?}"
                );
            }
        }
        assert_eq!(
            std::fs::read(tasks_dir.join("config.yml")).unwrap(),
            config_before
        );
        assert_eq!(
            std::fs::read_dir(&tasks_dir).unwrap().count(),
            1,
            "rejected inputs must not create tasks, sprints, reports, or project overrides"
        );

        drop(guard);
    }

    #[test]
    fn project_list_and_backlog_no_longer_advertise_ignored_filters() {
        let guard = super::super::test_env::lock_tasks_dir();
        let tmp = tempfile::tempdir().unwrap();
        let tasks_dir = tmp.path().join(".tasks");
        std::fs::create_dir_all(&tasks_dir).unwrap();
        super::super::test_env::set_tasks_dir(&tasks_dir);

        // Handlers never read these filters, so advertising them promised
        // silently-unfiltered results; they are intentionally rejected now.
        for tool in ["project_list", "sprint_backlog"] {
            for ignored in [
                json!({"sort_by": "priority"}),
                json!({"order": "asc"}),
                json!({"due": "today"}),
                json!({"recent": "7d"}),
                json!({"needs": "effort"}),
            ] {
                for wrapped in [false, true] {
                    let response = dispatch_tool(tool, ignored.clone(), wrapped);
                    assert_eq!(
                        response["error"]["code"], -32602,
                        "{tool} {ignored}: {response}"
                    );
                    assert!(response.get("result").is_none());
                    let field = ignored.as_object().unwrap().keys().next().unwrap();
                    assert!(
                        response["error"]["data"]["issues"]
                            .to_string()
                            .contains(&format!("unknown property '{field}'"))
                    );
                }
            }
        }

        // Implemented parameters still work: plain calls succeed.
        for (tool, args) in [
            ("project_list", json!({"limit": 5, "cursor": 0})),
            ("sprint_backlog", json!({"project": "MCP"})),
        ] {
            let spec = find_tool_for_call(tool).unwrap();
            let (response, _) = invoke_tool(spec, Some(json!(24)), args.clone());
            assert!(
                response.error.is_none(),
                "{tool} {args} must stay accepted: {:?}",
                response.error
            );
        }

        drop(guard);
    }

    #[test]
    fn agent_and_sprint_domain_failures_convert_to_is_error() {
        let guard = super::super::test_env::lock_tasks_dir();
        let tmp = tempfile::tempdir().unwrap();
        let tasks_dir = tmp.path().join(".tasks");
        std::fs::create_dir_all(&tasks_dir).unwrap();
        std::fs::write(
            tasks_dir.join("config.yml"),
            "default:\n  project: MCP\nmembers:\n  - alice\n",
        )
        .unwrap();
        super::super::test_env::set_tasks_dir(&tasks_dir);

        for (tool, arguments, explanation) in [
            (
                "agent_run",
                json!({"ticket_id": "MCP-404", "prompt": "do it", "runner": "command"}),
                "Task not found: MCP-404",
            ),
            (
                "agent_run",
                json!({"ticket_id": "MCP-404", "prompt": "do it", "runner": "bogus-runner"}),
                "Unsupported runner 'bogus-runner'",
            ),
            (
                "agent_run",
                json!({"ticket_id": "MCP-404", "prompt": "do it", "agent": "missing-profile"}),
                "Unknown agent profile 'missing-profile'",
            ),
            (
                "agent_cancel",
                json!({"id": "job-does-not-exist"}),
                "Job not found",
            ),
            (
                "agent_send_message",
                json!({"id": "job-does-not-exist", "message": "hi"}),
                "Validation error: Job not found",
            ),
            (
                "sprint_update",
                json!({"sprint": "#424242", "label": "x"}),
                "Sprint #424242 not found",
            ),
            (
                "sprint_delete",
                json!({"sprint": "#424242"}),
                "Sprint #424242 not found",
            ),
        ] {
            for wrapped in [false, true] {
                let response = dispatch_tool(tool, arguments.clone(), wrapped);
                assert!(response.get("error").is_none(), "{tool}: {response}");
                let result = &response["result"];
                assert_eq!(result["isError"], true, "{tool}: {response}");
                assert!(
                    result["content"][0]["text"]
                        .as_str()
                        .unwrap()
                        .contains(explanation),
                    "{tool}: {response}"
                );
                if wrapped {
                    let envelope = &result["functionResponse"];
                    assert_eq!(envelope["name"], tool);
                    assert_eq!(envelope["response"]["isError"], true);
                    assert!(
                        envelope["response"].to_string().contains(explanation),
                        "{tool}: {response}"
                    );
                } else {
                    assert!(result.get("functionResponse").is_none());
                }
            }
        }

        drop(guard);
    }

    #[test]
    fn service_error_classification_preserves_internal_faults() {
        use crate::errors::LoTaRError;

        for (error, code) in [
            (
                LoTaRError::ValidationError("invalid profile".into()),
                -32000,
            ),
            (LoTaRError::TaskNotFound("MCP-404".into()), -32000),
            (LoTaRError::SprintNotFound(404), -32000),
            (LoTaRError::InvalidTaskId("invalid".into()), -32000),
            (LoTaRError::ProjectNotFound("MCP".into()), -32000),
            (LoTaRError::IndexError("invalid index".into()), -32000),
            (
                LoTaRError::SerializationError("cannot encode".into()),
                -32603,
            ),
            (
                LoTaRError::IoError(std::io::Error::other("cannot read")),
                -32603,
            ),
        ] {
            let response =
                super::super::domain_service_error(Some(json!(41)), "Service failed", &error);
            let failure = response.error.unwrap();
            assert_eq!(failure.code, code);
            assert_eq!(failure.message, "Service failed");
            assert_eq!(failure.data.unwrap()["message"], error.to_string());
        }
    }

    #[test]
    fn domain_failures_become_is_error_results() {
        let lock = super::super::test_env::lock_tasks_dir();
        let tmp = tempfile::tempdir().unwrap();
        let tasks_dir = tmp.path().join(".tasks");
        std::fs::create_dir_all(&tasks_dir).unwrap();
        std::fs::write(
            tasks_dir.join("config.yml"),
            "default:\n  project: MCP\nmembers:\n  - alice\n",
        )
        .unwrap();
        super::super::test_env::set_tasks_dir(&tasks_dir);

        let spec = find_tool_by_method("task/get").unwrap();
        let (response, domain) = invoke_tool(spec, Some(json!(7)), json!({"id": "MCP-404"}));
        let domain = domain.expect("task lookup failure converts to a domain error");
        let result = response.result.as_ref().unwrap();
        assert_eq!(result.get("isError"), Some(&json!(true)));
        assert!(response.error.is_none());
        let text = result
            .get("content")
            .and_then(|c| c.get(0))
            .and_then(|entry| entry.get("text"))
            .and_then(|v| v.as_str())
            .unwrap();
        assert!(text.contains(&domain.message));

        let wrapped = wrap_for_tools_call(response, Some(domain.clone()), "task_get");
        let result = wrapped.result.as_ref().unwrap();
        let fr = result.get("functionResponse").unwrap();
        assert_eq!(fr.get("name").and_then(|v| v.as_str()), Some("task_get"));
        let payload = fr.get("response").unwrap();
        assert_eq!(payload.get("isError"), Some(&json!(true)));
        assert!(payload.get("message").is_some());

        drop(lock);
    }
}

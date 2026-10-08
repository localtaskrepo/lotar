//! DEV-82: semantic REST contract tests for the repaired spec areas.
//!
//! These tests go beyond `$ref` existence: they read the *published*
//! `docs/openapi.json`, deserialize its request/response examples into the
//! real Rust DTOs, validate every example against its declared schema with a
//! small strict-3.1 ref-resolving validator (plus negative mutations proving
//! the validator catches drift), compare actual `ApiServer` responses —
//! empty and populated — against the declared schemas, and check that the
//! serialization shape (explicit null vs omitted key) of actual responses
//! matches the published examples. The quick-reference examples embedded in
//! the help/developers markdown are extracted and validated against the same
//! published schemas.
//!
//! Fixtures are isolated temporary workspaces (`LOTAR_TASKS_DIR`); no git
//! tooling is involved, and no `.tasks/DEV` data is read or written.

mod common;

use common::env_mutex::EnvVarGuard;
use common::openapi_semantics::SpecValidator;
use lotar::api_server::{ApiServer, HttpRequest, HttpResponse};
use lotar::api_types::{
    AttachmentRemoveRequest, AttachmentRemoveResponse, AttachmentUploadRequest,
    AttachmentUploadResponse, AutomationSimulateRequest, AutomationSimulateResponse,
    SprintCreateRequest, SprintCreateResponse, SprintDeleteRequest, SprintDeleteResponse,
    SprintUpdateRequest, SprintUpdateResponse, TaskListResponse,
};
use lotar::routes;
use serde_json::{Map, Value, json};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

/// The in-scope operations for DEV-82 (the repaired paths).
const IN_SCOPE: &[(&str, &str)] = &[
    ("get", "/api/tasks/list"),
    ("post", "/api/sprints/create"),
    ("post", "/api/sprints/update"),
    ("post", "/api/sprints/delete"),
    ("post", "/api/tasks/attachments/upload"),
    ("post", "/api/tasks/attachments/remove"),
    ("post", "/api/automation/simulate"),
];

// ---------------------------------------------------------------------------
// Fixture
// ---------------------------------------------------------------------------

struct Dev82Fixture {
    _tmp: tempfile::TempDir,
    tasks_dir: PathBuf,
    _guard_tasks: EnvVarGuard,
    _guard_fast: EnvVarGuard,
}

fn isolated_workspace() -> Dev82Fixture {
    let _guard_fast = EnvVarGuard::set("LOTAR_TEST_FAST_IO", "1");
    let tmp = tempfile::tempdir_in(Path::new(env!("CARGO_TARGET_TMPDIR"))).unwrap();
    let tasks_dir = tmp.path().join("ws").join(".tasks");
    std::fs::create_dir_all(&tasks_dir).unwrap();
    let _guard_tasks = EnvVarGuard::set("LOTAR_TASKS_DIR", &tasks_dir.to_string_lossy());
    Dev82Fixture {
        _tmp: tmp,
        tasks_dir,
        _guard_tasks,
        _guard_fast,
    }
}

fn server() -> ApiServer {
    let mut api = ApiServer::new();
    routes::initialize(&mut api);
    api
}

fn req(method: &str, path: &str, body: Value) -> HttpRequest {
    req_query(method, path, &[], body)
}

fn req_query(method: &str, path: &str, query: &[(&str, &str)], body: Value) -> HttpRequest {
    let mut q = HashMap::new();
    for (key, value) in query {
        q.insert((*key).to_string(), (*value).to_string());
    }
    HttpRequest {
        method: method.to_string(),
        path: path.to_string(),
        query: q,
        headers: HashMap::new(),
        body: serde_json::to_vec(&body).unwrap(),
    }
}

fn body_of(resp: &HttpResponse) -> Value {
    serde_json::from_slice(&resp.body).unwrap()
}

fn create_task(api: &ApiServer, project: &str, title: &str) -> String {
    let resp = api.handle_request(&req(
        "POST",
        "/api/tasks/add",
        json!({"title": title, "project": project}),
    ));
    assert_eq!(
        resp.status,
        201,
        "task create failed: {}",
        String::from_utf8_lossy(&resp.body)
    );
    body_of(&resp)["data"]["id"].as_str().unwrap().to_string()
}

fn base64_encode(data: &[u8]) -> String {
    const TABLE: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::new();
    for chunk in data.chunks(3) {
        let b = [
            chunk[0],
            *chunk.get(1).unwrap_or(&0),
            *chunk.get(2).unwrap_or(&0),
        ];
        let n = (u32::from(b[0]) << 16) | (u32::from(b[1]) << 8) | u32::from(b[2]);
        out.push(TABLE[(n >> 18) as usize & 63] as char);
        out.push(TABLE[(n >> 12) as usize & 63] as char);
        out.push(if chunk.len() > 1 {
            TABLE[(n >> 6) as usize & 63] as char
        } else {
            '='
        });
        out.push(if chunk.len() > 2 {
            TABLE[n as usize & 63] as char
        } else {
            '='
        });
    }
    out
}

// ---------------------------------------------------------------------------
// Published-spec accessors
// ---------------------------------------------------------------------------

fn operation<'a>(spec: &'a SpecValidator, method: &str, path: &str) -> &'a Value {
    spec.root()
        .pointer(&format!(
            "/paths/{}/{}",
            path.replace('~', "~0").replace('/', "~1"),
            method
        ))
        .unwrap_or_else(|| panic!("operation {method} {path} missing from published spec"))
}

fn request_media(op: &Value) -> Option<&Value> {
    op.get("requestBody")
        .and_then(|body| body.get("content"))
        .and_then(|content| content.get("application/json"))
}

fn response_media<'a>(op: &'a Value, status: &str) -> &'a Value {
    op.get("responses")
        .and_then(|responses| responses.get(status))
        .and_then(|response| response.get("content"))
        .and_then(|content| content.get("application/json"))
        .unwrap_or_else(|| panic!("no application/json body for status {status}"))
}

fn examples_under(media: &Value) -> Vec<(String, Value)> {
    media
        .get("examples")
        .and_then(Value::as_object)
        .map(|examples| {
            examples
                .iter()
                .map(|(name, body)| {
                    (
                        name.clone(),
                        body.get("value")
                            .cloned()
                            .unwrap_or_else(|| panic!("example {name} has no value")),
                    )
                })
                .collect()
        })
        .unwrap_or_default()
}

fn request_examples(spec: &SpecValidator, method: &str, path: &str) -> Vec<(String, Value)> {
    request_media(operation(spec, method, path))
        .map(examples_under)
        .unwrap_or_default()
}

fn response_examples(
    spec: &SpecValidator,
    method: &str,
    path: &str,
    status: &str,
) -> Vec<(String, Value)> {
    examples_under(response_media(operation(spec, method, path), status))
}

fn request_schema(spec: &SpecValidator, method: &str, path: &str) -> Value {
    request_media(operation(spec, method, path))
        .and_then(|media| media.get("schema"))
        .cloned()
        .unwrap_or_else(|| panic!("no request schema for {method} {path}"))
}

fn response_schema(spec: &SpecValidator, method: &str, path: &str, status: &str) -> Value {
    response_media(operation(spec, method, path), status)
        .get("schema")
        .cloned()
        .unwrap_or_else(|| panic!("no schema for {method} {path} {status}"))
}

fn assert_valid(spec: &SpecValidator, schema: &Value, value: &Value, context: &str) {
    spec.validate(schema, value)
        .unwrap_or_else(|e| panic!("{context} must validate against its declared schema: {e}"));
}

/// Assert two JSON objects carry exactly the same key set (explicit nulls
/// count as present). Roundtrip deserialization is lenient about null-vs-
/// omitted; this catches the difference the serializer actually produces.
fn assert_same_keys(actual: &Map<String, Value>, example: &Map<String, Value>, context: &str) {
    let actual_keys: HashSet<&str> = actual.keys().map(String::as_str).collect();
    let example_keys: HashSet<&str> = example.keys().map(String::as_str).collect();
    let missing: Vec<&str> = example_keys.difference(&actual_keys).copied().collect();
    let extra: Vec<&str> = actual_keys.difference(&example_keys).copied().collect();
    assert!(
        missing.is_empty() && extra.is_empty(),
        "{context}: key sets differ (missing from actual: {missing:?}, unexpected in actual: {extra:?});\nactual:  {actual:#?}\nexample: {example:#?}"
    );
}

fn named(examples: &[(String, Value)], name: &str) -> Value {
    examples
        .iter()
        .find(|(example_name, _)| example_name == name)
        .unwrap_or_else(|| panic!("example {name} missing"))
        .1
        .clone()
}

// ---------------------------------------------------------------------------
// 1. Published examples deserialize into the real Rust DTOs
// ---------------------------------------------------------------------------

#[test]
fn published_request_examples_deserialize_into_request_dtos() {
    let spec = SpecValidator::load_published();

    for (name, example) in request_examples(&spec, "post", "/api/sprints/create") {
        serde_json::from_value::<SprintCreateRequest>(example)
            .unwrap_or_else(|e| panic!("sprints/create example {name}: {e}"));
    }
    for (name, example) in request_examples(&spec, "post", "/api/sprints/update") {
        let parsed: SprintUpdateRequest = serde_json::from_value(example)
            .unwrap_or_else(|e| panic!("sprints/update example {name}: {e}"));
        if name == "nullClearCapacity" {
            assert_eq!(
                parsed.capacity_points,
                Some(None),
                "null capacity_points must deserialize as an explicit clear"
            );
        }
    }
    for (name, example) in request_examples(&spec, "post", "/api/sprints/delete") {
        let parsed: SprintDeleteRequest = serde_json::from_value(example)
            .unwrap_or_else(|e| panic!("sprints/delete example {name}: {e}"));
        assert!(parsed.sprint > 0, "delete example {name} needs a sprint id");
    }
    for (name, example) in request_examples(&spec, "post", "/api/tasks/attachments/upload") {
        let parsed: AttachmentUploadRequest = serde_json::from_value(example)
            .unwrap_or_else(|e| panic!("attachments/upload example {name}: {e}"));
        assert!(!parsed.content_base64.is_empty());
    }
    for (name, example) in request_examples(&spec, "post", "/api/tasks/attachments/remove") {
        serde_json::from_value::<AttachmentRemoveRequest>(example)
            .unwrap_or_else(|e| panic!("attachments/remove example {name}: {e}"));
    }
    for (name, example) in request_examples(&spec, "post", "/api/automation/simulate") {
        let parsed: AutomationSimulateRequest = serde_json::from_value(example)
            .unwrap_or_else(|e| panic!("automation/simulate example {name}: {e}"));
        assert!(!parsed.event.is_empty());
    }
}

#[test]
fn published_response_examples_deserialize_into_response_dtos() {
    let spec = SpecValidator::load_published();

    let list_examples = response_examples(&spec, "get", "/api/tasks/list", "200");
    assert_eq!(
        list_examples.len(),
        2,
        "tasks/list must publish empty and populated examples"
    );
    for (name, example) in &list_examples {
        let envelope = example
            .get("data")
            .expect("list example must envelope data");
        let parsed: TaskListResponse = serde_json::from_value(envelope.clone())
            .unwrap_or_else(|e| panic!("tasks/list example {name}: {e}"));
        match name.as_str() {
            "empty" => {
                assert!(parsed.tasks.is_empty());
                assert_eq!(parsed.total, 0);
            }
            "populated" => {
                assert_eq!(parsed.tasks.len(), 1);
                assert!(parsed.total >= 1);
            }
            other => panic!("unexpected tasks/list example name {other}"),
        }
    }

    for (name, example) in response_examples(&spec, "post", "/api/sprints/create", "200") {
        serde_json::from_value::<SprintCreateResponse>(example["data"].clone())
            .unwrap_or_else(|e| panic!("sprints/create example {name}: {e}"));
    }
    for (name, example) in response_examples(&spec, "post", "/api/sprints/update", "200") {
        serde_json::from_value::<SprintUpdateResponse>(example["data"].clone())
            .unwrap_or_else(|e| panic!("sprints/update example {name}: {e}"));
    }
    for (name, example) in response_examples(&spec, "post", "/api/sprints/delete", "200") {
        let parsed: SprintDeleteResponse = serde_json::from_value(example["data"].clone())
            .unwrap_or_else(|e| panic!("sprints/delete example {name}: {e}"));
        assert!(parsed.deleted);
    }
    for (name, example) in response_examples(&spec, "post", "/api/tasks/attachments/upload", "200")
    {
        let parsed: AttachmentUploadResponse = serde_json::from_value(example["data"].clone())
            .unwrap_or_else(|e| panic!("attachments/upload example {name}: {e}"));
        assert!(!parsed.stored_path.is_empty());
        assert!(parsed.attached);
    }
    for (name, example) in response_examples(&spec, "post", "/api/tasks/attachments/remove", "200")
    {
        let parsed: AttachmentRemoveResponse = serde_json::from_value(example["data"].clone())
            .unwrap_or_else(|e| panic!("attachments/remove example {name}: {e}"));
        assert!(parsed.deleted);
        assert!(!parsed.still_referenced);
    }

    let simulate = response_examples(&spec, "post", "/api/automation/simulate", "200");
    assert_eq!(
        simulate.len(),
        2,
        "simulate must publish matched and no-rules examples"
    );
    for (name, example) in &simulate {
        let data = example
            .get("data")
            .expect("simulate example must envelope data");
        for key in [
            "matched",
            "rule_name",
            "actions",
            "task_before",
            "task_after",
        ] {
            assert!(
                data.get(key).is_some(),
                "simulate example {name} must always carry {key}"
            );
        }
        let parsed: AutomationSimulateResponse = serde_json::from_value(data.clone())
            .unwrap_or_else(|e| panic!("automation/simulate example {name}: {e}"));
        match name.as_str() {
            "matched" => {
                assert!(parsed.matched);
                assert_eq!(
                    parsed.rule_name.as_deref(),
                    Some("Move verify work forward")
                );
                assert_eq!(parsed.actions.len(), 1);
                assert!(parsed.task_after.is_some());
            }
            "noRules" => {
                assert!(!parsed.matched);
                assert!(parsed.rule_name.is_none());
                assert!(parsed.actions.is_empty());
                assert!(parsed.task_after.is_none());
            }
            other => panic!("unexpected simulate example name {other}"),
        }
    }
}

// ---------------------------------------------------------------------------
// 2. Published examples validate against their declared schemas
// ---------------------------------------------------------------------------

#[test]
fn published_examples_validate_against_declared_schemas() {
    let spec = SpecValidator::load_published();

    for &(method, path) in IN_SCOPE {
        for (name, example) in request_examples(&spec, method, path) {
            assert_valid(
                &spec,
                &request_schema(&spec, method, path),
                &example,
                &format!("{method} {path} request example {name}"),
            );
        }
        for (name, example) in response_examples(&spec, method, path, "200") {
            assert_valid(
                &spec,
                &response_schema(&spec, method, path, "200"),
                &example,
                &format!("{method} {path} 200 example {name}"),
            );
        }
    }

    // Error examples must validate against ApiError.
    let remove_400 = response_media(
        operation(&spec, "post", "/api/tasks/attachments/remove"),
        "400",
    )
    .get("examples")
    .and_then(|examples| examples.get("wrongKind"))
    .and_then(|example| example.get("value"))
    .expect("attachments/remove 400 needs a wrong-kind example");
    assert_valid(
        &spec,
        spec.component("ApiError"),
        remove_400,
        "attachments/remove 400 example",
    );
}

/// Pin the exact spec repairs DEV-82 made, so a regression cannot silently
/// reintroduce the old (wrong) declarations.
#[test]
fn published_spec_declares_the_repaired_shapes() {
    let spec = SpecValidator::load_published();

    let tlr = spec.component("TaskListResponse");
    let required = tlr
        .get("required")
        .and_then(Value::as_array)
        .expect("TaskListResponse.required");
    assert_eq!(
        serde_json::to_string(required).unwrap(),
        r#"["total","limit","offset"]"#,
        "tasks must not be required: the server omits it on empty pages"
    );

    // Nullable composed $ref must be declared via anyOf, not a 3.0 sibling.
    let task_after = spec
        .component("AutomationSimulateResponse")
        .pointer("/properties/task_after");
    let composed = task_after
        .and_then(|node| node.get("anyOf"))
        .and_then(Value::as_array)
        .expect("task_after must compose TaskDTO with null via anyOf");
    assert!(
        composed
            .iter()
            .any(|branch| branch.get("type").and_then(Value::as_str) == Some("null")),
        "task_after anyOf must include the null branch"
    );
    assert!(
        composed.iter().any(|branch| branch.get("$ref").is_some()),
        "task_after anyOf must include the TaskDTO $ref branch"
    );

    // The event field must not carry a restrictive enum: legacy aliases are
    // accepted by the server.
    let event = spec
        .component("AutomationSimulateRequest")
        .pointer("/properties/event");
    assert!(
        event.and_then(|node| node.get("enum")).is_none(),
        "event enum would reject accepted legacy aliases"
    );
    assert!(
        event
            .and_then(|node| node.get("description"))
            .and_then(Value::as_str)
            .is_some_and(|d| d.contains("job_cancelled")),
        "event description must list the canonical events"
    );

    // Request null-as-omission unions on single-value Option fields; the
    // explicit-clear double options stay documented as clears.
    let create_label = spec
        .component("SprintCreateRequest")
        .pointer("/properties/label");
    assert_eq!(
        create_label.and_then(|n| n.get("type")),
        Some(&json!(["string", "null"])),
        "request Option fields accept null-as-omission and must declare the union"
    );
    let update_capacity = spec
        .component("SprintUpdateRequest")
        .pointer("/properties/capacity_points");
    assert_eq!(
        update_capacity.and_then(|n| n.get("type")),
        Some(&json!(["integer", "null"]))
    );
    assert!(
        update_capacity
            .and_then(|n| n.get("description"))
            .and_then(Value::as_str)
            .is_some_and(|d| d.contains("clears")),
        "double-option fields must document null as an explicit clear"
    );

    // Serialization-accurate required lists and request minimums.
    assert_eq!(
        spec.component("TaskRelationships").get("required"),
        Some(&json!([])),
        "every TaskRelationships member is omitted when empty"
    );
    assert_eq!(
        spec.component("TaskChangeLogEntry").get("required"),
        Some(&json!(["at"])),
        "changes is omitted when empty"
    );
    assert_eq!(
        spec.component("SprintUpdateRequest")
            .pointer("/properties/sprint/minimum"),
        Some(&json!(1)),
        "sprint ids must be >= 1 (0 returns 400 on update)"
    );
    assert_eq!(
        spec.component("SprintDeleteRequest")
            .pointer("/properties/sprint/minimum"),
        Some(&json!(1)),
        "sprint ids must be >= 1 (unknown/0 ids return 404 on delete)"
    );
}

/// Scoped dialect guard: the transitive closure reachable from the seven
/// in-scope operations (request bodies, all response statuses, and the
/// shared ApiError) must carry no inert OpenAPI 3.0 `nullable` keywords.
/// The remaining ~200 occurrences outside this closure are tracked as
/// DEV-83 dialect debt and are deliberately untouched.
#[test]
fn in_scope_closure_carries_no_legacy_nullable() {
    let spec = SpecValidator::load_published();
    let root = spec.root();

    let mut closure: HashSet<String> = HashSet::new();
    let mut offenders: Vec<String> = Vec::new();

    fn walk(
        node: &Value,
        root: &Value,
        closure: &mut HashSet<String>,
        offenders: &mut Vec<String>,
        trail: &str,
    ) {
        if let Value::Object(obj) = node {
            if obj.contains_key("nullable") {
                offenders.push(trail.to_string());
            }
            if let Some(Value::String(reference)) = obj.get("$ref") {
                let name = reference
                    .strip_prefix("#/components/schemas/")
                    .unwrap_or_default()
                    .to_string();
                if !name.is_empty()
                    && closure.insert(name.clone())
                    && let Some(component) = root.pointer(&format!("/components/schemas/{name}"))
                {
                    walk(
                        component,
                        root,
                        closure,
                        offenders,
                        &mut format!("schema:{name}"),
                    );
                }
            }
            for (key, value) in obj {
                walk(value, root, closure, offenders, &format!("{trail}.{key}"));
            }
        } else if let Value::Array(items) = node {
            for (index, item) in items.iter().enumerate() {
                walk(item, root, closure, offenders, &format!("{trail}[{index}]"));
            }
        }
    }

    for &(method, path) in IN_SCOPE {
        let op = operation(&spec, method, path);
        walk(
            op,
            root,
            &mut closure,
            &mut offenders,
            &format!("{method} {path}"),
        );
    }
    for (name, response) in root
        .pointer("/components/responses")
        .and_then(Value::as_object)
        .unwrap()
    {
        // Shared error responses referenced by the in-scope operations.
        if ["BadRequest", "NotFound", "InternalError"].contains(&name.as_str()) {
            walk(
                response,
                root,
                &mut closure,
                &mut offenders,
                &mut format!("response:{name}"),
            );
        }
    }

    assert!(
        offenders.is_empty(),
        "inert 3.0 nullable keywords inside the DEV-82 closure (DEV-83 debt is outside it): {offenders:#?}"
    );
    assert!(
        closure.contains("TaskDTO") && closure.contains("ApiError"),
        "closure computation unexpectedly thin: {closure:?}"
    );
}

// ---------------------------------------------------------------------------
// 3. The validator catches drift (negative mutations)
// ---------------------------------------------------------------------------

#[test]
fn validator_catches_contract_drift() {
    let spec = SpecValidator::load_published();

    // Missing envelope.
    let err = spec
        .validate_component(
            "EnvelopeTaskList",
            &json!({"total": 0, "limit": 50, "offset": 0}),
        )
        .unwrap_err();
    assert!(
        err.contains("data"),
        "envelope error should name the field: {err}"
    );

    // Wrong envelope shape: data as array instead of the page object.
    let err = spec
        .validate_component("EnvelopeTaskList", &json!({"data": []}))
        .unwrap_err();
    assert!(err.contains("expected type object"), "{err}");

    // Missing required meta field.
    let err = spec
        .validate_component("TaskListResponse", &json!({"limit": 50, "offset": 0}))
        .unwrap_err();
    assert!(err.contains("total"), "{err}");

    // Wrong field type inside the page.
    let err = spec
        .validate_component(
            "TaskListResponse",
            &json!({"total": 0, "limit": "50", "offset": 0}),
        )
        .unwrap_err();
    assert!(err.contains("limit"), "{err}");

    // Wrong element type in tasks[].
    let err = spec
        .validate_component(
            "TaskListResponse",
            &json!({"total": 1, "limit": 50, "offset": 0, "tasks": [42]}),
        )
        .unwrap_err();
    assert!(err.contains("tasks[0]"), "{err}");

    // Wrong nested field type on a TaskDTO entry.
    let err = spec
        .validate_component(
            "TaskListResponse",
            &json!({"total": 1, "limit": 50, "offset": 0, "tasks": [
                {"id": 7, "title": "t", "status": "Todo", "priority": "Medium",
                 "task_type": "Feature", "created": "x", "modified": "y"}
            ]}),
        )
        .unwrap_err();
    assert!(err.contains("tasks[0].id"), "{err}");

    // Non-null task_after must fail the composed nullable schema.
    let err = spec
        .validate_component(
            "AutomationSimulateResponse",
            &json!({
                "matched": true, "rule_name": "r", "actions": [],
                "task_before": {"id": "QA-1", "title": "t", "status": "Todo",
                                "priority": "Medium", "task_type": "Feature",
                                "created": "x", "modified": "y"},
                "task_after": "not-a-task"
            }),
        )
        .unwrap_err();
    assert!(err.contains("task_after"), "{err}");

    // A boolean is not an integer (serde_json pitfall the validator guards).
    let err = spec
        .validate_component("EnvelopeSprintDelete", &json!({"data": {"status": "ok", "deleted": true, "sprint_id": true, "removed_references": 0, "updated_tasks": 0}}))
        .unwrap_err();
    assert!(err.contains("sprint_id"), "{err}");

    // Missing required response member.
    let err = spec
        .validate_component(
            "AttachmentUploadResponse",
            &json!({"stored_path": "a.txt", "attached": true}),
        )
        .unwrap_err();
    assert!(err.contains("task"), "{err}");

    // The same documents with the drift removed must validate.
    assert_valid(
        &spec,
        spec.component("EnvelopeTaskList"),
        &json!({"data": {"total": 1, "limit": 50, "offset": 0, "tasks": [
            {"id": "QA-1", "title": "t", "status": "Todo", "priority": "Medium",
             "task_type": "Feature", "created": "x", "modified": "y"}
        ]}}),
        "repaired list envelope",
    );
    assert_valid(
        &spec,
        spec.component("AutomationSimulateResponse"),
        &json!({
            "matched": false, "rule_name": null, "actions": [],
            "task_before": {"id": "QA-1", "title": "t", "status": "Todo",
                            "priority": "Medium", "task_type": "Feature",
                            "created": "x", "modified": "y"},
            "task_after": null
        }),
        "nullable simulate response",
    );
}

/// The validator must fail loudly on unsupported constructs instead of
/// silently passing them (no false assurance).
#[test]
fn validator_enforces_the_declared_subset() {
    let spec = SpecValidator::load_published();

    // Type-array form: object branch still enforces required members.
    let schema = json!({"type": ["object", "null"], "required": ["a"]});
    assert!(spec.validate(&schema, &json!({"a": 1})).is_ok());
    assert!(spec.validate(&schema, &Value::Null).is_ok());
    let err = spec.validate(&schema, &json!({})).unwrap_err();
    assert!(err.contains("missing required field(s) a"), "{err}");

    // Non-null rejection through a null-union type.
    let schema = json!({"type": ["string", "null"]});
    let err = spec.validate(&schema, &json!(42)).unwrap_err();
    assert!(err.contains("expected type string|null"), "{err}");

    // anyOf branches AND sibling constraints both apply.
    let schema = json!({"anyOf": [{"type": "string"}, {"type": "integer"}], "minimum": 5});
    assert!(spec.validate(&schema, &json!(7)).is_ok());
    assert!(spec.validate(&schema, &json!("x")).is_ok());
    let err = spec.validate(&schema, &json!(3)).unwrap_err();
    assert!(err.contains("below minimum 5"), "{err}");

    // allOf branches all apply.
    let schema = json!({"allOf": [{"type": "integer"}, {"minimum": 2}]});
    assert!(spec.validate(&schema, &json!(1)).is_err());

    // additionalProperties: false.
    let schema = json!({"type": "object", "properties": {"a": {}}, "additionalProperties": false});
    let err = spec
        .validate(&schema, &json!({"a": 1, "b": 2}))
        .unwrap_err();
    assert!(err.contains("unexpected field b"), "{err}");

    // minimum, enum (including enum rejecting null).
    let err = spec
        .validate(&json!({"type": "integer", "minimum": 1}), &json!(0))
        .unwrap_err();
    assert!(err.contains("below minimum 1"), "{err}");
    let err = spec
        .validate(&json!({"enum": ["A", "B"]}), &json!("C"))
        .unwrap_err();
    assert!(err.contains("not in enum"), "{err}");
    assert!(
        spec.validate(&json!({"enum": ["A", "B"]}), &json!(null))
            .is_err()
    );

    // Dangling $ref must surface as an error, not a pass.
    let err = spec
        .validate(
            &json!({"$ref": "#/components/schemas/DoesNotExist"}),
            &json!({}),
        )
        .unwrap_err();
    assert!(err.contains("dangling ref"), "{err}");

    // Unsupported validation keywords fail instead of being ignored.
    let err = spec
        .validate(&json!({"type": "string", "pattern": "^x+$"}), &json!("yyy"))
        .unwrap_err();
    assert!(
        err.contains("unsupported validation keyword 'pattern'"),
        "{err}"
    );

    // The inert 3.0 `nullable` keyword is rejected (this spec declares 3.1).
    let err = spec
        .validate(&json!({"type": "string", "nullable": true}), &json!(null))
        .unwrap_err();
    assert!(err.contains("nullable"), "{err}");

    // $ref composes with siblings: both the target and the extra sibling
    // requirement apply.
    let schema =
        json!({"$ref": "#/components/schemas/EnvelopeTaskList", "required": ["data", "bonus"]});
    let err = spec
        .validate(
            &schema,
            &json!({"data": {"total": 0, "limit": 1, "offset": 0}}),
        )
        .unwrap_err();
    assert!(
        err.contains("bonus"),
        "sibling keywords must apply beside $ref: {err}"
    );
}

// ---------------------------------------------------------------------------
// 4. Actual ApiServer responses match the declared contracts
// ---------------------------------------------------------------------------

#[test]
fn actual_task_list_responses_match_declared_schema() {
    let fx = isolated_workspace();
    let api = server();
    let spec = SpecValidator::load_published();
    let envelope_schema = response_schema(&spec, "get", "/api/tasks/list", "200");
    let list_examples = response_examples(&spec, "get", "/api/tasks/list", "200");

    // Empty workspace: the tasks key is omitted entirely, exactly like the
    // published empty example.
    let resp = api.handle_request(&req("GET", "/api/tasks/list", json!({})));
    assert_eq!(resp.status, 200);
    let body = body_of(&resp);
    assert!(
        body["data"].get("tasks").is_none(),
        "empty page must omit tasks, got {}",
        body["data"]
    );
    assert_valid(&spec, &envelope_schema, &body, "empty task list");
    let parsed: TaskListResponse = serde_json::from_value(body["data"].clone()).unwrap();
    assert_eq!((parsed.total, parsed.tasks.len()), (0, 0));
    assert_same_keys(
        body["data"].as_object().unwrap(),
        named(&list_examples, "empty")["data"].as_object().unwrap(),
        "empty list data keys vs published empty example",
    );

    // Populated workspace: full TaskDTO entries under tasks[].
    create_task(&api, "QA", "Contract semantics one");
    create_task(&api, "QA", "Contract semantics two");
    let resp = api.handle_request(&req("GET", "/api/tasks/list", json!({})));
    assert_eq!(resp.status, 200);
    let body = body_of(&resp);
    assert_valid(&spec, &envelope_schema, &body, "populated task list");
    let parsed: TaskListResponse = serde_json::from_value(body["data"].clone()).unwrap();
    assert_eq!(parsed.total, 2);
    assert_eq!(parsed.tasks.len(), 2);
    for task in &parsed.tasks {
        assert_valid(
            &spec,
            spec.component("TaskDTO"),
            &serde_json::to_value(task).unwrap(),
            "actual task entry",
        );
    }
    assert_same_keys(
        body["data"].as_object().unwrap(),
        named(&list_examples, "populated")["data"]
            .as_object()
            .unwrap(),
        "populated list data keys vs published populated example",
    );

    // Pagination keeps the same envelope: full total, sliced page.
    let resp = api.handle_request(&req_query(
        "GET",
        "/api/tasks/list",
        &[("limit", "1")],
        json!({}),
    ));
    assert_eq!(resp.status, 200, "{}", String::from_utf8_lossy(&resp.body));
    let parsed: TaskListResponse = serde_json::from_value(body_of(&resp)["data"].clone()).unwrap();
    assert_eq!(parsed.total, 2);
    assert_eq!(parsed.tasks.len(), 1);

    let _ = fx;
}

#[test]
fn actual_sprint_crud_responses_match_declared_contracts() {
    let fx = isolated_workspace();
    std::fs::write(
        fx.tasks_dir.join("config.yml"),
        "sprints:\n  defaults:\n    capacity_points: 20\n",
    )
    .unwrap();
    let api = server();
    let spec = SpecValidator::load_published();
    let create_examples = response_examples(&spec, "post", "/api/sprints/create", "200");
    let update_examples = response_examples(&spec, "post", "/api/sprints/update", "200");
    let delete_examples = response_examples(&spec, "post", "/api/sprints/delete", "200");

    // Create with configured defaults applied.
    let example = &request_examples(&spec, "post", "/api/sprints/create")
        .into_iter()
        .find(|(name, _)| name == "withDefaults")
        .expect("withDefaults example")
        .1;
    let resp = api.handle_request(&req("POST", "/api/sprints/create", example.clone()));
    assert_eq!(resp.status, 200, "{}", String::from_utf8_lossy(&resp.body));
    let body = body_of(&resp);
    assert_valid(
        &spec,
        &response_schema(&spec, "post", "/api/sprints/create", "200"),
        &body,
        "sprint create",
    );
    let created: SprintCreateResponse = serde_json::from_value(body["data"].clone()).unwrap();
    assert!(
        created
            .applied_defaults
            .iter()
            .any(|d| d == "capacity_points"),
        "configured default must be reported: {:?}",
        created.applied_defaults
    );
    assert_eq!(created.sprint.capacity_points, Some(20));
    assert_eq!(created.sprint.state, "pending");
    let sprint_id = created.sprint.id;
    // Serialization shape matches the published created example (key sets).
    assert_same_keys(
        body["data"].as_object().unwrap(),
        named(&create_examples, "created")["data"]
            .as_object()
            .unwrap(),
        "create data keys (applied_defaults present) vs published example",
    );
    assert_same_keys(
        body["data"]["sprint"].as_object().unwrap(),
        named(&create_examples, "created")["data"]["sprint"]
            .as_object()
            .unwrap(),
        "create sprint keys vs published example",
    );

    // Create with skip_defaults: empty vectors are OMITTED (not []).
    let skip = &request_examples(&spec, "post", "/api/sprints/create")
        .into_iter()
        .find(|(name, _)| name == "skipDefaults")
        .expect("skipDefaults example")
        .1;
    let resp = api.handle_request(&req("POST", "/api/sprints/create", skip.clone()));
    assert_eq!(resp.status, 200);
    let skip_body = body_of(&resp);
    let skipped: SprintCreateResponse = serde_json::from_value(skip_body["data"].clone()).unwrap();
    assert!(skipped.applied_defaults.is_empty());
    assert_eq!(skipped.sprint.capacity_points, None);
    assert!(
        skip_body["data"].get("applied_defaults").is_none()
            && skip_body["data"].get("warnings").is_none(),
        "empty applied_defaults/warnings must be omitted, got {}",
        skip_body["data"]
    );

    // Update with null capacity clear: cleared value and empty warnings are
    // OMITTED, matching the published example (which shows actual output).
    let null_clear = json!({"sprint": sprint_id, "capacity_points": null});
    let resp = api.handle_request(&req("POST", "/api/sprints/update", null_clear));
    assert_eq!(resp.status, 200, "{}", String::from_utf8_lossy(&resp.body));
    let body = body_of(&resp);
    assert_valid(
        &spec,
        &response_schema(&spec, "post", "/api/sprints/update", "200"),
        &body,
        "sprint update",
    );
    let updated: SprintUpdateResponse = serde_json::from_value(body["data"].clone()).unwrap();
    assert_eq!(updated.sprint.capacity_points, None, "null must clear");
    assert!(
        body["data"].get("warnings").is_none()
            && body["data"]["sprint"].get("capacity_points").is_none(),
        "cleared capacity and empty warnings must be omitted, got {}",
        body["data"]
    );
    assert_same_keys(
        body["data"].as_object().unwrap(),
        named(&update_examples, "updated")["data"]
            .as_object()
            .unwrap(),
        "update data keys vs published example",
    );
    assert_same_keys(
        body["data"]["sprint"].as_object().unwrap(),
        named(&update_examples, "updated")["data"]["sprint"]
            .as_object()
            .unwrap(),
        "update sprint keys vs published example",
    );

    // Update with sprint 0: 400 (documented minimum is 1).
    let resp = api.handle_request(&req(
        "POST",
        "/api/sprints/update",
        json!({"sprint": 0, "label": "Zero"}),
    ));
    assert_eq!(resp.status, 400, "sprint 0 update must be 400");

    // Update unknown sprint: 404 (typed NOT_FOUND, DEV-58).
    let resp = api.handle_request(&req(
        "POST",
        "/api/sprints/update",
        json!({"sprint": 9999, "label": "Ghost"}),
    ));
    assert_eq!(resp.status, 404, "unknown sprint update must be 404");
    assert_valid(
        &spec,
        spec.component("ApiError"),
        &body_of(&resp),
        "update 404 body",
    );

    // Delete with the legacy {id, confirm} body: 400, no side effects.
    let resp = api.handle_request(&req(
        "POST",
        "/api/sprints/delete",
        json!({"id": sprint_id, "confirm": true}),
    ));
    assert_eq!(resp.status, 400, "legacy delete body must be rejected");
    assert_valid(
        &spec,
        spec.component("ApiError"),
        &body_of(&resp),
        "delete 400 body",
    );

    // Delete with the declared {sprint, cleanup_missing} body: 200; the
    // response carries the integrity cleanup summary, like the example.
    let resp = api.handle_request(&req(
        "POST",
        "/api/sprints/delete",
        json!({"sprint": sprint_id, "cleanup_missing": true}),
    ));
    assert_eq!(resp.status, 200, "{}", String::from_utf8_lossy(&resp.body));
    let body = body_of(&resp);
    assert_valid(
        &spec,
        &response_schema(&spec, "post", "/api/sprints/delete", "200"),
        &body,
        "sprint delete",
    );
    let deleted: SprintDeleteResponse = serde_json::from_value(body["data"].clone()).unwrap();
    assert!(deleted.deleted);
    assert_eq!(deleted.sprint_id, sprint_id);
    assert!(
        deleted.integrity.is_some(),
        "cleanup_missing=true must report the cleanup summary"
    );
    assert_same_keys(
        body["data"].as_object().unwrap(),
        named(&delete_examples, "deleted")["data"]
            .as_object()
            .unwrap(),
        "delete data keys vs published example",
    );

    // Delete again: 404.
    let resp = api.handle_request(&req(
        "POST",
        "/api/sprints/delete",
        json!({"sprint": sprint_id, "cleanup_missing": true}),
    ));
    assert_eq!(resp.status, 404, "unknown sprint delete must be 404");
    assert_valid(
        &spec,
        spec.component("ApiError"),
        &body_of(&resp),
        "delete 404 body",
    );

    // Delete without cleanup_missing: integrity is omitted when there is
    // nothing to report.
    let resp = api.handle_request(&req(
        "POST",
        "/api/sprints/create",
        json!({"label": "NoCleanup"}),
    ));
    assert_eq!(resp.status, 200);
    let no_cleanup_id = body_of(&resp)["data"]["sprint"]["id"].as_u64().unwrap() as u32;
    let resp = api.handle_request(&req(
        "POST",
        "/api/sprints/delete",
        json!({"sprint": no_cleanup_id}),
    ));
    assert_eq!(resp.status, 200);
    assert!(
        body_of(&resp)["data"].get("integrity").is_none(),
        "no cleanup and no missing refs means no integrity block"
    );
}

#[test]
fn actual_attachment_responses_match_declared_contracts() {
    let fx = isolated_workspace();
    let api = server();
    let spec = SpecValidator::load_published();
    let upload_examples = response_examples(&spec, "post", "/api/tasks/attachments/upload", "200");
    let id = create_task(&api, "QA", "Attachment contract target");

    // Upload using the published request example (id bound to the fixture task).
    let mut example = request_examples(&spec, "post", "/api/tasks/attachments/upload")
        .into_iter()
        .next()
        .expect("upload example")
        .1;
    example["id"] = json!(id);
    let resp = api.handle_request(&req(
        "POST",
        "/api/tasks/attachments/upload",
        example.clone(),
    ));
    assert_eq!(resp.status, 200, "{}", String::from_utf8_lossy(&resp.body));
    let body = body_of(&resp);
    assert_valid(
        &spec,
        &response_schema(&spec, "post", "/api/tasks/attachments/upload", "200"),
        &body,
        "attachment upload",
    );
    let uploaded: AttachmentUploadResponse = serde_json::from_value(body["data"].clone()).unwrap();
    assert!(uploaded.attached);
    assert!(!uploaded.stored_path.is_empty());
    assert!(
        uploaded
            .task
            .references
            .iter()
            .any(|r| r.attachment.as_deref() == Some(uploaded.stored_path.as_str())),
        "typed attachment reference must be on the task: {:?}",
        uploaded.task.references
    );
    assert_same_keys(
        body["data"].as_object().unwrap(),
        named(&upload_examples, "uploaded")["data"]
            .as_object()
            .unwrap(),
        "upload data keys vs published example",
    );
    let blob = fx
        .tasks_dir
        .join("@attachments")
        .join(&uploaded.stored_path);
    assert!(blob.exists(), "blob must exist at {}", blob.display());

    // Wrong-kind body (upload-shaped): 400, blob untouched.
    let resp = api.handle_request(&req(
        "POST",
        "/api/tasks/attachments/remove",
        json!({"id": id, "filename": "release-notes.md", "content_base64": "aGk="}),
    ));
    assert_eq!(resp.status, 400, "wrong-kind remove body must be rejected");
    assert_valid(
        &spec,
        spec.component("ApiError"),
        &body_of(&resp),
        "remove 400 body",
    );
    assert!(
        blob.exists(),
        "blob must be untouched after wrong-kind rejection"
    );

    // Typed remove via the published example shape: 200, blob reclaimed.
    let resp = api.handle_request(&req(
        "POST",
        "/api/tasks/attachments/remove",
        json!({"id": id, "stored_path": uploaded.stored_path}),
    ));
    assert_eq!(resp.status, 200, "{}", String::from_utf8_lossy(&resp.body));
    let body = body_of(&resp);
    assert_valid(
        &spec,
        &response_schema(&spec, "post", "/api/tasks/attachments/remove", "200"),
        &body,
        "attachment remove",
    );
    let removed: AttachmentRemoveResponse = serde_json::from_value(body["data"].clone()).unwrap();
    assert!(removed.deleted);
    assert!(!removed.still_referenced);
    assert!(!blob.exists(), "unreferenced blob must be reclaimed");

    // Removing again: typed-membership 400, nothing left to touch.
    let resp = api.handle_request(&req(
        "POST",
        "/api/tasks/attachments/remove",
        json!({"id": id, "stored_path": uploaded.stored_path}),
    ));
    assert_eq!(resp.status, 400, "membership must be re-checked");
    let body = body_of(&resp);
    assert_valid(
        &spec,
        spec.component("ApiError"),
        &body,
        "membership 400 body",
    );
    assert!(
        body["error"]["message"]
            .as_str()
            .is_some_and(|m| m.contains("does not reference attachment")),
        "membership failure must explain itself: {body}"
    );
}

#[test]
fn actual_attachment_refcount_response_matches_declared_contract() {
    let fx = isolated_workspace();
    let api = server();
    let spec = SpecValidator::load_published();
    let first = create_task(&api, "QA", "Shares the blob");
    let second = create_task(&api, "QA", "Also shares the blob");

    let payload = json!({
        "id": first,
        "filename": "shared.txt",
        "content_base64": base64_encode(b"shared content"),
    });
    let resp = api.handle_request(&req("POST", "/api/tasks/attachments/upload", payload));
    assert_eq!(resp.status, 200);
    let stored = body_of(&resp)["data"]["stored_path"]
        .as_str()
        .unwrap()
        .to_string();

    let payload = json!({
        "id": second,
        "filename": "shared.txt",
        "content_base64": base64_encode(b"shared content"),
    });
    let resp = api.handle_request(&req("POST", "/api/tasks/attachments/upload", payload));
    assert_eq!(resp.status, 200, "{}", String::from_utf8_lossy(&resp.body));
    assert_eq!(
        body_of(&resp)["data"]["stored_path"].as_str(),
        Some(stored.as_str()),
        "identical content must dedupe to the same stored path"
    );
    let blob = fx.tasks_dir.join("@attachments").join(&stored);

    // Removing from the first task keeps the blob (second still references it).
    let resp = api.handle_request(&req(
        "POST",
        "/api/tasks/attachments/remove",
        json!({"id": first, "stored_path": stored}),
    ));
    assert_eq!(resp.status, 200);
    let body = body_of(&resp);
    assert_valid(
        &spec,
        &response_schema(&spec, "post", "/api/tasks/attachments/remove", "200"),
        &body,
        "refcount remove",
    );
    let removed: AttachmentRemoveResponse = serde_json::from_value(body["data"].clone()).unwrap();
    assert!(!removed.deleted);
    assert!(removed.still_referenced);
    assert!(blob.exists(), "still-referenced blob must survive");

    // Removing from the last task reclaims it.
    let resp = api.handle_request(&req(
        "POST",
        "/api/tasks/attachments/remove",
        json!({"id": second, "stored_path": stored}),
    ));
    assert_eq!(resp.status, 200);
    let removed: AttachmentRemoveResponse =
        serde_json::from_value(body_of(&resp)["data"].clone()).unwrap();
    assert!(removed.deleted);
    assert!(!removed.still_referenced);
    assert!(!blob.exists());
}

#[test]
fn actual_automation_simulate_responses_match_declared_contract() {
    let fx = isolated_workspace();
    let api = server();
    let spec = SpecValidator::load_published();
    let envelope = response_schema(&spec, "post", "/api/automation/simulate", "200");
    let simulate_examples = response_examples(&spec, "post", "/api/automation/simulate", "200");
    let ticket = create_task(&api, "QA", "Simulate contract target");

    // No rules configured: all five keys present as explicit nulls/empty.
    let mut example = request_examples(&spec, "post", "/api/automation/simulate")
        .into_iter()
        .find(|(name, _)| name == "canonicalEvent")
        .expect("canonicalEvent example")
        .1;
    example["ticket_id"] = json!(ticket);
    let resp = api.handle_request(&req("POST", "/api/automation/simulate", example));
    assert_eq!(resp.status, 200, "{}", String::from_utf8_lossy(&resp.body));
    let body = body_of(&resp);
    assert_valid(&spec, &envelope, &body, "simulate without rules");
    let data = body["data"].as_object().unwrap();
    let mut keys: Vec<&str> = data.keys().map(String::as_str).collect();
    keys.sort_unstable();
    assert_eq!(
        keys,
        vec![
            "actions",
            "matched",
            "rule_name",
            "task_after",
            "task_before"
        ],
        "simulate response must always carry exactly the five contract keys"
    );
    assert_eq!(body["data"]["matched"], json!(false));
    assert_eq!(body["data"]["rule_name"], json!(null));
    assert_eq!(body["data"]["task_after"], json!(null));
    assert!(
        data.contains_key("rule_name") && data.contains_key("task_after"),
        "unmatched simulate must emit EXPLICIT nulls, not omitted keys"
    );
    assert_same_keys(
        data,
        named(&simulate_examples, "noRules")["data"]
            .as_object()
            .unwrap(),
        "no-rules simulate data keys vs published noRules example",
    );
    let parsed: AutomationSimulateResponse = serde_json::from_value(body["data"].clone()).unwrap();
    assert!(!parsed.matched);
    assert!(parsed.task_after.is_none());

    // Project-scoped rules from the ticket's project; body project is ignored.
    std::fs::create_dir_all(fx.tasks_dir.join("QA")).unwrap();
    std::fs::write(
        fx.tasks_dir.join("QA").join("automation.yml"),
        "automation:\n  rules:\n    \
         - name: QA verify flow\n      when:\n        status: Todo\n      on:\n        updated:\n          set:\n            status: InProgress\n    \
         - name: QA job kickoff\n      when:\n        status: Todo\n      on:\n          job_started:\n            set:\n              priority: High\n",
    )
    .unwrap();
    std::fs::write(
        fx.tasks_dir.join("automation.yml"),
        "automation:\n  rules:\n    - name: global catchall\n      on:\n        updated:\n          set:\n            status: Done\n",
    )
    .unwrap();

    let resp = api.handle_request(&req(
        "POST",
        "/api/automation/simulate",
        json!({"ticket_id": ticket, "event": "updated", "project": "OTHER"}),
    ));
    assert_eq!(resp.status, 200, "{}", String::from_utf8_lossy(&resp.body));
    let body = body_of(&resp);
    assert_valid(&spec, &envelope, &body, "simulate with project rule");
    let parsed: AutomationSimulateResponse = serde_json::from_value(body["data"].clone()).unwrap();
    assert_eq!(
        parsed.rule_name.as_deref(),
        Some("QA verify flow"),
        "scope must follow the ticket's project, and body project must be ignored"
    );
    assert!(parsed.matched);
    assert_eq!(parsed.actions.len(), 1);
    assert_eq!(parsed.actions[0].action, "set_status");
    let before = parsed.task_before.as_ref().unwrap();
    let after = parsed.task_after.as_ref().unwrap();
    assert_eq!(before.status.to_string(), "Todo");
    assert_eq!(after.status.to_string(), "InProgress");
    assert_ne!(after.status.to_string(), "Done");
    assert_same_keys(
        body["data"].as_object().unwrap(),
        named(&simulate_examples, "matched")["data"]
            .as_object()
            .unwrap(),
        "matched simulate data keys vs published matched example",
    );

    // Legacy case-insensitive alias is accepted and routed to its event hook.
    let resp = api.handle_request(&req(
        "POST",
        "/api/automation/simulate",
        json!({"ticket_id": ticket, "event": "JOB_START"}),
    ));
    assert_eq!(resp.status, 200, "legacy alias must be accepted");
    let parsed: AutomationSimulateResponse =
        serde_json::from_value(body_of(&resp)["data"].clone()).unwrap();
    assert_eq!(parsed.rule_name.as_deref(), Some("QA job kickoff"));
    assert_eq!(
        parsed.task_after.as_ref().unwrap().priority.to_string(),
        "High"
    );

    // The retired `change` request shape: 400.
    let resp = api.handle_request(&req(
        "POST",
        "/api/automation/simulate",
        json!({"ticket_id": ticket, "change": {"status": "Done"}}),
    ));
    assert_eq!(resp.status, 400, "change-based body must be rejected");
    assert_valid(
        &spec,
        spec.component("ApiError"),
        &body_of(&resp),
        "simulate 400 body",
    );

    // Unknown event: 400.
    let resp = api.handle_request(&req(
        "POST",
        "/api/automation/simulate",
        json!({"ticket_id": ticket, "event": "bogus"}),
    ));
    assert_eq!(resp.status, 400);
    assert_valid(
        &spec,
        spec.component("ApiError"),
        &body_of(&resp),
        "unknown event 400 body",
    );
}

// ---------------------------------------------------------------------------
// 5. Quick-reference examples are verified, not just claimed
// ---------------------------------------------------------------------------

/// Extract the `{ "data": ... }` JSON documents embedded in a quick-reference
/// page by brace matching (the examples must be literal, valid JSON).
fn quickref_data_examples(md_path: &Path) -> Vec<Value> {
    let text = std::fs::read_to_string(md_path)
        .unwrap_or_else(|e| panic!("read {}: {e}", md_path.display()));
    let mut found = Vec::new();
    let mut cursor = 0;
    while let Some(start) = text[cursor..].find("{ \"data\":") {
        let start = cursor + start;
        let mut depth = 0usize;
        let mut in_string = false;
        let mut escaped = false;
        for (offset, ch) in text[start..].char_indices() {
            match ch {
                '"' if !escaped => in_string = !in_string,
                '\\' if in_string => escaped = !escaped,
                '{' if !in_string => depth += 1,
                '}' if !in_string => {
                    depth -= 1;
                    if depth == 0 {
                        let document = &text[start..start + offset + 1];
                        found.push(serde_json::from_str(document).unwrap_or_else(|e| {
                            panic!(
                                "{} embeds invalid JSON `{document}`: {e}",
                                md_path.display()
                            )
                        }));
                        cursor = start + offset + 1;
                        break;
                    }
                }
                _ => escaped = false,
            }
            if ch != '\\' {
                escaped = false;
            }
        }
        if depth != 0 {
            panic!(
                "{} has an unbalanced example starting at byte {start}",
                md_path.display()
            );
        }
    }
    found
}

#[test]
fn quick_reference_examples_validate_and_cover_the_repaired_scope() {
    let spec = SpecValidator::load_published();
    let manifest = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));

    for relative in [
        "docs/help/api-quick-reference.md",
        "docs/developers/api-quick-reference.md",
    ] {
        let md_path = manifest.join(relative);
        let text = std::fs::read_to_string(&md_path)
            .unwrap_or_else(|e| panic!("read {}: {e}", md_path.display()));

        // Every repaired endpoint is listed in BOTH quick references.
        for endpoint in [
            "/api/tasks/list",
            "/api/sprints/create",
            "/api/sprints/update",
            "/api/sprints/delete",
            "/api/tasks/attachments/upload",
            "/api/tasks/attachments/remove",
            "/api/automation/simulate",
        ] {
            assert!(text.contains(endpoint), "{relative} must list {endpoint}",);
        }

        // The embedded examples are literal, valid JSON and validate against
        // the published envelope schema (empty page omits tasks, populated
        // page carries a complete TaskDTO).
        let examples = quickref_data_examples(&md_path);
        assert_eq!(
            examples.len(),
            2,
            "{relative} must embed exactly the empty and populated examples"
        );
        for example in &examples {
            assert_valid(
                &spec,
                &response_schema(&spec, "get", "/api/tasks/list", "200"),
                example,
                &format!("{relative} embedded example"),
            );
        }
        assert!(
            examples[0]["data"].get("tasks").is_none(),
            "{relative} empty example must omit tasks"
        );
        assert!(
            examples[1]["data"].get("tasks").is_some(),
            "{relative} populated example must carry tasks"
        );
    }
}

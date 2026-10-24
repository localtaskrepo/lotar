mod common;

use common::env_mutex::EnvVarGuard;
use common::openapi_semantics::SpecValidator;
use lotar::api_server::{ApiServer, HttpRequest};
use lotar::storage::manager::Storage;
use serde_json::{Value, json};
use std::collections::HashMap;
use std::path::PathBuf;

struct Fixture {
    _tmp: tempfile::TempDir,
    _env: EnvVarGuard,
    root: PathBuf,
    api: ApiServer,
}

impl Fixture {
    fn new() -> Self {
        let tmp = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).unwrap();
        let root = tmp.path().join("workspace/.tasks");
        std::fs::create_dir_all(&root).unwrap();
        std::fs::write(
            root.join("config.yml"),
            "default.project: TEST\nissue.states: [Todo, InProgress, Done]\nissue.types: [Feature, Bug, Chore]\nissue.priorities: [Low, Medium, High]\n",
        )
        .unwrap();
        let env = EnvVarGuard::set("LOTAR_TASKS_DIR", root.to_str().unwrap());
        let mut api = ApiServer::new();
        lotar::routes::initialize(&mut api);
        let fixture = Self {
            _tmp: tmp,
            _env: env,
            root,
            api,
        };
        for title in ["Dependent", "Prerequisite", "Other"] {
            let (status, response) = fixture.request(
                "POST",
                "/api/tasks/add",
                &[],
                json!({"project": "TEST", "title": title}),
            );
            assert_eq!(status, 201, "{response}");
        }
        fixture
    }

    fn request(
        &self,
        method: &str,
        path: &str,
        query: &[(&str, &str)],
        body: Value,
    ) -> (u16, Value) {
        let response = self.api.handle_request(&HttpRequest {
            method: method.into(),
            path: path.into(),
            query: query
                .iter()
                .map(|(k, v)| ((*k).into(), (*v).into()))
                .collect(),
            headers: HashMap::new(),
            body: serde_json::to_vec(&body).unwrap(),
        });
        (
            response.status,
            serde_json::from_slice(&response.body).unwrap(),
        )
    }

    fn bytes(&self) -> Vec<Vec<u8>> {
        (1..=3)
            .map(|number| std::fs::read(self.root.join(format!("TEST/{number}.yml"))).unwrap())
            .collect()
    }

    fn relations(&self, number: usize) -> lotar::types::TaskRelationships {
        Storage {
            root_path: self.root.clone(),
        }
        .get(&format!("TEST-{number}"), "TEST")
        .unwrap()
        .relationships
    }
}

fn assert_schema(name: &str, value: &Value) {
    let spec = SpecValidator::load_published();
    let issues = spec.validate(spec.component(name), value);
    assert!(issues.is_ok(), "{name}: {issues:?}; {value}");
}

#[test]
fn dev151_graph_projection_and_issue_selection_transport_contracts() {
    let f = Fixture::new();
    let before = f.bytes();
    for (query, count) in [
        (vec![("kinds", ""), ("include_isolated", "false")], 0),
        (
            vec![
                ("kinds", "dependency,hierarchy"),
                ("include_isolated", "true"),
            ],
            3,
        ),
        (
            vec![
                ("kinds", ""),
                ("include_isolated", "false"),
                ("task_id", "TEST-1"),
            ],
            1,
        ),
    ] {
        let (status, response) = f.request("GET", "/api/relationships/graph", &query, json!(null));
        assert_eq!(status, 200, "{response}");
        assert_eq!(response["data"]["nodes"].as_array().unwrap().len(), count);
        assert_schema("RelationshipGraphResponse", &response["data"]);
    }
    for query in [
        vec![("include_isolated", "1")],
        vec![("include_isolated", "TRUE")],
        vec![("include_isolated", "")],
        vec![("kinds", "blocks")],
        vec![("kinds", "dependency,")],
    ] {
        assert_eq!(
            f.request("GET", "/api/relationships/graph", &query, json!(null))
                .0,
            400
        );
    }
    for body in [
        json!({"issue_ids":["unknown"]}),
        json!({"issue_ids":"unknown"}),
    ] {
        assert_eq!(
            f.request("POST", "/api/relationships/repair", &[], body).0,
            400
        );
    }
    let definitions: Value = serde_json::from_str(&lotar::mcp::server::handle_json_line(
        &json!({"jsonrpc":"2.0", "id":1, "method":"tools/list", "params":{}}).to_string(),
    ))
    .unwrap();
    let tools = definitions["result"]["tools"].as_array().unwrap();
    let graph = tools
        .iter()
        .find(|tool| tool["name"] == "relationship_graph")
        .unwrap();
    assert_eq!(
        graph["inputSchema"]["properties"]["include_isolated"]["type"],
        json!(["boolean", "null"])
    );
    assert_eq!(
        graph["inputSchema"]["properties"]["relationship_kinds"]["items"]["enum"]
            .as_array()
            .unwrap()
            .len(),
        5
    );
    let repair = tools
        .iter()
        .find(|tool| tool["name"] == "relationship_repair")
        .unwrap();
    assert_eq!(
        repair["inputSchema"]["properties"]["issue_ids"]["type"],
        "array"
    );
    assert_eq!(before, f.bytes());
}

#[test]
fn graph_filter_rest_and_mcp_contract_is_strict_and_compact() {
    let f = Fixture::new();
    let before = f.bytes();
    let (status, response) = f.request(
        "GET",
        "/api/relationships/graph",
        &[(
            "filters",
            r#"{"q":"Prerequisite","status":"todo","sort_by":"id","order":"desc"}"#,
        )],
        json!(null),
    );
    assert_eq!(status, 200, "{response}");
    assert_schema("RelationshipGraphResponse", &response["data"]);
    let nodes = response["data"]["nodes"].as_array().unwrap();
    assert_eq!(nodes.len(), 1);
    assert_eq!(nodes[0]["matches_filter"], true);
    assert_eq!(nodes[0]["task"]["title"], "Prerequisite");
    assert_schema("RelationshipTaskSummary", &nodes[0]["task"]);
    for bad in [
        r#"{"status":"invalid"}"#,
        r#"{"sprints":"0"}"#,
        r#"{"due":"yesterday"}"#,
        r#"{"q":7}"#,
        "null",
        "[]",
        "{broken",
    ] {
        let (status, response) = f.request(
            "GET",
            "/api/relationships/graph",
            &[("filters", bad)],
            json!(null),
        );
        assert_eq!(status, 400, "{bad}: {response}");
        assert_eq!(response["error"]["code"], "INVALID_ARGUMENT");
    }
    let legacy: lotar::api_types::RelationshipGraphRequest =
        serde_json::from_value(json!({})).unwrap();
    assert!(legacy.filters.is_empty());
    let request: lotar::api_types::RelationshipGraphRequest =
        serde_json::from_value(json!({"filters":{"q":"Other"}})).unwrap();
    assert_eq!(request.filters["q"], "Other");
    assert!(
        serde_json::from_value::<lotar::api_types::RelationshipGraphRequest>(
            json!({"filters":{"q":7}})
        )
        .is_err()
    );
    assert_eq!(before, f.bytes());
}

#[test]
fn relationship_inspection_does_not_create_environment_selected_workspace() {
    let tmp = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).unwrap();
    let missing = tmp.path().join("workspace/.tasks");
    let _env = EnvVarGuard::set("LOTAR_TASKS_DIR", missing.to_str().unwrap());
    let mut api = ApiServer::new();
    lotar::routes::initialize(&mut api);
    for (method, path) in [
        ("GET", "/api/relationships/graph"),
        ("GET", "/api/relationships/check"),
        ("POST", "/api/relationships/repair"),
    ] {
        let response = api.handle_request(&HttpRequest {
            method: method.into(),
            path: path.into(),
            query: HashMap::new(),
            headers: HashMap::new(),
            body: b"{}".to_vec(),
        });
        assert!(
            response.status >= 400,
            "{path}: expected explicit unavailable response"
        );
        assert!(!missing.exists(), "{path} initialized a workspace");
    }
    let response: Value = serde_json::from_str(&lotar::mcp::server::handle_json_line(&json!({
        "jsonrpc": "2.0", "id": 1, "method": "tools/call", "params": {"name": "relationship_check", "arguments": {}}
    }).to_string())).unwrap();
    assert!(
        response.get("error").is_some() || response["result"]["isError"] == true,
        "{response}"
    );
    assert!(!missing.exists());
}

#[test]
fn relationship_rest_operations_preserve_reciprocity_and_change_direction_atomically() {
    let fixture = Fixture::new();
    let (status, response) = fixture.request(
        "POST",
        "/api/relationships/mutate",
        &[],
        json!({
            "source": "TEST-1", "target": "TEST-2", "kind": "depends_on", "operation": "add"
        }),
    );
    assert_eq!(status, 200, "{response}");
    assert_schema("RelationshipMutationResponse", &response["data"]);
    assert_eq!(fixture.relations(1).depends_on, ["TEST-2"]);
    assert_eq!(fixture.relations(2).blocks, ["TEST-1"]);

    let before_noop = fixture.bytes();
    let (_, response) = fixture.request(
        "POST",
        "/api/relationships/mutate",
        &[],
        json!({
            "source": "TEST-1", "target": "TEST-2", "kind": "depends_on", "operation": "add"
        }),
    );
    assert_eq!(response["data"]["changed"], false);
    assert_eq!(fixture.bytes(), before_noop);

    let (status, response) = fixture.request("POST", "/api/relationships/mutate", &[], json!({
        "source": "TEST-1", "target": "TEST-2", "kind": "blocks", "from_kind": "depends_on", "operation": "change_kind"
    }));
    assert_eq!(status, 200, "{response}");
    assert!(fixture.relations(1).depends_on.is_empty());
    assert!(fixture.relations(2).blocks.is_empty());
    assert_eq!(fixture.relations(1).blocks, ["TEST-2"]);
    assert_eq!(fixture.relations(2).depends_on, ["TEST-1"]);

    let (status, response) = fixture.request(
        "GET",
        "/api/relationships/graph",
        &[("task_id", "TEST-1")],
        Value::Null,
    );
    assert_eq!(status, 200, "{response}");
    assert_schema("RelationshipGraphResponse", &response["data"]);
    let edges = response["data"]["edges"].as_array().unwrap();
    assert_eq!(edges.len(), 1);
    assert_eq!(edges[0]["source"], "TEST-1");
    assert_eq!(edges[0]["target"], "TEST-2");
    assert_eq!(edges[0]["kind"], "dependency");
    assert_eq!(edges[0]["provenance"].as_array().unwrap().len(), 2);
}

#[test]
fn relationship_rest_rejects_invalid_queries_and_cycles_without_changes() {
    let fixture = Fixture::new();
    let initial = fixture.bytes();
    for query in [
        [("depth", "-1")],
        [("depth", "9")],
        [("max_nodes", "")],
        [("max_nodes", "0")],
        [("max_nodes", "501")],
        [("task_id", " ")],
        [("project", "")],
    ] {
        let (status, response) =
            fixture.request("GET", "/api/relationships/graph", &query, Value::Null);
        assert_eq!(status, 400, "{query:?}: {response}");
        assert_eq!(response["error"]["code"], "INVALID_ARGUMENT");
    }
    let (status, response) = fixture.request(
        "POST",
        "/api/relationships/mutate",
        &[],
        json!({
            "source": "TEST-1", "target": "TEST-1", "kind": "depends_on", "operation": "add"
        }),
    );
    assert_eq!(status, 400, "{response}");
    assert_eq!(fixture.bytes(), initial);

    let (status, _) = fixture.request(
        "POST",
        "/api/relationships/mutate",
        &[],
        json!({
            "source": "TEST-1", "target": "TEST-2", "kind": "depends_on", "operation": "add"
        }),
    );
    assert_eq!(status, 200);
    let before_cycle = fixture.bytes();
    let (status, response) = fixture.request(
        "POST",
        "/api/relationships/mutate",
        &[],
        json!({
            "source": "TEST-2", "target": "TEST-1", "kind": "depends_on", "operation": "add"
        }),
    );
    assert_eq!(status, 400, "{response}");
    assert_eq!(fixture.bytes(), before_cycle);
}

#[test]
fn relationship_singleton_replacement_is_guarded_and_updates_old_parent() {
    let fixture = Fixture::new();
    for kind in ["parent", "duplicate_of"] {
        let (status, response) = fixture.request(
            "POST",
            "/api/relationships/mutate",
            &[],
            json!({
                "source": "TEST-1", "target": "TEST-2", "kind": kind, "operation": "add"
            }),
        );
        assert_eq!(status, 200, "{response}");
        let before = fixture.bytes();
        for guard in [Value::Null, json!("TEST-3")] {
            let (status, response) = fixture.request("POST", "/api/relationships/mutate", &[], json!({
                "source": "TEST-1", "target": "TEST-3", "kind": kind, "operation": "add", "expected_current": guard
            }));
            assert_eq!(status, 400, "{kind}: {response}");
            assert_eq!(fixture.bytes(), before);
        }
        let (status, response) = fixture.request("POST", "/api/relationships/mutate", &[], json!({
            "source": "TEST-1", "target": "TEST-3", "kind": kind, "operation": "add", "expected_current": "TEST-2"
        }));
        assert_eq!(status, 200, "{response}");
    }
    assert_eq!(fixture.relations(1).parent.as_deref(), Some("TEST-3"));
    assert_eq!(fixture.relations(1).duplicate_of.as_deref(), Some("TEST-3"));
    assert!(fixture.relations(2).children.is_empty());
    assert_eq!(fixture.relations(3).children, ["TEST-1"]);
}

#[test]
fn relationship_explicit_removal_can_clear_a_missing_target_without_creating_it() {
    let fixture = Fixture::new();
    let mut storage = Storage {
        root_path: fixture.root.clone(),
    };
    let mut task = storage.get("TEST-1", "TEST").unwrap();
    task.relationships.related.push("ABSENT-99".into());
    storage.edit("TEST-1", &task).unwrap();
    let (status, response) = fixture.request(
        "POST",
        "/api/relationships/mutate",
        &[],
        json!({
            "source": "TEST-1", "target": "ABSENT-99", "kind": "related", "operation": "remove"
        }),
    );
    assert_eq!(status, 200, "{response}");
    assert!(fixture.relations(1).related.is_empty());
    assert!(!fixture.root.join("ABSENT").exists());
}

#[test]
fn relationship_repair_requires_explicit_current_preview_and_reads_never_repair() {
    let fixture = Fixture::new();
    let mut storage = Storage {
        root_path: fixture.root.clone(),
    };
    let mut task = storage.get("TEST-1", "TEST").unwrap();
    task.relationships.depends_on.push("TEST-2".into());
    storage.edit("TEST-1", &task).unwrap();
    let before = fixture.bytes();
    let (status, check) = fixture.request("GET", "/api/relationships/check", &[], Value::Null);
    assert_eq!(status, 200, "{check}");
    assert_schema("RelationshipCheckResponse", &check["data"]);
    assert!(
        check["data"]["issues"]
            .as_array()
            .unwrap()
            .iter()
            .any(|issue| issue["repairable"] == true)
    );
    let (status, preview) = fixture.request("POST", "/api/relationships/repair", &[], json!({}));
    assert_eq!(status, 200, "{preview}");
    assert_schema("RelationshipRepairResponse", &preview["data"]);
    assert_eq!(preview["data"]["applied"], false);
    assert_eq!(fixture.bytes(), before);

    for request in [
        json!({"apply": true}),
        json!({"apply": true, "revision": "stale"}),
    ] {
        let (status, response) = fixture.request("POST", "/api/relationships/repair", &[], request);
        assert_eq!(status, 400, "{response}");
        assert_eq!(fixture.bytes(), before);
    }
    let (status, response) = fixture.request(
        "POST",
        "/api/relationships/repair",
        &[],
        json!({
            "apply": true, "revision": preview["data"]["revision"]
        }),
    );
    assert_eq!(status, 200, "{response}");
    assert_eq!(fixture.relations(2).blocks, ["TEST-1"]);
}

#[test]
fn relationship_mcp_tools_share_graph_and_mutation_contracts() {
    let fixture = Fixture::new();
    let call = |name: &str, arguments: Value| -> Value {
        serde_json::from_str(&lotar::mcp::server::handle_json_line(&json!({
            "jsonrpc": "2.0", "id": 1, "method": "tools/call", "params": {"name": name, "arguments": arguments}
        }).to_string())).unwrap()
    };
    let before = fixture.bytes();
    let invalid = call(
        "relationship_mutate",
        json!({
            "source": "TEST-1", "target": "TEST-2", "kind": "dependency", "operation": "add"
        }),
    );
    assert!(invalid.get("error").is_some(), "{invalid}");
    assert_eq!(fixture.bytes(), before);

    let response = call(
        "relationship_mutate",
        json!({
            "source": "TEST-1", "target": "TEST-2", "kind": "related", "operation": "add"
        }),
    );
    assert!(response.get("error").is_none(), "{response}");
    assert_ne!(response["result"]["isError"], true, "{response}");
    assert_eq!(fixture.relations(1).related, ["TEST-2"]);
    assert_eq!(fixture.relations(2).related, ["TEST-1"]);
    let response = call("relationship_graph", json!({"task_id": "TEST-1"}));
    let graph: Value =
        serde_json::from_str(response["result"]["content"][0]["text"].as_str().unwrap()).unwrap();
    assert_schema("RelationshipGraphResponse", &graph);
    assert_eq!(graph["edges"].as_array().unwrap().len(), 1);
    let response = call("relationship_graph", json!({"filters": {"q": "Other"}}));
    let graph: Value =
        serde_json::from_str(response["result"]["content"][0]["text"].as_str().unwrap()).unwrap();
    assert_schema("RelationshipGraphResponse", &graph);
    assert_eq!(graph["nodes"].as_array().unwrap().len(), 1);
    assert_eq!(graph["nodes"][0]["task"]["title"], "Other");
    assert_eq!(graph["nodes"][0]["matches_filter"], true);
    let invalid = call("relationship_graph", json!({"filters": {"due": "invalid"}}));
    assert!(
        invalid.get("error").is_some() || invalid["result"]["isError"] == true,
        "{invalid}"
    );
    let tools: Value = serde_json::from_str(&lotar::mcp::server::handle_json_line(
        &json!({
            "jsonrpc": "2.0", "id": 2, "method": "tools/list", "params": {}
        })
        .to_string(),
    ))
    .unwrap();
    let graph_tool = tools["result"]["tools"]
        .as_array()
        .unwrap()
        .iter()
        .find(|tool| tool["name"] == "relationship_graph")
        .unwrap();
    assert_eq!(
        graph_tool["inputSchema"]["properties"]["filters"]["type"],
        "object"
    );
    assert_eq!(
        graph_tool["inputSchema"]["properties"]["filters"]["additionalProperties"]["type"],
        "string"
    );
}

//! DEV-317: a finding key reports a problem once, from the CLI, REST and MCP.
use crate::common::env_mutex::EnvVarGuard;
use crate::common::{TestFixtures, cargo_bin_in};
use lotar::api_server::{ApiServer, HttpRequest};
use lotar::routes;
use serde_json::{Value, json};
use std::collections::HashMap;

const CONFIG: &str = "default:\n  project: REPO\nissue:\n  states: [Todo, InProgress, Done, WontDo]\n  done_states: [Done, WontDo]\ncustom:\n  fields: [product]\n";

fn add(fixtures: &TestFixtures, args: &[&str]) -> std::process::Output {
    cargo_bin_in(fixtures)
        .env("LOTAR_DEFAULT_REPORTER", "scanner")
        .args(["--tasks-dir", fixtures.tasks_root.to_str().unwrap()])
        .args(args)
        .output()
        .unwrap()
}

fn add_json(fixtures: &TestFixtures, key: &str, description: &str) -> Value {
    let output = add(
        fixtures,
        &[
            "--format",
            "json",
            "add",
            "Leftover worktrees",
            "--finding",
            key,
            "-D",
            description,
        ],
    );
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).unwrap()
}

fn set_status(fixtures: &TestFixtures, id: &str, status: &str) {
    let output = add(fixtures, &["status", id, status]);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

fn listed_ids(fixtures: &TestFixtures, query: &str) -> Vec<String> {
    let output = add(fixtures, &["--format", "json", "list", query]);
    let listed: Value = serde_json::from_slice(&output.stdout).unwrap_or_else(|err| {
        panic!(
            "{err}: {} {}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        )
    });
    listed["tasks"]
        .as_array()
        .unwrap()
        .iter()
        .map(|task| task["id"].as_str().unwrap().to_string())
        .collect()
}

#[test]
fn cli_finding_is_reported_once_and_the_newest_ticket_decides() {
    let fixtures = TestFixtures::new();
    fixtures.create_config_in_dir(&fixtures.tasks_root, CONFIG);

    // None: a new ticket that carries the key, even though the project
    // limits custom fields to `product`.
    let first = add_json(&fixtures, "worktrees:lotar", "3 stale worktrees");
    assert_eq!(first["created"], json!(true));
    assert_eq!(first["state"], json!("new"));
    assert_eq!(first["id"], json!("REPO-1"));
    assert_eq!(
        first["task"]["custom_fields"]["finding"],
        json!("worktrees:lotar")
    );

    // Open: a comment with the new report instead of a second ticket.
    let again = add_json(&fixtures, "worktrees:lotar", "now 4 stale worktrees");
    assert_eq!(
        (
            again["created"].clone(),
            again["state"].clone(),
            again["id"].clone()
        ),
        (json!(false), json!("open"), json!("REPO-1"))
    );
    let comment = again["task"]["comments"][0]["text"].as_str().unwrap();
    assert!(
        comment.starts_with("Reported again by scanner on "),
        "{comment}"
    );
    assert!(comment.ends_with("now 4 stale worktrees"), "{comment}");

    // Done: a new ticket related to the fixed one.
    set_status(&fixtures, "REPO-1", "Done");
    let recurred = add_json(&fixtures, "worktrees:lotar", "back again");
    assert_eq!(
        (
            recurred["created"].clone(),
            recurred["state"].clone(),
            recurred["id"].clone()
        ),
        (json!(true), json!("recurred"), json!("REPO-2"))
    );
    assert_eq!(
        recurred["task"]["relationships"]["related"],
        json!(["REPO-1"])
    );

    // WontDo: the owner decided to live with it; nothing is created.
    set_status(&fixtures, "REPO-2", "WontDo");
    let output = add(
        &fixtures,
        &["add", "Leftover worktrees", "--finding", "worktrees:lotar"],
    );
    assert!(output.status.success());
    let text = String::from_utf8_lossy(&output.stdout);
    assert!(text.contains("REPO-2"), "{text}");
    assert!(text.contains("won't do"), "{text}");
    let suppressed = add_json(&fixtures, "worktrees:lotar", "still there");
    assert_eq!(
        (
            suppressed["created"].clone(),
            suppressed["state"].clone(),
            suppressed["id"].clone()
        ),
        (json!(false), json!("suppressed"), json!("REPO-2"))
    );

    // A different key is its own finding, and the key is a list filter.
    let other = add_json(&fixtures, "ci:lotar", "CI is red");
    assert_eq!(other["id"], json!("REPO-3"));
    assert_eq!(
        listed_ids(&fixtures, "finding=worktrees:lotar"),
        vec!["REPO-2".to_string(), "REPO-1".to_string()]
    );
}

#[test]
fn cli_parallel_reports_of_one_key_create_one_ticket() {
    let fixtures = TestFixtures::new();
    fixtures.create_config_in_dir(&fixtures.tasks_root, CONFIG);
    let outputs: Vec<std::process::Output> = std::thread::scope(|scope| {
        let handles: Vec<_> = (0..6)
            .map(|n| {
                let fixtures = &fixtures;
                scope.spawn(move || {
                    let description = format!("run {n}");
                    add(
                        fixtures,
                        &[
                            "--format",
                            "json",
                            "add",
                            "CI is red",
                            "--finding",
                            "ci:main",
                            "-D",
                            &description,
                        ],
                    )
                })
            })
            .collect();
        handles.into_iter().map(|h| h.join().unwrap()).collect()
    });
    let mut states = Vec::new();
    for output in outputs {
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        let result: Value = serde_json::from_slice(&output.stdout).unwrap();
        assert_eq!(result["id"], json!("REPO-1"), "{result}");
        states.push(result["state"].as_str().unwrap().to_string());
    }
    states.sort();
    assert_eq!(states, ["new", "open", "open", "open", "open", "open"]);
    assert_eq!(listed_ids(&fixtures, "finding=ci:main"), ["REPO-1"]);
    let stored = std::fs::read_to_string(fixtures.tasks_root.join("REPO/1.yml")).unwrap();
    assert_eq!(
        stored.matches("Reported again by scanner").count(),
        5,
        "{stored}"
    );
}

#[test]
fn cli_created_automation_runs_only_for_new_tickets() {
    let fixtures = TestFixtures::new();
    fixtures.create_config_in_dir(&fixtures.tasks_root, CONFIG);
    std::fs::write(
        fixtures.tasks_root.join("automation.yml"),
        "automation:\n  rules:\n    - name: Triage\n      on:\n        created:\n          comment: \"Triaged ${{ticket.id}}\"\n",
    )
    .unwrap();
    add_json(&fixtures, "ci:main", "first");
    add_json(&fixtures, "ci:main", "second");
    let stored = std::fs::read_to_string(fixtures.tasks_root.join("REPO/1.yml")).unwrap();
    assert_eq!(stored.matches("Triaged REPO-1").count(), 1, "{stored}");
    assert_eq!(stored.matches("Reported again").count(), 1, "{stored}");
}

#[test]
fn rest_and_mcp_report_findings() {
    let fixtures = TestFixtures::new();
    fixtures.create_config_in_dir(&fixtures.tasks_root, CONFIG);
    let _env = [
        EnvVarGuard::set("LOTAR_TASKS_DIR", fixtures.tasks_root.to_str().unwrap()),
        EnvVarGuard::set("LOTAR_IGNORE_HOME_CONFIG", "1"),
    ];
    let mut api = ApiServer::new();
    routes::initialize(&mut api);
    let post = |body: Value| {
        let response = api.handle_request(&HttpRequest {
            method: "POST".into(),
            path: "/api/tasks/add".into(),
            query: HashMap::new(),
            headers: HashMap::new(),
            body: serde_json::to_vec(&body).unwrap(),
        });
        let parsed: Value = serde_json::from_slice(&response.body).unwrap();
        (response.status, parsed)
    };

    let (status, created) =
        post(json!({"title": "CI is red", "finding": "ci:main", "reporter": "scanner"}));
    assert_eq!(status, 201, "{created}");
    assert_eq!(
        created["finding"],
        json!({"created": true, "state": "new", "id": "REPO-1"})
    );
    assert_eq!(
        created["data"]["custom_fields"]["finding"],
        json!("ci:main")
    );

    let (status, again) =
        post(json!({"title": "CI is red", "finding": "ci:main", "description": "still red"}));
    assert_eq!(status, 200, "{again}");
    assert_eq!(
        again["finding"],
        json!({"created": false, "state": "open", "id": "REPO-1"})
    );
    assert_eq!(again["data"]["id"], json!("REPO-1"));

    let (status, invalid) = post(json!({"title": "CI is red", "finding": "  "}));
    assert_eq!(status, 400, "{invalid}");

    let call = |arguments: Value| {
        let request = json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "tools/call",
            "params": {"name": "task_create", "arguments": arguments}
        });
        let response: Value = serde_json::from_str(&lotar::mcp::server::handle_json_line(
            &serde_json::to_string(&request).unwrap(),
        ))
        .unwrap();
        assert!(response.get("error").is_none(), "{response}");
        let text = response["result"]["content"][0]["text"].as_str().unwrap();
        serde_json::from_str::<Value>(text).unwrap()
    };
    let open = call(json!({"title": "CI is red", "finding": "ci:main"}));
    assert_eq!(
        open["finding"],
        json!({"created": false, "state": "open", "id": "REPO-1"})
    );
    assert_eq!(open["task"]["id"], json!("REPO-1"));

    let mut storage = fixtures.create_storage();
    lotar::services::task_service::TaskService::update(
        &mut storage,
        "REPO-1",
        lotar::api_types::TaskUpdate {
            status: Some("WontDo".to_string()),
            ..Default::default()
        },
    )
    .unwrap();
    let suppressed = call(json!({"title": "CI is red", "finding": "ci:main"}));
    assert_eq!(
        suppressed["finding"],
        json!({"created": false, "state": "suppressed", "id": "REPO-1"})
    );
    let plain = call(json!({"title": "No finding"}));
    assert!(plain.get("finding").is_none(), "{plain}");
    assert_eq!(plain["task"]["id"], json!("REPO-2"));
}

//! DEV-231: `lotar list` takes the web search-box syntax, with values from
//! the configuration rather than hard-coded shortcut flags.

use serde_json::Value;

mod common;

struct Workspace {
    _tmp: tempfile::TempDir,
    tasks_dir: std::path::PathBuf,
}

fn workspace() -> Workspace {
    let tmp = tempfile::tempdir().unwrap();
    let tasks_dir = tmp.path().join(".tasks");
    std::fs::create_dir_all(tasks_dir.join("CP")).unwrap();
    std::fs::write(
        tasks_dir.join("config.yml"),
        "default:\n  project: CP\nissue:\n  priorities: [P0, P1, P2]\n  states: [Todo, In Progress, Done]\n  custom_fields: [area]\n",
    )
    .unwrap();
    let task = |num: u32, title: &str, priority: &str, status: &str, extra: &str| {
        let modified = format!("2026-01-01T00:00:0{num}Z");
        std::fs::write(
            tasks_dir.join("CP").join(format!("{num}.yml")),
            format!(
                "title: {title}\nstatus: {status}\npriority: {priority}\ntype: Feature\ncreated: 2026-01-01T00:00:00Z\nmodified: {modified}\n{extra}"
            ),
        )
        .unwrap();
    };
    task(
        1,
        "Login redirect loop",
        "P0",
        "In Progress",
        "tags: [auth]\ncustom_fields:\n  area: billing\n",
    );
    task(
        2,
        "Billing export",
        "P1",
        "Todo",
        "custom_fields:\n  area: billing\n",
    );
    task(3, "Docs title fix", "P2", "Done", "tags: [docs]\n");
    Workspace {
        _tmp: tmp,
        tasks_dir,
    }
}

fn run(ws: &Workspace, args: &[&str]) -> Result<Vec<String>, String> {
    let mut cmd = common::cargo_bin_silent();
    cmd.env("LOTAR_TASKS_DIR", &ws.tasks_dir);
    cmd.current_dir(ws.tasks_dir.parent().unwrap());
    cmd.args(["--format", "json", "list"]);
    cmd.args(args);
    let out = cmd.output().map_err(|e| e.to_string())?;
    if !out.status.success() {
        return Err(format!(
            "{}{}",
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr)
        ));
    }
    let payload: Value = serde_json::from_slice(&out.stdout).map_err(|e| e.to_string())?;
    Ok(payload["tasks"]
        .as_array()
        .map(|tasks| {
            tasks
                .iter()
                .map(|t| t["id"].as_str().unwrap().to_string())
                .collect()
        })
        .unwrap_or_default())
}

fn ids(nums: &[u32]) -> Vec<String> {
    nums.iter().map(|n| format!("CP-{n}")).collect()
}

#[test]
fn configured_values_work_in_the_query_and_the_flags() {
    let ws = workspace();
    // Priorities are whatever the project configures.
    assert_eq!(run(&ws, &["priority:p1"]).unwrap(), ids(&[2]));
    assert_eq!(run(&ws, &["prio:P0,P2"]).unwrap(), ids(&[3, 1]));
    assert_eq!(run(&ws, &["--priority", "p1"]).unwrap(), ids(&[2]));
    // Statuses with spaces are quoted, as in the web UI.
    assert_eq!(run(&ws, &["status:\"In Progress\""]).unwrap(), ids(&[1]));
    // A value the project does not have is an error, not an empty list.
    let err = run(&ws, &["priority:high"]).unwrap_err();
    assert!(err.contains("Priority validation failed"), "{err}");
}

#[test]
fn query_mixes_filters_custom_fields_and_search_text() {
    let ws = workspace();
    assert_eq!(
        run(&ws, &["area=billing", "status:todo"]).unwrap(),
        ids(&[2])
    );
    assert_eq!(
        run(&ws, &["field:area=billing", "login"]).unwrap(),
        ids(&[1])
    );
    // Repeated list filters add up; flags add to the query.
    assert_eq!(
        run(&ws, &["status:todo status:done"]).unwrap(),
        ids(&[3, 2])
    );
    assert_eq!(
        run(&ws, &["tag:auth", "--tag", "docs"]).unwrap(),
        ids(&[3, 1])
    );
    // Unknown keys stay search text (here a title scope).
    assert_eq!(run(&ws, &["title:docs"]).unwrap(), ids(&[3]));
    // Sorting in the query.
    assert_eq!(
        run(&ws, &["sort_by=title", "order=asc"]).unwrap(),
        ids(&[2, 3, 1])
    );
}

#[test]
fn malformed_terms_and_removed_shortcuts_are_errors() {
    let ws = workspace();
    let err = run(&ws, &["owner="]).unwrap_err();
    assert!(err.contains("Add a value for"), "{err}");
    let err = run(&ws, &["status:\"unclosed"]).unwrap_err();
    assert!(err.contains("Unmatched quote"), "{err}");
    for removed in [
        "--high",
        "--critical",
        "--mine",
        "--overdue",
        "--due-soon",
        "--unassigned",
    ] {
        let err = run(&ws, &[removed]).unwrap_err();
        assert!(err.contains("unexpected argument"), "{removed}: {err}");
        assert!(err.contains("was removed; write"), "{removed}: {err}");
    }
}

/// DEV-235: REST and MCP take the same text as a `query` parameter.
#[test]
fn one_query_string_works_on_every_interface() {
    use crate::common::env_mutex::EnvVarGuard;
    use lotar::api_server::{ApiServer, HttpRequest};
    use serde_json::json;
    use std::collections::HashMap;

    let ws = workspace();
    let _guard = EnvVarGuard::set("LOTAR_TASKS_DIR", ws.tasks_dir.to_string_lossy().as_ref());
    let mut api = ApiServer::new();
    lotar::routes::initialize(&mut api);
    let rest = |path: &str, params: &[(&str, &str)]| {
        let resp = api.handle_request(&HttpRequest {
            method: "GET".into(),
            path: path.into(),
            query: params
                .iter()
                .map(|(k, v)| ((*k).to_string(), (*v).to_string()))
                .collect::<HashMap<_, _>>(),
            headers: HashMap::new(),
            body: Vec::new(),
        });
        (resp.status, String::from_utf8(resp.body).unwrap())
    };
    let mcp = |params: serde_json::Value| -> serde_json::Value {
        let line = json!({"jsonrpc": "2.0", "id": 1, "method": "task/list", "params": params});
        serde_json::from_str(&lotar::mcp::server::handle_json_line(&line.to_string())).unwrap()
    };

    for (text, expected) in [
        ("area=billing status:todo", ids(&[2])),
        ("prio:P0,P2 sort_by=title order=asc", ids(&[3, 1])),
        ("field:area=billing login", ids(&[1])),
        ("tag:docs", ids(&[3])),
    ] {
        assert_eq!(run(&ws, &[text]).unwrap(), expected, "cli {text}");

        let (status, body) = rest("/api/tasks/list", &[("query", text)]);
        assert_eq!(status, 200, "{body}");
        let body: Value = serde_json::from_str(&body).unwrap();
        let rest_ids: Vec<String> = body["data"]["tasks"]
            .as_array()
            .map(|t| {
                t.iter()
                    .map(|t| t["id"].as_str().unwrap().to_string())
                    .collect()
            })
            .unwrap_or_default();
        assert_eq!(rest_ids, expected, "rest {text}");

        let (status, csv) = rest("/api/tasks/export", &[("query", text)]);
        assert_eq!(status, 200, "{csv}");
        let export_ids: Vec<String> = csv
            .lines()
            .skip(1)
            .map(|line| line.split(',').next().unwrap().to_string())
            .collect();
        assert_eq!(export_ids, expected, "export {text}");

        let resp = mcp(json!({"query": text}));
        let payload: Value =
            serde_json::from_str(resp["result"]["content"][0]["text"].as_str().unwrap()).unwrap();
        let mcp_ids: Vec<String> = payload["tasks"]
            .as_array()
            .unwrap()
            .iter()
            .map(|t| t["id"].as_str().unwrap().to_string())
            .collect();
        assert_eq!(mcp_ids, expected, "mcp {text}");
    }

    // A malformed query is an error everywhere.
    let (status, _) = rest("/api/tasks/list", &[("query", "owner=")]);
    assert_eq!(status, 400);
    assert!(mcp(json!({"query": "owner="}))["error"]["code"] == -32602);
}

//! DEV-230: one task query for every interface.
//!
//! The same filter must select the same tasks in the same order from the CLI
//! (`lotar list`), REST (`/api/tasks/list` and `/api/tasks/export`) and MCP
//! (`task_list`), including the filters that used to exist on only one side.

use chrono::{Duration, Local, Utc};
use lotar::api_server::{ApiServer, HttpRequest};
use lotar::routes;
use serde_json::{Value, json};
use std::collections::HashMap;

mod common;
use crate::common::env_mutex::EnvVarGuard;

struct Fixture {
    _tmp: tempfile::TempDir,
    tasks_dir: std::path::PathBuf,
}

impl Fixture {
    fn new() -> Self {
        let tmp = tempfile::tempdir().unwrap();
        let tasks_dir = tmp.path().join(".tasks");
        std::fs::create_dir_all(tasks_dir.join("QP")).unwrap();
        std::fs::write(
            tasks_dir.join("config.yml"),
            "default:\n  project: QP\nissue:\n  priorities: [Low, Medium, High, Critical]\n  custom_fields: ['*']\n",
        )
        .unwrap();
        Self {
            _tmp: tmp,
            tasks_dir,
        }
    }

    fn task(&self, num: u32, title: &str, priority: &str, extra: &str) {
        let modified =
            (Utc::now() - Duration::days(30) + Duration::minutes(i64::from(num))).to_rfc3339();
        let yaml = format!(
            "title: {title}\nstatus: Todo\npriority: {priority}\ntype: Feature\ncreated: 2026-01-01T00:00:00Z\nmodified: {modified}\n{extra}"
        );
        std::fs::write(self.tasks_dir.join("QP").join(format!("{num}.yml")), yaml).unwrap();
    }
}

fn date(days_from_today: i64) -> String {
    (Local::now().date_naive() + Duration::days(days_from_today))
        .format("%Y-%m-%d")
        .to_string()
}

/// Tasks covering every filter below; QP-n has modified time order n.
fn seed(fx: &Fixture) {
    let recent = Utc::now().to_rfc3339();
    fx.task(
        1,
        "Tagged estimate",
        "Medium",
        "tags: [ui]\neffort: 4h\nassignee: alice\n",
    );
    fx.task(
        2,
        "Untagged big",
        "Medium",
        &format!("effort: 2d\ndue_date: {}\nassignee: bob\n", date(2)),
    );
    fx.task(
        3,
        "Overdue screenshot",
        "Medium",
        &format!(
            "tags: [bug]\ndue_date: {}\nreferences:\n- attachment: shot.png\n",
            date(-3)
        ),
    );
    fx.task(
        4,
        "Linked later",
        "High",
        &format!(
            "due_date: {}\nreferences:\n- link: https://example.com\ncustom_fields:\n  area: billing\n",
            date(20)
        ),
    );
    fx.task(
        5,
        "Points task",
        "Critical",
        &format!("effort: 3pt\ntags: [ui, api]\ndue_date: {}\n", date(0)),
    );
    // Recently modified (overrides the generated timestamp).
    std::fs::write(
        fx.tasks_dir.join("QP").join("6.yml"),
        format!(
            "title: Fresh\nstatus: Todo\npriority: Medium\ntype: Feature\ncreated: 2026-01-01T00:00:00Z\nmodified: {recent}\nassignee: alice\ncustom_fields:\n  area: billing\n"
        ),
    )
    .unwrap();
    let sprints = fx.tasks_dir.join("@sprints");
    std::fs::create_dir_all(&sprints).unwrap();
    std::fs::write(sprints.join("1.yml"), "tasks:\n  - QP-1\n  - QP-4\n").unwrap();
    std::fs::write(sprints.join("2.yml"), "tasks:\n  - QP-5\n").unwrap();
}

fn api() -> ApiServer {
    let mut api = ApiServer::new();
    routes::initialize(&mut api);
    api
}

fn request(path: &str, query: &[(&str, &str)]) -> HttpRequest {
    HttpRequest {
        method: "GET".to_string(),
        path: path.to_string(),
        query: query
            .iter()
            .map(|(k, v)| ((*k).to_string(), (*v).to_string()))
            .collect::<HashMap<_, _>>(),
        headers: HashMap::new(),
        body: Vec::new(),
    }
}

fn rest_ids(api: &ApiServer, query: &[(&str, &str)]) -> Result<Vec<String>, String> {
    let mut query = query.to_vec();
    query.push(("limit", "1000"));
    let resp = api.handle_request(&request("/api/tasks/list", &query));
    let body: Value = serde_json::from_slice(&resp.body).unwrap();
    if resp.status != 200 {
        return Err(body["error"]["message"].as_str().unwrap_or("").to_string());
    }
    Ok(body["data"]["tasks"]
        .as_array()
        .map(|tasks| {
            tasks
                .iter()
                .map(|t| t["id"].as_str().unwrap().to_string())
                .collect()
        })
        .unwrap_or_default())
}

fn export_ids(api: &ApiServer, query: &[(&str, &str)]) -> Vec<String> {
    let resp = api.handle_request(&request("/api/tasks/export", query));
    assert_eq!(resp.status, 200, "{}", String::from_utf8_lossy(&resp.body));
    String::from_utf8(resp.body)
        .unwrap()
        .lines()
        .skip(1)
        .map(|line| line.split(',').next().unwrap().to_string())
        .collect()
}

fn mcp_ids(params: Value) -> Result<Vec<String>, String> {
    let mut params = params;
    params["limit"] = json!(200);
    let line = serde_json::to_string(&json!({
        "jsonrpc": "2.0",
        "id": 230,
        "method": "task/list",
        "params": params,
    }))
    .unwrap();
    let resp: Value = serde_json::from_str(&lotar::mcp::server::handle_json_line(&line)).unwrap();
    if let Some(error) = resp.get("error") {
        return Err(error.to_string());
    }
    let payload: Value =
        serde_json::from_str(resp["result"]["content"][0]["text"].as_str().unwrap()).unwrap();
    Ok(payload["tasks"]
        .as_array()
        .unwrap()
        .iter()
        .map(|t| t["id"].as_str().unwrap().to_string())
        .collect())
}

fn cli_ids(tasks_dir: &std::path::Path, args: &[&str]) -> Result<Vec<String>, String> {
    let mut cmd = common::cargo_bin_silent();
    cmd.env("LOTAR_TASKS_DIR", tasks_dir);
    cmd.current_dir(tasks_dir.parent().unwrap());
    cmd.args(["--format", "json", "list", "--page-size", "200"]);
    cmd.args(args);
    let out = cmd.output().map_err(|e| e.to_string())?;
    if !out.status.success() {
        return Err(String::from_utf8_lossy(&out.stderr).to_string());
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
    nums.iter().map(|n| format!("QP-{n}")).collect()
}

struct Case<'a> {
    name: &'a str,
    cli: &'a [&'a str],
    rest: &'a [(&'a str, &'a str)],
    mcp: Value,
    expected: &'a [u32],
}

#[test]
fn every_interface_runs_the_same_query() {
    let fx = Fixture::new();
    seed(&fx);
    let _guard = EnvVarGuard::set("LOTAR_TASKS_DIR", fx.tasks_dir.to_string_lossy().as_ref());
    let api = api();

    // Default order is newest change first: QP-6, then 5..1.
    let cases = [
        Case {
            name: "has tag",
            cli: &["--has", "tag"],
            rest: &[("has", "tag")],
            mcp: json!({"has": ["tag"]}),
            expected: &[5, 3, 1],
        },
        Case {
            name: "has image and link kinds",
            cli: &["with:image"],
            rest: &[("has", "image")],
            mcp: json!({"has": "image"}),
            expected: &[3],
        },
        Case {
            name: "needs effort",
            cli: &["needs:effort"],
            rest: &[("needs", "effort")],
            mcp: json!({"needs": "effort"}),
            expected: &[6, 4, 3],
        },
        Case {
            name: "effort window",
            cli: &["--effort-min", "2h", "--effort-max", "1d"],
            rest: &[("effort_min", "2h"), ("effort_max", "1d")],
            mcp: json!({"effort_min": "2h", "effort_max": "1d"}),
            expected: &[1],
        },
        Case {
            name: "due within three days",
            cli: &["due_within:3"],
            rest: &[("due_within", "3")],
            mcp: json!({"due_within": 3}),
            expected: &[5, 2],
        },
        Case {
            name: "overdue",
            cli: &["due:overdue"],
            rest: &[("due", "overdue")],
            mcp: json!({"due": "overdue"}),
            expected: &[3],
        },
        Case {
            name: "due later bucket",
            cli: &["--due", "later"],
            rest: &[("due", "later")],
            mcp: json!({"due": "later"}),
            expected: &[4],
        },
        Case {
            name: "recent",
            cli: &["recent:7d"],
            rest: &[("recent", "7d")],
            mcp: json!({"recent": "7d"}),
            expected: &[6],
        },
        Case {
            name: "unassigned",
            cli: &["assignee:__none__"],
            rest: &[("assignee", "__none__")],
            mcp: json!({"assignee": "__none__"}),
            expected: &[5, 4, 3],
        },
        Case {
            name: "assignee",
            cli: &["--assignee", "alice"],
            rest: &[("assignee", "alice")],
            mcp: json!({"assignee": "alice"}),
            expected: &[6, 1],
        },
        Case {
            name: "sprints",
            cli: &["--sprint", "1,2"],
            rest: &[("sprints", "1,2")],
            mcp: json!({"sprints": [1, 2]}),
            expected: &[5, 4, 1],
        },
        Case {
            name: "custom field",
            cli: &["area=billing"],
            rest: &[("area", "billing")],
            mcp: json!({"custom_fields": {"area": "billing"}}),
            expected: &[6, 4],
        },
        Case {
            name: "priority values add up",
            cli: &["priority:high", "prio:critical"],
            rest: &[("priority", "high,critical")],
            mcp: json!({"priority": ["high", "critical"]}),
            expected: &[5, 4],
        },
        Case {
            // Configured order, highest priority first by default (DEV-234).
            name: "priority sort",
            cli: &["--sort-by", "priority"],
            rest: &[("sort_by", "priority")],
            mcp: json!({"sort_by": "priority"}),
            expected: &[5, 4, 1, 2, 3, 6],
        },
        Case {
            name: "search with filters, sorted",
            cli: &["t tag:ui", "sort_by=title", "order=asc"],
            rest: &[
                ("q", "t"),
                ("tags", "ui"),
                ("sort_by", "title"),
                ("order", "asc"),
            ],
            mcp: json!({"search": "t", "tags": "ui", "sort_by": "title", "order": "asc"}),
            expected: &[5, 1],
        },
    ];

    for case in cases {
        let expected = ids(case.expected);
        let cli =
            cli_ids(&fx.tasks_dir, case.cli).unwrap_or_else(|e| panic!("{}: cli {e}", case.name));
        let rest = rest_ids(&api, case.rest).unwrap_or_else(|e| panic!("{}: rest {e}", case.name));
        let export = export_ids(&api, case.rest);
        let mcp = mcp_ids(case.mcp).unwrap_or_else(|e| panic!("{}: mcp {e}", case.name));
        assert_eq!(cli, expected, "{}: cli", case.name);
        assert_eq!(rest, expected, "{}: rest", case.name);
        assert_eq!(export, expected, "{}: export", case.name);
        assert_eq!(mcp, expected, "{}: mcp", case.name);
    }
}

#[test]
fn every_interface_rejects_the_same_invalid_values() {
    let fx = Fixture::new();
    seed(&fx);
    let _guard = EnvVarGuard::set("LOTAR_TASKS_DIR", fx.tasks_dir.to_string_lossy().as_ref());
    let api = api();

    // (CLI flag, REST/MCP param, value, expected message)
    let cases = [
        ("--has", "has", "photo", "Invalid has entry"),
        ("--due", "due", "someday", "Invalid due"),
        ("--effort-min", "effort_min", "lots", "Invalid effort_min"),
        ("--recent", "recent", "30x", "Invalid recent"),
    ];
    for (flag, param, value, message) in cases {
        let rest = [(param, value)];
        let mut mcp = json!({});
        mcp[param] = json!(value);
        let cli_err = cli_ids(&fx.tasks_dir, &[flag, value]).expect_err("cli should fail");
        assert!(cli_err.contains(message), "cli: {cli_err}");
        let rest_err = rest_ids(&api, &rest).expect_err("rest should fail");
        assert!(rest_err.contains(message), "rest: {rest_err}");
        // MCP may reject an out-of-vocabulary value at its schema layer
        // first; either way it is an invalid-params error naming the param.
        let mcp_err = mcp_ids(mcp).expect_err("mcp should fail");
        assert!(
            mcp_err.contains("-32602") && mcp_err.contains(param),
            "mcp: {mcp_err}"
        );
    }
}

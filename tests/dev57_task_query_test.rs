//! DEV-57: unified task query filtering, ordering, pagination, and export.
//!
//! Regression coverage for the shared query executor:
//! - transport parity: REST list pages, REST export CSV, and MCP task_list
//!   pages must agree on the filtered set AND the global order;
//! - export honors the full filter grammar and requested order;
//! - custom-field sorting, missing values, and the canonical-ID ascending
//!   tiebreak;
//! - strict rejection of invalid explicit enum/sprint/sort/page params on
//!   REST and MCP (nothing fails open);
//! - date/DST-safe due buckets and the inclusive recent=7d cutoff.

use chrono::{Duration, Local, TimeZone, Utc};
use lotar::api_server::{ApiServer, HttpRequest};
use lotar::routes;
use serde_json::{Value, json};
use std::collections::HashMap;

mod common;
use crate::common::env_mutex::EnvVarGuard;

fn mk_req(method: &str, path: &str, query: &[(&str, &str)]) -> HttpRequest {
    let mut q = HashMap::new();
    for (k, v) in query {
        q.insert((*k).to_string(), (*v).to_string());
    }
    HttpRequest {
        method: method.to_string(),
        path: path.to_string(),
        query: q,
        headers: HashMap::new(),
        body: Vec::new(),
    }
}

struct Fixture {
    _tmp: tempfile::TempDir,
    tasks_dir: std::path::PathBuf,
    project_dir: std::path::PathBuf,
}

impl Fixture {
    fn new(project: &str) -> Self {
        let tmp = tempfile::tempdir().unwrap();
        let tasks_dir = tmp.path().join(".tasks");
        let project_dir = tasks_dir.join(project);
        std::fs::create_dir_all(&project_dir).unwrap();
        Self {
            _tmp: tmp,
            tasks_dir,
            project_dir,
        }
    }

    fn write(&self, num: u32, body: &str) {
        std::fs::write(self.project_dir.join(format!("{num}.yml")), body).unwrap();
    }

    fn task_yaml(&self, num: u32, title: &str, modified: &str, extra: &[(&str, String)]) -> String {
        let mut yaml = format!(
            "title: {title}\nstatus: Todo\npriority: medium\ntype: task\ncreated: 2026-01-01T00:00:00Z\nmodified: {modified}\ntags: []\n"
        );
        for (key, value) in extra {
            yaml.push_str(&format!("{key}: {value}\n"));
        }
        let _ = num;
        yaml
    }
}

fn api() -> ApiServer {
    let mut api = ApiServer::new();
    routes::initialize(&mut api);
    api
}

fn rest_json(api: &ApiServer, query: &[(&str, &str)]) -> (u16, Value) {
    let resp = api.handle_request(&mk_req("GET", "/api/tasks/list", query));
    let body: Value = serde_json::from_slice(&resp.body).unwrap();
    (resp.status, body)
}

fn rest_ids_from_json(body: &Value) -> Vec<String> {
    // Empty pages omit the `tasks` field entirely.
    body["data"]["tasks"]
        .as_array()
        .map(|tasks| {
            tasks
                .iter()
                .map(|t| t["id"].as_str().unwrap().to_string())
                .collect()
        })
        .unwrap_or_default()
}

fn rest_export(api: &ApiServer, query: &[(&str, &str)]) -> (u16, Vec<String>) {
    let resp = api.handle_request(&mk_req("GET", "/api/tasks/export", query));
    let text = String::from_utf8_lossy(&resp.body).to_string();
    let ids = text
        .lines()
        .skip(1)
        .filter(|line| !line.trim().is_empty())
        .map(|line| line.split(',').next().unwrap().to_string())
        .collect();
    (resp.status, ids)
}

/// Collect every task id from paged REST list responses.
fn rest_list_all(api: &ApiServer, query: &[(&str, &str)], limit: usize) -> (u16, Vec<String>) {
    let mut all = Vec::new();
    let mut offset = 0usize;
    let (status, body) = rest_json(api, query);
    if status != 200 {
        return (status, Vec::new());
    }
    let total = body["data"]["total"].as_u64().unwrap_or(0) as usize;
    let _ = total;
    loop {
        let mut with_page: Vec<(&str, String)> =
            query.iter().map(|(k, v)| (*k, v.to_string())).collect();
        with_page.push(("limit", limit.to_string()));
        with_page.push(("offset", offset.to_string()));
        let pairs: Vec<(&str, &str)> = with_page.iter().map(|(k, v)| (*k, v.as_str())).collect();
        let (status, body) = rest_json(api, &pairs);
        if status != 200 {
            return (status, all);
        }
        let ids = rest_ids_from_json(&body);
        if ids.is_empty() {
            break;
        }
        all.extend(ids);
        offset += limit;
    }
    (200, all)
}

fn mcp_call(method: &str, params: Value) -> Result<Value, Value> {
    let req = json!({
        "jsonrpc": "2.0",
        "id": 57,
        "method": method,
        "params": params
    });
    let line = serde_json::to_string(&req).unwrap();
    let resp_line = lotar::mcp::server::handle_json_line(&line);
    let resp: Value = serde_json::from_str(&resp_line).unwrap();
    if let Some(error) = resp.get("error") {
        return Err(error.clone());
    }
    let text = resp["result"]["content"][0]["text"].as_str().unwrap();
    Ok(serde_json::from_str(text).unwrap())
}

fn mcp_ids(payload: &Value) -> Vec<String> {
    payload["tasks"]
        .as_array()
        .unwrap()
        .iter()
        .map(|t| t["id"].as_str().unwrap().to_string())
        .collect()
}

fn mcp_list_all(params: Value, limit: usize) -> Vec<String> {
    let mut all = Vec::new();
    let mut cursor = 0usize;
    loop {
        let mut page_params = params.clone();
        page_params["limit"] = json!(limit);
        page_params["cursor"] = json!(cursor);
        let payload = mcp_call("task/list", page_params).expect("task/list page");
        let ids = mcp_ids(&payload);
        if ids.is_empty() {
            break;
        }
        all.extend(ids);
        cursor += limit;
        if payload["hasMore"].as_bool() != Some(true) {
            break;
        }
    }
    all
}

fn local_today() -> chrono::NaiveDate {
    Local::now().date_naive()
}

fn date_only(days_from_today: i64) -> String {
    (local_today() + Duration::days(days_from_today))
        .format("%Y-%m-%d")
        .to_string()
}

/// RFC3339 instant of a local calendar date at the given wall-clock time.
fn local_instant(days_from_today: i64, h: u32, m: u32) -> String {
    let date = local_today() + Duration::days(days_from_today);
    let naive = date.and_hms_opt(h, m, 0).unwrap();
    Local
        .from_local_datetime(&naive)
        .single()
        .unwrap()
        .with_timezone(&Utc)
        .to_rfc3339()
}

fn ids_prefixed(project: &str, nums: &[u32]) -> Vec<String> {
    nums.iter().map(|n| format!("{project}-{n}")).collect()
}

fn assert_ids(actual: &[String], expected: &[String]) {
    assert_eq!(actual, expected, "expected {expected:?}, got {actual:?}");
}

#[test]
fn transports_agree_on_filter_and_global_order() {
    let fx = Fixture::new("QA");
    let _guard = EnvVarGuard::set("LOTAR_TASKS_DIR", fx.tasks_dir.to_string_lossy().as_ref());

    // Sizes chosen so lexical order and ties are unambiguous:
    // l < m < s < xl; QA-2/QA-5 tie on "m", QA-4/QA-6 tie on "s".
    let cases = [
        (1u32, "l", "2026-01-01T00:00:05Z"),
        (2, "m", "2026-01-01T00:00:04Z"),
        (3, "xl", "2026-01-01T00:00:03Z"),
        (4, "s", "2026-01-01T00:00:02Z"),
        (5, "m", "2026-01-01T00:00:01Z"),
        (6, "s", "2026-01-01T00:00:00Z"),
    ];
    for (num, size, modified) in cases {
        let yaml = format!(
            "title: size {size}\nstatus: Todo\npriority: medium\ntype: task\ncreated: 2026-01-01T00:00:00Z\nmodified: {modified}\ntags: []\ncustom_fields:\n  size: {size}\n"
        );
        fx.write(num, &yaml);
    }

    let api = api();
    let query = [
        ("project", "QA"),
        ("sort_by", "custom:size"),
        ("order", "asc"),
    ];

    let (status, rest_ids) = rest_list_all(&api, &query, 2);
    assert_eq!(status, 200);
    assert_eq!(rest_ids.len(), 6);
    assert_ids(&rest_ids, &ids_prefixed("QA", &[1, 2, 5, 4, 6, 3]));

    let (status, export_ids) = rest_export(&api, &query);
    assert_eq!(status, 200);
    assert_ids(&export_ids, &ids_prefixed("QA", &[1, 2, 5, 4, 6, 3]));

    let mcp_ids = mcp_list_all(
        json!({"project": "QA", "sort_by": "custom:size", "order": "asc"}),
        2,
    );
    assert_ids(&mcp_ids, &ids_prefixed("QA", &[1, 2, 5, 4, 6, 3]));

    // Desc flips the primary order only; equal-size ties stay ID asc.
    let (_, rest_desc) = rest_list_all(
        &api,
        &[
            ("project", "QA"),
            ("sort_by", "custom:size"),
            ("order", "desc"),
        ],
        3,
    );
    assert_ids(&rest_desc, &ids_prefixed("QA", &[3, 4, 6, 2, 5, 1]));
}

#[test]
fn export_applies_smart_filters_and_global_order() {
    let fx = Fixture::new("QB");
    let _guard = EnvVarGuard::set("LOTAR_TASKS_DIR", fx.tasks_dir.to_string_lossy().as_ref());

    let now = Utc::now();
    let recent = (now - Duration::days(3)).to_rfc3339();
    let old = (now - Duration::days(30)).to_rfc3339();

    fx.write(
        1,
        &fx.task_yaml(1, "overdue recent", &recent, &[("due_date", date_only(-1))]),
    );
    fx.write(
        2,
        &fx.task_yaml(2, "today old", &old, &[("due_date", date_only(0))]),
    );
    fx.write(
        3,
        &fx.task_yaml(3, "soon recent", &recent, &[("due_date", date_only(3))]),
    );
    fx.write(
        4,
        &fx.task_yaml(4, "later old", &old, &[("due_date", date_only(30))]),
    );

    let api = api();

    // due filter + ascending due order on export (previously ignored).
    let (status, ids) = rest_export(
        &api,
        &[
            ("project", "QB"),
            ("due", "overdue"),
            ("sort_by", "due-date"),
            ("order", "asc"),
        ],
    );
    assert_eq!(status, 200);
    assert_ids(&ids, &ids_prefixed("QB", &[1]));

    let (status, ids) = rest_export(&api, &[("project", "QB"), ("due", "today")]);
    assert_eq!(status, 200);
    assert_ids(&ids, &ids_prefixed("QB", &[2]));

    let (status, ids) = rest_export(&api, &[("project", "QB"), ("due", "soon")]);
    assert_eq!(status, 200);
    assert_ids(&ids, &ids_prefixed("QB", &[3]));

    let (status, ids) = rest_export(&api, &[("project", "QB"), ("due", "later")]);
    assert_eq!(status, 200);
    assert_ids(&ids, &ids_prefixed("QB", &[4]));

    // recent=7d excludes the 30-day-old tasks on export.
    let (status, ids) = rest_export(&api, &[("project", "QB"), ("recent", "7d")]);
    assert_eq!(status, 200);
    assert_ids(&ids, &ids_prefixed("QB", &[1, 3]));

    // needs=due keeps only tasks without a due date (none here have none).
    fx.write(5, &fx.task_yaml(5, "no due recent", &recent, &[]));
    let (status, ids) = rest_export(&api, &[("project", "QB"), ("needs", "due")]);
    assert_eq!(status, 200);
    assert_ids(&ids, &ids_prefixed("QB", &[5]));

    // Global order on export: modified asc puts the older tasks first.
    let (status, ids) = rest_export(
        &api,
        &[("project", "QB"), ("sort_by", "modified"), ("order", "asc")],
    );
    assert_eq!(status, 200);
    assert_ids(&ids, &ids_prefixed("QB", &[2, 4, 1, 3, 5]));
}

#[test]
fn modified_ties_break_on_canonical_id_asc_even_in_desc_order() {
    let fx = Fixture::new("QC");
    let _guard = EnvVarGuard::set("LOTAR_TASKS_DIR", fx.tasks_dir.to_string_lossy().as_ref());

    let same = "2026-02-02T10:00:00Z";
    fx.write(10, &fx.task_yaml(10, "tie a", same, &[]));
    fx.write(2, &fx.task_yaml(2, "tie b", same, &[]));

    let api = api();
    let (_, ids) = rest_list_all(&api, &[("project", "QC"), ("order", "desc")], 50);
    // Lexical canonical-ID tiebreak: QC-10 < QC-2.
    assert_ids(&ids, &ids_prefixed("QC", &[10, 2]));

    let (_, ids) = rest_list_all(&api, &[("project", "QC"), ("order", "asc")], 50);
    assert_ids(&ids, &ids_prefixed("QC", &[10, 2]));
}

#[test]
fn custom_sort_missing_values_are_deterministic() {
    let fx = Fixture::new("QD");
    let _guard = EnvVarGuard::set("LOTAR_TASKS_DIR", fx.tasks_dir.to_string_lossy().as_ref());

    fx.write(
        1,
        "title: has field\nstatus: Todo\npriority: medium\ntype: task\ncreated: 2026-01-01T00:00:00Z\nmodified: 2026-01-01T00:00:00Z\ntags: []\ncustom_fields:\n  size: big\n",
    );
    fx.write(2, &fx.task_yaml(2, "missing", "2026-01-01T00:00:00Z", &[]));
    fx.write(
        3,
        &fx.task_yaml(3, "missing too", "2026-01-01T00:00:00Z", &[]),
    );

    let api = api();
    // Ascending: missing (empty) values first, then present values; ties ID asc.
    let (_, ids) = rest_list_all(
        &api,
        &[
            ("project", "QD"),
            ("sort_by", "custom:size"),
            ("order", "asc"),
        ],
        50,
    );
    assert_ids(&ids, &ids_prefixed("QD", &[2, 3, 1]));

    // Descending: present values first, missing last, ties still ID asc.
    let (_, ids) = rest_list_all(
        &api,
        &[
            ("project", "QD"),
            ("sort_by", "field:size"),
            ("order", "desc"),
        ],
        50,
    );
    assert_ids(&ids, &ids_prefixed("QD", &[1, 2, 3]));
}

#[test]
fn rest_rejects_invalid_explicit_query_params() {
    let fx = Fixture::new("QE");
    let _guard = EnvVarGuard::set("LOTAR_TASKS_DIR", fx.tasks_dir.to_string_lossy().as_ref());
    fx.write(1, &fx.task_yaml(1, "one", "2026-01-01T00:00:00Z", &[]));

    let api = api();
    let invalid: Vec<(&str, &str)> = vec![
        ("order", "up"),
        ("order", ""),
        ("sort_by", "bogus"),
        ("sort_by", "custom:"),
        ("due", "tomorrow"),
        ("recent", "30d"),
        ("needs", "size"),
        ("needs", "effort,,due"),
        ("sprints", "1,x"),
        ("sprints", "0"),
        ("limit", "0"),
        ("limit", "201"),
        ("limit", "abc"),
        ("offset", "-1"),
    ];
    for (key, value) in &invalid {
        let query = [("project", "QE"), (key, value)];
        let (status, body) = rest_json(&api, &query);
        assert_eq!(
            status, 400,
            "list {key}={value} should be rejected, got {body}"
        );
        let (status, body_text) = rest_export(&api, &query);
        let _ = body_text;
        // Export shares the filter grammar; page params are its only
        // exception (still ignored, matching its no-pagination contract).
        let expects_400 = !matches!(*key, "limit" | "offset");
        assert_eq!(
            status,
            if expects_400 { 400 } else { 200 },
            "export {key}={value} unexpected status"
        );
    }

    // Valid edge values still succeed.
    for (key, value) in [
        ("limit", "200"),
        ("sprints", "1,2"),
        ("needs", "effort,due"),
    ] {
        let (status, _) = rest_json(&api, &[("project", "QE"), (key, value)]);
        assert_eq!(status, 200, "list {key}={value} should be valid");
    }
}

#[test]
fn mcp_task_list_strict_params_and_new_filters() {
    let fx = Fixture::new("QF");
    let _guard = EnvVarGuard::set("LOTAR_TASKS_DIR", fx.tasks_dir.to_string_lossy().as_ref());
    fx.write(
        1,
        &fx.task_yaml(
            1,
            "older",
            "2026-01-01T00:00:00Z",
            &[("effort", "2h".to_string())],
        ),
    );
    fx.write(
        2,
        &fx.task_yaml(2, "newer no effort", "2026-02-01T00:00:00Z", &[]),
    );

    // Default order is deterministic: modified desc.
    let payload = mcp_call("task/list", json!({"project": "QF"})).unwrap();
    assert_ids(&mcp_ids(&payload), &ids_prefixed("QF", &[2, 1]));

    // needs accepts arrays and filters tasks missing effort.
    let payload = mcp_call(
        "task/list",
        json!({"project": "QF", "needs": ["effort"], "sort_by": "modified", "order": "asc"}),
    )
    .unwrap();
    assert_ids(&mcp_ids(&payload), &ids_prefixed("QF", &[2]));

    // due/recent smart filters work over MCP too.
    let payload = mcp_call("task/list", json!({"project": "QF", "due": "overdue"})).unwrap();
    assert!(mcp_ids(&payload).is_empty());

    // Invalid explicit values are errors, never silently dropped filters.
    let cases: Vec<(String, Value)> = vec![
        ("status".into(), json!("Bogus")),
        ("status".into(), json!(["Todo", "AlsoBogus"])),
        ("priority".into(), json!("Ultra")),
        ("type".into(), json!("Widget")),
        ("order".into(), json!("up")),
        ("sort_by".into(), json!("bogus")),
        ("due".into(), json!("tomorrow")),
        ("recent".into(), json!("30d")),
        ("needs".into(), json!("size")),
        ("sprints".into(), json!([1, "x"])),
        ("sprints".into(), json!(0)),
    ];
    for (key, value) in cases {
        let err = mcp_call("task/list", json!({"project": "QF", &key: value})).unwrap_err();
        assert_eq!(
            err["code"].as_i64(),
            Some(-32602),
            "task_list {key}={value} must fail closed: {err}"
        );
    }

    // Type-confused params are also rejected.
    let err = mcp_call(
        "task/list",
        json!({"project": "QF", "order": {"dir": "asc"}}),
    )
    .unwrap_err();
    assert_eq!(err["code"].as_i64(), Some(-32602));

    // Invalid status errors carry enum hints like the mutation tools.
    let err = mcp_call("task/list", json!({"project": "QF", "status": "Bogus"})).unwrap_err();
    let message = err["message"].as_str().unwrap();
    assert!(
        message.to_lowercase().contains("status"),
        "error should name the field: {message}"
    );
}

#[test]
fn due_buckets_match_local_calendar_dates() {
    let fx = Fixture::new("QG");
    let _guard = EnvVarGuard::set("LOTAR_TASKS_DIR", fx.tasks_dir.to_string_lossy().as_ref());

    // Date-only values around every bucket boundary.
    fx.write(
        1,
        &fx.task_yaml(
            1,
            "overdue",
            "2026-01-01T00:00:00Z",
            &[("due_date", date_only(-1))],
        ),
    );
    fx.write(
        2,
        &fx.task_yaml(
            2,
            "today",
            "2026-01-01T00:00:00Z",
            &[("due_date", date_only(0))],
        ),
    );
    fx.write(
        3,
        &fx.task_yaml(
            3,
            "soon first",
            "2026-01-01T00:00:00Z",
            &[("due_date", date_only(1))],
        ),
    );
    fx.write(
        4,
        &fx.task_yaml(
            4,
            "soon last",
            "2026-01-01T00:00:00Z",
            &[("due_date", date_only(7))],
        ),
    );
    fx.write(
        5,
        &fx.task_yaml(
            5,
            "later first",
            "2026-01-01T00:00:00Z",
            &[("due_date", date_only(8))],
        ),
    );
    // Stored RFC3339 instants: local midnight today and late tonight both
    // bucket as "today" (datetime dues bucket by local date, not instant).
    fx.write(
        6,
        &fx.task_yaml(
            6,
            "rfc midnight",
            "2026-01-01T00:00:00Z",
            &[("due_date", local_instant(0, 0, 0))],
        ),
    );
    fx.write(
        7,
        &fx.task_yaml(
            7,
            "rfc late",
            "2026-01-01T00:00:00Z",
            &[("due_date", local_instant(0, 23, 0))],
        ),
    );
    fx.write(8, &fx.task_yaml(8, "no due", "2026-01-01T00:00:00Z", &[]));

    let api = api();
    let bucket = |due: &str| -> Vec<String> {
        let (_, ids) = rest_list_all(&api, &[("project", "QG"), ("due", due)], 50);
        let mut ids = ids;
        ids.sort();
        ids
    };

    assert_ids(&bucket("overdue"), &ids_prefixed("QG", &[1]));
    assert_ids(&bucket("today"), &ids_prefixed("QG", &[2, 6, 7]));
    assert_ids(&bucket("soon"), &ids_prefixed("QG", &[3, 4]));
    assert_ids(&bucket("later"), &ids_prefixed("QG", &[5]));

    // Tasks without a due date never match any due bucket.
    let all: Vec<String> = ids_prefixed("QG", &[8]);
    for due in ["today", "soon", "later", "overdue"] {
        assert!(!bucket(due).contains(&all[0]), "no-due task in {due}");
    }
    let _ = all;
}

#[test]
fn recent_boundary_is_inclusive_at_seven_days() {
    let fx = Fixture::new("QH");
    let _guard = EnvVarGuard::set("LOTAR_TASKS_DIR", fx.tasks_dir.to_string_lossy().as_ref());

    // The handler evaluates `now` slightly after this fixture is written,
    // so the boundary is asserted with a +/-2s tolerance band around the
    // exact 7-day cutoff; exact-cutoff semantics are covered by the unit
    // test with an injected clock.
    let now = Utc::now();
    let cutoff = now - Duration::days(7);
    fx.write(
        1,
        &fx.task_yaml(
            1,
            "just inside cutoff",
            &(cutoff + Duration::seconds(2)).to_rfc3339(),
            &[],
        ),
    );
    fx.write(
        2,
        &fx.task_yaml(
            2,
            "just outside cutoff",
            &(cutoff - Duration::seconds(2)).to_rfc3339(),
            &[],
        ),
    );

    let api = api();
    let (status, ids) = rest_list_all(&api, &[("project", "QH"), ("recent", "7d")], 50);
    assert_eq!(status, 200);
    assert_ids(&ids, &ids_prefixed("QH", &[1]));

    // Transport parity for the same smart filter.
    let (status, export_ids) = rest_export(&api, &[("project", "QH"), ("recent", "7d")]);
    assert_eq!(status, 200);
    assert_ids(&export_ids, &ids_prefixed("QH", &[1]));

    let mcp_ids = mcp_list_all(json!({"project": "QH", "recent": "7d"}), 50);
    assert_ids(&mcp_ids, &ids_prefixed("QH", &[1]));
}

#[test]
fn effort_sort_compares_parsed_values_across_transports() {
    let fx = Fixture::new("QI");
    let _guard = EnvVarGuard::set("LOTAR_TASKS_DIR", fx.tasks_dir.to_string_lossy().as_ref());

    fx.write(
        1,
        &fx.task_yaml(
            1,
            "days",
            "2026-01-01T00:00:00Z",
            &[("effort", quote("1d"))],
        ),
    );
    fx.write(
        2,
        &fx.task_yaml(
            2,
            "hours",
            "2026-01-01T00:00:00Z",
            &[("effort", quote("2h"))],
        ),
    );
    fx.write(3, &fx.task_yaml(3, "none", "2026-01-01T00:00:00Z", &[]));

    let api = api();
    // 2h < 1d (8h); missing effort sorts last in ascending order.
    let (_, ids) = rest_list_all(
        &api,
        &[("project", "QI"), ("sort_by", "effort"), ("order", "asc")],
        50,
    );
    assert_ids(&ids, &ids_prefixed("QI", &[2, 1, 3]));

    let mcp_ids = mcp_list_all(
        json!({"project": "QI", "sort_by": "effort", "order": "asc"}),
        50,
    );
    assert_ids(&mcp_ids, &ids_prefixed("QI", &[2, 1, 3]));
}

fn quote(value: &str) -> String {
    format!("{value:?}")
}

#[test]
fn project_sort_uses_id_prefixes() {
    let fx = Fixture::new("QJ");
    let _guard = EnvVarGuard::set("LOTAR_TASKS_DIR", fx.tasks_dir.to_string_lossy().as_ref());
    let other = fx.tasks_dir.join("QZ");
    std::fs::create_dir_all(&other).unwrap();

    fx.write(1, &fx.task_yaml(1, "in qj", "2026-01-01T00:00:00Z", &[]));
    std::fs::write(
        other.join("1.yml"),
        "title: in qz\nstatus: Todo\npriority: medium\ntype: task\ncreated: 2026-01-01T00:00:00Z\nmodified: 2026-01-01T00:00:00Z\ntags: []\n",
    )
    .unwrap();

    let api = api();
    let (_, ids) = rest_list_all(&api, &[("sort_by", "project"), ("order", "desc")], 50);
    assert_ids(&ids, &["QZ-1".to_string(), "QJ-1".to_string()]);
}

fn cli_list_json(tasks_dir: &std::path::Path, extra: &[&str]) -> Result<Value, String> {
    let mut cmd = common::cargo_bin_silent();
    cmd.env("LOTAR_TASKS_DIR", tasks_dir);
    cmd.current_dir(tasks_dir.parent().unwrap());
    let mut args: Vec<&str> = vec!["--format", "json", "list"];
    args.extend_from_slice(extra);
    cmd.args(&args);
    let out = cmd.output().map_err(|e| e.to_string())?;
    let stdout = String::from_utf8_lossy(&out.stdout).to_string();
    if !out.status.success() {
        return Err(format!(
            "exit {:?}: {}",
            out.status.code(),
            String::from_utf8_lossy(&out.stderr)
        ));
    }
    serde_json::from_str(&stdout).map_err(|e| format!("bad json {e}: {stdout}"))
}

fn cli_ids(payload: &Value) -> Vec<String> {
    payload["tasks"]
        .as_array()
        .map(|tasks| {
            tasks
                .iter()
                .map(|t| t["id"].as_str().unwrap().to_string())
                .collect()
        })
        .unwrap_or_default()
}

#[test]
fn project_scoped_enum_validation_rest_and_mcp() {
    let fx = Fixture::new("PA");
    // Base config allows Todo/Done; PA narrows to Todo/Special; PB keeps a
    // third set so "valid in one project, invalid in another" is exercised.
    std::fs::write(
        fx.tasks_dir.join("config.yml"),
        "issue.states: [Todo, Done]\nissue.types: [Task]\n",
    )
    .unwrap();
    std::fs::create_dir_all(fx.tasks_dir.join("PB")).unwrap();
    std::fs::write(
        fx.tasks_dir.join("PA").join("config.yml"),
        "issue.states: [Todo, Special]\n",
    )
    .unwrap();
    std::fs::write(
        fx.tasks_dir.join("PB").join("config.yml"),
        "issue.states: [Todo, Other]\n",
    )
    .unwrap();

    fx.write(
        1,
        "title: special one\nstatus: Special\npriority: medium\ntype: Task\ncreated: 2026-01-01T00:00:00Z\nmodified: 2026-01-01T00:00:00Z\ntags: []\n",
    );
    std::fs::write(
        fx.tasks_dir.join("PB").join("1.yml"),
        "title: other one\nstatus: Other\npriority: medium\ntype: Task\ncreated: 2026-01-01T00:00:00Z\nmodified: 2026-01-01T00:00:00Z\ntags: []\n",
    )
    .unwrap();

    let _guard = EnvVarGuard::set("LOTAR_TASKS_DIR", fx.tasks_dir.to_string_lossy().as_ref());
    let api = api();

    // Project-only enum values validate against the project's config.
    let (status, body) = rest_json(&api, &[("project", "PA"), ("status", "Special")]);
    assert_eq!(status, 200, "project-only status must validate: {body}");
    assert_ids(&rest_ids_from_json(&body), &ids_prefixed("PA", &[1]));

    // Valid elsewhere but not in the requested project -> 400.
    let (status, _) = rest_json(&api, &[("project", "PA"), ("status", "Other")]);
    assert_eq!(status, 400, "other-project status must be rejected for PA");

    // No project -> base config scope.
    let (status, _) = rest_json(&api, &[("status", "Todo")]);
    assert_eq!(status, 200);
    let (status, _) = rest_json(&api, &[("status", "Special")]);
    assert_eq!(status, 400, "base scope must reject project-only status");

    // MCP parity: same project-scoped validation.
    let payload = mcp_call("task/list", json!({"project": "PA", "status": "Special"}))
        .expect("project-scoped status must validate over MCP");
    assert_ids(&mcp_ids(&payload), &ids_prefixed("PA", &[1]));

    let err = mcp_call("task/list", json!({"project": "PA", "status": "Done"})).unwrap_err();
    assert_eq!(err["code"].as_i64(), Some(-32602));
}

#[test]
fn new_sort_keys_cover_title_reporter_tags_sprints() {
    let fx = Fixture::new("PK");
    let _guard = EnvVarGuard::set("LOTAR_TASKS_DIR", fx.tasks_dir.to_string_lossy().as_ref());

    let mk = |title: &str, reporter: Option<&str>, tags: &str| {
        format!(
            "title: {title}\nstatus: Todo\npriority: medium\ntype: task\ncreated: 2026-01-01T00:00:00Z\nmodified: 2026-01-01T00:00:00Z\ntags: {tags}\nreporter: {}\n",
            reporter.unwrap_or(""),
        )
    };
    fx.write(1, &mk("Beta", Some("zoe"), "[alpha]"));
    fx.write(2, &mk("alpha", None, "[]"));
    fx.write(3, &mk("Gamma", Some("amy"), "[alpha, beta]"));

    // Sprint membership is derived from @sprints files, not the task YAML.
    let sprints_dir = fx.tasks_dir.join("@sprints");
    std::fs::create_dir_all(&sprints_dir).unwrap();
    std::fs::write(sprints_dir.join("9.yml"), "tasks:\n  - PK-1\n").unwrap();
    std::fs::write(sprints_dir.join("2.yml"), "tasks:\n  - PK-2\n").unwrap();
    std::fs::write(sprints_dir.join("3.yml"), "tasks:\n  - PK-2\n").unwrap();

    let api = api();
    let sorted = |key: &str, order: &str| -> Vec<String> {
        let (_, ids) = rest_list_all(
            &api,
            &[("project", "PK"), ("sort_by", key), ("order", order)],
            50,
        );
        ids
    };

    // Title is a case-sensitive scalar: "Beta" < "Gamma" < "alpha".
    assert_ids(&sorted("title", "asc"), &ids_prefixed("PK", &[1, 3, 2]));

    // Reporter mirrors assignee: None first ascending.
    assert_ids(&sorted("reporter", "asc"), &ids_prefixed("PK", &[2, 3, 1]));
    assert_ids(&sorted("reporter", "desc"), &ids_prefixed("PK", &[1, 3, 2]));

    // Tags: array-lex, empty first ascending.
    assert_ids(&sorted("tags", "asc"), &ids_prefixed("PK", &[2, 1, 3]));

    // Sprints: numeric vec-lex on the ascending id list, empty first.
    assert_ids(&sorted("sprints", "asc"), &ids_prefixed("PK", &[3, 2, 1]));

    // MCP parity for the two structured keys.
    let mcp_sorted = mcp_list_all(
        json!({"project": "PK", "sort_by": "sprints", "order": "asc"}),
        50,
    );
    assert_ids(&mcp_sorted, &ids_prefixed("PK", &[3, 2, 1]));
    let mcp_sorted = mcp_list_all(
        json!({"project": "PK", "sort_by": "tags", "order": "desc"}),
        50,
    );
    assert_ids(&mcp_sorted, &ids_prefixed("PK", &[3, 1, 2]));
}

#[test]
fn strict_blank_smart_filters_are_errors() {
    let fx = Fixture::new("PL");
    let _guard = EnvVarGuard::set("LOTAR_TASKS_DIR", fx.tasks_dir.to_string_lossy().as_ref());
    fx.write(1, &fx.task_yaml(1, "one", "2026-01-01T00:00:00Z", &[]));

    let api = api();
    for key in ["due", "recent", "needs"] {
        let (status, body) = rest_json(&api, &[("project", "PL"), (key, "")]);
        assert_eq!(status, 400, "list {key}= (blank) must 400: {body}");
        let (status, _) = rest_export(&api, &[("project", "PL"), (key, "")]);
        assert_eq!(status, 400, "export {key}= (blank) must 400");

        let err = mcp_call("task/list", json!({"project": "PL", key: ""})).unwrap_err();
        assert_eq!(
            err["code"].as_i64(),
            Some(-32602),
            "mcp blank {key} must fail closed: {err}"
        );
    }
}

#[test]
fn cli_shares_default_order_strict_sort_and_due_buckets() {
    let fx = Fixture::new("PM");
    std::fs::write(
        fx.tasks_dir.join("config.yml"),
        "issue.states: [Todo]\nissue.types: [Task]\ncustom_fields: [size]\n",
    )
    .unwrap();

    let today = local_today();
    let today_str = today.format("%Y-%m-%d").to_string();
    let mk = |num: u32, modified: &str, extra: &[(&str, String)]| {
        let mut yaml = format!(
            "title: t{num}\nstatus: Todo\npriority: medium\ntype: Task\ncreated: 2026-01-01T00:00:00Z\nmodified: {modified}\ntags: []\nassignee: Alice\n"
        );
        for (key, value) in extra {
            yaml.push_str(&format!("{key}: {value}\n"));
        }
        yaml
    };
    // Newest modified first by default; PM-2/PM-3 tie and break on ID asc.
    fx.write(
        1,
        &mk(
            1,
            "2026-01-01T00:00:00Z",
            &[("due_date", today_str.clone())],
        ),
    );
    fx.write(
        2,
        &mk(2, "2026-02-01T00:00:00Z", &[("due_date", date_only(8))]),
    );
    fx.write(
        3,
        &mk(3, "2026-02-01T00:00:00Z", &[("due_date", date_only(-1))]),
    );

    let payload = cli_list_json(&fx.tasks_dir, &[]).unwrap();
    assert_ids(&cli_ids(&payload), &ids_prefixed("PM", &[2, 3, 1]));

    // Explicit invalid sort key errors instead of being ignored.
    let err = cli_list_json(&fx.tasks_dir, &["--sort-by", "bogus"]).unwrap_err();
    assert!(
        err.contains("Invalid --sort-by"),
        "expected strict sort error, got: {err}"
    );

    // Bare configured custom field still sorts; unknown bare names error.
    fx.write(
        4,
        "title: t4\nstatus: Todo\npriority: medium\ntype: Task\ncreated: 2026-01-01T00:00:00Z\nmodified: 2026-03-01T00:00:00Z\ntags: []\nassignee: Alice\ncustom_fields:\n  size: small\n",
    );
    let payload = cli_list_json(&fx.tasks_dir, &["--sort-by", "size"]).unwrap();
    assert_ids(&cli_ids(&payload), &ids_prefixed("PM", &[1, 2, 3, 4]));

    // --overdue uses the shared date bucket: yesterday only (a due later
    // today is "today", never overdue, regardless of wall-clock).
    let payload = cli_list_json(&fx.tasks_dir, &["--overdue", "--page-size", "50"]).unwrap();
    assert_ids(&cli_ids(&payload), &ids_prefixed("PM", &[3]));

    // --due-soon window includes today through today+7 inclusive.
    let payload = cli_list_json(&fx.tasks_dir, &["--due-soon", "--page-size", "50"]).unwrap();
    assert_ids(&cli_ids(&payload), &ids_prefixed("PM", &[1]));

    // Custom window: today..=today+3 (nothing here fits since PM-1 is today
    // and PM-2 is +8), and +7 stays inclusive for the default window.
    fx.write(
        5,
        &mk(5, "2026-01-15T00:00:00Z", &[("due_date", date_only(3))]),
    );
    let payload = cli_list_json(&fx.tasks_dir, &["--due-soon=3", "--page-size", "50"]).unwrap();
    // Default order (modified desc) applies within the filtered set.
    assert_ids(&cli_ids(&payload), &ids_prefixed("PM", &[5, 1]));
    let payload = cli_list_json(&fx.tasks_dir, &["--due-soon=2", "--page-size", "50"]).unwrap();
    assert_ids(&cli_ids(&payload), &ids_prefixed("PM", &[1]));

    // Assignee matching is fuzzy like the API: stored "Alice" vs --assignee alice.
    let payload =
        cli_list_json(&fx.tasks_dir, &["--assignee", "alice", "--page-size", "50"]).unwrap();
    assert_eq!(cli_ids(&payload).len(), 5);
}

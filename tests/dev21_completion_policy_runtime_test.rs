//! DEV-21: configurable task completion policy — runtime regressions.
//!
//! Covers the shared policy end to end across surfaces:
//! - `task_state` computed on TaskDTO get/list/update (and the task YAML
//!   staying untouched);
//! - overdue terminal exclusion with per-project policies across REST
//!   list/export, MCP, the CLI `--overdue` flag, and the `stats due`
//!   histogram (due-today stays today even when done);
//! - mixed-project sprint review/velocity/burndown resolving each task
//!   against its own project policy;
//! - reopened / terminal-to-terminal completion timing and per-calendar-cut
//!   burndown replay;
//! - query predicates evaluating the injected clock instead of trusting
//!   TaskDTO snapshots.

mod common;

use chrono::{DateTime, Duration, Local, TimeZone, Utc};
use lotar::api_server::{ApiServer, HttpRequest};
use lotar::routes;
use lotar::services::sprint_reports::{compute_sprint_burndown, compute_sprint_review};
use lotar::services::sprint_service::SprintService;
use lotar::services::sprint_velocity::compute_velocity;
use lotar::storage::manager::Storage;
use lotar::storage::sprint::{Sprint, SprintActual, SprintPlan, SprintTaskEntry};
use serde_json::{Value, json};
use std::collections::HashMap;

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
}

impl Fixture {
    fn new() -> Self {
        let tmp = tempfile::tempdir().unwrap();
        let tasks_dir = tmp.path().join(".tasks");
        std::fs::create_dir_all(&tasks_dir).unwrap();
        Self {
            _tmp: tmp,
            tasks_dir,
        }
    }

    fn project(&self, prefix: &str) -> std::path::PathBuf {
        let dir = self.tasks_dir.join(prefix);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn write_task(&self, prefix: &str, num: u32, body: &str) {
        std::fs::write(self.project(prefix).join(format!("{num}.yml")), body).unwrap();
    }
}

fn local_today() -> chrono::NaiveDate {
    Local::now().date_naive()
}

fn date_only(days_from_today: i64) -> String {
    (local_today() + Duration::days(days_from_today))
        .format("%Y-%m-%d")
        .to_string()
}

fn api() -> ApiServer {
    let mut api = ApiServer::new();
    routes::initialize(&mut api);
    api
}

fn rest_ids(api: &ApiServer, query: &[(&str, &str)]) -> (u16, Vec<String>) {
    let resp = api.handle_request(&mk_req("GET", "/api/tasks/list", query));
    let body: Value = serde_json::from_slice(&resp.body).unwrap();
    (
        resp.status,
        body["data"]["tasks"]
            .as_array()
            .map(|tasks| {
                tasks
                    .iter()
                    .map(|t| t["id"].as_str().unwrap().to_string())
                    .collect()
            })
            .unwrap_or_default(),
    )
}

fn rest_export_ids(api: &ApiServer, query: &[(&str, &str)]) -> Vec<String> {
    let resp = api.handle_request(&mk_req("GET", "/api/tasks/export", query));
    let text = String::from_utf8_lossy(&resp.body).to_string();
    text.lines()
        .skip(1)
        .filter(|line| !line.trim().is_empty())
        .map(|line| line.split(',').next().unwrap().to_string())
        .collect()
}

fn mcp_task_ids(params: Value) -> Vec<String> {
    let req = json!({
        "jsonrpc": "2.0",
        "id": 21,
        "method": "task/list",
        "params": params
    });
    let resp_line = lotar::mcp::server::handle_json_line(&serde_json::to_string(&req).unwrap());
    let resp: Value = serde_json::from_str(&resp_line).unwrap();
    assert!(resp.get("error").is_none(), "mcp error: {resp}");
    let text = resp["result"]["content"][0]["text"].as_str().unwrap();
    let payload: Value = serde_json::from_str(text).unwrap();
    payload["tasks"]
        .as_array()
        .unwrap()
        .iter()
        .map(|t| t["id"].as_str().unwrap().to_string())
        .collect()
}

fn task_yaml(status: &str, due: Option<&str>, extra: &[(&str, &str)]) -> String {
    let mut yaml = format!(
        "title: task {status}\nstatus: {status}\npriority: medium\ntype: task\ncreated: 2026-01-01T00:00:00Z\nmodified: 2026-01-02T00:00:00Z\ntags: []\n"
    );
    if let Some(due) = due {
        yaml.push_str(&format!("due_date: {due}\n"));
    }
    for (key, value) in extra {
        yaml.push_str(&format!("{key}: {value}\n"));
    }
    yaml
}

/// Two projects with different explicit policies: ALPHA treats only
/// `Shipped` as terminal (its `Done` is NOT done), BETA treats `Done` as
/// terminal. Global states cover both.
fn mixed_policy_fixture() -> Fixture {
    let fx = Fixture::new();
    std::fs::write(
        fx.tasks_dir.join("config.yml"),
        "issue:\n  states: [Todo, InProgress, Done, Shipped, Archived]\n",
    )
    .unwrap();
    std::fs::write(
        fx.project("ALPHA").join("config.yml"),
        "project:\n  name: Alpha\nissue:\n  states: [Todo, Done, Shipped, Archived]\n  done_states: [Shipped]\n",
    )
    .unwrap();
    std::fs::write(
        fx.project("BETA").join("config.yml"),
        "project:\n  name: Beta\nissue:\n  states: [Todo, InProgress, Done]\n  done_states: [Done]\n",
    )
    .unwrap();

    fx.write_task("ALPHA", 1, &task_yaml("Todo", Some(&date_only(-1)), &[]));
    // ALPHA Done is NOT terminal under ALPHA's policy: still overdue.
    fx.write_task("ALPHA", 2, &task_yaml("Done", Some(&date_only(-1)), &[]));
    // ALPHA Shipped is terminal: completed work is not overdue.
    fx.write_task("ALPHA", 3, &task_yaml("Shipped", Some(&date_only(-1)), &[]));
    // BETA Done IS terminal: excluded from overdue.
    fx.write_task("BETA", 1, &task_yaml("Done", Some(&date_only(-1)), &[]));
    // Due-today tasks stay today, open or done.
    fx.write_task("BETA", 2, &task_yaml("Todo", Some(&date_only(0)), &[]));
    fx.write_task("BETA", 3, &task_yaml("Done", Some(&date_only(0)), &[]));
    // Later bucket for completeness.
    fx.write_task("BETA", 4, &task_yaml("Todo", Some(&date_only(10)), &[]));
    fx
}

#[test]
fn overdue_excludes_only_terminal_tasks_with_own_project_policy() {
    let fx = mixed_policy_fixture();
    let _guard = EnvVarGuard::set("LOTAR_TASKS_DIR", fx.tasks_dir.to_string_lossy().as_ref());
    let api = api();

    let (status, ids) = rest_ids(&api, &[("due", "overdue"), ("limit", "50")]);
    assert_eq!(status, 200);
    let mut ids = ids;
    ids.sort();
    // ALPHA-2 (Done, not terminal under ALPHA) stays overdue; ALPHA-3
    // (Shipped, terminal) and BETA-1 (Done, terminal) are excluded.
    assert_eq!(ids, vec!["ALPHA-1", "ALPHA-2"]);

    // Export shares the same executor and therefore the same semantics.
    let mut export = rest_export_ids(&api, &[("due", "overdue")]);
    export.sort();
    assert_eq!(export, vec!["ALPHA-1", "ALPHA-2"]);

    // MCP pages over the same filtered set.
    let mut mcp = mcp_task_ids(json!({"due": "overdue", "limit": 50}));
    mcp.sort();
    assert_eq!(mcp, vec!["ALPHA-1", "ALPHA-2"]);

    // Due-today keeps done tasks (only Overdue excludes terminal).
    let (status, ids) = rest_ids(&api, &[("due", "today"), ("limit", "50")]);
    assert_eq!(status, 200);
    let mut ids = ids;
    ids.sort();
    assert_eq!(ids, vec!["BETA-2", "BETA-3"]);
}

#[test]
fn task_state_is_computed_on_dto_and_never_persisted() {
    let fx = mixed_policy_fixture();
    let _guard = EnvVarGuard::set("LOTAR_TASKS_DIR", fx.tasks_dir.to_string_lossy().as_ref());
    let api = api();

    let resp = api.handle_request(&mk_req("GET", "/api/tasks/get", &[("id", "BETA-1")]));
    let body: Value = serde_json::from_slice(&resp.body).unwrap();
    let task = &body["data"];
    let state = &task["task_state"];
    assert!(state.is_object(), "task_state must be populated: {task}");
    // BETA-1: Done + due yesterday -> done, terminal past-due has no
    // bucket; the embedded ordered policy is BETA's own effective list.
    assert_eq!(state["is_done"], json!(true));
    assert_eq!(state["done_states"], json!(["Done"]));
    assert!(state["due_bucket"].is_null());
    assert!(
        state["calendar_day"]
            .as_str()
            .is_some_and(|d| d.len() == 10),
        "calendar_day snapshot: {state}"
    );

    // ALPHA-2: Done but NOT terminal under ALPHA's explicit policy.
    let resp = api.handle_request(&mk_req("GET", "/api/tasks/get", &[("id", "ALPHA-2")]));
    let body: Value = serde_json::from_slice(&resp.body).unwrap();
    let state = &body["data"]["task_state"];
    assert_eq!(state["is_done"], json!(false));
    assert_eq!(state["done_states"], json!(["Shipped"]));
    assert_eq!(state["due_bucket"], json!("overdue"));

    // BETA-3: Done today stays `today`.
    let resp = api.handle_request(&mk_req("GET", "/api/tasks/get", &[("id", "BETA-3")]));
    let body: Value = serde_json::from_slice(&resp.body).unwrap();
    assert_eq!(body["data"]["task_state"]["due_bucket"], json!("today"));

    // The stored YAML never carries computed fields.
    let raw = std::fs::read_to_string(fx.tasks_dir.join("BETA/1.yml")).unwrap();
    assert!(!raw.contains("task_state"), "YAML must stay clean: {raw}");
    assert!(!raw.contains("calendar_day"));
    assert!(!raw.contains("is_done"));

    // Mutations recompute the projection: BETA-2 (open, due today) -> Done.
    let mut storage = Storage::new(&fx.tasks_dir);
    let dto = lotar::services::task_service::TaskService::update(
        &mut storage,
        "BETA-2",
        lotar::api_types::TaskUpdate {
            status: Some("Done".to_string()),
            ..Default::default()
        },
    )
    .unwrap();
    assert!(dto.task_state.as_ref().unwrap().is_done);
    assert_eq!(
        dto.task_state.as_ref().unwrap().due_bucket,
        Some(lotar::api_types::TaskDueBucketDTO::Today)
    );
}

#[test]
fn cli_overdue_and_stats_due_use_project_policies() {
    let fx = mixed_policy_fixture();

    let mut cmd = common::lotar_cmd().unwrap();
    let output = cmd
        .env("LOTAR_TASKS_DIR", fx.tasks_dir.to_string_lossy().as_ref())
        .arg("--format")
        .arg("json")
        .args(["list", "--overdue"])
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    assert!(output.status.success(), "stdout: {stdout}");
    assert!(
        stdout.contains("ALPHA-2"),
        "ALPHA Done stays overdue: {stdout}"
    );
    assert!(stdout.contains("ALPHA-1"));
    assert!(
        !stdout.contains("ALPHA-3") && !stdout.contains("BETA-1"),
        "terminal tasks excluded: {stdout}"
    );

    // stats due histogram: overdue counts only non-terminal past-due work;
    // today includes the done-today task.
    let mut cmd = common::lotar_cmd().unwrap();
    let output = cmd
        .env("LOTAR_TASKS_DIR", fx.tasks_dir.to_string_lossy().as_ref())
        .arg("--format")
        .arg("json")
        .args(["stats", "due", "--global"])
        .output()
        .unwrap();
    assert!(output.status.success());
    let body: Value = serde_json::from_slice(&output.stdout).unwrap();
    let items: HashMap<String, u64> = body["items"]
        .as_array()
        .unwrap()
        .iter()
        .map(|item| {
            (
                item["bucket"].as_str().unwrap().to_string(),
                item["count"].as_u64().unwrap_or(0),
            )
        })
        .collect();
    assert_eq!(items.get("overdue"), Some(&2), "items: {items:?}");
    assert_eq!(items.get("today"), Some(&2), "items: {items:?}");
    // +10 days lands in the legacy "month" bucket (<=31 days), not "later".
    assert_eq!(items.get("month"), Some(&1), "items: {items:?}");
    assert_eq!(items.get("later"), Some(&0), "items: {items:?}");
}

fn sprint_window(tasks: &[&str], start: &str, end: &str) -> Sprint {
    Sprint {
        plan: Some(SprintPlan {
            label: Some("DEV21 Mixed".to_string()),
            starts_at: Some(format!("{start}T09:00:00Z")),
            ends_at: Some(format!("{end}T17:00:00Z")),
            ..SprintPlan::default()
        }),
        actual: Some(SprintActual {
            started_at: Some(format!("{start}T09:00:00Z")),
            closed_at: Some(format!("{end}T17:00:00Z")),
        }),
        tasks: tasks
            .iter()
            .enumerate()
            .map(|(idx, id)| SprintTaskEntry {
                id: (*id).to_string(),
                order: Some((idx + 1) as u32),
            })
            .collect(),
        ..Sprint::default()
    }
}

fn sprint_with(tasks: &[&str]) -> Sprint {
    sprint_window(tasks, "2026-05-01", "2026-05-08")
}

#[test]
fn mixed_project_sprint_analytics_resolve_each_task_policy() {
    let fx = mixed_policy_fixture();
    let mut storage = Storage::new(&fx.tasks_dir);
    let created = SprintService::create(
        &mut storage,
        sprint_with(&["ALPHA-3", "BETA-1", "ALPHA-1"]),
        None,
    )
    .unwrap();
    let records = SprintService::list(&storage).unwrap();
    let record = records
        .iter()
        .find(|r| r.id == created.record.id)
        .unwrap()
        .clone();
    let config = lotar::config::resolution::load_and_merge_configs(Some(&fx.tasks_dir)).unwrap();
    let now = Utc.with_ymd_and_hms(2026, 5, 20, 12, 0, 0).unwrap();

    // Review: ALPHA-3 (Shipped) and BETA-1 (Done) are done under their own
    // policies; ALPHA-1 stays open. Before DEV-21 the single global set
    // (last state = Archived + named done) miscounted one of them.
    let review = compute_sprint_review(&storage, &record, &config, now);
    assert_eq!(review.payload.metrics.total_tasks, 3);
    assert_eq!(review.payload.metrics.done_tasks, 2);

    // Velocity agrees with the review counts.
    let velocity = compute_velocity(
        &storage,
        &records,
        &config,
        &lotar::services::sprint_velocity::VelocityOptions {
            limit: 10,
            include_active: true,
            metric: lotar::services::sprint_metrics::SprintBurndownMetric::Tasks,
        },
        now,
    );
    let entry = velocity
        .entries
        .iter()
        .find(|e| e.sprint_id == record.id)
        .unwrap();
    assert_eq!(entry.completed, 2.0);
    assert_eq!(entry.committed, 3.0);

    // Burndown ends with exactly the open task remaining.
    let burndown = compute_sprint_burndown(&storage, &record, &config, now).unwrap();
    assert_eq!(burndown.computation.totals.tasks, 3);
    let last = burndown.computation.series.last().unwrap();
    assert_eq!(last.remaining_tasks, 1);

    // Project stats agree with each project's own policy.
    let alpha = lotar::services::project_service::ProjectService::stats(&storage, "ALPHA");
    // ALPHA: Done is open, only Shipped is done.
    assert_eq!(alpha.done_count, 1);
    assert_eq!(alpha.open_count, 2);
    let beta = lotar::services::project_service::ProjectService::stats(&storage, "BETA");
    assert_eq!(beta.done_count, 2);
    assert_eq!(beta.open_count, 2);
}

fn history_task_yaml(status: &str, history: &str) -> String {
    format!(
        "title: replay\ncrstatus: placeholder\nstatus: {status}\npriority: medium\ntype: task\ncreated: 2026-09-01T08:00:00Z\nmodified: 2026-09-06T10:00:00Z\ntags: []\neffort: 2pt\n{history}"
    )
}

#[test]
fn reopened_and_terminal_to_terminal_timing_replay_per_cut() {
    let fx = Fixture::new();
    std::fs::write(
        fx.tasks_dir.join("config.yml"),
        "issue:\n  states: [Todo, Done, Closed]\n  done_states: [Done, Closed]\n",
    )
    .unwrap();
    let history = "history:\n  - at: 2026-09-02T10:00:00Z\n    changes:\n      - field: status\n        old: Todo\n        new: Done\n  - at: 2026-09-04T10:00:00Z\n    changes:\n      - field: status\n        old: Done\n        new: Todo\n  - at: 2026-09-05T10:00:00Z\n    changes:\n      - field: status\n        old: Todo\n        new: Done\n  - at: 2026-09-06T10:00:00Z\n    changes:\n      - field: status\n        old: Done\n        new: Closed\n";
    fx.write_task("RP", 1, &history_task_yaml("Closed", history));

    let mut storage = Storage::new(&fx.tasks_dir);
    let created = SprintService::create(
        &mut storage,
        sprint_window(&["RP-1"], "2026-09-01", "2026-09-08"),
        None,
    )
    .unwrap();
    let records = SprintService::list(&storage).unwrap();
    let record = records
        .iter()
        .find(|r| r.id == created.record.id)
        .unwrap()
        .clone();
    let config = lotar::config::resolution::load_and_merge_configs(Some(&fx.tasks_dir)).unwrap();

    let task = storage.get("RP-1", "RP").expect("task");
    let done: std::collections::HashSet<String> =
        ["done", "closed"].iter().map(|s| s.to_string()).collect();

    // Current unbroken interval: the reopen cleared the first completion,
    // the re-completion at 09-05 started the current interval, and the
    // Done -> Closed move did NOT restart it.
    let started = lotar::services::completion::current_completion_started_at(&task, &done);
    assert_eq!(
        started,
        Some(
            DateTime::parse_from_rfc3339("2026-09-05T10:00:00Z")
                .unwrap()
                .with_timezone(&Utc)
        )
    );

    // Per-cut replay: done on 09-02/09-03, open after the reopen until the
    // re-completion, done afterwards (including the Closed day).
    let cut = |day: &str, hour: u32| {
        DateTime::parse_from_rfc3339(&format!("2026-09-{day}T{hour:02}:00:00Z"))
            .unwrap()
            .with_timezone(&Utc)
    };
    assert!(lotar::services::completion::task_done_at_cut(
        &task,
        &done,
        cut("03", 0)
    ));
    // Cuts are strictly-before: at the reopen instant itself the
    // pre-transition (Done) state still holds; one hour later it does not.
    assert!(lotar::services::completion::task_done_at_cut(
        &task,
        &done,
        cut("04", 10)
    ));
    assert!(!lotar::services::completion::task_done_at_cut(
        &task,
        &done,
        cut("04", 11)
    ));
    assert!(!lotar::services::completion::task_done_at_cut(
        &task,
        &done,
        cut("05", 0)
    ));
    assert!(lotar::services::completion::task_done_at_cut(
        &task,
        &done,
        cut("06", 0)
    ));

    // Burndown replays per calendar cut: remaining flips back up after the
    // reopen instead of using the earliest-ever done timestamp.
    let now = Utc.with_ymd_and_hms(2026, 9, 20, 12, 0, 0).unwrap();
    let burndown = compute_sprint_burndown(&storage, &record, &config, now).unwrap();
    let remaining: Vec<usize> = burndown
        .computation
        .series
        .iter()
        .map(|point| point.remaining_tasks)
        .collect();
    // Days 09-01..09-08 (window anchored by actual start/end).
    assert_eq!(
        remaining,
        vec![1, 0, 0, 1, 0, 0, 0, 0],
        "series: {remaining:?}"
    );
}

#[test]
fn missing_history_falls_back_to_modified_estimate() {
    let fx = Fixture::new();
    std::fs::write(
        fx.tasks_dir.join("config.yml"),
        "issue:\n  states: [Todo, Done]\n",
    )
    .unwrap();
    // No history at all: legacy conscious estimate from modified.
    fx.write_task("LG", 1, &task_yaml("Done", None, &[]));

    let storage = Storage::new(&fx.tasks_dir);
    let task = storage.get("LG-1", "LG").expect("task");
    let done: std::collections::HashSet<String> = ["done"].iter().map(|s| s.to_string()).collect();
    let started = lotar::services::completion::current_completion_started_at(&task, &done);
    assert_eq!(
        started,
        Some(
            DateTime::parse_from_rfc3339("2026-01-02T00:00:00Z")
                .unwrap()
                .with_timezone(&Utc)
        )
    );
    // Before the estimate the task counts as open.
    assert!(!lotar::services::completion::task_done_at_cut(
        &task,
        &done,
        DateTime::parse_from_rfc3339("2026-01-01T12:00:00Z")
            .unwrap()
            .with_timezone(&Utc)
    ));
}

#[test]
fn query_predicates_evaluate_injected_clock_not_snapshots() {
    use lotar::api_types::{TaskDTO, TaskDueBucketDTO, TaskStateDTO};
    use lotar::services::task_query::{self, TaskQueryOptions};

    let now = Utc.with_ymd_and_hms(2026, 6, 15, 12, 0, 0).unwrap();
    let today = task_query::today_local(now);
    let tomorrow_local = (today + Duration::days(1)).and_hms_opt(18, 0, 0).unwrap();
    let due_tomorrow = Local
        .from_local_datetime(&tomorrow_local)
        .single()
        .unwrap()
        .to_rfc3339();

    // Stale snapshot claims a different calendar day and bucket; predicates
    // must ignore it and classify from the raw due value + injected clock.
    let dto = |due: Option<String>| TaskDTO {
        id: "X-1".to_string(),
        title: "clock".to_string(),
        status: lotar::types::TaskStatus::from("Todo"),
        task_state: Some(TaskStateDTO {
            is_done: false,
            done_states: Vec::new(),
            due_bucket: Some(TaskDueBucketDTO::Later),
            calendar_day: "2000-01-01".to_string(),
        }),
        priority: lotar::types::Priority::from("Medium"),
        task_type: lotar::types::TaskType::from("Feature"),
        deleted_at: None,
        reporter: None,
        assignee: None,
        created: now.to_rfc3339(),
        modified: now.to_rfc3339(),
        due_date: due,
        effort: None,
        subtitle: None,
        description: None,
        tags: Vec::new(),
        relationships: Default::default(),
        comments: Vec::new(),
        references: Vec::new(),
        acceptance_criteria: Vec::new(),
        sprints: Vec::new(),
        sprint_order: Default::default(),
        history: Vec::new(),
        custom_fields: Default::default(),
    };

    let mut tasks = vec![
        ("X-1".to_string(), dto(Some(due_tomorrow.clone()))),
        (
            "X-2".to_string(),
            dto(Some(today.format("%Y-%m-%d").to_string())),
        ),
    ];
    let mut options = TaskQueryOptions {
        due: Some(task_query::parse_due("soon").unwrap()),
        ..TaskQueryOptions::default()
    };
    task_query::apply(&mut tasks, &options, now);
    let ids: Vec<&str> = tasks.iter().map(|(id, _)| id.as_str()).collect();
    // The stale `later` snapshot did not leak into the predicate.
    assert_eq!(ids, vec!["X-1"]);

    // Same dues, later clock: on rollover the previous tomorrow is today,
    // while the previous date-only today must no longer match.
    let later = now + Duration::hours(12);
    let mut tasks = vec![
        ("X-1".to_string(), dto(Some(due_tomorrow.clone()))),
        (
            "X-2".to_string(),
            dto(Some(today.format("%Y-%m-%d").to_string())),
        ),
    ];
    options = TaskQueryOptions {
        due: Some(task_query::parse_due("today").unwrap()),
        ..TaskQueryOptions::default()
    };
    task_query::apply(&mut tasks, &options, later);
    let today_later = task_query::today_local(later);
    let ids: Vec<&str> = tasks.iter().map(|(id, _)| id.as_str()).collect();
    if today_later != today {
        assert_eq!(
            ids,
            vec!["X-1"],
            "date rollover must re-bucket tomorrow into today"
        );
    } else {
        assert_eq!(ids, vec!["X-2"]);
    }
}

#[test]
fn due_parser_and_buckets_agree_across_dst_boundary() {
    use lotar::services::task_query::{DueFilter, due_raw_matches_bucket, today_local};

    // Pin both sides of spring-forward and both passes of fall-back.
    // Explicit offsets avoid ambiguous local-time conversion and silent skips.
    for rfc3339 in [
        "2026-03-08T01:30:00-05:00",
        "2026-03-08T03:30:00-04:00",
        "2026-11-01T01:30:00-04:00",
        "2026-11-01T01:30:00-05:00",
    ] {
        let instant = chrono::DateTime::parse_from_rfc3339(rfc3339).unwrap();
        let now = instant.with_timezone(&Utc) - Duration::days(2);
        let today = today_local(now);
        let date_only = instant.with_timezone(&Local).date_naive();
        let bucket_rfc = due_raw_matches_bucket(Some(rfc3339), DueFilter::Soon, today);
        let bucket_date = due_raw_matches_bucket(
            Some(&date_only.format("%Y-%m-%d").to_string()),
            DueFilter::Soon,
            today,
        );
        assert!(bucket_rfc, "boundary instant must be due soon: {rfc3339}");
        assert_eq!(bucket_rfc, bucket_date, "rfc3339: {rfc3339}");
    }
}

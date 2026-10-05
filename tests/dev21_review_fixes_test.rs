//! DEV-21 review fixes: actual-root policy resolution, per-task status
//! aggregates, genuine DST boundary tests, and read-time (fail-closed)
//! validation of hand-edited explicit done policies.
//!
//! F1: two DIFFERENT task IDs sharing one prefix live in two workspace
//! roots with different terminal sets. Every classification surface
//! (metadata/TaskDTO, query filters, sprint metrics, dependency blocking,
//! cleanup eligibility) must resolve each task against its OWN root's
//! policy, in both input orders.
//! F2: sprint status aggregates count done per task; the breakdown row's
//! done flag is unanimous (mixed rows report false).
//! F3: DST boundaries pinned to actual 2026-03-08 / 2026-11-01 instants
//! with explicit offsets — no environment mutation, no silent-skip guards.
//! Load-time: hand-edited invalid explicit policies fail closed on read;
//! repair through the candidate pipeline keeps working.

mod common;

use chrono::{Duration, NaiveDate, TimeZone, Utc};
use lotar::api_server::{ApiServer, HttpRequest};
use lotar::routes;
use lotar::services::sprint_reports::compute_sprint_review;
use lotar::services::sprint_service::SprintService;
use lotar::services::task_service::TaskService;
use lotar::storage::manager::Storage;
use lotar::storage::sprint::{Sprint, SprintActual, SprintPlan, SprintTaskEntry};
use serde_json::{Value, json};
use std::collections::HashMap;

use crate::common::env_mutex::EnvVarGuard;

// ---------------------------------------------------------------------------
// F1 fixture: nested workspace roots, same prefix, different policies.
// ---------------------------------------------------------------------------

struct NestedFixture {
    _tmp: tempfile::TempDir,
    /// Primary workspace tasks root (policy: only `Shipped` is terminal).
    root_a: std::path::PathBuf,
    /// Nested workspace tasks root discovered from A (policy: `Done` is
    /// terminal). The locator discovers downward only (monorepo layout).
    root_b: std::path::PathBuf,
}

impl NestedFixture {
    fn new() -> Self {
        let tmp = tempfile::tempdir().unwrap();
        let root_a = tmp.path().join("wsa").join(".tasks");
        let root_b = tmp.path().join("wsa").join("wsb").join(".tasks");
        for (root, done) in [(&root_a, "Shipped"), (&root_b, "Done")] {
            std::fs::create_dir_all(root.join("PX")).unwrap();
            std::fs::write(
                root.join("config.yml"),
                format!("issue:\n  states: [Todo, Done, Shipped]\n  done_states: [{done}]\n"),
            )
            .unwrap();
        }
        let yesterday = (chrono::Local::now().date_naive() - Duration::days(1))
            .format("%Y-%m-%d")
            .to_string();
        for (root, num) in [(&root_a, 1u32), (&root_b, 2u32)] {
            std::fs::write(
                root.join("PX").join(format!("{num}.yml")),
                format!(
                    "title: px{num}\nstatus: Done\npriority: medium\ntype: task\ncreated: 2026-01-01T00:00:00Z\nmodified: 2026-01-02T00:00:00Z\ntags: []\ndue_date: {yesterday}\n"
                ),
            )
            .unwrap();
        }
        Self {
            _tmp: tmp,
            root_a,
            root_b,
        }
    }

    fn storage_a(&self) -> Storage {
        Storage::new(&self.root_a)
    }
}

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

fn api() -> ApiServer {
    let mut api = ApiServer::new();
    routes::initialize(&mut api);
    api
}

#[test]
fn f1_done_sets_resolve_per_task_actual_root_in_both_orders() {
    let fx = NestedFixture::new();
    let storage = fx.storage_a();

    let forward = lotar::services::completion::resolve_task_done_sets(&storage, ["PX-1", "PX-2"]);
    let backward = lotar::services::completion::resolve_task_done_sets(&storage, ["PX-2", "PX-1"]);
    for (label, sets) in [("forward", forward.clone()), ("backward", backward.clone())] {
        let done_1: std::collections::HashSet<String> = sets.done_set("PX-1").unwrap().clone();
        let done_2: std::collections::HashSet<String> = sets.done_set("PX-2").unwrap().clone();
        // PX-1 sits in A (Shipped terminal); PX-2 sits in B (Done terminal).
        assert!(
            done_1.contains("shipped") && !done_1.contains("done"),
            "{label}"
        );
        assert!(
            done_2.contains("done") && !done_2.contains("shipped"),
            "{label}"
        );
    }
    // The two orders produce identical maps: the cache key is
    // (root, prefix), never the first task's root alone.
    assert_eq!(forward.done_set("PX-1"), backward.done_set("PX-1"));
    assert_eq!(forward.done_set("PX-2"), backward.done_set("PX-2"));
}

#[test]
fn f1_metadata_and_query_classify_by_actual_root() {
    let fx = NestedFixture::new();
    let _guard = EnvVarGuard::set("LOTAR_TASKS_DIR", fx.root_a.to_string_lossy().as_ref());
    let api = api();

    // TaskDTO.task_state embeds each task's OWN effective policy and
    // classification (Done + past due: terminal in B, overdue in A).
    let resp = api.handle_request(&mk_req("GET", "/api/tasks/get", &[("id", "PX-1")]));
    let body: Value = serde_json::from_slice(&resp.body).unwrap();
    let state = &body["data"]["task_state"];
    assert_eq!(state["is_done"], json!(false));
    assert_eq!(state["due_bucket"], json!("overdue"));
    assert_eq!(
        state["done_states"],
        json!(["Shipped"]),
        "embedded policy must be A's for PX-1: {state}"
    );

    let resp = api.handle_request(&mk_req("GET", "/api/tasks/get", &[("id", "PX-2")]));
    let body: Value = serde_json::from_slice(&resp.body).unwrap();
    let state = &body["data"]["task_state"];
    assert_eq!(state["is_done"], json!(true));
    assert!(state["due_bucket"].is_null());
    assert_eq!(
        state["done_states"],
        json!(["Done"]),
        "embedded policy must be B's for PX-2: {state}"
    );

    // Query: the overdue filter excludes only the terminal task (PX-2),
    // wherever the iteration meets it. storage.search spans the nested
    // root, so both tasks compete for the same filter.
    let resp = api.handle_request(&mk_req(
        "GET",
        "/api/tasks/list",
        &[("due", "overdue"), ("limit", "50")],
    ));
    let body: Value = serde_json::from_slice(&resp.body).unwrap();
    let mut ids: Vec<String> = body["data"]["tasks"]
        .as_array()
        .unwrap()
        .iter()
        .map(|t| t["id"].as_str().unwrap().to_string())
        .collect();
    ids.sort();
    assert_eq!(ids, vec!["PX-1"], "overdue must keep A's Done task");
}

#[test]
fn f1_optimistic_status_flips_use_embedded_policy() {
    let fx = NestedFixture::new();
    // Mutate from the root that owns the ticket: Done -> Todo -> Done must
    // flip is_done while done_states stays the ticket's own policy.
    let mut storage = Storage::new(&fx.root_b);
    let flip = |storage: &mut Storage, status: &str| {
        TaskService::update(
            storage,
            "PX-2",
            lotar::api_types::TaskUpdate {
                status: Some(status.to_string()),
                ..Default::default()
            },
        )
        .unwrap()
    };
    let reopened = flip(&mut storage, "Todo");
    assert!(!reopened.task_state.as_ref().unwrap().is_done);
    assert_eq!(
        reopened.task_state.as_ref().unwrap().done_states,
        vec!["Done"]
    );
    let reclosed = flip(&mut storage, "Done");
    assert!(reclosed.task_state.as_ref().unwrap().is_done);
    assert_eq!(
        reclosed.task_state.as_ref().unwrap().done_states,
        vec!["Done"]
    );
}

#[test]
fn f1_sprint_metrics_classify_cross_root_tasks_per_policy() {
    let fx = NestedFixture::new();
    let mut storage = fx.storage_a();
    let sprint = Sprint {
        plan: Some(SprintPlan {
            label: Some("Cross-root".to_string()),
            starts_at: Some("2026-05-01T09:00:00Z".to_string()),
            ends_at: Some("2026-05-08T17:00:00Z".to_string()),
            ..SprintPlan::default()
        }),
        actual: Some(SprintActual {
            started_at: Some("2026-05-01T09:00:00Z".to_string()),
            closed_at: Some("2026-05-08T17:00:00Z".to_string()),
        }),
        tasks: vec![
            SprintTaskEntry {
                id: "PX-1".to_string(),
                order: Some(1),
            },
            SprintTaskEntry {
                id: "PX-2".to_string(),
                order: Some(2),
            },
        ],
        ..Sprint::default()
    };
    let created = SprintService::create(&mut storage, sprint, None).unwrap();
    let record = SprintService::list(&storage)
        .unwrap()
        .into_iter()
        .find(|r| r.id == created.record.id)
        .unwrap();
    let config = lotar::config::resolution::load_and_merge_configs(Some(&fx.root_a)).unwrap();
    let now = Utc.with_ymd_and_hms(2026, 5, 20, 12, 0, 0).unwrap();

    let review = compute_sprint_review(&storage, &record, &config, now);
    assert_eq!(review.payload.metrics.total_tasks, 2);
    // PX-2 (terminal in B) done; PX-1 (Done, non-terminal in A) remaining.
    assert_eq!(review.payload.metrics.done_tasks, 1);
    let remaining: Vec<&str> = review
        .payload
        .remaining_tasks
        .iter()
        .map(|t| t.id.as_str())
        .collect();
    assert_eq!(remaining, vec!["PX-1"]);
    // F2 row semantics on the shared "Done" label: mixed terminality ->
    // unanimous flag false, count 2.
    let done_row = review
        .payload
        .metrics
        .status_breakdown
        .iter()
        .find(|row| row.status.eq_ignore_ascii_case("done"))
        .unwrap();
    assert_eq!(done_row.count, 2);
    assert!(!done_row.done, "mixed terminality must not be unanimous");
}

// ---------------------------------------------------------------------------
// F2: per-task status aggregates in both sprint entry orders.
// ---------------------------------------------------------------------------

struct MixedFixture {
    _tmp: tempfile::TempDir,
}

fn mixed_both_done_fixture(
    order: &[&str],
) -> (
    MixedFixture,
    Storage,
    lotar::services::sprint_service::SprintRecord,
) {
    let tmp = tempfile::tempdir().unwrap();
    let tasks_dir = tmp.path().join(".tasks");
    std::fs::create_dir_all(&tasks_dir).unwrap();
    std::fs::write(
        tasks_dir.join("config.yml"),
        "issue:\n  states: [Todo, Done, Shipped]\n",
    )
    .unwrap();
    for (prefix, done) in [("ALPHA", "Shipped"), ("BETA", "Done")] {
        let dir = tasks_dir.join(prefix);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            dir.join("config.yml"),
            format!("project:\n  name: {prefix}\nissue:\n  done_states: [{done}]\n"),
        )
        .unwrap();
        std::fs::write(
            dir.join("1.yml"),
            "title: both done\nstatus: Done\npriority: medium\ntype: task\ncreated: 2026-05-01T00:00:00Z\nmodified: 2026-05-02T00:00:00Z\ntags: []\n",
        )
        .unwrap();
    }
    let mut storage = Storage::new(&tasks_dir);
    let sprint = Sprint {
        plan: Some(SprintPlan {
            label: Some("Mixed Done".to_string()),
            starts_at: Some("2026-05-01T09:00:00Z".to_string()),
            ends_at: Some("2026-05-08T17:00:00Z".to_string()),
            ..SprintPlan::default()
        }),
        actual: Some(SprintActual {
            started_at: Some("2026-05-01T09:00:00Z".to_string()),
            closed_at: Some("2026-05-08T17:00:00Z".to_string()),
        }),
        tasks: order
            .iter()
            .enumerate()
            .map(|(idx, id)| SprintTaskEntry {
                id: (*id).to_string(),
                order: Some((idx + 1) as u32),
            })
            .collect(),
        ..Sprint::default()
    };
    let created = SprintService::create(&mut storage, sprint, None).unwrap();
    let record = SprintService::list(&storage)
        .unwrap()
        .into_iter()
        .find(|r| r.id == created.record.id)
        .unwrap();
    (MixedFixture { _tmp: tmp }, storage, record)
}

#[test]
fn f2_same_status_string_terminal_in_one_project_only() {
    let now = Utc.with_ymd_and_hms(2026, 5, 20, 12, 0, 0).unwrap();
    for order in [["ALPHA-1", "BETA-1"], ["BETA-1", "ALPHA-1"]] {
        let (_fx, storage, record) = mixed_both_done_fixture(&order);
        let config =
            lotar::config::resolution::load_and_merge_configs(Some(&storage.root_path)).unwrap();
        let review = compute_sprint_review(&storage, &record, &config, now);

        // Both tasks carry status Done, but only BETA's Done is terminal.
        assert_eq!(review.payload.metrics.total_tasks, 2);
        assert_eq!(review.payload.metrics.done_tasks, 1, "order: {order:?}");
        let remaining: Vec<&str> = review
            .payload
            .remaining_tasks
            .iter()
            .map(|t| t.id.as_str())
            .collect();
        assert_eq!(remaining, vec!["ALPHA-1"], "order: {order:?}");

        let done_row = review
            .payload
            .metrics
            .status_breakdown
            .iter()
            .find(|row| row.status.eq_ignore_ascii_case("done"))
            .unwrap();
        assert_eq!(done_row.count, 2);
        // Mixed terminality under one label: unanimous flag is false.
        assert!(!done_row.done, "order: {order:?}");
    }
}

#[test]
fn f2_unanimous_status_rows_stay_done() {
    let tmp = tempfile::tempdir().unwrap();
    let tasks_dir = tmp.path().join(".tasks");
    std::fs::create_dir_all(tasks_dir.join("B1")).unwrap();
    std::fs::create_dir_all(tasks_dir.join("B2")).unwrap();
    std::fs::write(
        tasks_dir.join("config.yml"),
        "issue:\n  states: [Todo, Done]\n",
    )
    .unwrap();
    for prefix in ["B1", "B2"] {
        std::fs::write(
            tasks_dir.join(prefix).join("1.yml"),
            "title: unanimous\nstatus: Done\npriority: medium\ntype: task\ncreated: 2026-05-01T00:00:00Z\nmodified: 2026-05-02T00:00:00Z\ntags: []\n",
        )
        .unwrap();
    }
    let mut storage = Storage::new(&tasks_dir);
    let sprint = Sprint {
        plan: Some(SprintPlan {
            starts_at: Some("2026-05-01T09:00:00Z".to_string()),
            ends_at: Some("2026-05-08T17:00:00Z".to_string()),
            ..SprintPlan::default()
        }),
        tasks: vec![
            SprintTaskEntry {
                id: "B1-1".into(),
                order: Some(1),
            },
            SprintTaskEntry {
                id: "B2-1".into(),
                order: Some(2),
            },
        ],
        ..Sprint::default()
    };
    let created = SprintService::create(&mut storage, sprint, None).unwrap();
    let record = SprintService::list(&storage)
        .unwrap()
        .into_iter()
        .find(|r| r.id == created.record.id)
        .unwrap();
    let config = lotar::config::resolution::load_and_merge_configs(Some(&tasks_dir)).unwrap();
    let now = Utc.with_ymd_and_hms(2026, 5, 20, 12, 0, 0).unwrap();
    let review = compute_sprint_review(&storage, &record, &config, now);
    assert_eq!(review.payload.metrics.done_tasks, 2);
    let done_row = review
        .payload
        .metrics
        .status_breakdown
        .iter()
        .find(|row| row.status.eq_ignore_ascii_case("done"))
        .unwrap();
    assert_eq!(done_row.count, 2);
    assert!(done_row.done, "all tasks terminal -> unanimous row");
}

// ---------------------------------------------------------------------------
// F3: genuine DST boundary tests (pinned offsets, no silent skips).
// ---------------------------------------------------------------------------

#[test]
fn f3_fall_back_hour_offsets_parse_deterministically() {
    use lotar::services::task_query::parse_stored_due;

    // 2026-11-01 US fall-back: the ambiguous local 01:30 exists twice with
    // different offsets. Pinned RFC3339 values with explicit offsets are
    // distinct instants (no local ambiguity resolution involved).
    let first_pass = parse_stored_due("2026-11-01T01:30:00-04:00")
        .expect("first pass of the repeated hour must parse");
    let second_pass =
        parse_stored_due("2026-11-01T01:30:00-05:00").expect("second pass must parse");
    match (first_pass, second_pass) {
        (
            lotar::services::task_query::StoredDue::Instant(a),
            lotar::services::task_query::StoredDue::Instant(b),
        ) => {
            assert_eq!(a.to_rfc3339(), "2026-11-01T05:30:00+00:00");
            assert_eq!(b.to_rfc3339(), "2026-11-01T06:30:00+00:00");
            assert_ne!(a, b);
        }
        other => panic!("expected instants, got {other:?}"),
    }
    // Date-only values keep their calendar identity on the boundary day.
    assert_eq!(
        parse_stored_due("2026-11-01"),
        Some(lotar::services::task_query::StoredDue::Date(
            NaiveDate::from_ymd_opt(2026, 11, 1).unwrap()
        ))
    );
    // Offset-carrying values never consult local midnight for parsing;
    // classification lands on the SERVER's local date of the instant
    // (environment-dependent by contract, asserted self-consistently).
    let instant = parse_stored_due("2026-03-08T02:30:00-05:00")
        .expect("offset instant parses regardless of local gaps");
    assert_eq!(
        instant.local_date(),
        chrono::DateTime::parse_from_rfc3339("2026-03-08T02:30:00-05:00")
            .unwrap()
            .with_timezone(&chrono::Local)
            .date_naive()
    );
}

#[test]
fn f3_bucket_and_replay_boundaries_pinned_at_dst_days() {
    use lotar::services::completion::{compute_task_state, task_done_at_cut};
    use lotar::services::task_query::{DueFilter, due_raw_matches_bucket, today_local};
    use lotar::types::TaskStatus;

    let spring = NaiveDate::from_ymd_opt(2026, 3, 8).unwrap();
    let fall = NaiveDate::from_ymd_opt(2026, 11, 1).unwrap();
    // Pure bucket predicates on the boundary days themselves.
    assert!(due_raw_matches_bucket(
        Some("2026-03-08"),
        DueFilter::Today,
        spring
    ));
    assert!(due_raw_matches_bucket(
        Some("2026-11-01"),
        DueFilter::Today,
        fall
    ));
    assert!(due_raw_matches_bucket(
        Some("2026-03-07"),
        DueFilter::Overdue,
        spring
    ));
    assert!(due_raw_matches_bucket(
        Some("2026-10-31"),
        DueFilter::Overdue,
        fall
    ));

    // Injected-clock snapshot: calendar_day is the local date OF THE
    // INJECTED INSTANT (never Utc::now()), for boundary instants on both
    // transition days.
    let config = lotar::config::types::ResolvedConfig::from_global(
        lotar::config::types::GlobalConfig::default(),
    );
    for now in [
        Utc.with_ymd_and_hms(2026, 3, 8, 7, 0, 0).unwrap(), // 02:00 local jump (US)
        Utc.with_ymd_and_hms(2026, 11, 1, 6, 0, 0).unwrap(), // repeated hour boundary
    ] {
        let state = compute_task_state(&TaskStatus::from("Todo"), None, &config, now);
        assert_eq!(
            state.calendar_day,
            today_local(now).format("%Y-%m-%d").to_string()
        );
    }

    // Replay cuts are pure UTC instants: both passes of the repeated local
    // hour classify against the same strict-before cut.
    let task = lotar::storage::task::Task::default();
    let done: std::collections::HashSet<String> =
        ["done"].iter().map(|s| (*s).to_string()).collect();
    let cut = Utc.with_ymd_and_hms(2026, 11, 1, 12, 0, 0).unwrap();
    assert!(!task_done_at_cut(&task, &done, cut));
}

#[test]
fn f3_burndown_utc_grid_uniform_across_23_25h_local_days() {
    let build = |start: &str,
                 end: &str,
                 done_at: &[&str]|
     -> (
        tempfile::TempDir,
        Storage,
        lotar::services::sprint_service::SprintRecord,
    ) {
        let tmp = tempfile::tempdir().unwrap();
        let tasks_dir = tmp.path().join(".tasks");
        std::fs::create_dir_all(tasks_dir.join("DS")).unwrap();
        std::fs::write(
            tasks_dir.join("config.yml"),
            "issue:\n  states: [Todo, Done]\n",
        )
        .unwrap();
        let mut history = String::new();
        for at in done_at {
            history.push_str(&format!(
                "  - at: {at}\n    changes:\n      - field: status\n        old: Todo\n        new: Done\n"
            ));
        }
        std::fs::write(
            tasks_dir.join("DS").join("1.yml"),
            format!(
                "title: grid\nstatus: Done\npriority: medium\ntype: task\ncreated: 2026-03-05T00:00:00Z\nmodified: 2026-03-09T00:00:00Z\ntags: []\nhistory:\n{history}"
            ),
        )
        .unwrap();
        let mut storage = Storage::new(&tasks_dir);
        let sprint = Sprint {
            plan: Some(SprintPlan {
                starts_at: Some(format!("{start}T00:00:00Z")),
                ends_at: Some(format!("{end}T00:00:00Z")),
                ..SprintPlan::default()
            }),
            actual: Some(SprintActual {
                started_at: Some(format!("{start}T00:00:00Z")),
                closed_at: Some(format!("{end}T00:00:00Z")),
            }),
            tasks: vec![SprintTaskEntry {
                id: "DS-1".into(),
                order: Some(1),
            }],
            ..Sprint::default()
        };
        let created = SprintService::create(&mut storage, sprint, None).unwrap();
        let record = SprintService::list(&storage)
            .unwrap()
            .into_iter()
            .find(|r| r.id == created.record.id)
            .unwrap();
        (tmp, storage, record)
    };

    let config = lotar::config::types::ResolvedConfig::from_global(
        lotar::config::types::GlobalConfig::default(),
    );
    let now = Utc.with_ymd_and_hms(2026, 12, 1, 12, 0, 0).unwrap();

    // Spring-forward week (local day 2026-03-08 is 23h): the UTC grid still
    // flips at UTC midnights — done at 06:30Z on 03-08 counts from the
    // 03-09 cut (day whose day_end is 03-09T00:00Z).
    let (_keep, storage, record) = build("2026-03-06", "2026-03-10", &["2026-03-08T06:30:00Z"]);
    let burndown =
        lotar::services::sprint_reports::compute_sprint_burndown(&storage, &record, &config, now)
            .unwrap();
    let remaining: Vec<usize> = burndown
        .computation
        .series
        .iter()
        .map(|p| p.remaining_tasks)
        .collect();
    assert_eq!(remaining, vec![1, 1, 0, 0, 0], "spring-forward grid");

    // Fall-back week (local day 2026-11-01 is 25h): both passes of the
    // repeated local hour (05:30Z and 06:30Z) flip at the same UTC cut —
    // the 25h local day never splits a UTC grid cell.
    let (_keep, storage, record) = build("2026-10-30", "2026-11-03", &["2026-11-01T05:30:00Z"]);
    let burndown =
        lotar::services::sprint_reports::compute_sprint_burndown(&storage, &record, &config, now)
            .unwrap();
    let remaining: Vec<usize> = burndown
        .computation
        .series
        .iter()
        .map(|p| p.remaining_tasks)
        .collect();
    assert_eq!(remaining, vec![1, 1, 0, 0, 0], "fall-back grid");
}

// ---------------------------------------------------------------------------
// Load-time fail-closed validation + candidate repair.
// ---------------------------------------------------------------------------

fn resolver_for(path: &std::path::Path) -> lotar::workspace::TasksDirectoryResolver {
    lotar::workspace::TasksDirectoryResolver {
        path: path.to_path_buf(),
        source: lotar::workspace::TasksDirectorySource::CurrentDirectory,
    }
}

#[test]
fn hand_edited_invalid_done_policy_fails_closed_on_read() {
    use lotar::services::config_service::ConfigService;

    // Empty explicit list (global).
    let tmp = tempfile::tempdir().unwrap();
    let tasks_dir = tmp.path().join(".tasks");
    std::fs::create_dir_all(&tasks_dir).unwrap();
    std::fs::write(
        tasks_dir.join("config.yml"),
        "issue:\n  states: [Todo, Done]\n  done_states: []\n",
    )
    .unwrap();
    let resolver = resolver_for(&tasks_dir);
    let err = ConfigService::show(&resolver, None)
        .unwrap_err()
        .to_string();
    assert!(
        err.contains("issue.done_states") && err.to_lowercase().contains("empty"),
        "clear fail-closed error: {err}"
    );
    let err = ConfigService::inspect(&resolver, None)
        .unwrap_err()
        .to_string();
    assert!(err.contains("issue.done_states"), "inspect: {err}");
    let err = lotar::config::resolution::load_and_merge_configs(Some(&tasks_dir))
        .unwrap_err()
        .to_string();
    assert!(err.contains("issue.done_states"), "runtime load: {err}");

    // Subset violation in a PROJECT config (references a status the
    // project's own states lack).
    let tmp = tempfile::tempdir().unwrap();
    let tasks_dir = tmp.path().join(".tasks");
    std::fs::create_dir_all(tasks_dir.join("PX")).unwrap();
    std::fs::write(
        tasks_dir.join("config.yml"),
        "issue:\n  states: [Todo, Done, Shipped]\n",
    )
    .unwrap();
    std::fs::write(
        tasks_dir.join("PX").join("config.yml"),
        "project:\n  name: Broken\nissue:\n  states: [Todo, Done]\n  done_states: [Shipped]\n",
    )
    .unwrap();
    let err = lotar::config::resolution::config_for_project(&tasks_dir, Some("PX"))
        .unwrap_err()
        .to_string();
    assert!(
        err.contains("issue.done_states") && err.contains("Shipped"),
        "project subset violation: {err}"
    );
    let resolver = resolver_for(&tasks_dir);
    let err = ConfigService::show(&resolver, Some("PX"))
        .unwrap_err()
        .to_string();
    assert!(err.contains("Failed to load project config"), "{err}");

    // Environment layer: valid statuses that violate the subset contract.
    let tmp = tempfile::tempdir().unwrap();
    let tasks_dir = tmp.path().join(".tasks");
    std::fs::create_dir_all(&tasks_dir).unwrap();
    std::fs::write(
        tasks_dir.join("config.yml"),
        "issue:\n  states: [Todo, Done]\n",
    )
    .unwrap();
    let _guard = EnvVarGuard::set("LOTAR_ISSUE_DONE_STATES", "Shipped");
    let err = lotar::config::resolution::load_and_merge_configs(Some(&tasks_dir))
        .unwrap_err()
        .to_string();
    assert!(
        err.contains("issue.done_states") && err.contains("Shipped"),
        "env subset violation: {err}"
    );
}

#[test]
fn invalid_explicit_policy_does_not_turn_into_builtin_done_in_task_projection() {
    let tmp = tempfile::tempdir().unwrap();
    let tasks_dir = tmp.path().join(".tasks");
    std::fs::create_dir_all(tasks_dir.join("PX")).unwrap();
    std::fs::write(
        tasks_dir.join("config.yml"),
        "issue:\n  states: [Todo, Done]\n  done_states: []\n",
    )
    .unwrap();
    let task_file = tasks_dir.join("PX/1.yml");
    let original = "title: invalid policy\nstatus: Done\npriority: Medium\ntype: Task\ncreated: 2026-01-01T00:00:00Z\nmodified: 2026-01-02T00:00:00Z\n";
    std::fs::write(&task_file, original).unwrap();
    let task = TaskService::get(&Storage::new(&tasks_dir), "PX-1", None).unwrap();
    let state = task.task_state.unwrap();
    assert!(!state.is_done);
    assert!(state.done_states.is_empty());
    assert_eq!(std::fs::read_to_string(task_file).unwrap(), original);
}

#[test]
fn invalid_done_policy_is_repairable_through_the_candidate() {
    use lotar::services::config_service::ConfigService;
    use std::collections::BTreeMap;

    let tmp = tempfile::tempdir().unwrap();
    let tasks_dir = tmp.path().join(".tasks");
    std::fs::create_dir_all(&tasks_dir).unwrap();
    std::fs::write(
        tasks_dir.join("config.yml"),
        "issue:\n  states: [Todo, Done]\n  done_states: []\n",
    )
    .unwrap();
    let resolver = resolver_for(&tasks_dir);

    // Repair the invalid global list through the full candidate pipeline.
    let mut values = BTreeMap::new();
    values.insert("issue_done_states".to_string(), "Done".to_string());
    ConfigService::set(&resolver, &values, true, None)
        .expect("repair must stay possible despite the invalid current value");

    let show = ConfigService::show(&resolver, None).unwrap();
    assert_eq!(show["done_states_mode"].as_str(), Some("explicit"));
    assert_eq!(show["effective_done_states"], json!(["Done"]));

    // Project repair: clear an invalid project override back to inherit.
    std::fs::create_dir_all(tasks_dir.join("PX")).unwrap();
    std::fs::write(
        tasks_dir.join("PX").join("config.yml"),
        "project:\n  name: Broken\nissue:\n  done_states: [NotAStatus]\n",
    )
    .unwrap();
    let mut values = BTreeMap::new();
    values.insert("issue_done_states".to_string(), String::new());
    ConfigService::set(&resolver, &values, false, Some("PX"))
        .expect("project repair must stay possible");
    let show = ConfigService::show(&resolver, Some("PX")).unwrap();
    assert_eq!(show["effective_done_states"], json!(["Done"]));
}

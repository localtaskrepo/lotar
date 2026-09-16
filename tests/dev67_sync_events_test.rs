mod common;

use lotar::api_events::{self, ApiEvent};
use lotar::workspace::{TasksDirectoryResolver, TasksDirectorySource};
use std::sync::mpsc::Receiver;

fn workspace(config: &str) -> tempfile::TempDir {
    let root = tempfile::tempdir().unwrap();
    std::fs::write(lotar::utils::paths::global_config_path(root.path()), config).unwrap();
    root
}

fn resolver(root: &tempfile::TempDir) -> TasksDirectoryResolver {
    TasksDirectoryResolver {
        path: root.path().to_path_buf(),
        source: TasksDirectorySource::CommandLineFlag,
    }
}

fn drain_run(rx: &Receiver<ApiEvent>, run_id: &str) -> Vec<ApiEvent> {
    let mut events = Vec::new();
    while let Ok(evt) = rx.try_recv() {
        if evt.data.get("run_id").and_then(|v| v.as_str()) == Some(run_id) {
            events.push(evt);
        }
    }
    events
}

fn sole_event<'a>(events: &'a [ApiEvent], kind: &str) -> &'a ApiEvent {
    let matched: Vec<&ApiEvent> = events.iter().filter(|e| e.kind == kind).collect();
    assert_eq!(
        matched.len(),
        1,
        "expected exactly one {}, saw kinds {:?}",
        kind,
        events.iter().map(|e| e.kind.clone()).collect::<Vec<_>>()
    );
    matched[0]
}

fn dry_run_push(
    root: &tempfile::TempDir,
    project: Option<&str>,
    run_id: &str,
) -> lotar::api_types::SyncResponse {
    lotar::services::sync_service::SyncService::push(
        &resolver(root),
        "origin",
        project,
        true,
        None,
        None,
        Some(false),
        true,
        Some(run_id),
    )
    .unwrap()
}

#[test]
fn completion_event_carries_execution_project_after_zero_items() {
    let root = workspace(
        "default.project: TEST\nremotes:\n  origin:\n    provider: github\n    repo: org/repo\n",
    );
    let rx = api_events::subscribe();
    let response = dry_run_push(&root, Some("TEST"), "dev67-explicit");

    let events = drain_run(&rx, "dev67-explicit");
    let started = sole_event(&events, "sync_started");
    let completed = sole_event(&events, "sync_completed");
    assert!(
        !events.iter().any(|e| e.kind == "sync_failed"),
        "dry-run push must not fail, saw {:?}",
        events.iter().map(|e| e.kind.clone()).collect::<Vec<_>>()
    );

    assert_eq!(
        completed.data.get("project").and_then(|v| v.as_str()),
        Some("TEST"),
        "completion must expose the top-level project string the SSE project filter matches"
    );
    assert_eq!(
        started.data.get("project").and_then(|v| v.as_str()),
        Some("TEST")
    );
    assert_eq!(
        completed.data["report"]
            .get("project")
            .and_then(|v| v.as_str()),
        Some("TEST"),
        "top-level project equals the embedded report project"
    );
    assert_eq!(
        completed.data.get("run_id").and_then(|v| v.as_str()),
        Some("dev67-explicit")
    );
    assert_eq!(response.project.as_deref(), Some("TEST"));
    assert_eq!(
        completed.data["report"]
            .get("entries_total")
            .and_then(|v| v.as_u64()),
        Some(0),
        "completion fires even when the run processed zero items"
    );
}

#[test]
fn completion_event_falls_back_to_default_project_for_global_run() {
    let root = workspace(
        "default.project: TEST\nremotes:\n  origin:\n    provider: github\n    repo: org/repo\n",
    );
    let rx = api_events::subscribe();
    let response = dry_run_push(&root, None, "dev67-default");

    let events = drain_run(&rx, "dev67-default");
    let started = sole_event(&events, "sync_started");
    let completed = sole_event(&events, "sync_completed");
    assert_eq!(
        completed.data.get("project").and_then(|v| v.as_str()),
        Some("TEST"),
        "a run with no explicit project resolves the configured default project"
    );
    assert_eq!(
        started.data.get("project").and_then(|v| v.as_str()),
        Some("TEST")
    );
    assert_eq!(response.project.as_deref(), Some("TEST"));
}

#[test]
fn completion_event_serializes_null_project_when_none_resolves() {
    let root = workspace("remotes:\n  origin:\n    provider: github\n    repo: org/repo\n");
    let rx = api_events::subscribe();
    let response = dry_run_push(&root, None, "dev67-null");

    let events = drain_run(&rx, "dev67-null");
    let completed = sole_event(&events, "sync_completed");
    assert!(
        completed.data.get("project").is_some_and(|v| v.is_null()),
        "project key stays present and serializes as null when no project resolves"
    );
    assert!(
        completed
            .data
            .get("project")
            .and_then(|v| v.as_str())
            .is_none(),
        "a null project never matches a project-filtered SSE stream"
    );
    assert_eq!(response.project, None);
}

//! DEV-771: `GET /api/jobs` (the Agents page) counts and lists the queue of
//! every process for the tasks directory, as `lotar agent queue` does: the
//! tickets another process queued and the jobs another process runs.

use crate::common;
use crate::common::env_mutex::EnvVarGuard;
use lotar::api_server::{ApiServer, HttpRequest};
use lotar::services::agent_active_jobs::{self, ActiveJob};
use serde_json::Value;
use std::collections::HashMap;

#[test]
fn the_jobs_route_shows_what_another_process_queued_and_runs() {
    let fixtures = common::TestFixtures::new();
    let root = fixtures.get_temp_path().to_path_buf();
    let queue_dir = root.join("queue");
    std::fs::write(fixtures.tasks_root.join("config.yml"), "default:\n  project: TEST\nagents:\n  quick:\n    runner: command\n    command: 'true'\n").unwrap();
    let cmd = || {
        let mut cmd = common::cargo_bin_silent();
        cmd.current_dir(&root).env("LOTAR_AGENT_QUEUE_DIR", &queue_dir).env("LOTAR_AGENT_QUEUE_DISABLE_WORKER", "1");
        cmd
    };
    for title in ["first", "second", "third"] {
        assert!(cmd().args(["add", title]).output().unwrap().status.success());
    }
    // Another process queues two tickets; no worker starts them.
    for ticket in ["TEST-1", "TEST-2"] {
        let run = cmd().args(["agent", "run", ticket, "go", "--agent", "quick"]).output().unwrap();
        assert!(run.status.success(), "{}{}", String::from_utf8_lossy(&run.stdout), String::from_utf8_lossy(&run.stderr));
    }

    // The CLI found the tasks directory from its working directory, symlinks resolved.
    let _tasks = EnvVarGuard::set("LOTAR_TASKS_DIR", &fixtures.tasks_root.canonicalize().unwrap().to_string_lossy());
    let _queue = EnvVarGuard::set("LOTAR_AGENT_QUEUE_DIR", &queue_dir.to_string_lossy());
    // And a live process (pid 1) runs a job on the third ticket.
    let elsewhere = ActiveJob {
        job_id: "job-elsewhere".into(),
        ticket_id: "TEST-3".into(),
        runner: "command".into(),
        agent: Some("quick".into()),
        owner_pid: 1,
        child_pid: 1,
        started_at: "2026-11-21T00:00:00Z".into(),
        session_id: None,
        merge: false,
        assignee_at_start: None,
        in_setup: false,
        request: None,
    };
    agent_active_jobs::register(&fixtures.tasks_root, &elsewhere);

    let cli = cmd().args(["--format", "json", "agent", "queue"]).output().unwrap();
    let cli: Value = serde_json::from_slice(&cli.stdout).unwrap();

    let mut api = ApiServer::new();
    lotar::routes::initialize(&mut api);
    let resp =
        api.handle_request(&HttpRequest { method: "GET".into(), path: "/api/jobs".into(), query: HashMap::new(), headers: HashMap::new(), body: Vec::new() });
    agent_active_jobs::unregister(&fixtures.tasks_root, &elsewhere.job_id);
    assert_eq!(resp.status, 200, "{}", String::from_utf8_lossy(&resp.body));
    let data = serde_json::from_slice::<Value>(&resp.body).unwrap()["data"].clone();

    assert_eq!(data["queue_stats"]["running"], 1, "{data}");
    assert_eq!(data["queue_stats"]["queued"], 2, "{data}");
    assert_eq!(data["queue_stats"]["running"], cli["running_jobs"], "{cli}");
    assert_eq!(data["queue_stats"]["queued"], cli["queued_jobs"], "{cli}");

    let queue = &data["queue"];
    assert_eq!(queue["worker_state"], "stopped", "{queue}");
    let pending = queue["pending"].as_array().unwrap();
    let tickets: Vec<_> = pending.iter().map(|entry| entry["ticket_id"].as_str().unwrap()).collect();
    assert_eq!(tickets, ["TEST-1", "TEST-2"], "queue order: {queue}");
    for entry in pending {
        assert_eq!(entry["agent"], "quick", "{entry}");
        assert!(entry["queued_at"].as_str().is_some_and(|at| !at.is_empty()), "{entry}");
        assert!(entry["waits_for"].as_str().is_some_and(|why| why.contains("lotar agent queue resume")), "{entry}");
    }
    assert_eq!(queue["pending"], cli["pending"], "the CLI lists the same entries");
    assert_eq!(queue["waiting_for_prerequisites"], serde_json::json!([]), "{queue}");
}

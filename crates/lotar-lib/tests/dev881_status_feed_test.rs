//! DEV-881: `GET /api/status` lists this instance's running jobs, its agent
//! queue (held while memory is short) and the tickets that wait for someone,
//! each with an absolute link into this instance's UI.

use crate::common;
use crate::common::env_mutex::EnvVarGuard;
use lotar::api_server::{ApiServer, HttpRequest};
use lotar::services::agent_active_jobs::{self, ActiveJob};
use serde_json::Value;
use std::collections::HashMap;

fn status(api: &ApiServer) -> Value {
    let headers = HashMap::from([("host".to_string(), "127.0.0.1:4242".to_string())]);
    let resp = api.handle_request(&HttpRequest { method: "GET".into(), path: "/api/status".into(), query: HashMap::new(), headers, body: Vec::new() });
    assert_eq!(resp.status, 200, "{}", String::from_utf8_lossy(&resp.body));
    serde_json::from_slice::<Value>(&resp.body).unwrap()["data"].clone()
}

#[test]
fn the_status_feed_shows_running_jobs_a_held_queue_and_who_a_ticket_waits_on() {
    let fixtures = common::TestFixtures::new();
    let root = fixtures.get_temp_path().to_path_buf();
    let queue_dir = root.join("queue");
    std::fs::write(
        fixtures.tasks_root.join("config.yml"),
        "default:\n  project: TEST\nissue:\n  states: [Todo, InProgress, HelpNeeded, Blocked, Done]\nui:\n  name: Scenario\nagent:\n  memory_gate:\n    enabled: true\nagents:\n  quick:\n    runner: command\n    command: 'true'\n",
    )
    .unwrap();
    let cmd = || {
        let mut cmd = common::cargo_bin_silent();
        cmd.current_dir(&root).env("LOTAR_AGENT_QUEUE_DIR", &queue_dir).env("LOTAR_AGENT_QUEUE_DISABLE_WORKER", "1");
        cmd
    };
    let run = |args: &[&str]| {
        let out = cmd().args(args).output().unwrap();
        assert!(out.status.success(), "{args:?}: {}{}", String::from_utf8_lossy(&out.stdout), String::from_utf8_lossy(&out.stderr));
    };
    for title in ["queued one", "needs the owner", "running one", "blocked later"] {
        run(&["add", title]);
    }
    run(&["agent", "run", "TEST-1", "go", "--agent", "quick"]);
    run(&["status", "TEST-2", "HelpNeeded"]);
    run(&["assignee", "TEST-2", "mallox"]);

    let tasks_dir = fixtures.tasks_root.canonicalize().unwrap();
    let _tasks = EnvVarGuard::set("LOTAR_TASKS_DIR", &tasks_dir.to_string_lossy());
    let _queue = EnvVarGuard::set("LOTAR_AGENT_QUEUE_DIR", &queue_dir.to_string_lossy());
    // Memory is short: the queue starts no job.
    let _memory = EnvVarGuard::set("LOTAR_MEMORY_READING", "pressure=warn,swap=9.4/10");
    assert!(lotar::services::memory_gate::check(&tasks_dir).is_some(), "the gate holds");
    // A live process (pid 1) runs a job on TEST-3.
    let elsewhere = ActiveJob {
        job_id: "job-elsewhere".into(),
        ticket_id: "TEST-3".into(),
        runner: "command".into(),
        agent: Some("quick".into()),
        owner_pid: 1,
        child_pid: 1,
        started_at: "2026-11-25T10:00:00Z".into(),
        session_id: None,
        merge: false,
        assignee_at_start: None,
        in_setup: false,
        request: None,
    };
    agent_active_jobs::register(&fixtures.tasks_root, &elsewhere);

    let mut api = ApiServer::new();
    lotar::routes::initialize(&mut api);
    let feed = status(&api);

    assert_eq!(feed["version"], 1, "{feed}");
    assert_eq!(feed["instance"], serde_json::json!({"name": "Scenario", "url": "http://127.0.0.1:4242"}), "{feed}");

    let running = feed["running"].as_array().unwrap();
    assert_eq!(running.len(), 1, "{feed}");
    assert_eq!(running[0]["ticket_id"], "TEST-3");
    assert_eq!(running[0]["title"], "running one");
    assert_eq!(running[0]["profile"], "quick");
    assert_eq!(running[0]["started_at"], "2026-11-25T10:00:00Z");
    assert_eq!(running[0]["job_url"], "http://127.0.0.1:4242/agents?job=job-elsewhere");
    assert_eq!(running[0]["ticket_url"], "http://127.0.0.1:4242/task/TEST-3");

    let queue = &feed["queue"];
    assert_eq!(queue["count"], 1, "{queue}");
    assert_eq!(queue["held"], true, "{queue}");
    assert!(queue["held_reason"].as_str().is_some_and(|why| why.starts_with("waiting for memory: macOS reports memory pressure warn")), "{queue}");
    assert_eq!(queue["entries"][0]["ticket_id"], "TEST-1", "{queue}");
    assert_eq!(queue["entries"][0]["profile"], "quick", "{queue}");
    assert_eq!(queue["entries"][0]["ticket_url"], "http://127.0.0.1:4242/task/TEST-1", "{queue}");

    let waiting = feed["waiting"].as_array().unwrap();
    assert_eq!(waiting.len(), 1, "{feed}");
    assert_eq!(waiting[0]["ticket_id"], "TEST-2");
    assert_eq!(waiting[0]["title"], "needs the owner");
    assert_eq!(waiting[0]["status"], "HelpNeeded");
    assert_eq!(waiting[0]["assignee"], "mallox");
    assert!(waiting[0]["since"].as_str().is_some_and(|since| since.starts_with("20")), "{feed}");
    assert_eq!(waiting[0]["ticket_url"], "http://127.0.0.1:4242/task/TEST-2");

    // Another process blocks TEST-4; the server's file watcher announces it,
    // and the feed reads that one ticket again.
    run(&["status", "TEST-4", "Blocked"]);
    lotar::api_events::emit(&lotar::api_events::ApiEvent { kind: "task_updated".into(), data: serde_json::json!({"id": "TEST-4"}) });
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    let feed = loop {
        let feed = status(&api);
        if feed["waiting"].as_array().unwrap().len() == 2 || std::time::Instant::now() > deadline {
            break feed;
        }
        std::thread::sleep(std::time::Duration::from_millis(20));
    };
    agent_active_jobs::unregister(&fixtures.tasks_root, &elsewhere.job_id);
    let tickets: Vec<_> = feed["waiting"].as_array().unwrap().iter().map(|ticket| (ticket["ticket_id"].clone(), ticket["status"].clone())).collect();
    assert_eq!(tickets, [("TEST-2".into(), "HelpNeeded".into()), ("TEST-4".into(), "Blocked".into())], "{feed}");
}

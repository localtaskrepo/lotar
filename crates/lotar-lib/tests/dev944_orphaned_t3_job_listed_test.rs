//! DEV-944: a `t3` job whose lotar process exited before it ended still runs
//! in T3 Code, so `agent list-running` lists it, marked, until a worker or
//! server takes it over: the live stall check must not restart its ticket.

use crate::common::env_mutex::EnvVarGuard;
use lotar::services::agent_active_jobs::{self, ActiveJob};
use serde_json::Value;

fn job(id: &str, ticket: &str, runner: &str, owner_pid: u32) -> ActiveJob {
    ActiveJob {
        job_id: id.into(),
        ticket_id: ticket.into(),
        runner: runner.into(),
        agent: None,
        owner_pid,
        child_pid: 0,
        started_at: "2026-11-27T22:25:41Z".into(),
        session_id: Some("mcp:launch:thread".into()),
        merge: false,
        assignee_at_start: None,
        in_setup: false,
        request: None,
        host: Default::default(),
    }
}

#[test]
fn an_orphaned_t3_job_is_still_listed_as_running() {
    let temp = tempfile::tempdir().unwrap();
    let queue = temp.path().join("queue");
    let tasks = temp.path().join("repo/.tasks");
    std::fs::create_dir_all(&tasks).unwrap();
    let _env = EnvVarGuard::set("LOTAR_AGENT_QUEUE_DIR", queue.to_str().unwrap());
    let alive = std::process::id();
    let mut gone = std::process::Command::new("true").spawn().unwrap();
    let dead = gone.id();
    gone.wait().unwrap();

    agent_active_jobs::register(&tasks, &job("job-live", "QIS-1", "t3", alive));
    agent_active_jobs::register(&tasks, &job("job-orphan", "QIS-217", "t3", dead));
    // Not running any more: a command job's agent stopped with its process, and
    // a t3 job that died in its worktree setup started no thread.
    agent_active_jobs::register(&tasks, &job("job-command", "QIS-2", "command", dead));
    agent_active_jobs::register(&tasks, &ActiveJob { in_setup: true, ..job("job-setup", "QIS-3", "t3", dead) });

    let run = |args: &[&str]| {
        let out = std::process::Command::new(crate::common::lotar_bin())
            .current_dir(tasks.parent().unwrap())
            .env_remove("RUST_TEST_THREADS")
            .env("LOTAR_IGNORE_HOME_CONFIG", "1")
            .env("LOTAR_AGENT_QUEUE_DIR", &queue)
            .env("LOTAR_TASKS_DIR", &tasks)
            .env("LOTAR_TEST_SILENT", "1")
            .args(args)
            .output()
            .unwrap();
        assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
        String::from_utf8(out.stdout).unwrap()
    };
    let ours = |json: &Value| -> Vec<(String, bool)> {
        json["jobs"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|job| job["ticket_id"].as_str().is_some_and(|id| id.starts_with("QIS-")))
            .map(|job| (job["job_id"].as_str().unwrap().to_string(), job["orphaned"] == true))
            .collect()
    };

    for all in [false, true] {
        let mut args = vec!["--format", "json", "agent", "list-running"];
        if all {
            args.push("--all");
        }
        let json: Value = serde_json::from_str(&run(&args)).unwrap();
        let mut jobs = ours(&json);
        jobs.sort();
        assert_eq!(jobs, [("job-live".to_string(), false), ("job-orphan".to_string(), true)], "--all {all}: {json}");
    }

    let text = run(&["agent", "list-running"]);
    assert!(text.lines().any(|line| line == format!("0\t{dead} (exited)\tjob-orphan\tQIS-217\tt3\t2026-11-27T22:25:41Z")), "{text}");
    assert!(text.lines().any(|line| line == format!("0\t{alive}\tjob-live\tQIS-1\tt3\t2026-11-27T22:25:41Z")), "{text}");

    // The status guards and the queue still see only live owners: the orphan
    // is a worker's or server's to take over.
    let live: Vec<String> = agent_active_jobs::list(&tasks).into_iter().map(|job| job.job_id).collect();
    assert_eq!(live, ["job-live"]);
}

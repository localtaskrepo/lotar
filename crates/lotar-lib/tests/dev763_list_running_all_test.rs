//! DEV-763: `agent list-running --all` lists the running jobs of every
//! tasks folder on this machine, with the process that runs each one.

use crate::common::env_mutex::EnvVarGuard;
use lotar::services::agent_active_jobs::{self, ActiveJob};
use serde_json::Value;

fn job(id: &str, ticket: &str, owner_pid: u32, started_at: &str) -> ActiveJob {
    ActiveJob {
        job_id: id.into(),
        ticket_id: ticket.into(),
        runner: "t3".into(),
        agent: None,
        owner_pid,
        child_pid: 0,
        started_at: started_at.into(),
        session_id: None,
        merge: false,
        assignee_at_start: None,
        in_setup: false,
        request: None,
    }
}

#[test]
fn list_running_all_shows_the_jobs_of_every_tasks_folder() {
    let temp = tempfile::tempdir().unwrap();
    let queue = temp.path().join("queue");
    let first = temp.path().join("one/.tasks");
    let second = temp.path().join("two/.tasks");
    std::fs::create_dir_all(&first).unwrap();
    std::fs::create_dir_all(&second).unwrap();
    let _env = EnvVarGuard::set("LOTAR_AGENT_QUEUE_DIR", queue.to_str().unwrap());
    let alive = std::process::id();
    let mut gone = std::process::Command::new("true").spawn().unwrap();
    let dead = gone.id();
    gone.wait().unwrap();

    agent_active_jobs::register(&first, &job("job-one", "ONE-1", alive, "2026-11-21T10:00:00Z"));
    agent_active_jobs::register(&second, &job("job-two", "TWO-1", alive, "2026-11-21T11:00:00Z"));
    agent_active_jobs::register(&second, &job("job-dead", "TWO-2", dead, "2026-11-21T09:00:00Z"));
    // A record written before records named their tasks folder.
    let old = queue.join("agent-queue/active-old");
    std::fs::create_dir_all(&old).unwrap();
    std::fs::write(old.join("job-old.json"), serde_json::to_string(&job("job-old", "OLD-1", alive, "2026-11-21T12:00:00Z")).unwrap()).unwrap();

    let run = |args: &[&str]| {
        let out = std::process::Command::new(crate::common::lotar_bin())
            .current_dir(first.parent().unwrap())
            .env_remove("RUST_TEST_THREADS")
            .env("LOTAR_IGNORE_HOME_CONFIG", "1")
            .env("LOTAR_AGENT_QUEUE_DIR", &queue)
            .env("LOTAR_TASKS_DIR", &first)
            .env("LOTAR_TEST_SILENT", "1")
            .args(args)
            .output()
            .unwrap();
        assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
        String::from_utf8(out.stdout).unwrap()
    };

    let json: Value = serde_json::from_str(&run(&["--format", "json", "agent", "list-running", "--all"])).unwrap();
    // Other jobs on this machine (lotar-agent-wrapper processes) may show too.
    let ours = ["ONE-1", "TWO-1", "TWO-2", "OLD-1"];
    let jobs: Vec<&Value> = json["jobs"].as_array().unwrap().iter().filter(|job| ours.iter().any(|t| job["ticket_id"] == *t)).collect();
    let ids: Vec<&str> = jobs.iter().filter_map(|job| job["job_id"].as_str()).collect();
    assert_eq!(ids, ["job-one", "job-two", "job-old"], "{json}");
    let canonical = |dir: &std::path::Path| dir.canonicalize().unwrap().to_string_lossy().into_owned();
    assert_eq!(jobs[0]["tasks_dir"], canonical(&first).as_str());
    assert_eq!(jobs[1]["tasks_dir"], canonical(&second).as_str());
    assert_eq!(jobs[2]["tasks_dir"], Value::Null);
    assert_eq!(jobs[1]["owner_pid"], alive);
    assert_eq!(jobs[1]["pid"], 0);
    assert_eq!(jobs[1]["started_at"], "2026-11-21T11:00:00Z");

    let text = run(&["agent", "list-running", "--all"]);
    let lines: Vec<&str> = text.lines().collect();
    assert_eq!(lines[0], "pid\towner_pid\tjob_id\tticket\trunner\tstarted_at\ttasks_dir");
    assert!(lines.contains(&format!("0\t{alive}\tjob-old\tOLD-1\tt3\t2026-11-21T12:00:00Z\t-").as_str()), "{text}");

    // Without --all only this tasks folder's jobs show.
    let json: Value = serde_json::from_str(&run(&["--format", "json", "agent", "list-running"])).unwrap();
    let ids: Vec<&str> =
        json["jobs"].as_array().unwrap().iter().filter(|job| ours.iter().any(|t| job["ticket_id"] == *t)).filter_map(|job| job["job_id"].as_str()).collect();
    assert_eq!(ids, ["job-one"], "{json}");
    let text = run(&["agent", "list-running"]);
    assert_eq!(text.lines().next(), Some("pid\towner_pid\tjob_id\tticket\trunner\tstarted_at"));
}

//! DEV-320: status and assignee changes made while an agent job runs on a
//! ticket are not refused; they become the job's outcome and win over the
//! `set` of the rule that runs when the job ends.
#![cfg(unix)]

use crate::common;
use common::TestFixtures;
use std::time::{Duration, Instant};

/// Lets the agent finish and reaps it, also when an assertion fails.
struct Finish {
    go: std::path::PathBuf,
    runner: std::process::Child,
}

impl Drop for Finish {
    fn drop(&mut self) {
        let _ = std::fs::write(&self.go, b"");
        let deadline = Instant::now() + Duration::from_secs(10);
        while matches!(self.runner.try_wait(), Ok(None)) && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(100));
        }
        let _ = self.runner.kill();
        let _ = self.runner.wait();
    }
}

#[test]
fn status_and_assignee_set_during_a_job_win_over_the_job_completed_rule() {
    let fixtures = TestFixtures::new();
    let root = fixtures.get_temp_path().to_path_buf();
    let queue = root.join("queue");
    let go = root.join("GO");
    std::fs::write(
        fixtures.tasks_root.join("config.yml"),
        format!(
            "default:\n  project: OUTC\nissue:\n  states: [Todo, InProgress, HelpNeeded, Done]\nagents:\n  fix:\n    runner: command\n    command: sh\n    args: ['-c', 'while [ ! -f \"{}\" ]; do sleep 0.1; done']\n",
            go.display()
        ),
    )
    .unwrap();
    std::fs::write(
        fixtures.tasks_root.join("automation.yml"),
        "automation:\n  rules:\n    - name: Finish\n      on:\n        job_completed:\n          set:\n            status: Done\n            assignee: robot\n          comment: \"Job ${{agent.job_id}} finished\"\n",
    )
    .unwrap();
    let cmd = || {
        let mut cmd = std::process::Command::new(assert_cmd::cargo::cargo_bin!("lotar"));
        cmd.current_dir(&root)
            .env_remove("RUST_TEST_THREADS")
            .env_remove("LOTAR_AGENT_JOB_ID")
            .env_remove("LOTAR_TICKET_ID")
            .env("LOTAR_IGNORE_HOME_CONFIG", "1")
            .env("LOTAR_AGENT_QUEUE_DIR", &queue)
            .env("LOTAR_TEST_SILENT", "1");
        cmd
    };
    assert!(cmd().args(["add", "needs a decision"]).status().unwrap().success());

    let runner = cmd()
        .args(["agent", "run", "OUTC-1", "go", "--agent", "fix", "--wait"])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .unwrap();
    let mut job = Finish { go: go.clone(), runner };

    let deadline = Instant::now() + Duration::from_secs(15);
    let job_id = loop {
        let out = cmd().args(["--format", "json", "agent", "list-running"]).output().unwrap();
        let json: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap_or(serde_json::Value::Null);
        if let Some(id) = json["jobs"].as_array().and_then(|jobs| jobs.iter().find(|job| job["ticket_id"] == "OUTC-1")).and_then(|job| job["job_id"].as_str()) {
            break id.to_string();
        }
        assert!(Instant::now() < deadline, "job never showed up: {json}");
        std::thread::sleep(Duration::from_millis(200));
    };
    let show = |field: &str| {
        let out = cmd().args(["--format", "json", "list", "OUTC-1"]).output().unwrap();
        let json: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
        json["tasks"][0][field].clone()
    };
    let status_before = show("status");

    // The agent hands the ticket back: both commands succeed and say when.
    let status = cmd().args(["status", "OUTC-1", "HelpNeeded"]).output().unwrap();
    assert!(status.status.success(), "{}", String::from_utf8_lossy(&status.stderr));
    assert!(
        String::from_utf8_lossy(&status.stdout).contains(&format!("HelpNeeded will be set when job {job_id} ends")),
        "{}",
        String::from_utf8_lossy(&status.stdout)
    );
    let assignee = cmd().args(["assignee", "OUTC-1", "owner"]).output().unwrap();
    assert!(assignee.status.success(), "{}", String::from_utf8_lossy(&assignee.stderr));
    assert!(
        String::from_utf8_lossy(&assignee.stdout).contains(&format!("owner will be assigned when job {job_id} ends")),
        "{}",
        String::from_utf8_lossy(&assignee.stdout)
    );
    // Nothing races the running job: the ticket is unchanged until it ends.
    assert_eq!(show("status"), status_before);

    std::fs::write(&go, b"").unwrap();
    let deadline = Instant::now() + Duration::from_secs(20);
    while job.runner.try_wait().unwrap().is_none() {
        assert!(Instant::now() < deadline, "job never finished");
        std::thread::sleep(Duration::from_millis(100));
    }

    // The agent's word beats the rule's set; the rule's comment still ran.
    assert_eq!(show("status"), "HelpNeeded");
    assert_eq!(show("assignee"), "owner");
    let stored = std::fs::read_to_string(fixtures.tasks_root.join("OUTC/1.yml")).unwrap();
    assert!(stored.contains(&format!("Job {job_id} finished")), "{stored}");
}

/// DEV-360: the job's record exists before its runner starts. Other
/// processes see the runner's wrapper process at once; without the record
/// they refused the agent's status and assignee changes.
#[test]
fn the_job_record_exists_when_the_runner_starts() {
    use std::os::unix::fs::PermissionsExt;

    let fixtures = TestFixtures::new();
    let root = fixtures.get_temp_path().to_path_buf();
    let queue = root.join("queue");
    let seen = root.join("seen");
    std::fs::write(fixtures.tasks_root.join("config.yml"), "default:\n  project: OUTS\nagents:\n  fix:\n    runner: command\n    command: 'true'\n").unwrap();
    // A wrapper that only looks for the record, as soon as it runs.
    let wrapper = root.join("wrapper.sh");
    std::fs::write(&wrapper, format!("#!/bin/sh\nls \"{}\"/agent-queue/active-*/job-*.json > \"{}\" 2>&1\nexit 0\n", queue.display(), seen.display())).unwrap();
    std::fs::set_permissions(&wrapper, std::fs::Permissions::from_mode(0o755)).unwrap();
    let cmd = || {
        let mut cmd = std::process::Command::new(assert_cmd::cargo::cargo_bin!("lotar"));
        cmd.current_dir(&root)
            .env_remove("RUST_TEST_THREADS")
            .env_remove("LOTAR_AGENT_JOB_ID")
            .env_remove("LOTAR_TICKET_ID")
            .env("LOTAR_IGNORE_HOME_CONFIG", "1")
            .env("LOTAR_AGENT_QUEUE_DIR", &queue)
            .env("LOTAR_AGENT_WRAPPER", &wrapper)
            .env("LOTAR_TEST_SILENT", "1");
        cmd
    };
    assert!(cmd().args(["add", "hands back at once"]).status().unwrap().success());
    let run = cmd().args(["agent", "run", "OUTS-1", "go", "--agent", "fix", "--wait"]).output().unwrap();
    assert!(run.status.success(), "{}", String::from_utf8_lossy(&run.stderr));
    let seen = std::fs::read_to_string(&seen).unwrap();
    assert!(seen.trim_end().ends_with(".json") && !seen.contains("No such file"), "{seen}");
}

/// REST and MCP get the same treatment, and the job's end applies what they
/// recorded in place of the rule's `set`.
#[test]
fn rest_and_mcp_changes_during_a_job_wait_for_its_end() {
    use crate::common::env_mutex::EnvVarGuard;
    use lotar::api_server::{ApiServer, HttpRequest};
    use lotar::services::agent_active_jobs::{self, ActiveJob};
    use lotar::services::automation_service::{AutomationEvent, AutomationJobContext, AutomationService};
    use serde_json::{Value, json};

    let fixtures = TestFixtures::new();
    let tasks_dir = fixtures.tasks_root.clone();
    std::fs::write(tasks_dir.join("config.yml"), "default:\n  project: OUTR\nissue:\n  states: [Todo, InProgress, HelpNeeded, Done]\n").unwrap();
    std::fs::write(
        tasks_dir.join("automation.yml"),
        "automation:\n  rules:\n    - name: Finish\n      on:\n        job_completed:\n          set:\n            status: Done\n            tags: [finished]\n          comment: finished\n",
    )
    .unwrap();
    let _env = [
        EnvVarGuard::set("LOTAR_TASKS_DIR", tasks_dir.to_str().unwrap()),
        EnvVarGuard::set("LOTAR_IGNORE_HOME_CONFIG", "1"),
        EnvVarGuard::set("LOTAR_AGENT_QUEUE_DIR", fixtures.get_temp_path().join("queue").to_str().unwrap()),
    ];
    let mut storage = fixtures.create_storage();
    lotar::services::task_service::TaskService::create(
        &mut storage,
        lotar::api_types::TaskCreate { title: "Waits for the owner".into(), ..Default::default() },
    )
    .unwrap();
    // A job another (live) process runs for the ticket.
    agent_active_jobs::register(
        &tasks_dir,
        &ActiveJob {
            job_id: "job-elsewhere".into(),
            ticket_id: "OUTR-1".into(),
            runner: "t3".into(),
            agent: Some("fix".into()),
            owner_pid: 1,
            child_pid: 1,
            started_at: "2026-11-11T00:00:00Z".into(),
        },
    );

    let mut api = ApiServer::new();
    lotar::routes::initialize(&mut api);
    let response = api.handle_request(&HttpRequest {
        method: "POST".into(),
        path: "/api/tasks/status".into(),
        query: Default::default(),
        headers: Default::default(),
        body: serde_json::to_vec(&json!({"id": "OUTR-1", "status": "InProgress"})).unwrap(),
    });
    let body: Value = serde_json::from_slice(&response.body).unwrap();
    assert_eq!(response.status, 200, "{body}");
    assert_eq!(body["data"]["status"], json!("Todo"));
    assert_eq!(body["deferred"], json!({"job_id": "job-elsewhere", "status": "InProgress"}));

    let request = json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "tools/call",
        "params": {"name": "task_update", "arguments": {
            "id": "OUTR-1",
            "patch": {"status": "HelpNeeded", "assignee": "ravi", "description": "Needs the owner"}
        }}
    });
    let response: Value = serde_json::from_str(&lotar::mcp::server::handle_json_line(&serde_json::to_string(&request).unwrap())).unwrap();
    assert!(response.get("error").is_none(), "{response}");
    let task: Value = serde_json::from_str(response["result"]["content"][0]["text"].as_str().unwrap()).unwrap();
    // Other fields apply now; status and assignee wait (the later status wins).
    assert_eq!(task["description"], json!("Needs the owner"));
    assert_eq!(task["status"], json!("Todo"));
    assert_eq!(task["deferred"], json!({"job_id": "job-elsewhere", "status": "HelpNeeded", "assignee": "ravi"}));
    assert_eq!(task["message"], json!("HelpNeeded will be set when job job-elsewhere ends; ravi will be assigned when job job-elsewhere ends"));

    AutomationService::apply_job_event(
        &tasks_dir,
        "OUTR-1",
        AutomationEvent::JobCompleted,
        Some(AutomationJobContext {
            job_id: "job-elsewhere".into(),
            runner: "t3".into(),
            agent: Some("fix".into()),
            worktree_path: None,
            worktree_branch: None,
            usage: None,
        }),
    )
    .unwrap();
    let task = lotar::services::task_service::TaskService::get(&storage, "OUTR-1", None).unwrap();
    assert_eq!(task.status.as_str(), "HelpNeeded");
    assert_eq!(task.assignee.as_deref(), Some("ravi"));
    assert_eq!(task.tags, vec!["finished".to_string()], "the rest of the set ran");
    assert_eq!(task.comments.len(), 1, "the rule's comment ran");
    // The job's guard ended with it: changes apply directly again.
    assert!(agent_active_jobs::find(&tasks_dir, "job-elsewhere").is_none());
    assert!(agent_active_jobs::peek_outcome(&tasks_dir, "job-elsewhere").is_none());
}

/// DEV-340: setting the current status or assignee back during a job drops
/// what was recorded, so the `job_completed` rule's `set` applies again.
#[test]
fn setting_the_current_value_back_lets_the_job_completed_rule_decide() {
    use lotar::services::agent_active_jobs::{self, ActiveJob};
    use lotar::services::automation_service::{AutomationEvent, AutomationJobContext, AutomationService};

    let fixtures = TestFixtures::new();
    let root = fixtures.get_temp_path().to_path_buf();
    let tasks_dir = fixtures.tasks_root.clone();
    let queue = root.join("queue");
    std::fs::write(tasks_dir.join("config.yml"), "default:\n  project: OUTB\nissue:\n  states: [Todo, InProgress, HelpNeeded, Done]\n").unwrap();
    std::fs::write(
        tasks_dir.join("automation.yml"),
        "automation:\n  rules:\n    - name: Finish\n      on:\n        job_completed:\n          set:\n            status: Done\n            assignee: merger\n",
    )
    .unwrap();
    let cmd = || {
        let mut cmd = std::process::Command::new(assert_cmd::cargo::cargo_bin!("lotar"));
        cmd.current_dir(&root)
            .env_remove("RUST_TEST_THREADS")
            .env_remove("LOTAR_AGENT_JOB_ID")
            .env_remove("LOTAR_TICKET_ID")
            .env("LOTAR_IGNORE_HOME_CONFIG", "1")
            .env("LOTAR_AGENT_QUEUE_DIR", &queue)
            .env("LOTAR_TEST_SILENT", "1");
        cmd
    };
    assert!(cmd().args(["add", "fix it", "--assignee", "fixer"]).status().unwrap().success());
    // A job another (live) process runs for the ticket.
    let _queue_env = crate::common::env_mutex::EnvVarGuard::set("LOTAR_AGENT_QUEUE_DIR", queue.to_str().unwrap());
    agent_active_jobs::register(
        &tasks_dir,
        &ActiveJob {
            job_id: "job-back".into(),
            ticket_id: "OUTB-1".into(),
            runner: "t3".into(),
            agent: Some("fix".into()),
            owner_pid: 1,
            child_pid: 1,
            started_at: "2026-11-11T00:00:00Z".into(),
        },
    );
    let run = |args: &[&str]| {
        let out = cmd().args(args).output().unwrap();
        assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
        String::from_utf8_lossy(&out.stdout).to_string()
    };

    // A mistake, then the current values back: both recorded changes go.
    let out = run(&["assignee", "OUTB-1", "claude"]);
    assert!(out.contains("claude will be assigned when job job-back ends"), "{out}");
    assert!(run(&["status", "OUTB-1", "HelpNeeded"]).contains("HelpNeeded will be set when job job-back ends"));
    let out = run(&["assignee", "OUTB-1", "fixer"]);
    assert!(out.contains("OUTB-1: deferred assignee cleared; the job's rules decide when job job-back ends"), "{out}");
    let json: serde_json::Value = serde_json::from_str(&run(&["--format", "json", "status", "OUTB-1", "Todo"])).unwrap();
    assert_eq!(json["changed"], serde_json::json!(false));
    assert_eq!(json["deferred"], serde_json::json!({"job_id": "job-back", "cleared": ["status"]}));
    assert!(agent_active_jobs::peek_outcome(&tasks_dir, "job-back").is_none(), "nothing is left recorded");

    AutomationService::apply_job_event(
        &tasks_dir,
        "OUTB-1",
        AutomationEvent::JobCompleted,
        Some(AutomationJobContext {
            job_id: "job-back".into(),
            runner: "t3".into(),
            agent: Some("fix".into()),
            worktree_path: None,
            worktree_branch: None,
            usage: None,
        }),
    )
    .unwrap();
    let storage = fixtures.create_storage();
    let task = lotar::services::task_service::TaskService::get(&storage, "OUTB-1", None).unwrap();
    assert_eq!(task.status.as_str(), "Done");
    assert_eq!(task.assignee.as_deref(), Some("merger"));
}

/// `--clear` records a cleared assignee, also when the ticket has none.
#[test]
fn clearing_the_assignee_during_a_job_records_a_cleared_assignee() {
    use lotar::services::agent_active_jobs::{self, ActiveJob};

    let fixtures = TestFixtures::new();
    let root = fixtures.get_temp_path().to_path_buf();
    let tasks_dir = fixtures.tasks_root.clone();
    let queue = root.join("queue");
    std::fs::write(tasks_dir.join("config.yml"), "default:\n  project: OUTL\n").unwrap();
    let cmd = || {
        let mut cmd = std::process::Command::new(assert_cmd::cargo::cargo_bin!("lotar"));
        cmd.current_dir(&root)
            .env_remove("RUST_TEST_THREADS")
            .env_remove("LOTAR_AGENT_JOB_ID")
            .env_remove("LOTAR_TICKET_ID")
            .env("LOTAR_IGNORE_HOME_CONFIG", "1")
            .env("LOTAR_AGENT_QUEUE_DIR", &queue)
            .env("LOTAR_TEST_SILENT", "1");
        cmd
    };
    assert!(cmd().args(["add", "nobody's yet"]).status().unwrap().success());
    let _queue_env = crate::common::env_mutex::EnvVarGuard::set("LOTAR_AGENT_QUEUE_DIR", queue.to_str().unwrap());
    agent_active_jobs::register(
        &tasks_dir,
        &ActiveJob {
            job_id: "job-clear".into(),
            ticket_id: "OUTL-1".into(),
            runner: "t3".into(),
            agent: None,
            owner_pid: 1,
            child_pid: 1,
            started_at: "2026-11-11T00:00:00Z".into(),
        },
    );
    let out = cmd().args(["assignee", "OUTL-1", "--clear"]).output().unwrap();
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
    assert!(String::from_utf8_lossy(&out.stdout).contains("The assignee will be cleared when job job-clear ends"), "{}", String::from_utf8_lossy(&out.stdout));
    assert_eq!(agent_active_jobs::peek_outcome(&tasks_dir, "job-clear").and_then(|outcome| outcome.assignee).as_deref(), Some(""));
}

/// DEV-350: an update that saw the job before it ended and records its
/// outcome after the end took it applies the change directly instead.
#[test]
fn an_assignment_recorded_after_the_job_ended_applies_directly() {
    use lotar::services::agent_active_jobs::{self, ActiveJob};
    use lotar::services::task_service::TaskService;

    let fixtures = TestFixtures::new();
    let tasks_dir = fixtures.tasks_root.clone();
    std::fs::write(tasks_dir.join("config.yml"), "default:\n  project: OUTE\nissue:\n  states: [Todo, InProgress, HelpNeeded, Done]\n").unwrap();
    let _queue_env = crate::common::env_mutex::EnvVarGuard::set("LOTAR_AGENT_QUEUE_DIR", fixtures.get_temp_path().join("queue").to_str().unwrap());
    let mut storage = fixtures.create_storage();
    TaskService::create(&mut storage, lotar::api_types::TaskCreate { title: "Hand it back".into(), ..Default::default() }).unwrap();
    let job = ActiveJob {
        job_id: "job-ending".into(),
        ticket_id: "OUTE-1".into(),
        runner: "t3".into(),
        agent: None,
        owner_pid: 1,
        child_pid: 1,
        started_at: "2026-11-11T00:00:00Z".into(),
    };
    agent_active_jobs::register(&tasks_dir, &job);
    assert!(agent_active_jobs::end_and_take_outcome(&tasks_dir, "job-ending").is_none());
    // The update still sees the job, as one that checked just before its end.
    agent_active_jobs::register(&tasks_dir, &job);

    let patch = lotar::api_types::TaskUpdate { status: Some("HelpNeeded".into()), assignee: Some("owner".into()), ..Default::default() };
    let receipt = TaskService::update_with_receipt(&mut storage, "OUTE-1", patch, Default::default()).unwrap();
    assert!(receipt.deferred.is_none(), "{:?}", receipt.deferred);
    assert!(receipt.changed);
    assert_eq!(receipt.dto.status.as_str(), "HelpNeeded");
    assert_eq!(receipt.dto.assignee.as_deref(), Some("owner"));
    assert!(agent_active_jobs::peek_outcome(&tasks_dir, "job-ending").is_none(), "nothing is left for nobody to apply");
    agent_active_jobs::unregister(&tasks_dir, "job-ending");
}

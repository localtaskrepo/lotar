//! DEV-626: a T3 Code thread that runs again after its job ended (T3 Code resumed a turn a restart cut off) is taken
//! over as a job, whose end rules run once; a second running thread of the ticket gets a comment, and a thread that
//! waits for an answer is left alone. A shell script stands in for T3's CLI.
#![cfg(unix)]

use crate::common::TestFixtures;
use crate::common::env_mutex::EnvVarGuard;
use lotar::api_types::TaskCreate;
use lotar::services::agent_job_service::AgentJobService;
use lotar::services::task_service::TaskService;
use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;
use std::time::{Duration, Instant};

/// `t3 thread start` names the thread after `thread-id` and prints "Started."; `thread show <id>` prints
/// `show-<id>.json`; `thread list` prints `list.json`. Every call goes to `calls`.
const FAKE_T3: &str = r#"#!/bin/sh
dir=$(dirname "$0")
echo "$1 $2" >> "$dir/calls"
case "$1 $2" in
  "thread start") cat > /dev/null; echo "Thread $(cat "$dir/thread-id")" >&2; echo "Started." ;;
  "thread send") cat > /dev/null; echo "Sent." ;;
  "thread show") cat "$dir/show-$3.json" ;;
  "thread list") cat "$dir/list.json" ;;
esac
"#;

fn link(thread: &str) -> String {
    format!("https://app.t3.codes/threads/env/{thread}")
}

struct Harness {
    fixtures: TestFixtures,
    dir: PathBuf,
    rule_ran: PathBuf,
    _env: Vec<EnvVarGuard>,
}

/// Profiles `fixer` and `other` on t3; a `job_completed` rule for tickets assigned to `@fixer` appends to `rule-ran`.
fn harness() -> Harness {
    let fixtures = TestFixtures::new();
    let root = fixtures.get_temp_path().to_path_buf();
    let dir = root.join("t3");
    std::fs::create_dir_all(&dir).unwrap();
    let script = dir.join("t3");
    std::fs::write(&script, FAKE_T3).unwrap();
    std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).unwrap();
    let home = root.join("harnessmux");
    std::fs::create_dir_all(home.join("config")).unwrap();
    std::fs::write(home.join("config/agents.toml"), format!("[agents.t3]\nkind = \"t3\"\ncommand = [\"{}\"]\n", script.display())).unwrap();
    fixtures.create_config_in_dir(
        &fixtures.tasks_root,
        "default:\n  project: TEST\nissue:\n  states: [Todo, InProgress, HelpNeeded, Done]\nagents:\n  fixer:\n    runner: t3\n  other:\n    runner: t3\n",
    );
    let rule_ran = dir.join("rule-ran");
    std::fs::write(
        fixtures.tasks_root.join("automation.yml"),
        format!(
            "automation:\n  rules:\n    - name: Fixer done\n      when:\n        assignee: \"@fixer\"\n      on:\n        job_completed:\n          run: \"sh -c 'echo x >> {}'\"\n",
            rule_ran.display()
        ),
    )
    .unwrap();
    let guards = vec![
        EnvVarGuard::set("HARNESSMUX_HOME", home.to_string_lossy().as_ref()),
        EnvVarGuard::set("LOTAR_TASKS_DIR", fixtures.tasks_root.to_string_lossy().as_ref()),
        EnvVarGuard::set("LOTAR_TEST_SILENT", "1"),
        EnvVarGuard::set("LOTAR_TRUST_ALL", "1"),
    ];
    let create = TaskCreate { title: "Thread job".into(), project: Some("TEST".into()), reporter: Some("owner".into()), ..Default::default() };
    TaskService::create(&mut fixtures.create_storage(), create).unwrap();
    Harness { fixtures, dir, rule_ran, _env: guards }
}

impl Harness {
    /// How T3 Code shows the thread: its status, its active run, the questions it waits on and its last reply.
    fn show(&self, thread: &str, status: &str, active: Option<&str>, pending: u64, reply: &str) {
        let active = active.map_or("null".to_string(), |run| format!("\"{run}\""));
        let json = serde_json::json!({
            "thread": {"threadId": thread, "link": link(thread), "status": status, "activeRunId": serde_json::from_str::<serde_json::Value>(&active).unwrap(), "latestRunId": "r2", "pendingRequestCount": pending},
            "messages": [{"role": "assistant", "runId": "r2", "text": reply}],
        });
        std::fs::write(self.dir.join(format!("show-{thread}.json")), json.to_string()).unwrap();
    }

    /// The threads `t3 thread list` lists, with their status.
    fn list(&self, threads: &[(&str, &str)]) {
        let listed: Vec<_> = threads
            .iter()
            .map(|(thread, status)| serde_json::json!({"threadId": thread, "title": "TEST-1 Thread job", "link": link(thread), "status": status, "latestRunId": "r2"}))
            .collect();
        std::fs::write(self.dir.join("list.json"), serde_json::Value::Array(listed).to_string()).unwrap();
    }

    /// Run a job of `agent` to its end in its own thread `thread`.
    fn run_job(&self, agent: &str, thread: &str) {
        std::fs::write(self.dir.join("thread-id"), thread).unwrap();
        self.show(thread, "completed", None, 0, "Started.");
        let mut cmd = crate::common::lotar_cmd().unwrap();
        cmd.current_dir(self.fixtures.get_temp_path()).args(["agent", "run", "TEST-1", "Fix it", "--agent", agent, "--wait"]);
        cmd.assert().success();
    }

    fn calls(&self) -> String {
        std::fs::read_to_string(self.dir.join("calls")).unwrap_or_default()
    }

    fn task(&self) -> lotar::api_types::TaskDTO {
        TaskService::get(&self.fixtures.create_storage(), "TEST-1", None).unwrap()
    }

    fn comments_with(&self, text: &str) -> usize {
        self.task().comments.iter().filter(|comment| comment.text.contains(text)).count()
    }

    fn rule_runs(&self) -> usize {
        std::fs::read_to_string(&self.rule_ran).unwrap_or_default().lines().count()
    }
}

fn wait_for(what: &str, mut done: impl FnMut() -> bool) {
    let deadline = Instant::now() + Duration::from_secs(60);
    while !done() {
        assert!(Instant::now() < deadline, "timed out waiting for {what}");
        std::thread::sleep(Duration::from_millis(25));
    }
}

// A taken-over job starts as queued, so wait for an end status, not for "not running". Settled: its worker has also
// dropped the job's queue files, which the process scratch cleanup removes (DEV-648).
fn wait_for_end(job_id: &str) {
    wait_for("the job to end", || {
        AgentJobService::job_settled(job_id)
            && AgentJobService::get_job(job_id).is_some_and(|job| matches!(job.status.as_str(), "completed" | "failed" | "cancelled"))
    });
}

#[test]
fn a_thread_t3_code_runs_again_after_its_job_ended_is_taken_over_and_its_end_rules_run_once() {
    let harness = harness();
    harness.run_job("fixer", "t1");
    assert_eq!(harness.rule_runs(), 0, "the ticket wasn't assigned to @fixer while the job ran");
    let sends = harness.calls().matches("thread send").count();

    // T3 Code resumes the thread after a restart; no lotar job follows it.
    harness.list(&[("t1", "running")]);
    harness.show("t1", "running", Some("r2"), 0, "Working.");
    let tasks_dir = harness.fixtures.tasks_root.clone();
    AgentJobService::adopt_resumed_t3_threads(&tasks_dir);

    let task = harness.task();
    assert_eq!(task.assignee.as_deref(), Some("@fixer"), "the ticket goes back to the job's profile");
    assert_eq!(harness.comments_with("lotar takes it over as the job"), 1, "{:?}", task.comments);
    assert!(task.comments.iter().any(|comment| comment.text.contains(&link("t1"))), "the comment names the thread");
    let job = lotar::services::agent_active_jobs::for_ticket(&tasks_dir, "TEST-1").expect("the taken-over job is recorded for other processes");
    assert_eq!(AgentJobService::queue_stats().running, 1, "it counts as a running job");

    harness.show("t1", "completed", None, 0, "Finished.");
    harness.list(&[("t1", "completed")]);
    wait_for_end(&job.job_id);
    let ended = AgentJobService::get_job(&job.job_id).unwrap();
    assert_eq!(ended.status, "completed", "{ended:?}");
    wait_for("the job_completed rule", || harness.rule_runs() > 0);
    assert_eq!(harness.rule_runs(), 1, "the end rule ran once");
    assert_eq!(harness.calls().matches("thread send").count(), sends, "the job follows the thread's run, it sends no turn: {}", harness.calls());
    assert_eq!(harness.calls().matches("thread start").count(), 1, "{}", harness.calls());
}

#[test]
fn a_second_running_thread_of_the_ticket_gets_a_comment_and_only_the_newest_is_taken_over() {
    let harness = harness();
    harness.run_job("other", "t1");
    harness.run_job("fixer", "t2");

    harness.list(&[("t1", "running"), ("t2", "running")]);
    harness.show("t1", "running", Some("r2"), 0, "Working.");
    harness.show("t2", "running", Some("r2"), 0, "Working.");
    let tasks_dir = harness.fixtures.tasks_root.clone();
    AgentJobService::adopt_resumed_t3_threads(&tasks_dir);

    let task = harness.task();
    assert_eq!(task.assignee.as_deref(), Some("@fixer"), "the newest job's thread is taken over");
    let taken = task.comments.iter().find(|comment| comment.text.contains("lotar takes it over")).expect("a takeover comment");
    assert!(taken.text.contains(&link("t2")), "{}", taken.text);
    let beside = task.comments.iter().find(|comment| comment.text.contains("stop it in T3 Code")).expect("a comment on the second thread");
    assert!(beside.text.contains(&link("t1")) && beside.text.contains(&link("t2")), "{}", beside.text);
    assert_eq!(harness.comments_with("stop it in T3 Code"), 1);

    let job = lotar::services::agent_active_jobs::for_ticket(&tasks_dir, "TEST-1").expect("one job runs");
    harness.show("t2", "completed", None, 0, "Finished.");
    wait_for_end(&job.job_id);
}

#[test]
fn a_thread_that_waits_for_an_answer_is_left_alone() {
    let harness = harness();
    harness.run_job("fixer", "t1");
    let comments = harness.task().comments.len();

    harness.list(&[("t1", "running")]);
    harness.show("t1", "running", Some("r2"), 1, "Which one?");
    let tasks_dir = harness.fixtures.tasks_root.clone();
    AgentJobService::adopt_resumed_t3_threads(&tasks_dir);

    assert!(lotar::services::agent_active_jobs::for_ticket(&tasks_dir, "TEST-1").is_none(), "no job takes it over");
    assert_eq!(harness.task().comments.len(), comments, "{:?}", harness.task().comments);
    assert_eq!(harness.task().assignee, None);
}

#[test]
fn the_thread_of_a_cancelled_job_is_left_alone() {
    let harness = harness();
    harness.run_job("fixer", "t1");
    let logs = harness.fixtures.tasks_root.join("@agent-logs");
    for entry in std::fs::read_dir(&logs).unwrap().flatten() {
        let log = std::fs::read_to_string(entry.path()).unwrap();
        std::fs::write(entry.path(), log.replace("\"status\":\"completed\"", "\"status\":\"cancelled\"")).unwrap();
    }
    let comments = harness.task().comments.len();

    harness.list(&[("t1", "running")]);
    harness.show("t1", "running", Some("r2"), 0, "Working.");
    let tasks_dir = harness.fixtures.tasks_root.clone();
    AgentJobService::adopt_resumed_t3_threads(&tasks_dir);

    assert!(lotar::services::agent_active_jobs::for_ticket(&tasks_dir, "TEST-1").is_none(), "no job takes it over");
    assert_eq!(harness.task().comments.len(), comments);
    assert!(!harness.calls().contains("thread list"), "T3 Code isn't asked: {}", harness.calls());
}

/// DEV-639: a waiting thread holds the ticket (DEV-600), so lotar takes over no other thread of it.
#[test]
fn no_thread_is_taken_over_while_a_newer_thread_of_the_ticket_waits_for_an_answer() {
    let harness = harness();
    harness.run_job("other", "t1");
    harness.run_job("fixer", "t2");
    let assignee = harness.task().assignee;

    harness.list(&[("t1", "running"), ("t2", "running")]);
    harness.show("t1", "running", Some("r2"), 0, "Working.");
    harness.show("t2", "running", Some("r2"), 1, "Which one?");
    let tasks_dir = harness.fixtures.tasks_root.clone();
    AgentJobService::adopt_resumed_t3_threads(&tasks_dir);

    assert!(lotar::services::agent_active_jobs::for_ticket(&tasks_dir, "TEST-1").is_none(), "no job takes a thread over");
    assert_eq!(harness.comments_with("lotar takes it over"), 0, "{:?}", harness.task().comments);
    assert_eq!(harness.task().assignee, assignee);
    let beside = harness.task().comments.into_iter().find(|comment| comment.text.contains("stop it in T3 Code")).expect("a comment on the running thread");
    assert!(beside.text.contains(&link("t1")) && beside.text.contains(&link("t2")), "{}", beside.text);

    // Another process looks at the same runs through its own path to the tasks directory.
    let other_path = harness.fixtures.get_temp_path().join("tasks-link");
    std::os::unix::fs::symlink(&tasks_dir, &other_path).unwrap();
    AgentJobService::adopt_resumed_t3_threads(&other_path);
    assert_eq!(harness.comments_with("stop it in T3 Code"), 1, "one comment per thread run");
    assert!(lotar::services::agent_active_jobs::for_ticket(&tasks_dir, "TEST-1").is_none());
}

#[test]
fn a_thread_t3_code_lists_as_waiting_holds_the_ticket_too() {
    let harness = harness();
    harness.run_job("other", "t1");
    harness.run_job("fixer", "t2");

    harness.list(&[("t1", "waiting"), ("t2", "running")]);
    harness.show("t1", "waiting", Some("r2"), 0, "Which one?");
    harness.show("t2", "running", Some("r2"), 0, "Working.");
    let tasks_dir = harness.fixtures.tasks_root.clone();
    AgentJobService::adopt_resumed_t3_threads(&tasks_dir);

    assert!(lotar::services::agent_active_jobs::for_ticket(&tasks_dir, "TEST-1").is_none(), "no job takes a thread over");
    let beside = harness.task().comments.into_iter().find(|comment| comment.text.contains("stop it in T3 Code")).expect("a comment on the running thread");
    assert!(beside.text.contains(&link("t2")) && beside.text.contains(&link("t1")), "{}", beside.text);
}

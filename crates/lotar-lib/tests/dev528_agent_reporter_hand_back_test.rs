//! DEV-528: a failed job that hands its ticket back to the ticket's
//! reporter, when that reporter is an agent name (claude, codex), reached
//! no one and cancelled the retry the job_failed rule promised. When an
//! end rule assigns the ticket, that hand-back is dropped and the rule
//! decides; a hand-back to a person still wins (DEV-320).
#![cfg(unix)]

use crate::common;
use common::TestFixtures;
use lotar::api_types::JobOutcome;
use lotar::services::agent_active_jobs::{self, ActiveJob};
use lotar::services::automation_service::{AutomationEvent, AutomationJobContext, AutomationService};
use lotar::services::task_service::TaskService;

/// A ticket reported by `reporter` whose job failed after the agent set
/// HelpNeeded and assigned `hand_back_to`; returns (status, assignee, file).
fn fail_job_after_hand_back(reporter: &str, hand_back_to: &str) -> (String, Option<String>, String) {
    let fixtures = TestFixtures::new();
    let root = fixtures.get_temp_path().to_path_buf();
    let tasks_dir = fixtures.tasks_root.clone();
    let queue = root.join("queue");
    std::fs::write(tasks_dir.join("config.yml"), "default:\n  project: HAND\nissue:\n  states: [Todo, Merging, HelpNeeded, Done]\n").unwrap();
    std::fs::write(
        tasks_dir.join("automation.yml"),
        "automation:\n  rules:\n    - name: Retry\n      on:\n        job_failed:\n          set:\n            assignee: fixer\n          comment: \"the fix agent retries it\"\n",
    )
    .unwrap();
    let out = std::process::Command::new(common::lotar_bin())
        .current_dir(&root)
        .env_remove("RUST_TEST_THREADS")
        .env_remove("LOTAR_AGENT_JOB_ID")
        .env_remove("LOTAR_TICKET_ID")
        .env("LOTAR_IGNORE_HOME_CONFIG", "1")
        .env("LOTAR_AGENT_QUEUE_DIR", &queue)
        .env("LOTAR_TEST_SILENT", "1")
        .args(["add", "merge it", "--status", "Merging", "--assignee", "merger", "--reporter", reporter])
        .output()
        .unwrap();
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));

    let _queue_env = common::env_mutex::EnvVarGuard::set("LOTAR_AGENT_QUEUE_DIR", queue.to_str().unwrap());
    agent_active_jobs::register(
        &tasks_dir,
        &ActiveJob {
            job_id: "job-merge".into(),
            ticket_id: "HAND-1".into(),
            runner: "codex".into(),
            agent: Some("merge".into()),
            owner_pid: 1,
            child_pid: 1,
            started_at: "2026-11-13T19:39:38Z".into(),
            session_id: None,
            merge: false,
            assignee_at_start: None,
            in_setup: false,
            request: None,
        },
    );
    let outcome = JobOutcome { job_id: "job-merge".into(), status: Some("HelpNeeded".into()), assignee: Some(hand_back_to.into()), cleared: Vec::new() };
    assert!(agent_active_jobs::record_outcome(&tasks_dir, &outcome).unwrap().is_some());

    AutomationService::apply_job_event(
        &tasks_dir,
        "HAND-1",
        AutomationEvent::JobFailed,
        Some(AutomationJobContext {
            job_id: "job-merge".into(),
            runner: "codex".into(),
            agent: Some("merge".into()),
            worktree_path: None,
            worktree_branch: None,
            usage: None,
            assignee_at_start: None,
            error: None,
            provider: None,
            pause: None,
        }),
    )
    .unwrap();
    let task = TaskService::get(&fixtures.create_storage(), "HAND-1", None).unwrap();
    let stored = std::fs::read_to_string(tasks_dir.join("HAND/1.yml")).unwrap();
    (task.status.as_str().to_string(), task.assignee, stored)
}

#[test]
fn a_hand_back_to_an_agent_named_reporter_leaves_the_retry_to_the_rule() {
    let (status, assignee, stored) = fail_job_after_hand_back("claude", "claude");
    assert_eq!(assignee.as_deref(), Some("fixer"), "{stored}");
    assert_eq!(status, "Merging", "{stored}");
    assert!(stored.contains("the fix agent retries it"), "{stored}");
}

#[test]
fn a_hand_back_to_a_person_still_wins_over_the_rule() {
    let (status, assignee, _) = fail_job_after_hand_back("sam", "sam");
    assert_eq!(status, "HelpNeeded");
    assert_eq!(assignee.as_deref(), Some("sam"));
}

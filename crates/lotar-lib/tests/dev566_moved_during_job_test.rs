//! DEV-566: a fix agent that moved its ticket on during the job (with
//! `lotar automation run`, which applies a rule's `set` at once) had the
//! fix job's end rules move it again: Reviewing/@review became
//! Merging/@merge while the review still ran (LOCA-333). A job's end rules
//! don't advance a ticket that was reassigned while the job ran.
#![cfg(unix)]

use crate::common;
use common::TestFixtures;
use lotar::api_types::{TaskCreate, TaskUpdate};
use lotar::services::automation_service::{AutomationEvent, AutomationJobContext, AutomationService};
use lotar::services::task_service::{TaskService, TaskUpdateContext};

const RULES: &str = "automation:
  rules:
    - name: Fix done
      when:
        assignee: mallox
      on:
        job_completed:
          set:
            assignee: reviewer
            status: Reviewing
    - name: Review passed
      when:
        assignee: reviewer
      on:
        job_completed:
          set:
            assignee: merger
            status: Merging
";

/// The ticket after its job ended: it was assigned to mallox when the job
/// started, and to `moved_to` (Reviewing) during the job when given.
fn end_job(assignee_at_start: Option<&str>, moved_to: Option<&str>) -> (String, Option<String>) {
    let fixtures = TestFixtures::new();
    let tasks_dir = fixtures.tasks_root.clone();
    let queue = fixtures.get_temp_path().join("queue");
    let _queue_env = common::env_mutex::EnvVarGuard::set("LOTAR_AGENT_QUEUE_DIR", queue.to_str().unwrap());
    std::fs::write(tasks_dir.join("config.yml"), "default:\n  project: MOVE\nissue:\n  states: [HelpNeeded, Reviewing, Merging, Done]\n").unwrap();
    std::fs::write(tasks_dir.join("automation.yml"), RULES).unwrap();
    let mut storage = fixtures.create_storage();
    let created = TaskService::create(
        &mut storage,
        TaskCreate {
            title: "fix it".into(),
            project: Some("MOVE".into()),
            status: Some("HelpNeeded".into()),
            assignee: Some("mallox".into()),
            ..Default::default()
        },
    )
    .unwrap();
    if let Some(moved_to) = moved_to {
        let patch = TaskUpdate { status: Some("Reviewing".into()), assignee: Some(moved_to.into()), ..TaskUpdate::default() };
        TaskService::update_with_receipt(&mut storage, &created.id, patch, TaskUpdateContext::automation_disabled()).unwrap();
    }

    AutomationService::apply_job_event(
        &tasks_dir,
        &created.id,
        AutomationEvent::JobCompleted,
        Some(AutomationJobContext {
            job_id: "job-fix".into(),
            runner: "t3".into(),
            agent: Some("fix-fable".into()),
            worktree_path: None,
            worktree_branch: None,
            usage: None,
            assignee_at_start: assignee_at_start.map(str::to_string),
        }),
    )
    .unwrap();
    let task = TaskService::get(&fixtures.create_storage(), &created.id, None).unwrap();
    (task.status.as_str().to_string(), task.assignee)
}

#[test]
fn end_rules_leave_a_ticket_reassigned_during_the_job() {
    let (status, assignee) = end_job(Some("mallox"), Some("reviewer"));
    assert_eq!(status, "Reviewing");
    assert_eq!(assignee.as_deref(), Some("reviewer"));
}

#[test]
fn end_rules_move_a_ticket_the_job_left_alone() {
    let (status, assignee) = end_job(Some("mallox"), None);
    assert_eq!(status, "Reviewing");
    assert_eq!(assignee.as_deref(), Some("reviewer"));
}

#[test]
fn a_job_without_a_known_start_assignee_keeps_running_its_end_rules() {
    let (status, assignee) = end_job(None, Some("reviewer"));
    assert_eq!(status, "Merging");
    assert_eq!(assignee.as_deref(), Some("merger"));
}

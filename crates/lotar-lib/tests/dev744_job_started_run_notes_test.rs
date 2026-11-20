//! DEV-744: each `run` of a job's `job_started` rules leaves a line in the
//! job's log (rule, exit code, duration), also one that ignore_failure lets
//! pass. A run skipped while untrusted: `run_note_tests` in the automation
//! service (a job with a repository profile doesn't start untrusted).
#![cfg(unix)]

use crate::common;
use common::TestFixtures;
use lotar::api_types::{AgentJobCreateRequest, TaskCreate};
use lotar::services::agent_job_service::AgentJobService;
use lotar::services::task_service::TaskService;
use std::time::{Duration, Instant};

fn wait_idle() {
    let deadline = Instant::now() + Duration::from_secs(20);
    loop {
        let stats = AgentJobService::queue_stats();
        if stats.running == 0 && stats.queued == 0 {
            return;
        }
        assert!(Instant::now() < deadline, "the job never ended");
        std::thread::sleep(Duration::from_millis(25));
    }
}

fn job_log(fixtures: &TestFixtures, ticket: &str) -> String {
    let request = AgentJobCreateRequest { ticket_id: ticket.into(), prompt: "go".into(), agent: Some("fix".into()), ..Default::default() };
    let job = AgentJobService::start_job_with_tasks_dir(request, &fixtures.tasks_root).unwrap();
    wait_idle();
    AgentJobService::events_for(&job.id).into_iter().filter_map(|event| event.message).collect::<Vec<_>>().join("\n")
}

#[test]
fn job_started_runs_leave_a_line_with_exit_and_duration() {
    let fixtures = TestFixtures::new();
    fixtures.create_config_in_dir(
        &fixtures.tasks_root,
        "default:\n  project: JSR\nissue:\n  states: [Todo, Done]\nagents:\n  fix:\n    runner: command\n    command: 'true'\n",
    );
    std::fs::write(
        fixtures.tasks_root.join("automation.yml"),
        "automation:\n  rules:\n    - name: Set up\n      on:\n        job_started:\n          run: 'echo installed'\n    - name: Optional\n      on:\n        job_started:\n          run:\n            command: sh\n            args: ['-c', 'echo broke >&2; exit 3']\n            ignore_failure: true\n    - name: Broken\n      on:\n        job_started:\n          run: 'exit 5'\n",
    )
    .unwrap();
    let ticket =
        TaskService::create(&mut fixtures.create_storage(), TaskCreate { title: "Start notes".into(), project: Some("JSR".into()), ..Default::default() })
            .unwrap()
            .id;

    let log = job_log(&fixtures, &ticket);
    assert!(log.contains(&format!("Automation rule 'Set up' ran `echo installed` on {ticket}: exit 0 in ")), "{log}");
    assert!(log.contains("ms. Output:\ninstalled") || log.contains("s. Output:\ninstalled"), "{log}");
    assert!(log.contains(&format!("Automation rule 'Optional' ran `echo broke >&2; exit 3` on {ticket}: failed (exit 3) in ")), "{log}");
    assert!(log.contains(", ignored (ignore_failure). Output:\nbroke"), "{log}");
    // A run that fails without ignore_failure says so, and the job still starts.
    assert!(log.contains(&format!("Automation rule 'Broken' ran `exit 5` on {ticket} for ")), "{log}");
    assert!(log.contains(&format!("The start rules for {ticket} failed: ")), "{log}");
}

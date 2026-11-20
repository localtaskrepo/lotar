//! DEV-733: the queue worker an agent's lotar call starts drops that agent's
//! session variables, so later jobs' agents don't act as the agent's session.
#![cfg(unix)]

use crate::common;
use common::TestFixtures;
use lotar::services::automation_service::AutomationService;
use std::process::Command;
use std::time::{Duration, Instant};

#[test]
fn a_job_run_by_a_worker_an_agent_started_has_no_session_variables_of_that_agent() {
    let fixtures = TestFixtures::new();
    let root = fixtures.get_temp_path().to_path_buf();
    let env_file = root.join("job.env");
    std::fs::write(
        fixtures.tasks_root.join("config.yml"),
        format!(
            "default:\n  project: TEST\nagents:\n  dump:\n    runner: command\n    command: sh\n    args: ['-c', 'env > \"{f}.tmp\" && mv \"{f}.tmp\" \"{f}\"']\n",
            f = env_file.display(),
        ),
    )
    .unwrap();
    AutomationService::set(
        &fixtures.tasks_root,
        None,
        "automation:\n  rules:\n    - name: Dump new tickets\n      on:\n        created:\n          set:\n            assignee: \"@dump\"\n",
    )
    .expect("set automation");

    let out = Command::new(common::lotar_bin())
        .current_dir(&root)
        .env_remove("RUST_TEST_THREADS")
        .env_remove("LOTAR_AGENT_QUEUE_DISABLE_WORKER")
        .env("LOTAR_IGNORE_HOME_CONFIG", "1")
        .env("LOTAR_AGENT_QUEUE_DIR", root.join("queue"))
        .env("LOTAR_DEFAULT_REPORTER", "tester")
        .env("LOTAR_TEST_SILENT", "1")
        .env("T3CODE_THREAD_ID", "x")
        .env("CLAUDE_CODE_SESSION_ID", "y")
        .env("CLAUDECODE", "1")
        .args(["add", "dump the env"])
        .output()
        .unwrap();
    assert!(out.status.success(), "{}{}", String::from_utf8_lossy(&out.stdout), String::from_utf8_lossy(&out.stderr));

    let deadline = Instant::now() + Duration::from_secs(30);
    while !env_file.exists() {
        assert!(Instant::now() < deadline, "the job did not run");
        std::thread::sleep(Duration::from_millis(100));
    }
    let env = std::fs::read_to_string(&env_file).unwrap();
    for key in ["T3CODE_THREAD_ID=", "CLAUDE_CODE_SESSION_ID=", "CLAUDECODE="] {
        assert!(!env.lines().any(|line| line.starts_with(key)), "the job inherited {key}\n{env}");
    }
    assert!(env.lines().any(|line| line.starts_with("LOTAR_TASKS_DIR=")), "the job's env is not the worker's\n{env}");

    // Let the worker go idle before the fixture removes its folder.
    let deadline = Instant::now() + Duration::from_secs(30);
    while Instant::now() < deadline {
        let out = Command::new(common::lotar_bin())
            .current_dir(&root)
            .env("LOTAR_IGNORE_HOME_CONFIG", "1")
            .env("LOTAR_AGENT_QUEUE_DIR", root.join("queue"))
            .args(["--format", "json", "agent", "queue"])
            .output()
            .ok();
        let json: serde_json::Value = out.and_then(|out| serde_json::from_slice(&out.stdout).ok()).unwrap_or_default();
        if json["worker_state"] != "active" && json["running_jobs"] == 0 {
            break;
        }
        std::thread::sleep(Duration::from_millis(100));
    }
}

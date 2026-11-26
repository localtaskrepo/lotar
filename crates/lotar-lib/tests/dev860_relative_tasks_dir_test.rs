//! DEV-860: a command given a relative `--tasks-dir` (`.tasks`) queued the
//! agent job its answer started in a queue keyed by that relative path, which
//! no worker reads, and its `run` rules failed to start in the empty folder
//! `.tasks` names as its parent. STD-336 sat on @fix with no job.

use crate::common;
use common::TestFixtures;
use std::path::Path;
use std::process::Command;

fn lotar(cwd: &Path, queue: &Path) -> Command {
    let mut cmd = Command::new(crate::common::lotar_bin());
    cmd.current_dir(cwd)
        .env_remove("LOTAR_TASKS_DIR")
        .env("LOTAR_AGENT_QUEUE_DISABLE_WORKER", "1")
        .env("LOTAR_TRUST_ALL", "1")
        .env("LOTAR_IGNORE_HOME_CONFIG", "1")
        .env("LOTAR_AGENT_QUEUE_DIR", queue)
        .env("LOTAR_DEFAULT_REPORTER", "tester")
        .env("LOTAR_TEST_SILENT", "1");
    cmd
}

fn run_ok(cmd: &mut Command) -> String {
    let out = cmd.output().unwrap();
    let stderr = String::from_utf8_lossy(&out.stderr).to_string();
    assert!(out.status.success(), "{}{stderr}", String::from_utf8_lossy(&out.stdout));
    stderr
}

#[test]
fn an_answer_with_a_relative_tasks_dir_queues_the_job_where_the_worker_reads_it() {
    let fixtures = TestFixtures::new();
    let root = fixtures.get_temp_path().to_path_buf();
    let tasks = fixtures.tasks_root.clone();
    let queue = root.join("queue");
    std::fs::write(tasks.join("config.yml"), "default:\n  project: TEST\nagents:\n  fix:\n    runner: command\n    command: 'true'\n").unwrap();
    std::fs::write(tasks.join("automation.yml"), "automation:\n  rules:\n    - name: Note updates\n      on:\n        updated:\n          run: 'true'\n")
        .unwrap();
    let relative = tasks.file_name().unwrap().to_str().unwrap().to_string();
    let workspace = tasks.parent().unwrap();

    run_ok(lotar(workspace, &queue).args(["--tasks-dir", &relative, "add", "Retry me", "--assignee", "mallox"]));
    run_ok(lotar(workspace, &queue).args([
        "--tasks-dir",
        &relative,
        "comment",
        "TEST-1",
        "--question",
        "Retry?",
        "--option",
        "fix: Retry with @fix",
        "--option",
        "stop: Stop",
        "--set",
        "fix:assignee=@fix",
    ]));
    let stderr = run_ok(lotar(workspace, &queue).args(["--tasks-dir", &relative, "comment", "TEST-1", "--answer", "fix"]));
    assert!(!stderr.contains("failed on TEST-1"), "a run rule failed: {stderr}");

    // The worker reads the queue of the absolute tasks folder; the child's
    // working folder is the canonical one (/private/var on macOS).
    let absolute = std::fs::canonicalize(&tasks).unwrap();
    let out = lotar(&root, &queue).args(["--tasks-dir", absolute.to_str().unwrap(), "--format", "json", "agent", "queue"]).output().unwrap();
    let json: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    let pending: Vec<&str> = json["pending"].as_array().unwrap().iter().filter_map(|entry| entry["ticket_id"].as_str()).collect();
    assert_eq!(pending, ["TEST-1"], "{json}");
}

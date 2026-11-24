//! DEV-837: while free disk on the job worktrees' volume is under
//! `agent.disk_gate.min_free_gb`, the queue worker starts no new job in a
//! job worktree; the entry stays queued and `lotar agent queue` and
//! `lotar agent list-jobs` say why. Once free space is over the floor (here:
//! the floor goes down), the job starts. The cleanup of idle builds, the
//! server log's lines and merge jobs passing the gate are unit tests in
//! `services::disk_gate` and `services::agent_queue_service`.
#![cfg(unix)]

use crate::common;
use common::TestFixtures;
use std::path::Path;
use std::process::Command;
use std::time::{Duration, Instant};

fn lotar(root: &Path, free_gb: &str) -> Command {
    let mut cmd = Command::new(crate::common::lotar_bin());
    cmd.current_dir(root)
        .env_remove("RUST_TEST_THREADS")
        .env_remove("LOTAR_AGENT_QUEUE_DISABLE_WORKER")
        .env("LOTAR_IGNORE_HOME_CONFIG", "1")
        .env("LOTAR_AGENT_QUEUE_DIR", root.join("queue"))
        .env("LOTAR_MEMORY_READING", "normal")
        .env("LOTAR_DISK_FREE_GB", free_gb)
        .env("LOTAR_DEFAULT_REPORTER", "tester")
        .env("LOTAR_TEST_SILENT", "1");
    cmd
}

fn json(root: &Path, args: &[&str]) -> serde_json::Value {
    let out = lotar(root, "500").arg("--format").arg("json").args(args).output().unwrap();
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
    serde_json::from_slice(&out.stdout).unwrap()
}

#[test]
fn a_queued_worktree_job_waits_while_the_disk_is_under_the_floor() {
    let fixtures = TestFixtures::new();
    let root = fixtures.get_temp_path().to_path_buf();
    let marker = root.join("ran");
    let config = |floor: u32| {
        std::fs::write(
            fixtures.tasks_root.join("config.yml"),
            format!(
                "default:\n  project: TEST\nagent:\n  worktree:\n    enabled: true\n    dir: {}\n  disk_gate:\n    min_free_gb: {floor}\nagents:\n  quick:\n    runner: command\n    command: sh\n    args: ['-c', 'touch \"{}\"']\n",
                root.join("worktrees").display(),
                marker.display()
            ),
        )
        .unwrap();
    };
    config(60);
    assert!(lotar(&root, "500").args(["add", "first"]).status().unwrap().success());
    // The job's worktree needs a repository.
    std::fs::write(root.join(".gitignore"), "worktrees/\nqueue/\nran\n").unwrap();
    for args in [
        &["init", "-q", "-b", "main"][..],
        &["add", "."],
        &["-c", "user.name=t", "-c", "user.email=t@example.invalid", "-c", "commit.gpgsign=false", "commit", "-qm", "init"],
    ] {
        assert!(Command::new("git").args(args).current_dir(&root).env("GIT_CONFIG_NOSYSTEM", "1").status().unwrap().success(), "git {args:?}");
    }

    // The worker this starts reads 41.5 GB free.
    let run = lotar(&root, "41.5").args(["agent", "run", "TEST-1", "go", "--agent", "quick"]).output().unwrap();
    assert!(run.status.success(), "{}{}", String::from_utf8_lossy(&run.stdout), String::from_utf8_lossy(&run.stderr));

    let deadline = Instant::now() + Duration::from_secs(15);
    let queue = loop {
        let queue = json(&root, &["agent", "queue"]);
        if !queue["disk_hold"].is_null() {
            break queue;
        }
        assert!(Instant::now() < deadline, "no disk hold: {queue}");
        std::thread::sleep(Duration::from_millis(100));
    };
    let reason = queue["disk_hold"]["reason"].as_str().unwrap();
    assert_eq!(reason, "41.5 GB free on the job worktrees' disk, under the floor of 60 GB");
    assert!(queue["disk_hold"]["since"].as_str().is_some_and(|since| since.ends_with('Z')), "{queue}");
    assert!(queue["memory_hold"].is_null(), "{queue}");
    assert_eq!(queue["worker_state"], "active", "{queue}");
    assert!(queue["reason"].as_str().unwrap().starts_with(&format!("waiting for disk space: {reason} (held since ")), "{queue}");
    assert!(queue["pending"][0]["waits_for"].as_str().unwrap().starts_with("waiting for disk space: "), "{queue}");

    // Held, not tried: the entry keeps its attempts and the job never ran.
    std::thread::sleep(Duration::from_secs(1));
    let queue = json(&root, &["agent", "queue"]);
    assert_eq!(queue["pending"][0]["ticket_id"], "TEST-1", "{queue}");
    assert_eq!(queue["pending"][0]["attempts"], 0, "{queue}");
    assert!(!marker.exists());

    let jobs = json(&root, &["agent", "list-jobs"]);
    assert_eq!(jobs["disk_hold"]["reason"], reason, "{jobs}");
    let text = lotar(&root, "500").args(["agent", "list-jobs"]).output().unwrap();
    assert!(String::from_utf8_lossy(&text.stdout).contains("new jobs wait for disk space: 41.5 GB free"), "{}", String::from_utf8_lossy(&text.stdout));

    // Under the new floor of 40 GB the gate reads again at once and the job starts.
    config(40);
    let deadline = Instant::now() + Duration::from_secs(30);
    while !marker.exists() {
        assert!(Instant::now() < deadline, "the held job did not start: {}", json(&root, &["agent", "queue"]));
        std::thread::sleep(Duration::from_millis(100));
    }
    let deadline = Instant::now() + Duration::from_secs(30);
    loop {
        let queue = json(&root, &["agent", "queue"]);
        if queue["worker_state"] == "idle" {
            assert!(queue["disk_hold"].is_null(), "{queue}");
            break;
        }
        assert!(Instant::now() < deadline, "the worker did not exit: {queue}");
        std::thread::sleep(Duration::from_millis(100));
    }
}

//! DEV-623: while the machine is short of memory the queue worker starts no
//! new job; the entry stays queued and `lotar agent queue` and
//! `lotar agent list-jobs` say why. The settle time and the way back to
//! normal are unit tests in `services::memory_gate`.
#![cfg(unix)]

use crate::common;
use common::TestFixtures;
use std::path::Path;
use std::process::Command;
use std::time::{Duration, Instant};

fn lotar(root: &Path, reading: &str) -> Command {
    let mut cmd = Command::new(crate::common::lotar_bin());
    cmd.current_dir(root)
        .env_remove("RUST_TEST_THREADS")
        .env_remove("LOTAR_AGENT_QUEUE_DISABLE_WORKER")
        .env("LOTAR_IGNORE_HOME_CONFIG", "1")
        .env("LOTAR_AGENT_QUEUE_DIR", root.join("queue"))
        .env("LOTAR_MEMORY_READING", reading)
        .env("LOTAR_DEFAULT_REPORTER", "tester")
        .env("LOTAR_TEST_SILENT", "1");
    cmd
}

fn json(root: &Path, args: &[&str]) -> serde_json::Value {
    let out = lotar(root, "normal").arg("--format").arg("json").args(args).output().unwrap();
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
    serde_json::from_slice(&out.stdout).unwrap()
}

#[test]
fn a_queued_job_waits_while_memory_is_short() {
    let fixtures = TestFixtures::new();
    let root = fixtures.get_temp_path().to_path_buf();
    let marker = root.join("ran");
    std::fs::write(
        fixtures.tasks_root.join("config.yml"),
        format!(
            "default:\n  project: TEST\nagent:\n  memory_gate:\n    enabled: true\nagents:\n  quick:\n    runner: command\n    command: sh\n    args: ['-c', 'touch \"{}\"']\n",
            marker.display()
        ),
    )
    .unwrap();
    assert!(lotar(&root, "normal").args(["add", "first"]).status().unwrap().success());

    // The worker this starts reads swap over the limit and pressure warn.
    let run = lotar(&root, "pressure=warn,swap=9.4/10").args(["agent", "run", "TEST-1", "go", "--agent", "quick"]).output().unwrap();
    assert!(run.status.success(), "{}{}", String::from_utf8_lossy(&run.stdout), String::from_utf8_lossy(&run.stderr));

    let deadline = Instant::now() + Duration::from_secs(15);
    let queue = loop {
        let queue = json(&root, &["agent", "queue"]);
        if !queue["memory_hold"].is_null() {
            break queue;
        }
        assert!(Instant::now() < deadline, "no memory hold: {queue}");
        std::thread::sleep(Duration::from_millis(100));
    };
    let reason = queue["memory_hold"]["reason"].as_str().unwrap();
    assert_eq!(reason, "macOS reports memory pressure warn; swap 9.4 of 10.0 GB used (94%, limit 85%)");
    assert!(queue["memory_hold"]["since"].as_str().is_some_and(|since| since.ends_with('Z')), "{queue}");
    assert_eq!(queue["worker_state"], "active", "{queue}");
    assert!(queue["reason"].as_str().unwrap().starts_with(&format!("waiting for memory: {reason} (held since ")), "{queue}");

    // Held, not tried: the entry keeps its attempts and the job never ran.
    std::thread::sleep(Duration::from_secs(1));
    let queue = json(&root, &["agent", "queue"]);
    assert_eq!(queue["pending"][0]["ticket_id"], "TEST-1", "{queue}");
    assert_eq!(queue["pending"][0]["attempts"], 0, "{queue}");
    assert!(!marker.exists());

    let jobs = json(&root, &["agent", "list-jobs"]);
    assert_eq!(jobs["memory_hold"]["reason"], reason, "{jobs}");
    let text = lotar(&root, "normal").args(["agent", "list-jobs"]).output().unwrap();
    assert!(
        String::from_utf8_lossy(&text.stdout).contains("new jobs wait for memory: macOS reports memory pressure warn"),
        "{}",
        String::from_utf8_lossy(&text.stdout)
    );

    // Flushed, the held worker has nothing left and exits.
    assert_eq!(json(&root, &["agent", "queue", "flush"])["flushed"], 1);
    let deadline = Instant::now() + Duration::from_secs(15);
    while json(&root, &["agent", "queue"])["worker_state"] != "idle" {
        assert!(Instant::now() < deadline, "the worker did not exit");
        std::thread::sleep(Duration::from_millis(100));
    }
}

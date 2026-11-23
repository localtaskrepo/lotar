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
    let (_fixtures, root) = gated_workspace();
    let marker = root.join("ran");

    // The worker this starts reads swap over the limit and pressure warn.
    run_quick(&root, "pressure=warn,swap=9.4/10");

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

/// A workspace whose only agent touches `ran` in it, with the gate on.
fn gated_workspace() -> (TestFixtures, std::path::PathBuf) {
    let fixtures = TestFixtures::new();
    let root = fixtures.get_temp_path().to_path_buf();
    std::fs::write(
        fixtures.tasks_root.join("config.yml"),
        format!(
            "default:\n  project: TEST\nagent:\n  memory_gate:\n    enabled: true\nagents:\n  quick:\n    runner: command\n    command: sh\n    args: ['-c', 'touch \"{}\"']\n",
            root.join("ran").display()
        ),
    )
    .unwrap();
    assert!(lotar(&root, "normal").args(["add", "first"]).status().unwrap().success());
    (fixtures, root)
}

fn run_quick(root: &Path, reading: &str) {
    let run = lotar(root, reading).args(["agent", "run", "TEST-1", "go", "--agent", "quick"]).output().unwrap();
    assert!(run.status.success(), "{}{}", String::from_utf8_lossy(&run.stdout), String::from_utf8_lossy(&run.stderr));
}

/// DEV-819: macOS keeps pages in swap after a shortage. Full, flat swap with
/// normal pressure and free memory holds nothing: the job starts.
#[test]
fn full_swap_with_free_memory_starts_the_job() {
    let (_fixtures, root) = gated_workspace();
    run_quick(&root, "pressure=normal,swap=3.6/4,free=63");
    let deadline = Instant::now() + Duration::from_secs(30);
    while !root.join("ran").exists() {
        assert!(Instant::now() < deadline, "the job did not start: {}", json(&root, &["agent", "queue"]));
        std::thread::sleep(Duration::from_millis(100));
    }
    let queue = json(&root, &["agent", "queue"]);
    assert!(queue["memory_hold"].is_null(), "{queue}");
    // A reading that fails holds nothing either: the job must start on a good one.
    assert!(queue["memory_error"].is_null(), "{queue}");
}

/// DEV-819: the same full swap with little free memory still holds.
#[test]
fn full_swap_with_little_free_memory_holds_the_job() {
    let (_fixtures, root) = gated_workspace();
    run_quick(&root, "pressure=normal,swap=3.6/4,free=10");
    let deadline = Instant::now() + Duration::from_secs(15);
    let queue = loop {
        let queue = json(&root, &["agent", "queue"]);
        if !queue["memory_hold"].is_null() {
            break queue;
        }
        assert!(Instant::now() < deadline, "no memory hold: {queue}");
        std::thread::sleep(Duration::from_millis(100));
    };
    assert_eq!(queue["memory_hold"]["reason"], "swap 3.6 of 4.0 GB used (90%, limit 85%, 10% of memory free)", "{queue}");
    assert!(queue["memory_error"].is_null(), "{queue}");
    assert!(!root.join("ran").exists());

    assert_eq!(json(&root, &["agent", "queue", "flush"])["flushed"], 1);
    let deadline = Instant::now() + Duration::from_secs(15);
    while json(&root, &["agent", "queue"])["worker_state"] != "idle" {
        assert!(Instant::now() < deadline, "the worker did not exit");
        std::thread::sleep(Duration::from_millis(100));
    }
}

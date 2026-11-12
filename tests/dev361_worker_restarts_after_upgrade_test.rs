//! DEV-361: the background queue worker notices an upgrade (here the way
//! Homebrew does it: a symlink moves to a new versioned copy), takes no new
//! entries, and once its running job ends a new worker starts the next one
//! from the upgraded binary.
#![cfg(unix)]

use crate::common;
use common::TestFixtures;
use std::path::Path;
use std::process::Command;
use std::time::{Duration, Instant};

/// Install `<root>/<version>/lotar` and point `<root>/bin/lotar` at it.
fn install(root: &Path, version: &str) {
    let dir = root.join(version);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::copy(env!("CARGO_BIN_EXE_lotar"), dir.join("lotar")).unwrap();
    std::fs::create_dir_all(root.join("bin")).unwrap();
    let link = root.join("bin/lotar");
    let _ = std::fs::remove_file(&link);
    std::os::unix::fs::symlink(dir.join("lotar"), &link).unwrap();
}

/// Worker pids, one line per job: the agent records its parent process.
fn wait_for_lines(path: &Path, count: usize) -> Vec<String> {
    let deadline = Instant::now() + Duration::from_secs(30);
    loop {
        let lines: Vec<String> = std::fs::read_to_string(path).unwrap_or_default().lines().map(str::to_string).collect();
        if lines.len() >= count {
            return lines;
        }
        assert!(Instant::now() < deadline, "only {} of {count} jobs ran: {lines:?}", lines.len());
        std::thread::sleep(Duration::from_millis(100));
    }
}

#[test]
fn worker_restarts_from_the_upgraded_binary_between_jobs() {
    let fixtures = TestFixtures::new();
    let root = fixtures.get_temp_path().to_path_buf();
    let install_root = root.join("install");
    install(&install_root, "0.1.0");
    let pids = root.join("worker-pids");
    std::fs::write(
        fixtures.tasks_root.join("config.yml"),
        format!(
            "default:\n  project: TEST\nagents:\n  slow:\n    runner: command\n    command: sh\n    args: ['-c', 'echo $PPID >> \"{}\"; sleep 3']\n",
            pids.display()
        ),
    )
    .unwrap();
    let lotar = || {
        let mut cmd = Command::new(install_root.join("bin/lotar"));
        cmd.current_dir(&root)
            .env_remove("RUST_TEST_THREADS")
            .env("LOTAR_IGNORE_HOME_CONFIG", "1")
            .env("LOTAR_AGENT_QUEUE_DIR", root.join("queue"))
            .env("LOTAR_BINARY_WATCH_INTERVAL_MS", "100")
            .env("LOTAR_TEST_SILENT", "1");
        cmd
    };
    assert!(lotar().args(["add", "first"]).status().unwrap().success());
    assert!(lotar().args(["add", "second"]).status().unwrap().success());

    assert!(lotar().args(["agent", "run", "TEST-1", "go", "--agent", "slow"]).status().unwrap().success());
    let first = wait_for_lines(&pids, 1);

    // Upgrade while TEST-1 runs, then queue TEST-2 for the busy worker.
    install(&install_root, "0.2.0");
    std::thread::sleep(Duration::from_millis(800));
    assert!(lotar().args(["agent", "run", "TEST-2", "go", "--agent", "slow"]).status().unwrap().success());

    let both = wait_for_lines(&pids, 2);
    assert_ne!(first[0], both[1], "the worker started before the upgrade also ran the next job");
}

//! DEV-361, DEV-806: the background queue worker notices an upgrade (here
//! the way Homebrew does it: a symlink moves to a new versioned copy), takes
//! no new entries and hands the queue to a worker from the upgraded binary,
//! which starts the next job while the old worker's job still runs.
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
    std::fs::copy(crate::common::lotar_bin(), dir.join("lotar")).unwrap();
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

/// Wait until the workers exited: they write the queue file as they end, and
/// a write after the fixture is gone leaves its folder behind.
fn wait_for_exit(pids: &[String]) {
    let deadline = Instant::now() + Duration::from_secs(30);
    for pid in pids {
        while Command::new("kill").args(["-0", pid]).stderr(std::process::Stdio::null()).status().is_ok_and(|s| s.success()) {
            assert!(Instant::now() < deadline, "worker {pid} did not exit");
            std::thread::sleep(Duration::from_millis(100));
        }
    }
}

/// Ends the jobs when dropped, also when the test fails: else they wait for
/// a file in a removed folder forever.
struct Release(std::path::PathBuf);

impl Drop for Release {
    fn drop(&mut self) {
        let _ = std::fs::write(&self.0, "");
    }
}

#[test]
fn upgraded_worker_starts_queued_jobs_while_the_old_worker_drains() {
    let fixtures = TestFixtures::new();
    let root = fixtures.get_temp_path().to_path_buf();
    let install_root = root.join("install");
    install(&install_root, "0.1.0");
    let pids = root.join("worker-pids");
    // Each job runs until the test creates `release`.
    let release = root.join("release");
    let _release = Release(release.clone());
    std::fs::write(
        fixtures.tasks_root.join("config.yml"),
        format!(
            "default:\n  project: TEST\nagents:\n  slow:\n    runner: command\n    command: sh\n    args: ['-c', 'echo $PPID >> \"{}\"; while [ ! -f \"{}\" ] && {}; do sleep 0.05; done']\n",
            pids.display(),
            release.display(),
            crate::common::while_this_test_runs()
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

    // Upgrade while TEST-1 runs, then queue TEST-2: it starts before TEST-1
    // ends, on a worker other than the one TEST-1 runs on.
    install(&install_root, "0.2.0");
    std::thread::sleep(Duration::from_millis(800));
    assert!(lotar().args(["agent", "run", "TEST-2", "go", "--agent", "slow"]).status().unwrap().success());

    let both = wait_for_lines(&pids, 2);
    assert_ne!(first[0], both[1], "the worker started before the upgrade also ran the next job");

    std::fs::write(&release, "").unwrap();
    wait_for_exit(&both);
}

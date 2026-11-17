//! DEV-660: the queue worker a command starts outlives the command's
//! process group. launchd kills a job's process group when the job ends, so
//! a worker started by `lotar add` from a launch agent (repos.ts --report)
//! died with the entries it held, and the tickets kept their agent assignee
//! with no job.
#![cfg(unix)]

use crate::common;
use common::TestFixtures;
use lotar::services::automation_service::AutomationService;
use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, Instant};

struct Workspace {
    _fixtures: TestFixtures,
    root: PathBuf,
    started: PathBuf,
    release: PathBuf,
}

impl Drop for Workspace {
    /// Let every job end before the fixture goes, so no worker writes into a
    /// removed folder.
    fn drop(&mut self) {
        let _ = std::fs::write(&self.release, "");
        let deadline = Instant::now() + Duration::from_secs(30);
        while Instant::now() < deadline {
            // No asserts here: a panic while a failed test unwinds aborts.
            let out = lotar(&self.root).args(["--format", "json", "agent", "queue"]).output().ok();
            let json: serde_json::Value = out.and_then(|out| serde_json::from_slice(&out.stdout).ok()).unwrap_or_default();
            if json["worker_state"] != "active" && json["running_jobs"] == 0 {
                return;
            }
            std::thread::sleep(Duration::from_millis(100));
        }
    }
}

fn lotar(root: &Path) -> Command {
    let mut cmd = Command::new(crate::common::lotar_bin());
    cmd.current_dir(root)
        .env_remove("RUST_TEST_THREADS")
        .env_remove("LOTAR_AGENT_QUEUE_DISABLE_WORKER")
        .env("LOTAR_IGNORE_HOME_CONFIG", "1")
        .env("LOTAR_AGENT_QUEUE_DIR", root.join("queue"))
        .env("LOTAR_DEFAULT_REPORTER", "tester")
        .env("LOTAR_TEST_SILENT", "1");
    cmd
}

/// Jobs that have started: each `slow` job leaves a file in `started`.
fn started(ws: &Workspace) -> usize {
    std::fs::read_dir(&ws.started).map(|dir| dir.count()).unwrap_or(0)
}

fn wait_until(what: &str, limit: Duration, done: impl Fn() -> bool) {
    let deadline = Instant::now() + limit;
    while !done() {
        assert!(Instant::now() < deadline, "{what}");
        std::thread::sleep(Duration::from_millis(100));
    }
}

#[test]
fn a_created_rules_job_starts_after_the_launchers_process_group_is_killed() {
    let fixtures = TestFixtures::new();
    let root = fixtures.get_temp_path().to_path_buf();
    let started_dir = root.join("started");
    let release = root.join("release");
    std::fs::create_dir_all(&started_dir).unwrap();
    // One slot: the second ticket's job waits in the queue for the first.
    std::fs::write(
        fixtures.tasks_root.join("config.yml"),
        format!(
            "default:\n  project: TEST\nagent:\n  worktree:\n    max_parallel_jobs: 1\nagents:\n  slow:\n    runner: command\n    command: sh\n    args: ['-c', 'touch \"$(mktemp \"{s}/job.XXXXXX\")\"; while [ ! -f \"{r}\" ]; do sleep 0.05; done']\n",
            s = started_dir.display(),
            r = release.display(),
        ),
    )
    .unwrap();
    AutomationService::set(
        &fixtures.tasks_root,
        None,
        "automation:\n  rules:\n    - name: Fix new tickets\n      on:\n        created:\n          set:\n            assignee: \"@slow\"\n",
    )
    .expect("set automation");
    let ws = Workspace { _fixtures: fixtures, root, started: started_dir, release };

    // A launch agent's run: its commands share one process group.
    let mut leader = Command::new("sleep").arg("60").process_group(0).spawn().unwrap();
    let group = i32::try_from(leader.id()).unwrap();
    for title in ["first", "second"] {
        let out = lotar(&ws.root).args(["add", title]).process_group(group).output().unwrap();
        assert!(out.status.success(), "{}{}", String::from_utf8_lossy(&out.stdout), String::from_utf8_lossy(&out.stderr));
    }
    wait_until("the first ticket's job did not start", Duration::from_secs(30), || started(&ws) == 1);

    // The run ends: launchd kills what is left in its process group.
    // SAFETY: plain kill(2) on the process group this test created.
    unsafe {
        libc::kill(-group, libc::SIGKILL);
    }
    let _ = leader.wait();

    std::fs::write(&ws.release, "").unwrap();
    wait_until("the second ticket's job did not start after the launcher's process group was killed", Duration::from_secs(30), || started(&ws) == 2);
}

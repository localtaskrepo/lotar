//! DEV-630: the queue worker applies the global
//! `agent.worktree.max_parallel_jobs` to all projects' jobs together and a
//! project's own limit on top of it, counting the jobs in every process.
#![cfg(unix)]

use crate::common;
use common::TestFixtures;
use std::path::{Path, PathBuf};
use std::process::{Child, Command};
use std::time::{Duration, Instant};

struct Workspace {
    _fixtures: TestFixtures,
    root: PathBuf,
    started: PathBuf,
    release: PathBuf,
    waiting_run: Option<Child>,
}

impl Drop for Workspace {
    /// Let every job end before the fixture goes, so no worker writes into a
    /// removed folder.
    fn drop(&mut self) {
        let _ = std::fs::write(&self.release, "");
        if let Some(child) = self.waiting_run.as_mut() {
            let _ = child.wait();
        }
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

fn run_ok(root: &Path, args: &[&str]) {
    let out = lotar(root).args(args).output().unwrap();
    assert!(out.status.success(), "{args:?}: {}{}", String::from_utf8_lossy(&out.stdout), String::from_utf8_lossy(&out.stderr));
}

fn queue_json(root: &Path) -> serde_json::Value {
    let out = lotar(root).args(["--format", "json", "agent", "queue"]).output().unwrap();
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
    serde_json::from_slice(&out.stdout).unwrap()
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

/// A workspace whose `slow` agent runs until `release` exists, with the
/// global config extended by `global` and the tickets added.
fn setup(global: &str, tickets: &[(&str, &str)]) -> Workspace {
    let fixtures = TestFixtures::new();
    let root = fixtures.get_temp_path().to_path_buf();
    let started_dir = root.join("started");
    let release = root.join("release");
    std::fs::create_dir_all(&started_dir).unwrap();
    std::fs::write(
        fixtures.tasks_root.join("config.yml"),
        format!(
            "default:\n  project: {project}\n{global}agents:\n  slow:\n    runner: command\n    command: sh\n    args: ['-c', 'touch \"$(mktemp \"{s}/job.XXXXXX\")\"; while [ ! -f \"{r}\" ]; do sleep 0.05; done']\n",
            project = tickets[0].1,
            s = started_dir.display(),
            r = release.display(),
        ),
    )
    .unwrap();
    let ws = Workspace { _fixtures: fixtures, root: root.clone(), started: started_dir, release, waiting_run: None };
    for (title, project) in tickets {
        run_ok(&root, &["add", title, "-p", project]);
    }
    ws
}

/// Run `first` in an `agent run --wait` process, then queue `queued` and
/// start the worker.
fn run_elsewhere_then_queue(ws: &mut Workspace, first: &str, queued: &[&str]) {
    ws.waiting_run = Some(lotar(&ws.root).args(["agent", "run", first, "go", "--agent", "slow", "--wait"]).spawn().unwrap());
    wait_until("the job of `agent run --wait` did not start", Duration::from_secs(30), || started(ws) == 1);
    for ticket in queued {
        let out = lotar(&ws.root).env("LOTAR_AGENT_QUEUE_DISABLE_WORKER", "1").args(["agent", "run", ticket, "go", "--agent", "slow"]).output().unwrap();
        assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
    }
    run_ok(&ws.root, &["agent", "queue", "resume"]);
}

fn pending(json: &serde_json::Value) -> Vec<String> {
    json["pending"].as_array().unwrap().iter().map(|entry| entry["ticket_id"].as_str().unwrap().to_string()).collect()
}

#[test]
fn worker_keeps_a_project_at_its_limit_counting_jobs_of_other_processes() {
    // No global limit: only LIM sets one.
    let mut ws = setup("", &[("one", "LIM"), ("two", "LIM"), ("three", "FREE"), ("four", "FREE")]);
    run_ok(&ws.root, &["config", "set", "agent_worktree_max_parallel_jobs", "1", "-p", "LIM"]);

    // LIM's one slot goes to a job another process runs.
    run_elsewhere_then_queue(&mut ws, "LIM-1", &["LIM-2", "FREE-1", "FREE-2"]);

    // FREE has no limit: both its jobs start; LIM-2 waits for LIM-1.
    wait_until("FREE's jobs did not start", Duration::from_secs(30), || started(&ws) == 3);
    std::thread::sleep(Duration::from_secs(1));
    assert_eq!(started(&ws), 3, "LIM-2 started while LIM-1 used LIM's only slot");
    let json = queue_json(&ws.root);
    assert_eq!(pending(&json), ["LIM-2"], "{json}");
    assert!(json["reason"].as_str().is_some_and(|reason| reason.contains("LIM's agent.worktree.max_parallel_jobs 1")), "{json}");

    // Once LIM-1 ends, LIM-2 gets the slot.
    std::fs::write(&ws.release, "").unwrap();
    wait_until("LIM-2 did not start after LIM-1 ended", Duration::from_secs(30), || started(&ws) == 4);
}

#[test]
fn worker_keeps_all_projects_together_at_the_global_limit() {
    let mut ws = setup("agent:\n  worktree:\n    max_parallel_jobs: 2\n", &[("one", "GLA"), ("two", "GLA"), ("three", "GLB"), ("four", "GLB")]);

    // One of the two global slots goes to a job another process runs.
    run_elsewhere_then_queue(&mut ws, "GLA-1", &["GLB-1", "GLB-2", "GLA-2"]);

    // Only GLB-1 gets the other slot, though no project sets a limit.
    wait_until("GLB-1 did not start", Duration::from_secs(30), || started(&ws) == 2);
    std::thread::sleep(Duration::from_secs(1));
    assert_eq!(started(&ws), 2, "the worker ran more jobs than the global limit 2");
    let json = queue_json(&ws.root);
    assert_eq!(pending(&json), ["GLB-2", "GLA-2"], "{json}");
    assert!(json["reason"].as_str().is_some_and(|reason| reason.contains("2 of the global agent.worktree.max_parallel_jobs 2")), "{json}");

    std::fs::write(&ws.release, "").unwrap();
    wait_until("the queued jobs did not start after the first ones ended", Duration::from_secs(30), || started(&ws) == 4);
}

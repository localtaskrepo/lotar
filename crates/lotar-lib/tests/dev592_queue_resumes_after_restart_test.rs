//! DEV-592: a queue a killed worker left behind starts again without a new
//! enqueue: when `lotar serve` starts, or with `lotar agent queue resume`.
//! `lotar agent queue` tells a stopped worker from a running one.
#![cfg(unix)]

use crate::common;
use common::TestFixtures;
use std::io::{BufRead, BufReader};
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::mpsc;
use std::time::{Duration, Instant};

struct Workspace {
    _fixtures: TestFixtures,
    root: PathBuf,
    marker: PathBuf,
    release: PathBuf,
}

impl Drop for Workspace {
    /// A worker outlives the command that started it: let it finish before
    /// the fixture goes, so it writes nothing into a removed folder.
    fn drop(&mut self) {
        let _ = std::fs::write(&self.release, "");
        let _ = std::fs::set_permissions(self.root.join("queue/agent-queue"), std::fs::Permissions::from_mode(0o755));
        let deadline = Instant::now() + Duration::from_secs(30);
        while Instant::now() < deadline {
            // No asserts here: a panic while a failed test unwinds aborts.
            let out = lotar(self).args(["--format", "json", "agent", "queue"]).output().ok();
            let json: serde_json::Value = out.and_then(|out| serde_json::from_slice(&out.stdout).ok()).unwrap_or_default();
            if json["worker_state"] != "active" && json["running_jobs"] == 0 {
                return;
            }
            std::thread::sleep(Duration::from_millis(100));
        }
    }
}

/// `quick` creates `marker`; `slow` does too and then runs until `release`
/// exists. `extra_config` is appended to the project config.
fn workspace(extra_config: &str) -> Workspace {
    let fixtures = TestFixtures::new();
    let root = fixtures.get_temp_path().to_path_buf();
    let marker = root.join("ran");
    let release = root.join("release");
    std::fs::write(
        fixtures.tasks_root.join("config.yml"),
        format!(
            "default:\n  project: TEST\nagents:\n  quick:\n    runner: command\n    command: sh\n    args: ['-c', 'touch \"{m}\"']\n  slow:\n    runner: command\n    command: sh\n    args: ['-c', 'touch \"{m}\"; while [ ! -f \"{r}\" ]; do sleep 0.05; done']\n{extra_config}",
            m = marker.display(),
            r = release.display(),
        ),
    )
    .unwrap();
    let ws = Workspace { _fixtures: fixtures, root, marker, release };
    for title in ["first", "second"] {
        assert!(lotar(&ws).args(["add", title]).status().unwrap().success());
    }
    ws
}

/// Queue `ticket` for `agent` with no worker to start it.
fn queue_without_worker(ws: &Workspace, ticket: &str, agent: &str) {
    let run = lotar(ws).env("LOTAR_AGENT_QUEUE_DISABLE_WORKER", "1").args(["agent", "run", ticket, "go", "--agent", agent]).output().unwrap();
    assert!(run.status.success(), "{}{}", String::from_utf8_lossy(&run.stdout), String::from_utf8_lossy(&run.stderr));
}

/// One ticket queued for `quick`, with no worker.
fn queued_workspace() -> Workspace {
    let ws = workspace("");
    queue_without_worker(&ws, "TEST-1", "quick");
    ws
}

fn lotar(ws: &Workspace) -> Command {
    let mut cmd = Command::new(crate::common::lotar_bin());
    cmd.current_dir(&ws.root)
        .env_remove("RUST_TEST_THREADS")
        .env_remove("LOTAR_AGENT_QUEUE_DISABLE_WORKER")
        .env("LOTAR_IGNORE_HOME_CONFIG", "1")
        .env("LOTAR_AGENT_QUEUE_DIR", ws.root.join("queue"))
        .env("LOTAR_DEFAULT_REPORTER", "tester")
        .env("LOTAR_TEST_SILENT", "1");
    cmd
}

fn queue_json(ws: &Workspace, args: &[&str]) -> serde_json::Value {
    let out = lotar(ws).args(["--format", "json", "agent", "queue"]).args(args).output().unwrap();
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
    serde_json::from_slice(&out.stdout).unwrap()
}

fn wait_for(path: &Path, limit: Duration) -> bool {
    let deadline = Instant::now() + limit;
    while Instant::now() < deadline {
        if path.exists() {
            return true;
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    false
}

fn wait_for_queue(ws: &Workspace, what: &str, done: impl Fn(&serde_json::Value) -> bool) -> serde_json::Value {
    let deadline = Instant::now() + Duration::from_secs(15);
    loop {
        let json = queue_json(ws, &[]);
        if done(&json) {
            return json;
        }
        assert!(Instant::now() < deadline, "{what}: {json}");
        std::thread::sleep(Duration::from_millis(100));
    }
}

/// A running `lotar serve`, stopped on drop.
struct Server {
    child: Child,
    stderr: mpsc::Receiver<String>,
}

impl Drop for Server {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

/// Start `lotar serve` and wait until it listens. Warnings are not silenced.
fn serve(ws: &Workspace) -> Server {
    let mut child = lotar(ws).env_remove("LOTAR_TEST_SILENT").args(["serve", "--port", "0"]).stdout(Stdio::piped()).stderr(Stdio::piped()).spawn().unwrap();
    let stdout = child.stdout.take().unwrap();
    let stderr = child.stderr.take().unwrap();
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        for line in BufReader::new(stdout).lines().map_while(Result::ok) {
            if line.contains("URL:") {
                let _ = tx.send(());
            }
        }
    });
    let (err_tx, err_rx) = mpsc::channel();
    std::thread::spawn(move || {
        for line in BufReader::new(stderr).lines().map_while(Result::ok) {
            let _ = err_tx.send(line);
        }
    });
    let server = Server { child, stderr: err_rx };
    assert!(rx.recv_timeout(Duration::from_secs(60)).is_ok(), "lotar serve did not start");
    server
}

/// Make the worker's lock file impossible to create; the queue file stays
/// readable and writable.
fn block_worker_lock(ws: &Workspace) {
    std::fs::set_permissions(ws.root.join("queue/agent-queue"), std::fs::Permissions::from_mode(0o555)).unwrap();
}

#[test]
fn queue_reports_a_stopped_worker() {
    let ws = queued_workspace();
    let json = queue_json(&ws, &[]);
    assert_eq!(json["worker_state"], "stopped", "{json}");
    assert_eq!(json["worker_running"], false, "{json}");
    assert!(json["reason"].as_str().is_some_and(|reason| reason.contains("lotar agent queue resume")), "{json}");

    let out = lotar(&ws).args(["agent", "queue"]).output().unwrap();
    let text = String::from_utf8_lossy(&out.stdout);
    assert!(text.contains("Worker: stopped"), "{text}");
}

#[test]
fn serve_starts_the_worker_for_a_queue_left_behind() {
    let ws = queued_workspace();
    let _server = serve(&ws);
    assert!(wait_for(&ws.marker, Duration::from_secs(15)), "the queued job did not run after the server started");
    wait_for_queue(&ws, "the worker did not finish the queue", |json| json["worker_state"] == "idle" && json["pending"].as_array().is_some_and(Vec::is_empty));
}

#[test]
fn serve_over_an_empty_queue_starts_no_worker() {
    let ws = workspace("");
    let _server = serve(&ws);
    std::thread::sleep(Duration::from_secs(1));
    let json = queue_json(&ws, &[]);
    assert_eq!(json["worker_state"], "idle", "{json}");
    assert_eq!(json["worker_running"], false, "{json}");
}

#[test]
fn serve_warns_and_serves_when_the_worker_cannot_start() {
    let ws = queued_workspace();
    block_worker_lock(&ws);
    let mut server = serve(&ws);
    let deadline = Instant::now() + Duration::from_secs(15);
    let mut stderr = Vec::new();
    while !stderr.iter().any(|line: &String| line.contains("Cannot start the agent queue worker")) {
        assert!(Instant::now() < deadline, "no warning: {stderr:?}");
        if let Ok(line) = server.stderr.recv_timeout(Duration::from_millis(200)) {
            stderr.push(line);
        }
    }
    assert!(server.child.try_wait().unwrap().is_none(), "the server exited");
    assert!(!ws.marker.exists(), "a job ran without a worker");
}

#[test]
fn resume_starts_the_worker_without_a_new_job() {
    let ws = queued_workspace();
    let json = queue_json(&ws, &["resume"]);
    assert_eq!(json["pending"], 1, "{json}");
    assert_eq!(json["started"], true, "{json}");
    assert_eq!(json["worker_state"], "active", "{json}");
    assert!(wait_for(&ws.marker, Duration::from_secs(15)), "the queued job did not run after resume");

    // The worker exits once the queue is empty.
    wait_for_queue(&ws, "the worker did not finish", |json| json["worker_state"] == "idle");
    let json = queue_json(&ws, &["resume"]);
    assert_eq!(json["pending"], 0, "{json}");
    assert_eq!(json["started"], false, "{json}");
    assert_eq!(json["worker_state"], "idle", "{json}");
}

#[test]
fn resume_leaves_a_running_worker_alone_and_queue_says_what_entries_wait_for() {
    let ws = workspace("agent:\n  worktree:\n    max_parallel_jobs: 1\n");
    queue_without_worker(&ws, "TEST-1", "slow");
    assert_eq!(queue_json(&ws, &["resume"])["started"], true);
    assert!(wait_for(&ws.marker, Duration::from_secs(15)), "the slow job did not start");
    queue_without_worker(&ws, "TEST-2", "quick");

    let json = wait_for_queue(&ws, "the queue never showed the running job", |json| json["running_jobs"] == 1);
    assert_eq!(json["worker_state"], "active", "{json}");
    assert!(json["reason"].as_str().is_some_and(|reason| reason.contains("waiting for a free slot") && reason.contains("max_parallel_jobs 1")), "{json}");

    let json = queue_json(&ws, &["resume"]);
    assert_eq!(json["pending"], 1, "{json}");
    assert_eq!(json["started"], false, "{json}");
    assert_eq!(json["worker_state"], "active", "{json}");
    let out = lotar(&ws).args(["agent", "queue", "resume"]).output().unwrap();
    assert!(String::from_utf8_lossy(&out.stdout).contains("already runs"), "{}", String::from_utf8_lossy(&out.stdout));
}

#[test]
fn resume_reports_a_worker_that_cannot_start_without_failing() {
    let ws = queued_workspace();
    block_worker_lock(&ws);
    let out = lotar(&ws).args(["--format", "json", "agent", "queue", "resume"]).output().unwrap();
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
    let json: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(json["pending"], 1, "{json}");
    assert_eq!(json["started"], false, "{json}");
    assert_eq!(json["worker_state"], "stopped", "{json}");
    assert!(json["error"].as_str().is_some_and(|err| !err.is_empty()), "{json}");
}

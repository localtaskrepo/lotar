//! DEV-646: jobs that `lotar serve` starts (`AgentJobService::start_job`)
//! keep the queue worker's `agent.worktree.max_parallel_jobs` limits, counting
//! the jobs of every process, and start once a slot frees in another process.
#![cfg(unix)]

use crate::common;
use common::TestFixtures;
use lotar::api_types::AgentJobCreateRequest;
use lotar::services::agent_job_service::AgentJobService;
use std::path::{Path, PathBuf};
use std::process::{Child, Command};
use std::time::{Duration, Instant};

struct Workspace {
    fixtures: TestFixtures,
    root: PathBuf,
    started: PathBuf,
    release: PathBuf,
    elsewhere: Option<Child>,
}

impl Drop for Workspace {
    /// Let every job end before the fixture goes.
    fn drop(&mut self) {
        let _ = std::fs::write(&self.release, "");
        if let Some(child) = self.elsewhere.as_mut() {
            let _ = child.wait();
        }
        let deadline = Instant::now() + Duration::from_secs(30);
        while Instant::now() < deadline {
            let stats = AgentJobService::queue_stats();
            if stats.running == 0 && stats.queued == 0 {
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
        .env("LOTAR_AGENT_QUEUE_DISABLE_WORKER", "1")
        .env("LOTAR_DEFAULT_REPORTER", "tester")
        .env("LOTAR_TEST_SILENT", "1");
    cmd
}

fn run_ok(root: &Path, args: &[&str]) {
    let out = lotar(root).args(args).output().unwrap();
    assert!(out.status.success(), "{args:?}: {}{}", String::from_utf8_lossy(&out.stdout), String::from_utf8_lossy(&out.stderr));
}

/// Jobs that have started: each `slow` job leaves a file in `started`.
fn started(ws: &Workspace) -> usize {
    std::fs::read_dir(&ws.started).map(|dir| dir.count()).unwrap_or(0)
}

fn wait_until(what: &str, done: impl Fn() -> bool) {
    let deadline = Instant::now() + Duration::from_secs(30);
    while !done() {
        assert!(Instant::now() < deadline, "{what}");
        std::thread::sleep(Duration::from_millis(100));
    }
}

/// A workspace whose `slow` agent runs until `release` exists, with the
/// global config extended by `global` and the tickets added; `first` then
/// runs in an `agent run --wait` process.
fn setup(global: &str, tickets: &[(&str, &str)], first: &str) -> Workspace {
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
    for (title, project) in tickets {
        run_ok(&root, &["add", title, "-p", project]);
    }
    let mut ws = Workspace { fixtures, root, started: started_dir, release, elsewhere: None };
    ws.elsewhere = Some(lotar(&ws.root).args(["agent", "run", first, "go", "--agent", "slow", "--wait"]).spawn().unwrap());
    wait_until("the job of `agent run --wait` did not start", || started(&ws) == 1);
    ws
}

fn start(ws: &Workspace, ticket: &str) -> String {
    let request = AgentJobCreateRequest { ticket_id: ticket.into(), prompt: "go".into(), agent: Some("slow".into()), ..Default::default() };
    AgentJobService::start_job_with_tasks_dir(request, &ws.fixtures.tasks_root).unwrap().id
}

/// The job waits in this process's queue for the reason; it hasn't started.
fn assert_waits(ws: &Workspace, job: &str, reason: &str, running: usize) {
    std::thread::sleep(Duration::from_secs(1));
    assert_eq!(started(ws), running, "a job started past the limit");
    assert_eq!(AgentJobService::get_job(job).unwrap().status, "queued");
    let waiting = AgentJobService::waiting_reason(job).unwrap();
    assert!(waiting.contains(reason), "{waiting}");
}

#[test]
fn serve_keeps_a_project_at_its_limit_counting_jobs_of_other_processes() {
    let ws = setup("", &[("one", "LIM"), ("two", "LIM"), ("three", "FREE")], "LIM-1");
    run_ok(&ws.root, &["config", "set", "agent_worktree_max_parallel_jobs", "1", "-p", "LIM"]);

    // LIM's one slot is the other process's; FREE sets no limit.
    let waiting = start(&ws, "LIM-2");
    start(&ws, "FREE-1");
    wait_until("FREE-1 did not start", || started(&ws) >= 2);
    assert_waits(&ws, &waiting, "waiting for a free slot (1 of LIM's agent.worktree.max_parallel_jobs 1 running)", 2);

    // The other process's job ends: LIM-2 gets the slot.
    std::fs::write(&ws.release, "").unwrap();
    wait_until("LIM-2 did not start after LIM-1 ended", || started(&ws) == 3);
}

#[test]
fn serve_keeps_all_projects_together_at_the_global_limit() {
    let ws = setup("agent:\n  worktree:\n    max_parallel_jobs: 2\n", &[("one", "GLA"), ("two", "GLB"), ("three", "GLB")], "GLA-1");

    // One of the two global slots is the other process's.
    start(&ws, "GLB-1");
    wait_until("GLB-1 did not start", || started(&ws) >= 2);
    let waiting = start(&ws, "GLB-2");
    assert_waits(&ws, &waiting, "waiting for a free slot (2 of the global agent.worktree.max_parallel_jobs 2 running)", 2);

    std::fs::write(&ws.release, "").unwrap();
    wait_until("GLB-2 did not start after the first jobs ended", || started(&ws) == 3);
}

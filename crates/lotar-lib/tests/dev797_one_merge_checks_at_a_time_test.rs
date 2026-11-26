//! DEV-797: one merge runs its checks on the machine at a time, under the
//! machine's check lock (DEV-784). Merges in one process already run one
//! after the other; these tests hold the lock the way another process's
//! merge would, on a port of their own, and the merge waits for it, says
//! whose checks it waits for, and runs once the lock is free. A merge agent
//! takes the lock too, or keeps the one lotar's merge handed off with.
#![cfg(unix)]

use crate::common::TestFixtures;
use crate::common::env_mutex::EnvVarGuard;
use lotar::api_types::{AgentJobCreateRequest, TaskCreate};
use lotar::services::agent_job_service::{AgentJobService, AgentOrchestratorMode};
use lotar::services::task_service::TaskService;
use std::fs;
use std::io::Write;
use std::net::{Ipv4Addr, TcpListener};
use std::path::Path;
use std::process::Command;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

fn git(dir: &Path, args: &[&str]) -> String {
    let output = Command::new("git").args(args).current_dir(dir).env_remove("GIT_DIR").env_remove("GIT_WORK_TREE").output().expect("git");
    assert!(output.status.success(), "git {args:?} in {}: {}", dir.display(), String::from_utf8_lossy(&output.stderr));
    String::from_utf8_lossy(&output.stdout).trim().to_string()
}

/// A repository whose ticket `<project>-1` has a fix on its branch. With
/// `checks`, lotar's merge runs them; the merge agent runs `agent`.
fn repo(project: &str, outside: &Path, checks: Option<&str>, agent: &str) -> TestFixtures {
    let fixtures = TestFixtures::new();
    let root = fixtures.get_temp_path().to_path_buf();
    git(&root, &["init", "-q"]);
    for (key, value) in [("user.name", "Agent Tests"), ("user.email", "agent-tests@example.com"), ("commit.gpgsign", "false")] {
        git(&root, &["config", key, value]);
    }
    fs::write(root.join("README.md"), "test repo\n").expect("write");
    git(&root, &["add", "README.md"]);
    git(&root, &["commit", "-qm", "Initial commit"]);
    let merge = checks.map(|checks| format!("  merge:\n    checks: [{checks}]\n")).unwrap_or_default();
    fs::write(
        fixtures.tasks_root.join("config.yml"),
        format!(
            "default:\n  project: {project}\nissue:\n  states: [Todo, Merging, Done]\n  priorities: [Low]\n  types: [Feature]\nagent:\n  worktree:\n    enabled: true\n    dir: '{}'\n{merge}agents:\n  merge:\n    runner: command\n    command: '{agent}'\n",
            outside.join(format!("worktrees-{project}")).display(),
        ),
    )
    .expect("write config");
    let mut storage = fixtures.create_storage();
    let task = TaskService::create(
        &mut storage,
        TaskCreate { title: "Merge me".to_string(), project: Some(project.to_string()), reporter: Some("Agent Tests".to_string()), ..Default::default() },
    )
    .expect("create task");
    let branch = format!("agent/{}", task.id);
    let fix = outside.join(format!("fix-{project}"));
    git(&root, &["worktree", "add", "-q", "-b", &branch, &fix.to_string_lossy()]);
    fs::write(fix.join("feature.txt"), "feature\n").expect("write fix");
    git(&fix, &["add", "feature.txt"]);
    git(&fix, &["commit", "-qm", &format!("{} Merge me", task.id)]);
    fixtures
}

fn start_merge(fixtures: &TestFixtures, ticket: &str) -> String {
    AgentJobService::start_job_with_tasks_dir(
        AgentJobCreateRequest { ticket_id: ticket.to_string(), prompt: "Merge the ticket".to_string(), agent: Some("merge".to_string()), ..Default::default() },
        &fixtures.tasks_root,
    )
    .expect("start merge job")
    .id
}

fn progress(job: &str) -> Vec<String> {
    AgentJobService::events_for(job).into_iter().filter(|event| event.kind == "agent_job_progress").filter_map(|event| event.message).collect()
}

/// The job's status once it ended, and its progress lines.
fn wait_for_end(job: &str) -> (String, Vec<String>) {
    let deadline = Instant::now() + Duration::from_secs(60);
    loop {
        let status = AgentJobService::get_job(job).map(|job| job.status).unwrap_or_default();
        if matches!(status.as_str(), "completed" | "failed" | "cancelled") {
            return (status, progress(job));
        }
        assert!(Instant::now() < deadline, "the merge job {job} didn't end: {status}");
        std::thread::sleep(Duration::from_millis(50));
    }
}

/// The test's own check lock: a free port, set for lotar, and no lock held
/// by the run that started the tests.
struct TestLock {
    port: u16,
    _env: (EnvVarGuard, EnvVarGuard),
}

fn test_lock() -> TestLock {
    let port = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).and_then(|listener| listener.local_addr()).map(|addr| addr.port()).expect("a free port");
    let env = (EnvVarGuard::set("LOTAR_CHECK_LOCK_PORT", &port.to_string()), EnvVarGuard::clear("LOTAR_CHECK_LOCK_HELD"));
    TestLock { port, _env: env }
}

/// Another merge's lock, as its process holds it: dropping it frees it.
struct OtherHolder {
    stop: Arc<AtomicBool>,
    server: Option<std::thread::JoinHandle<()>>,
}

impl Drop for OtherHolder {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(server) = self.server.take() {
            server.join().expect("holder");
        }
    }
}

fn hold_lock(port: u16, holder: &'static str) -> OtherHolder {
    let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, port)).expect("the lock is free");
    listener.set_nonblocking(true).expect("nonblocking");
    let stop = Arc::new(AtomicBool::new(false));
    let stopped = Arc::clone(&stop);
    let server = std::thread::spawn(move || {
        while !stopped.load(Ordering::Relaxed) {
            match listener.accept() {
                Ok((mut stream, _)) => {
                    let _ = stream.set_nonblocking(false);
                    let _ = writeln!(stream, "{holder}");
                }
                Err(_) => std::thread::sleep(Duration::from_millis(20)),
            }
        }
    });
    OtherHolder { stop, server: Some(server) }
}

fn is_held(port: u16) -> bool {
    TcpListener::bind((Ipv4Addr::LOCALHOST, port)).is_err()
}

/// The job's line that it waits for the lock, once it says it.
fn wait_for_the_lock(job: &str) -> String {
    let deadline = Instant::now() + Duration::from_secs(30);
    loop {
        if let Some(line) = progress(job).into_iter().find(|line| line.starts_with("Waiting for the machine's check lock")) {
            return line;
        }
        let status = AgentJobService::get_job(job).map(|job| job.status).unwrap_or_default();
        assert!(Instant::now() < deadline && !matches!(status.as_str(), "completed" | "failed" | "cancelled"), "the merge didn't wait: {status}");
        std::thread::sleep(Duration::from_millis(50));
    }
}

fn wait_for_file(job: &str, path: &Path) -> String {
    let deadline = Instant::now() + Duration::from_secs(30);
    while !path.exists() {
        assert!(Instant::now() < deadline, "{} wasn't written: {:?}", path.display(), AgentJobService::events_for(job));
        std::thread::sleep(Duration::from_millis(50));
    }
    std::thread::sleep(Duration::from_millis(100));
    fs::read_to_string(path).expect("read")
}

fn comments(fixtures: &TestFixtures) -> Vec<String> {
    TaskService::get(&fixtures.create_storage(), "ONE-1", None).expect("ticket").comments.into_iter().map(|comment| comment.text).collect()
}

/// A merge agent that writes to `log` whether it runs in the lock, holds
/// on a moment so the test sees the lock held, and fails.
fn recording_agent(outside: &Path, log: &Path) -> String {
    use std::os::unix::fs::PermissionsExt;
    let agent = outside.join("merge-agent.sh");
    fs::write(&agent, format!("#!/bin/sh\necho held=$LOTAR_CHECK_LOCK_HELD >> '{}'\nsleep 1\nexit 9\n", log.display())).expect("write agent");
    fs::set_permissions(&agent, fs::Permissions::from_mode(0o755)).expect("chmod agent");
    agent.display().to_string()
}

#[test]
fn lotars_merge_waits_for_another_merges_checks() {
    crate::common::require_git();
    AgentJobService::set_orchestrator_mode(AgentOrchestratorMode::Server);
    let lock = test_lock();
    let outside = tempfile::tempdir().expect("tempdir");
    let log = outside.path().join("ran.log");
    let fixtures = repo("ONE", outside.path(), Some(&format!("\"echo checks >> {}\"", log.display())), "exit 9");
    let other = hold_lock(lock.port, "OTHER-9's merge checks (pid 1), since 10:00 UTC");

    let job = start_merge(&fixtures, "ONE-1");
    let waiting = wait_for_the_lock(&job);
    assert!(waiting.ends_with("held by OTHER-9's merge checks (pid 1), since 10:00 UTC"), "it names the merge it waits for: {waiting}");
    std::thread::sleep(Duration::from_millis(1200));
    assert!(!log.exists(), "it ran its checks while the other merge held the lock");
    assert_eq!(AgentJobService::get_job(&job).map(|job| job.status).as_deref(), Some("running"), "a waiting merge runs");

    drop(other);
    let (status, progress) = wait_for_end(&job);
    assert_eq!(status, "completed", "{progress:?}");
    assert_eq!(fs::read_to_string(&log).expect("the checks ran"), "checks\n");
    assert!(
        comments(&fixtures).iter().any(|text| text.contains("for the machine's check lock, held by OTHER-9's merge checks")),
        "the merge's comment says whom it waited for"
    );
    assert!(!is_held(lock.port), "the merge freed the lock");
}

#[test]
fn a_merge_agent_waits_for_another_merges_checks_and_holds_the_lock_while_it_runs() {
    crate::common::require_git();
    AgentJobService::set_orchestrator_mode(AgentOrchestratorMode::Server);
    let lock = test_lock();
    let outside = tempfile::tempdir().expect("tempdir");
    let log = outside.path().join("agent.log");
    let fixtures = repo("ONE", outside.path(), None, &recording_agent(outside.path(), &log));
    let other = hold_lock(lock.port, "OTHER-9's merge checks (pid 1), since 10:00 UTC");

    let job = start_merge(&fixtures, "ONE-1");
    wait_for_the_lock(&job);
    std::thread::sleep(Duration::from_millis(1200));
    assert!(!log.exists(), "the merge agent started while the other merge held the lock");

    drop(other);
    assert_eq!(wait_for_file(&job, &log), "held=1\n", "the agent's checks are told the lock is held, so its scripts don't wait for it");
    assert!(is_held(lock.port), "the merge agent holds the lock while it runs");
    let (status, progress) = wait_for_end(&job);
    assert_eq!(status, "failed", "the merge agent ran (`exit 9`): {progress:?}");
    assert!(
        comments(&fixtures)
            .iter()
            .any(|text| text.contains("for the machine's check lock, held by OTHER-9's merge checks (pid 1), since 10:00 UTC; the merge agent starts now")),
        "the merge agent comments its wait"
    );
    let deadline = Instant::now() + Duration::from_secs(10);
    while is_held(lock.port) {
        assert!(Instant::now() < deadline, "the job didn't free the lock when it ended");
        std::thread::sleep(Duration::from_millis(50));
    }
}

/// A check that fails twice hands off with the lock held: the merge agent
/// reruns the checks in it, and no other merge's checks start in between.
#[test]
fn a_hand_off_from_the_checks_keeps_the_lock_for_the_merge_agent() {
    crate::common::require_git();
    AgentJobService::set_orchestrator_mode(AgentOrchestratorMode::Server);
    let lock = test_lock();
    let outside = tempfile::tempdir().expect("tempdir");
    let log = outside.path().join("agent.log");
    let fixtures = repo("ONE", outside.path(), Some("'exit 1'"), &recording_agent(outside.path(), &log));

    let job = start_merge(&fixtures, "ONE-1");
    assert_eq!(wait_for_file(&job, &log), "held=1\n");
    assert!(is_held(lock.port), "the merge agent holds the lock lotar's merge took");
    let (status, progress) = wait_for_end(&job);
    assert_eq!(status, "failed", "{progress:?}");
    assert!(comments(&fixtures).iter().any(|text| text.contains("failed, and again when rerun")), "lotar's merge handed off from its checks");
    assert!(!progress.iter().any(|line| line.starts_with("Waiting for the machine's check lock")), "nobody else held it: {progress:?}");
}

/// `lotar agent cancel` from another process writes a cancel request; the
/// waiting merge reads it, ends its wait and runs no checks.
#[test]
fn a_cancel_from_another_process_ends_the_wait() {
    crate::common::require_git();
    AgentJobService::set_orchestrator_mode(AgentOrchestratorMode::Server);
    let lock = test_lock();
    let outside = tempfile::tempdir().expect("tempdir");
    let log = outside.path().join("ran.log");
    let fixtures = repo("ONE", outside.path(), Some(&format!("\"echo checks >> {}\"", log.display())), "exit 9");
    let other = hold_lock(lock.port, "OTHER-8's merge checks (pid 1), since 10:00 UTC");

    let job = start_merge(&fixtures, "ONE-1");
    wait_for_the_lock(&job);
    assert!(lotar::services::agent_active_jobs::request_cancel(&fixtures.tasks_root, &job), "other processes see the waiting merge");
    let (status, progress) = wait_for_end(&job);
    assert_eq!(status, "cancelled", "{progress:?}");
    let deadline = Instant::now() + Duration::from_secs(10);
    while lotar::services::agent_active_jobs::find(&fixtures.tasks_root, &job).is_some() {
        assert!(Instant::now() < deadline, "the cancelled merge stays listed for other processes");
        std::thread::sleep(Duration::from_millis(50));
    }
    drop(other);
    std::thread::sleep(Duration::from_millis(1200));
    assert!(!log.exists(), "the cancelled merge ran its checks");
}

/// Guards DEV-890's default: the tests' merges never wait for the machine's
/// real check lock, also in a plain `cargo nextest run` that the
/// repository's runner didn't start, because another worktree's full suite
/// holds it for many minutes (`reset_lotar_test_environment` sets it).
#[test]
fn the_tests_never_wait_for_the_machines_check_lock() {
    assert_eq!(std::env::var("LOTAR_CHECK_LOCK_HELD").as_deref(), Ok("1"), "the test process starts as if it held the lock");
}

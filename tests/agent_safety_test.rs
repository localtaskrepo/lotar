#![cfg(unix)]
#![cfg_attr(no_git_tests, allow(dead_code))]

mod common;

use common::TestFixtures;
use lotar::api_types::{AgentJobCreateRequest, TaskCreate};
use lotar::services::agent_job_service::AgentJobService;
use lotar::services::task_service::TaskService;
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;
use std::process::{Command, Stdio};
use std::thread::sleep;
use std::time::{Duration, Instant};

fn git(root: &Path, args: &[&str]) {
    let output = Command::new("git")
        .args(args)
        .current_dir(root)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "git {args:?}: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

fn setup(body: &str, worktree: bool) -> (TestFixtures, String) {
    let fixture = TestFixtures::new();
    let root = fixture.get_temp_path();
    let script = root.join("runner.sh");
    fs::write(&script, format!("#!/bin/sh\n{body}\n")).unwrap();
    fs::set_permissions(&script, fs::Permissions::from_mode(0o755)).unwrap();
    fixture.create_config_in_dir(&fixture.tasks_root, &format!(
        "default:\n  project: SAFE\nissue:\n  states: [Todo, Done]\nagent:\n  worktree:\n    enabled: {worktree}\n    dir: '{}'\n    max_parallel_jobs: 1\n    cleanup_on_done: true\n    cleanup_delete_branches: true\nagents:\n  safety:\n    runner: claude\n    command: '{}'\n",
        root.join("worktrees").display(), script.display()
    ));
    let task = TaskService::create(
        &mut fixture.create_storage(),
        TaskCreate {
            title: "Safety regression".into(),
            project: Some("SAFE".into()),
            ..Default::default()
        },
    )
    .unwrap();
    if worktree {
        git(root, &["init", "-b", "main"]);
        git(root, &["config", "user.name", "Safety Tests"]);
        git(root, &["config", "user.email", "safety@example.invalid"]);
        git(root, &["config", "commit.gpgsign", "false"]);
        git(root, &["add", "."]);
        git(root, &["commit", "-m", "Test fixture"]);
    }
    (fixture, task.id)
}

fn request(ticket: &str) -> AgentJobCreateRequest {
    AgentJobCreateRequest {
        ticket_id: ticket.into(),
        prompt: "Run safety regression".into(),
        agent: Some("safety".into()),
        runner: None,
    }
}

fn wait_until(mut predicate: impl FnMut() -> bool) {
    let deadline = Instant::now() + Duration::from_secs(15);
    while !predicate() {
        assert!(Instant::now() < deadline, "timed out waiting for agent");
        sleep(Duration::from_millis(25));
    }
}

fn wait_idle() {
    wait_until(|| {
        let stats = AgentJobService::queue_stats();
        stats.running == 0 && stats.queued == 0
    });
}

fn worker_command(fixture: &TestFixtures, ticket: &str) -> Command {
    let queue_root = fixture.get_temp_path().join("queue");
    let queue_dir = queue_root.join("agent-queue");
    fs::create_dir_all(&queue_dir).unwrap();
    let hash = blake3::hash(fixture.tasks_root.to_string_lossy().as_bytes()).to_hex();
    fs::write(queue_dir.join(format!("queue-{hash}.json")), serde_json::to_vec(&serde_json::json!({
        "version": 1, "tasks_dir": fixture.tasks_root, "updated_at": "now",
        "pending": [{"ticket_id": ticket, "agent": "safety", "prompt": "run", "queued_at": "now", "attempts": 0}]
    })).unwrap()).unwrap();
    let mut worker = Command::new(env!("CARGO_BIN_EXE_lotar"));
    worker
        .args([
            "--tasks-dir",
            fixture.tasks_root.to_str().unwrap(),
            "agent",
            "worker",
        ])
        .env("LOTAR_IGNORE_HOME_CONFIG", "1")
        .env("LOTAR_AGENT_QUEUE_DIR", &queue_root)
        .current_dir(fixture.get_temp_path())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    worker
}

fn wait_worker(mut worker: std::process::Child) {
    let deadline = Instant::now() + Duration::from_secs(15);
    loop {
        if let Some(status) = worker.try_wait().unwrap() {
            assert!(status.success());
            break;
        }
        if Instant::now() >= deadline {
            worker.kill().unwrap();
            worker.wait().unwrap();
            panic!("worker did not exit after draining jobs");
        }
        sleep(Duration::from_millis(25));
    }
}

#[test]
fn worker_process_waits_for_runner_and_finalization() {
    let (fixture, ticket) = setup(
        "sleep 1\nprintf finished > \"$LOTAR_TASKS_DIR/../completed-marker\"",
        false,
    );
    wait_worker(worker_command(&fixture, &ticket).spawn().unwrap());
    assert_eq!(
        fs::read_to_string(fixture.get_temp_path().join("completed-marker")).unwrap(),
        "finished"
    );
    let config =
        lotar::config::resolution::config_for_project(&fixture.tasks_root, Some("SAFE")).unwrap();
    let context = lotar::services::agent_context_service::AgentContextService::load(
        &fixture.tasks_root,
        &config,
        &ticket,
    )
    .unwrap()
    .unwrap();
    assert!(
        context
            .messages
            .iter()
            .any(|message| message.role == "user" && message.content == "run"),
        "worker must await context persistence"
    );
}

#[test]
#[cfg_attr(no_git_tests, ignore = "Git repository creation unavailable")]
fn enabled_worktree_failure_never_executes_in_main() {
    if !common::git_available() {
        eprintln!("skipping: git unavailable in this sandbox");
        return;
    }
    let (fixture, ticket) = setup("touch executed-in-main", true);
    fs::write(fixture.get_temp_path().join("worktrees"), "not a directory").unwrap();
    let job =
        AgentJobService::start_job_with_tasks_dir(request(&ticket), &fixture.tasks_root).unwrap();
    wait_idle();
    let job = AgentJobService::get_job(&job.id).unwrap();
    assert_eq!(job.status, "failed");
    assert!(job.last_message.unwrap().contains("Worktree setup failed"));
    assert!(!fixture.get_temp_path().join("executed-in-main").exists());
}

#[test]
fn enabled_worktree_without_repository_fails_closed() {
    let (fixture, ticket) = setup("touch \"$LOTAR_TASKS_DIR/../executed-in-main\"", false);
    let path = fixture.tasks_root.join("config.yml");
    let mut config: serde_yaml_ng::Value =
        serde_yaml_ng::from_str(&fs::read_to_string(&path).unwrap()).unwrap();
    config["agent"]["worktree"]["enabled"] = true.into();
    fs::write(path, serde_yaml_ng::to_string(&config).unwrap()).unwrap();
    let job =
        AgentJobService::start_job_with_tasks_dir(request(&ticket), &fixture.tasks_root).unwrap();
    wait_idle();
    let job = AgentJobService::get_job(&job.id).unwrap();
    assert_eq!(job.status, "failed");
    assert!(job.last_message.unwrap().contains("Worktree setup failed"));
    assert!(!fixture.get_temp_path().join("executed-in-main").exists());
}

#[test]
fn terminal_cancellation_preserves_history_and_other_running_slot() {
    let (fixture, ticket) = setup(
        "if [ -f \"$LOTAR_TASKS_DIR/../block\" ]; then while [ ! -f \"$LOTAR_TASKS_DIR/../release\" ]; do sleep 0.05; done; fi",
        false,
    );
    let terminal =
        AgentJobService::start_job_with_tasks_dir(request(&ticket), &fixture.tasks_root).unwrap();
    wait_idle();
    let terminal = AgentJobService::get_job(&terminal.id).unwrap();
    assert_eq!(terminal.status, "completed");
    fs::write(fixture.get_temp_path().join("block"), "").unwrap();
    let running =
        AgentJobService::start_job_with_tasks_dir(request(&ticket), &fixture.tasks_root).unwrap();
    wait_until(|| AgentJobService::get_job(&running.id).unwrap().status == "running");
    let events = AgentJobService::events_for(&terminal.id).len();
    let cancelled = AgentJobService::cancel_job(&terminal.id).unwrap().unwrap();
    assert_eq!(
        serde_json::to_value(cancelled).unwrap(),
        serde_json::to_value(terminal.clone()).unwrap()
    );
    assert_eq!(AgentJobService::events_for(&terminal.id).len(), events);
    assert_eq!(AgentJobService::queue_stats().running, 1);
    assert!(AgentJobService::has_active_job(&ticket));
    let second = TaskService::create(
        &mut fixture.create_storage(),
        TaskCreate {
            title: "Queued cancellation".into(),
            project: Some("SAFE".into()),
            ..Default::default()
        },
    )
    .unwrap();
    let queued =
        AgentJobService::start_job_with_tasks_dir(request(&second.id), &fixture.tasks_root)
            .unwrap();
    assert_eq!(AgentJobService::queue_stats().queued, 1);
    AgentJobService::cancel_job(&queued.id).unwrap();
    assert_eq!(AgentJobService::queue_stats().queued, 0);
    assert_eq!(AgentJobService::queue_stats().running, 1);
    assert!(!AgentJobService::has_active_job(&second.id));
    AgentJobService::cancel_job(&running.id).unwrap();
    AgentJobService::cancel_job(&running.id).unwrap();
    wait_idle();
    assert_eq!(
        AgentJobService::get_job(&running.id).unwrap().status,
        "cancelled"
    );
}

#[test]
fn failed_terminal_cancellation_and_dispatch_races_preserve_slots() {
    let (fixture, ticket) = setup("exit 1", false);
    let job =
        AgentJobService::start_job_with_tasks_dir(request(&ticket), &fixture.tasks_root).unwrap();
    wait_idle();
    let failed = AgentJobService::get_job(&job.id).unwrap();
    assert_eq!(failed.status, "failed");
    assert_eq!(
        serde_json::to_value(AgentJobService::cancel_job(&job.id).unwrap().unwrap()).unwrap(),
        serde_json::to_value(failed).unwrap()
    );
    fs::write(
        fixture.get_temp_path().join("runner.sh"),
        "#!/bin/sh\nsleep 10\n",
    )
    .unwrap();
    for _ in 0..20 {
        let job = AgentJobService::start_job_with_tasks_dir(request(&ticket), &fixture.tasks_root)
            .unwrap();
        AgentJobService::cancel_job(&job.id).unwrap();
        wait_idle();
        assert_eq!(
            AgentJobService::get_job(&job.id).unwrap().status,
            "cancelled"
        );
        assert!(!AgentJobService::has_active_job(&ticket));
    }
}

fn cleanup_command(fixture: &TestFixtures) -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_lotar"));
    command
        .args([
            "--tasks-dir",
            fixture.tasks_root.to_str().unwrap(),
            "agent",
            "worktree",
            "cleanup",
            "--delete-branches",
        ])
        .env("LOTAR_IGNORE_HOME_CONFIG", "1")
        .current_dir(fixture.get_temp_path());
    command
}

fn cleanup(fixture: &TestFixtures) -> std::process::Output {
    cleanup_command(fixture).output().unwrap()
}

#[test]
#[cfg_attr(no_git_tests, ignore = "Git repository creation unavailable")]
fn cli_cleanup_preserves_indeterminate_dirty_and_unmerged_worktrees() {
    if !common::git_available() {
        eprintln!("skipping: git unavailable in this sandbox");
        return;
    }
    let (fixture, ticket) = setup("exit 0", true);
    let root = fixture.get_temp_path();
    let wt = root.join("worktrees").join(&ticket);
    let branch = format!("agent/{ticket}");
    git(
        root,
        &["worktree", "add", "-b", &branch, wt.to_str().unwrap()],
    );
    let task_path = fixture.tasks_root.join("SAFE/1.yml");
    let original = fs::read_to_string(&task_path).unwrap();
    fs::remove_file(&task_path).unwrap();
    assert!(cleanup(&fixture).status.success());
    assert!(wt.exists(), "missing task must not authorize removal");
    let all = Command::new(env!("CARGO_BIN_EXE_lotar"))
        .args([
            "--tasks-dir",
            fixture.tasks_root.to_str().unwrap(),
            "agent",
            "worktree",
            "cleanup",
            "--all",
            "--delete-branches",
        ])
        .env("LOTAR_IGNORE_HOME_CONFIG", "1")
        .current_dir(root)
        .output()
        .unwrap();
    assert!(all.status.success());
    assert!(
        wt.exists(),
        "--all must not authorize indeterminate cleanup"
    );
    for raw in [
        "broken: [",
        "title: malformed\nstatus: Done\ncreated: []\n",
        "title: unknown\nstatus: Unconfigured\ncreated: now\n",
    ] {
        fs::write(&task_path, raw).unwrap();
        assert!(cleanup(&fixture).status.success());
        assert!(wt.exists(), "indeterminate task must not authorize removal");
    }
    fs::remove_file(&task_path).unwrap();
    fs::create_dir(&task_path).unwrap();
    assert!(cleanup(&fixture).status.success());
    assert!(wt.exists(), "unreadable task must not authorize removal");
    fs::remove_dir(&task_path).unwrap();
    let mut task: serde_yaml_ng::Value = serde_yaml_ng::from_str(&original).unwrap();
    task["status"] = "Done".into();
    fs::write(&task_path, serde_yaml_ng::to_string(&task).unwrap()).unwrap();
    fs::write(wt.join("uncommitted"), "keep me").unwrap();
    assert!(!cleanup(&fixture).status.success());
    assert!(wt.join("uncommitted").exists());
    git(&wt, &["add", "uncommitted"]);
    git(&wt, &["commit", "-m", "Unmerged work"]);
    assert!(!cleanup(&fixture).status.success());
    assert!(wt.exists(), "unmerged worktree must be preserved");
    git(
        root,
        &["show-ref", "--verify", &format!("refs/heads/{branch}")],
    );
}

#[test]
#[cfg_attr(no_git_tests, ignore = "Git repository creation unavailable")]
fn automatic_cleanup_preserves_missing_malformed_dirty_and_unmerged_worktrees() {
    if !common::git_available() {
        eprintln!("skipping: git unavailable in this sandbox");
        return;
    }
    for body in [
        "rm \"$LOTAR_TASKS_DIR/SAFE/1.yml\"",
        "printf 'title: malformed\\nstatus: Done\\ncreated: []\\n' > \"$LOTAR_TASKS_DIR/SAFE/1.yml\"",
        "printf 'title: unknown\\nstatus: Unconfigured\\ncreated: now\\n' > \"$LOTAR_TASKS_DIR/SAFE/1.yml\"",
        "rm \"$LOTAR_TASKS_DIR/SAFE/1.yml\"; mkdir \"$LOTAR_TASKS_DIR/SAFE/1.yml\"",
        "touch uncommitted",
        "touch unmerged; git add unmerged; git commit -m 'Unmerged work'",
    ] {
        let (fixture, ticket) = setup(body, true);
        let path = fixture.tasks_root.join("SAFE/1.yml");
        let mut task: serde_yaml_ng::Value =
            serde_yaml_ng::from_str(&fs::read_to_string(&path).unwrap()).unwrap();
        task["status"] = "Done".into();
        fs::write(path, serde_yaml_ng::to_string(&task).unwrap()).unwrap();
        let job = AgentJobService::start_job_with_tasks_dir(request(&ticket), &fixture.tasks_root)
            .unwrap();
        wait_idle();
        let job = AgentJobService::get_job(&job.id).unwrap();
        assert_eq!(job.status, "completed", "{body}: {:?}", job.last_message);
        assert!(
            Path::new(job.worktree_path.as_deref().unwrap()).exists(),
            "{body}"
        );
        git(
            fixture.get_temp_path(),
            &[
                "show-ref",
                "--verify",
                &format!("refs/heads/agent/{ticket}"),
            ],
        );
    }
}

#[test]
#[cfg_attr(no_git_tests, ignore = "Git repository creation unavailable")]
fn cli_cleanup_accepts_clean_merged_worktree_with_project_done_status() {
    if !common::git_available() {
        eprintln!("skipping: git unavailable in this sandbox");
        return;
    }
    let (fixture, ticket) = setup("exit 0", true);
    let root = fixture.get_temp_path();
    let wt = root.join("worktrees").join(&ticket);
    let branch = format!("agent/{ticket}");
    git(
        root,
        &["worktree", "add", "-b", &branch, wt.to_str().unwrap()],
    );
    fixture.create_config_in_dir(
        &fixture.tasks_root.join("SAFE"),
        "project:\n  name: SAFE\nissue:\n  states: [Todo, Shipped]\n",
    );
    let path = fixture.tasks_root.join("SAFE/1.yml");
    let mut task: serde_yaml_ng::Value =
        serde_yaml_ng::from_str(&fs::read_to_string(&path).unwrap()).unwrap();
    task["status"] = "Shipped".into();
    fs::write(path, serde_yaml_ng::to_string(&task).unwrap()).unwrap();
    let output = cleanup(&fixture);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        !wt.exists(),
        "configured done state should allow clean merged cleanup"
    );
    assert!(
        !Command::new("git")
            .args(["show-ref", "--verify", &format!("refs/heads/{branch}")])
            .current_dir(root)
            .output()
            .unwrap()
            .status
            .success()
    );
}

#[test]
fn repository_env_sanitizer_removes_inherited_and_explicit_overrides_only() {
    let fixture = TestFixtures::new();
    let _dir = common::env_mutex::EnvVarGuard::set("GIT_DIR", "inherited-repository");
    let _config =
        common::env_mutex::EnvVarGuard::set("GIT_CONFIG_PARAMETERS", "'core.worktree=other'");
    let mut command = Command::new("sh");
    command.current_dir(fixture.get_temp_path()).args(["-c", r#"
        test -z "${GIT_DIR+x}${GIT_WORK_TREE+x}${GIT_COMMON_DIR+x}${GIT_INDEX_FILE+x}${GIT_CONFIG_PARAMETERS+x}${GIT_CONFIG_COUNT+x}${GIT_CONFIG_KEY_0+x}${GIT_CONFIG_VALUE_0+x}" &&
        test "$GIT_AUTHOR_NAME" = "Safety Author" && test "$GIT_COMMITTER_EMAIL" = "safety@example.invalid" &&
        test "$GIT_SSH_COMMAND" = "test-ssh" && test "$GIT_ASKPASS" = "test-askpass"
    "#]).envs([
        ("GIT_WORK_TREE", "profile-worktree"), ("GIT_COMMON_DIR", "profile-common"),
        ("GIT_INDEX_FILE", "profile-index"), ("GIT_CONFIG_COUNT", "1"),
        ("GIT_CONFIG_KEY_0", "core.worktree"), ("GIT_CONFIG_VALUE_0", "profile-worktree"),
        ("GIT_AUTHOR_NAME", "Safety Author"), ("GIT_COMMITTER_EMAIL", "safety@example.invalid"),
        ("GIT_SSH_COMMAND", "test-ssh"), ("GIT_ASKPASS", "test-askpass"),
    ]);
    lotar::utils::git::clear_repository_env(&mut command);
    assert!(command.status().unwrap().success());
}

#[test]
fn worker_persists_trailing_stderr_before_exit_and_summary() {
    let (fixture, ticket) = setup(
        "i=0; while [ $i -lt 6000 ]; do printf 'stderr-burst-%s\\n' \"$i\" >&2; i=$((i + 1)); done\nprintf 'unique-trailing-stderr-sentinel\\n' >&2",
        false,
    );
    let path = fixture.tasks_root.join("config.yml");
    let mut config: serde_yaml_ng::Value =
        serde_yaml_ng::from_str(&fs::read_to_string(&path).unwrap()).unwrap();
    config["agent"]["logs_dir"] = "agent-logs".into();
    fs::write(&path, serde_yaml_ng::to_string(&config).unwrap()).unwrap();
    wait_worker(worker_command(&fixture, &ticket).spawn().unwrap());
    let logs = lotar::services::agent_log_service::AgentLogService::list_logs(
        fixture.get_temp_path(),
        "agent-logs",
    )
    .unwrap();
    assert_eq!(logs.len(), 1);
    let events = lotar::services::agent_log_service::AgentLogService::load_events(
        fixture.get_temp_path(),
        "agent-logs",
        &logs[0],
    )
    .unwrap();
    let sentinel = events
        .iter()
        .position(|event| event.message.as_deref() == Some("unique-trailing-stderr-sentinel"))
        .expect("trailing stderr must be persisted");
    let completed = events
        .iter()
        .position(|event| event.kind == "agent_job_completed")
        .unwrap();
    assert!(sentinel < completed, "all output must precede completion");
    let status = lotar::services::agent_log_service::AgentLogService::load_status(
        fixture.get_temp_path(),
        "agent-logs",
        &logs[0],
    )
    .unwrap()
    .unwrap();
    assert_eq!(
        status.summary.as_deref(),
        Some("unique-trailing-stderr-sentinel")
    );
}

#[test]
fn inherited_output_streams_do_not_hold_job_open_after_parent_exit() {
    let (fixture, ticket) = setup(
        "sleep 20 &\nprintf '%s' \"$!\" > \"$LOTAR_TASKS_DIR/../descendant-pid\"\nprintf 'parent-finished\\n' >&2",
        false,
    );
    struct DescendantGuard(std::path::PathBuf);
    impl Drop for DescendantGuard {
        fn drop(&mut self) {
            if let Ok(raw) = fs::read_to_string(&self.0)
                && let Ok(pid) = raw.parse::<i32>()
            {
                // This PID is produced only by this test's isolated runner.
                unsafe {
                    libc::kill(pid, libc::SIGKILL);
                }
            }
        }
    }
    let _descendant = DescendantGuard(fixture.get_temp_path().join("descendant-pid"));
    let started = Instant::now();
    let job =
        AgentJobService::start_job_with_tasks_dir(request(&ticket), &fixture.tasks_root).unwrap();
    wait_idle();
    assert!(
        started.elapsed() < Duration::from_secs(10),
        "reader waited for surviving descendant EOF"
    );
    let job = AgentJobService::get_job(&job.id).unwrap();
    assert_eq!(job.status, "completed");
    assert_eq!(job.summary.as_deref(), Some("parent-finished"));
}

#[test]
fn dispatched_cancellation_hands_off_once_after_teardown() {
    use lotar::services::agent_job_service::AgentOrchestratorMode;
    AgentJobService::set_orchestrator_mode(AgentOrchestratorMode::Server);
    let (fixture, ticket) = setup(
        "printf '%s' \"$$\" > \"$LOTAR_TASKS_DIR/../old-pid\"\nprintf 'old-context\\n' >&2\nwhile :; do sleep 1; done",
        false,
    );
    let replacement = fixture.get_temp_path().join("replacement.sh");
    fs::write(&replacement, "#!/bin/sh\nif kill -0 \"$(cat \"$LOTAR_TASKS_DIR/../old-pid\")\" 2>/dev/null; then exit 9; fi\nprintf replacement >> \"$LOTAR_TASKS_DIR/../replacement-count\"\n").unwrap();
    fs::set_permissions(&replacement, fs::Permissions::from_mode(0o755)).unwrap();
    let path = fixture.tasks_root.join("config.yml");
    let mut config: serde_yaml_ng::Value =
        serde_yaml_ng::from_str(&fs::read_to_string(&path).unwrap()).unwrap();
    config["agents"]["replacement"] = config["agents"]["safety"].clone();
    config["agents"]["replacement"]["command"] = replacement.to_str().unwrap().into();
    fs::write(path, serde_yaml_ng::to_string(&config).unwrap()).unwrap();
    lotar::services::automation_service::AutomationService::set(&fixture.tasks_root, None,
        "automation:\n  rules:\n    - name: Cancellation handoff\n      when:\n        assignee: '@safety'\n      on:\n        job_cancelled:\n          set:\n            assignee: '@replacement'\n          comment: cancellation-handoff-once\n").unwrap();
    let mut storage = fixture.create_storage();
    TaskService::update(
        &mut storage,
        &ticket,
        lotar::api_types::TaskUpdate {
            assignee: Some("@safety".into()),
            ..Default::default()
        },
    )
    .unwrap();
    let old = AgentJobService::list_jobs()
        .into_iter()
        .find(|job| job.ticket_id == ticket)
        .unwrap();
    wait_until(|| fixture.get_temp_path().join("old-pid").exists());
    AgentJobService::cancel_job(&old.id).unwrap();
    AgentJobService::cancel_job(&old.id).unwrap();
    wait_idle();
    let jobs = AgentJobService::list_jobs();
    let replacements: Vec<_> = jobs
        .iter()
        .filter(|job| job.agent.as_deref() == Some("replacement"))
        .collect();
    assert_eq!(
        replacements.len(),
        1,
        "cancellation handoff must not be lost or duplicated"
    );
    assert_eq!(
        replacements[0].status, "completed",
        "replacement started before old process teardown"
    );
    assert_eq!(
        fs::read_to_string(fixture.get_temp_path().join("replacement-count")).unwrap(),
        "replacement"
    );
    assert_eq!(
        AgentJobService::events_for(&old.id)
            .iter()
            .filter(|event| event.kind == "agent_job_cancelled")
            .count(),
        1
    );
    let task = TaskService::get(&storage, &ticket, None).unwrap();
    assert_eq!(
        task.comments
            .iter()
            .filter(|comment| comment.text == "cancellation-handoff-once")
            .count(),
        1
    );
    let resolved =
        lotar::config::resolution::config_for_project(&fixture.tasks_root, Some("SAFE")).unwrap();
    let context = lotar::services::agent_context_service::AgentContextService::load(
        &fixture.tasks_root,
        &resolved,
        &ticket,
    )
    .unwrap()
    .unwrap();
    assert_eq!(
        context
            .messages
            .iter()
            .filter(|message| message.role == "user")
            .count(),
        2,
        "cancelled context must persist before replacement context"
    );
}

#[test]
#[cfg_attr(no_git_tests, ignore = "Git repository creation unavailable")]
fn inherited_and_profile_git_overrides_cannot_redirect_setup_or_runner() {
    if !common::git_available() {
        eprintln!("skipping: git unavailable in this sandbox");
        return;
    }
    let (fixture, ticket) = setup(
        "git rev-parse --show-toplevel > \"$LOTAR_TASKS_DIR/../runner-top\"\ngit rev-parse --path-format=absolute --git-common-dir > \"$LOTAR_TASKS_DIR/../runner-common\"",
        true,
    );
    let (other, _) = setup("exit 0", true);
    let other_root = other.get_temp_path();
    let overrides = [
        (
            "GIT_DIR",
            other_root.join(".git").to_string_lossy().to_string(),
        ),
        ("GIT_WORK_TREE", other_root.to_string_lossy().to_string()),
        (
            "GIT_COMMON_DIR",
            other_root.join(".git").to_string_lossy().to_string(),
        ),
        (
            "GIT_INDEX_FILE",
            other_root.join(".git/index").to_string_lossy().to_string(),
        ),
        ("GIT_CONFIG_COUNT", "1".into()),
        ("GIT_CONFIG_KEY_0", "core.worktree".into()),
        (
            "GIT_CONFIG_VALUE_0",
            other_root.to_string_lossy().to_string(),
        ),
    ];
    let path = fixture.tasks_root.join("config.yml");
    let mut config: serde_yaml_ng::Value =
        serde_yaml_ng::from_str(&fs::read_to_string(&path).unwrap()).unwrap();
    config["agents"]["safety"]["env"] = serde_yaml_ng::to_value(
        overrides
            .iter()
            .cloned()
            .collect::<std::collections::BTreeMap<_, _>>(),
    )
    .unwrap();
    fs::write(path, serde_yaml_ng::to_string(&config).unwrap()).unwrap();
    wait_worker(
        worker_command(&fixture, &ticket)
            .envs(overrides)
            .spawn()
            .unwrap(),
    );
    let top = fs::read_to_string(fixture.get_temp_path().join("runner-top")).unwrap();
    let common = fs::read_to_string(fixture.get_temp_path().join("runner-common")).unwrap();
    assert_eq!(
        fs::canonicalize(top.trim()).unwrap(),
        fs::canonicalize(fixture.get_temp_path().join("worktrees").join(&ticket)).unwrap()
    );
    assert_eq!(
        fs::canonicalize(common.trim()).unwrap(),
        fs::canonicalize(fixture.get_temp_path().join(".git")).unwrap()
    );
    assert!(
        !Command::new("git")
            .args([
                "show-ref",
                "--verify",
                &format!("refs/heads/agent/{ticket}")
            ])
            .current_dir(other_root)
            .output()
            .unwrap()
            .status
            .success()
    );
}

#[test]
fn cancelled_dispatch_waits_for_inflight_start_before_handoff() {
    AgentJobService::set_orchestrator_mode(
        lotar::services::agent_job_service::AgentOrchestratorMode::Server,
    );
    let (fixture, ticket) = setup("touch \"$LOTAR_TASKS_DIR/../old-executed\"", false);
    let replacement = fixture.get_temp_path().join("replacement.sh");
    fs::write(&replacement, "#!/bin/sh\nexit 0\n").unwrap();
    fs::set_permissions(&replacement, fs::Permissions::from_mode(0o755)).unwrap();
    let path = fixture.tasks_root.join("config.yml");
    let mut config: serde_yaml_ng::Value =
        serde_yaml_ng::from_str(&fs::read_to_string(&path).unwrap()).unwrap();
    config["agents"]["replacement"] = config["agents"]["safety"].clone();
    config["agents"]["replacement"]["command"] = replacement.to_str().unwrap().into();
    fs::write(path, serde_yaml_ng::to_string(&config).unwrap()).unwrap();
    lotar::services::automation_service::AutomationService::set(&fixture.tasks_root, None, r#"
automation:
  rules:
    - name: Delayed startup cancellation
      when:
        assignee: '@safety'
      on:
        job_started:
          run:
            command: sh
            args:
              - -c
              - 'touch "$LOTAR_TASKS_DIR/../hook-ready"; i=0; while [ ! -f "$LOTAR_TASKS_DIR/../release-hook" ] && [ "$i" -lt 100 ]; do sleep 0.05; i=$((i + 1)); done'
        job_cancelled:
          set:
            assignee: '@replacement'
          comment: deferred-handoff
"#).unwrap();
    let mut storage = fixture.create_storage();
    TaskService::update(
        &mut storage,
        &ticket,
        lotar::api_types::TaskUpdate {
            assignee: Some("@safety".into()),
            ..Default::default()
        },
    )
    .unwrap();
    wait_until(|| fixture.get_temp_path().join("hook-ready").exists());
    let old = AgentJobService::list_jobs()
        .into_iter()
        .find(|job| job.ticket_id == ticket)
        .unwrap();
    AgentJobService::cancel_job(&old.id).unwrap();
    AgentJobService::cancel_job(&old.id).unwrap();
    let while_blocked = AgentJobService::queue_stats();
    let before = TaskService::get(&storage, &ticket, None).unwrap();
    fs::write(fixture.get_temp_path().join("release-hook"), "").unwrap();
    wait_idle();
    assert_eq!(
        while_blocked.running, 1,
        "dispatched slot must stay owned during teardown"
    );
    assert_eq!(
        before.assignee.as_deref(),
        Some("@safety"),
        "cancellation automation ran before teardown"
    );
    assert!(
        before
            .comments
            .iter()
            .all(|comment| comment.text != "deferred-handoff")
    );
    let jobs = AgentJobService::list_jobs();
    let replacements: Vec<_> = jobs
        .iter()
        .filter(|job| job.agent.as_deref() == Some("replacement"))
        .collect();
    assert_eq!(replacements.len(), 1);
    assert_eq!(replacements[0].status, "completed");
    assert!(
        !fixture.get_temp_path().join("old-executed").exists(),
        "cancelled dispatch must not spawn its runner"
    );
    let after = TaskService::get(&storage, &ticket, None).unwrap();
    assert_eq!(
        after
            .comments
            .iter()
            .filter(|comment| comment.text == "deferred-handoff")
            .count(),
        1
    );
}

#[test]
#[cfg_attr(no_git_tests, ignore = "Git repository creation unavailable")]
fn inherited_git_dir_cannot_make_unmerged_cleanup_compare_branch_to_itself() {
    if !common::git_available() {
        eprintln!("skipping: git unavailable in this sandbox");
        return;
    }
    let (fixture, ticket) = setup("exit 0", true);
    let root = fixture.get_temp_path();
    let wt = root.join("worktrees").join(&ticket);
    let branch = format!("agent/{ticket}");
    git(
        root,
        &["worktree", "add", "-b", &branch, wt.to_str().unwrap()],
    );
    fs::write(wt.join("unmerged"), "must survive").unwrap();
    git(&wt, &["add", "unmerged"]);
    git(&wt, &["commit", "-m", "Unmerged test work"]);
    let path = fixture.tasks_root.join("SAFE/1.yml");
    let mut task: serde_yaml_ng::Value =
        serde_yaml_ng::from_str(&fs::read_to_string(&path).unwrap()).unwrap();
    task["status"] = "Done".into();
    fs::write(path, serde_yaml_ng::to_string(&task).unwrap()).unwrap();
    let gitdir = Command::new("git")
        .args(["rev-parse", "--absolute-git-dir"])
        .current_dir(&wt)
        .output()
        .unwrap();
    assert!(gitdir.status.success());
    let gitdir = String::from_utf8(gitdir.stdout).unwrap();
    let result = cleanup_command(&fixture)
        .env("GIT_DIR", gitdir.trim())
        .env("GIT_WORK_TREE", &wt)
        .env("GIT_COMMON_DIR", root.join(".git"))
        .env("GIT_INDEX_FILE", Path::new(gitdir.trim()).join("index"))
        .output()
        .unwrap();
    assert!(!result.status.success());
    assert!(wt.join("unmerged").exists());
    git(
        root,
        &["show-ref", "--verify", &format!("refs/heads/{branch}")],
    );
}

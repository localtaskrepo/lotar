#![cfg(unix)]

use crate::common;
use common::TestFixtures;
use lotar::api_types::{AgentJobCreateRequest, TaskCreate, TaskUpdate};
use lotar::services::agent_job_service::AgentJobService;
use lotar::services::task_service::{TaskService, TaskUpdateContext};
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;
use std::process::{Command, Stdio};
use std::thread::sleep;
use std::time::{Duration, Instant};

fn git(root: &Path, args: &[&str]) {
    let output = Command::new("git").args(args).current_dir(root).output().unwrap();
    assert!(output.status.success(), "git {args:?}: {}", String::from_utf8_lossy(&output.stderr));
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
        TaskCreate { title: "Safety regression".into(), project: Some("SAFE".into()), ..Default::default() },
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
    AgentJobCreateRequest { ticket_id: ticket.into(), prompt: "Run safety regression".into(), agent: Some("safety".into()), runner: None, ..Default::default() }
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
    fs::write(
        queue_dir.join(format!("queue-{hash}.json")),
        serde_json::to_vec(&serde_json::json!({
            "version": 1, "tasks_dir": fixture.tasks_root, "updated_at": "now",
            "pending": [{"ticket_id": ticket, "agent": "safety", "prompt": "run", "queued_at": "now", "attempts": 0}]
        }))
        .unwrap(),
    )
    .unwrap();
    let mut worker = Command::new(env!("CARGO_BIN_EXE_lotar"));
    worker
        .args(["--tasks-dir", fixture.tasks_root.to_str().unwrap(), "agent", "worker"])
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
    let (fixture, ticket) = setup("sleep 1\nprintf finished > \"$LOTAR_TASKS_DIR/../completed-marker\"", false);
    wait_worker(worker_command(&fixture, &ticket).spawn().unwrap());
    assert_eq!(fs::read_to_string(fixture.get_temp_path().join("completed-marker")).unwrap(), "finished");
    let config = lotar::config::resolution::config_for_project(&fixture.tasks_root, Some("SAFE")).unwrap();
    let context = lotar::services::agent_context_service::AgentContextService::load(&fixture.tasks_root, &config, &ticket).unwrap().unwrap();
    assert!(context.messages.iter().any(|message| message.role == "user" && message.content == "run"), "worker must await context persistence");
}

#[test]
fn enabled_worktree_without_repository_fails_closed() {
    let (fixture, ticket) = setup("touch \"$LOTAR_TASKS_DIR/../executed-in-main\"", false);
    let path = fixture.tasks_root.join("config.yml");
    let mut config: serde_yaml_ng::Value = serde_yaml_ng::from_str(&fs::read_to_string(&path).unwrap()).unwrap();
    config["agent"]["worktree"]["enabled"] = true.into();
    fs::write(path, serde_yaml_ng::to_string(&config).unwrap()).unwrap();
    let job = AgentJobService::start_job_with_tasks_dir(request(&ticket), &fixture.tasks_root).unwrap();
    wait_idle();
    let job = AgentJobService::get_job(&job.id).unwrap();
    assert_eq!(job.status, "failed");
    assert!(job.last_message.unwrap().contains("Worktree setup failed"));
    assert!(!fixture.get_temp_path().join("executed-in-main").exists());
}

#[test]
fn terminal_cancellation_preserves_history_and_other_running_slot() {
    let (fixture, ticket) =
        setup("if [ -f \"$LOTAR_TASKS_DIR/../block\" ]; then while [ ! -f \"$LOTAR_TASKS_DIR/../release\" ]; do sleep 0.05; done; fi", false);
    let terminal = AgentJobService::start_job_with_tasks_dir(request(&ticket), &fixture.tasks_root).unwrap();
    wait_idle();
    let terminal = AgentJobService::get_job(&terminal.id).unwrap();
    assert_eq!(terminal.status, "completed");
    fs::write(fixture.get_temp_path().join("block"), "").unwrap();
    let running = AgentJobService::start_job_with_tasks_dir(request(&ticket), &fixture.tasks_root).unwrap();
    wait_until(|| AgentJobService::get_job(&running.id).unwrap().status == "running");
    let events = AgentJobService::events_for(&terminal.id).len();
    let cancelled = AgentJobService::cancel_job(&terminal.id).unwrap().unwrap();
    assert_eq!(serde_json::to_value(cancelled).unwrap(), serde_json::to_value(terminal.clone()).unwrap());
    assert_eq!(AgentJobService::events_for(&terminal.id).len(), events);
    assert_eq!(AgentJobService::queue_stats().running, 1);
    assert!(AgentJobService::has_active_job(&ticket));
    let second = TaskService::create(
        &mut fixture.create_storage(),
        TaskCreate { title: "Queued cancellation".into(), project: Some("SAFE".into()), ..Default::default() },
    )
    .unwrap();
    let queued = AgentJobService::start_job_with_tasks_dir(request(&second.id), &fixture.tasks_root).unwrap();
    assert_eq!(AgentJobService::queue_stats().queued, 1);
    AgentJobService::cancel_job(&queued.id).unwrap();
    assert_eq!(AgentJobService::queue_stats().queued, 0);
    assert_eq!(AgentJobService::queue_stats().running, 1);
    assert!(!AgentJobService::has_active_job(&second.id));
    AgentJobService::cancel_job(&running.id).unwrap();
    AgentJobService::cancel_job(&running.id).unwrap();
    wait_idle();
    assert_eq!(AgentJobService::get_job(&running.id).unwrap().status, "cancelled");
}

#[test]
fn failed_terminal_cancellation_and_dispatch_races_preserve_slots() {
    let (fixture, ticket) = setup("exit 1", false);
    let job = AgentJobService::start_job_with_tasks_dir(request(&ticket), &fixture.tasks_root).unwrap();
    wait_idle();
    let failed = AgentJobService::get_job(&job.id).unwrap();
    assert_eq!(failed.status, "failed");
    assert_eq!(serde_json::to_value(AgentJobService::cancel_job(&job.id).unwrap().unwrap()).unwrap(), serde_json::to_value(failed).unwrap());
    fs::write(fixture.get_temp_path().join("runner.sh"), "#!/bin/sh\nsleep 10\n").unwrap();
    for _ in 0..20 {
        let job = AgentJobService::start_job_with_tasks_dir(request(&ticket), &fixture.tasks_root).unwrap();
        AgentJobService::cancel_job(&job.id).unwrap();
        wait_idle();
        assert_eq!(AgentJobService::get_job(&job.id).unwrap().status, "cancelled");
        assert!(!AgentJobService::has_active_job(&ticket));
    }
}

fn cleanup_command(fixture: &TestFixtures) -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_lotar"));
    command
        .args(["--tasks-dir", fixture.tasks_root.to_str().unwrap(), "agent", "worktree", "cleanup", "--delete-branches"])
        .env("LOTAR_IGNORE_HOME_CONFIG", "1")
        .current_dir(fixture.get_temp_path());
    command
}

fn cleanup(fixture: &TestFixtures) -> std::process::Output {
    cleanup_command(fixture).output().unwrap()
}

#[test]
fn repository_env_sanitizer_removes_inherited_and_explicit_overrides_only() {
    let fixture = TestFixtures::new();
    let _dir = common::env_mutex::EnvVarGuard::set("GIT_DIR", "inherited-repository");
    let _config = common::env_mutex::EnvVarGuard::set("GIT_CONFIG_PARAMETERS", "'core.worktree=other'");
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
    // About 100 KiB, more than a pipe buffer holds, so output is still
    // unread when the runner exits. Each line is one log event: few long
    // lines keep the run short on a busy machine (DEV-358).
    let (fixture, ticket) = setup(
        "i=0; while [ $i -lt 100 ]; do printf 'stderr-burst-%04d-%0980d\\n' \"$i\" 0 >&2; i=$((i + 1)); done\nprintf 'unique-trailing-stderr-sentinel\\n' >&2",
        false,
    );
    let path = fixture.tasks_root.join("config.yml");
    let mut config: serde_yaml_ng::Value = serde_yaml_ng::from_str(&fs::read_to_string(&path).unwrap()).unwrap();
    config["agent"]["logs_dir"] = "agent-logs".into();
    fs::write(&path, serde_yaml_ng::to_string(&config).unwrap()).unwrap();
    wait_worker(worker_command(&fixture, &ticket).spawn().unwrap());
    let logs = lotar::services::agent_log_service::AgentLogService::list_logs(fixture.get_temp_path(), "agent-logs").unwrap();
    assert_eq!(logs.len(), 1);
    let events = lotar::services::agent_log_service::AgentLogService::load_events(fixture.get_temp_path(), "agent-logs", &logs[0]).unwrap();
    let sentinel =
        events.iter().position(|event| event.message.as_deref() == Some("unique-trailing-stderr-sentinel")).expect("trailing stderr must be persisted");
    let completed = events.iter().position(|event| event.kind == "agent_job_completed").unwrap();
    assert!(sentinel < completed, "all output must precede completion");
    let status = lotar::services::agent_log_service::AgentLogService::load_status(fixture.get_temp_path(), "agent-logs", &logs[0]).unwrap().unwrap();
    assert_eq!(status.summary.as_deref(), Some("unique-trailing-stderr-sentinel"));
}

#[test]
fn inherited_output_streams_do_not_hold_job_open_after_parent_exit() {
    let (fixture, ticket) = setup("sleep 20 &\nprintf '%s' \"$!\" > \"$LOTAR_TASKS_DIR/../descendant-pid\"\nprintf 'parent-finished\\n' >&2", false);
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
    let job = AgentJobService::start_job_with_tasks_dir(request(&ticket), &fixture.tasks_root).unwrap();
    wait_idle();
    assert!(started.elapsed() < Duration::from_secs(10), "reader waited for surviving descendant EOF");
    let job = AgentJobService::get_job(&job.id).unwrap();
    assert_eq!(job.status, "completed");
    assert_eq!(job.summary.as_deref(), Some("parent-finished"));
}

#[test]
fn dispatched_cancellation_hands_off_once_after_teardown() {
    use lotar::services::agent_job_service::AgentOrchestratorMode;
    AgentJobService::set_orchestrator_mode(AgentOrchestratorMode::Server);
    let (fixture, ticket) = setup("printf '%s' \"$$\" > \"$LOTAR_TASKS_DIR/../old-pid\"\nprintf 'old-context\\n' >&2\nwhile :; do sleep 1; done", false);
    let replacement = fixture.get_temp_path().join("replacement.sh");
    fs::write(&replacement, "#!/bin/sh\nif kill -0 \"$(cat \"$LOTAR_TASKS_DIR/../old-pid\")\" 2>/dev/null; then exit 9; fi\nprintf replacement >> \"$LOTAR_TASKS_DIR/../replacement-count\"\n").unwrap();
    fs::set_permissions(&replacement, fs::Permissions::from_mode(0o755)).unwrap();
    let path = fixture.tasks_root.join("config.yml");
    let mut config: serde_yaml_ng::Value = serde_yaml_ng::from_str(&fs::read_to_string(&path).unwrap()).unwrap();
    config["agents"]["replacement"] = config["agents"]["safety"].clone();
    config["agents"]["replacement"]["command"] = replacement.to_str().unwrap().into();
    fs::write(path, serde_yaml_ng::to_string(&config).unwrap()).unwrap();
    lotar::services::automation_service::AutomationService::set(&fixture.tasks_root, None,
        "automation:\n  rules:\n    - name: Cancellation handoff\n      when:\n        assignee: '@safety'\n      on:\n        job_cancelled:\n          set:\n            assignee: '@replacement'\n          comment: cancellation-handoff-once\n").unwrap();
    let mut storage = fixture.create_storage();
    TaskService::update(&mut storage, &ticket, lotar::api_types::TaskUpdate { assignee: Some("@safety".into()), ..Default::default() }).unwrap();
    let old = AgentJobService::list_jobs().into_iter().find(|job| job.ticket_id == ticket).unwrap();
    wait_until(|| fixture.get_temp_path().join("old-pid").exists());
    AgentJobService::cancel_job(&old.id).unwrap();
    AgentJobService::cancel_job(&old.id).unwrap();
    wait_idle();
    let jobs = AgentJobService::list_jobs();
    let replacements: Vec<_> = jobs.iter().filter(|job| job.agent.as_deref() == Some("replacement")).collect();
    assert_eq!(replacements.len(), 1, "cancellation handoff must not be lost or duplicated");
    assert_eq!(replacements[0].status, "completed", "replacement started before old process teardown");
    assert_eq!(fs::read_to_string(fixture.get_temp_path().join("replacement-count")).unwrap(), "replacement");
    assert_eq!(AgentJobService::events_for(&old.id).iter().filter(|event| event.kind == "agent_job_cancelled").count(), 1);
    let task = TaskService::get(&storage, &ticket, None).unwrap();
    assert_eq!(task.comments.iter().filter(|comment| comment.text == "cancellation-handoff-once").count(), 1);
    let resolved = lotar::config::resolution::config_for_project(&fixture.tasks_root, Some("SAFE")).unwrap();
    let context = lotar::services::agent_context_service::AgentContextService::load(&fixture.tasks_root, &resolved, &ticket).unwrap().unwrap();
    assert_eq!(context.messages.iter().filter(|message| message.role == "user").count(), 2, "cancelled context must persist before replacement context");
}

#[test]
fn cancelled_dispatch_waits_for_inflight_start_before_handoff() {
    AgentJobService::set_orchestrator_mode(lotar::services::agent_job_service::AgentOrchestratorMode::Server);
    let (fixture, ticket) = setup("touch \"$LOTAR_TASKS_DIR/../old-executed\"", false);
    let replacement = fixture.get_temp_path().join("replacement.sh");
    fs::write(&replacement, "#!/bin/sh\nexit 0\n").unwrap();
    fs::set_permissions(&replacement, fs::Permissions::from_mode(0o755)).unwrap();
    let path = fixture.tasks_root.join("config.yml");
    let mut config: serde_yaml_ng::Value = serde_yaml_ng::from_str(&fs::read_to_string(&path).unwrap()).unwrap();
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
    TaskService::update(&mut storage, &ticket, lotar::api_types::TaskUpdate { assignee: Some("@safety".into()), ..Default::default() }).unwrap();
    wait_until(|| fixture.get_temp_path().join("hook-ready").exists());
    let old = AgentJobService::list_jobs().into_iter().find(|job| job.ticket_id == ticket).unwrap();
    AgentJobService::cancel_job(&old.id).unwrap();
    AgentJobService::cancel_job(&old.id).unwrap();
    let while_blocked = AgentJobService::queue_stats();
    let before = TaskService::get(&storage, &ticket, None).unwrap();
    fs::write(fixture.get_temp_path().join("release-hook"), "").unwrap();
    wait_idle();
    assert_eq!(while_blocked.running, 1, "dispatched slot must stay owned during teardown");
    assert_eq!(before.assignee.as_deref(), Some("@safety"), "cancellation automation ran before teardown");
    assert!(before.comments.iter().all(|comment| comment.text != "deferred-handoff"));
    let jobs = AgentJobService::list_jobs();
    let replacements: Vec<_> = jobs.iter().filter(|job| job.agent.as_deref() == Some("replacement")).collect();
    assert_eq!(replacements.len(), 1);
    assert_eq!(replacements[0].status, "completed");
    assert!(!fixture.get_temp_path().join("old-executed").exists(), "cancelled dispatch must not spawn its runner");
    let after = TaskService::get(&storage, &ticket, None).unwrap();
    assert_eq!(after.comments.iter().filter(|comment| comment.text == "deferred-handoff").count(), 1);
}

/// Tests that need a real Git repository (see `common::require_git`).
/// The `git_required` module path is what the gitless nextest profile
/// excludes when the runtime Git capability probe fails (DEV-79).
mod git_required {
    use super::*;

    #[test]
    fn enabled_worktree_failure_never_executes_in_main() {
        common::require_git();
        let (fixture, ticket) = setup("touch executed-in-main", true);
        fs::write(fixture.get_temp_path().join("worktrees"), "not a directory").unwrap();
        let job = AgentJobService::start_job_with_tasks_dir(request(&ticket), &fixture.tasks_root).unwrap();
        wait_idle();
        let job = AgentJobService::get_job(&job.id).unwrap();
        assert_eq!(job.status, "failed");
        assert!(job.last_message.unwrap().contains("Worktree setup failed"));
        assert!(!fixture.get_temp_path().join("executed-in-main").exists());
    }

    #[test]
    fn cli_cleanup_preserves_indeterminate_dirty_and_unmerged_worktrees() {
        common::require_git();
        let (fixture, ticket) = setup("exit 0", true);
        let root = fixture.get_temp_path();
        let wt = root.join("worktrees").join(&ticket);
        let branch = format!("agent/{ticket}");
        git(root, &["worktree", "add", "-b", &branch, wt.to_str().unwrap()]);
        let task_path = fixture.tasks_root.join("SAFE/1.yml");
        let original = fs::read_to_string(&task_path).unwrap();
        fs::remove_file(&task_path).unwrap();
        assert!(cleanup(&fixture).status.success());
        assert!(wt.exists(), "missing task must not authorize removal");
        let all = Command::new(env!("CARGO_BIN_EXE_lotar"))
            .args(["--tasks-dir", fixture.tasks_root.to_str().unwrap(), "agent", "worktree", "cleanup", "--all", "--delete-branches"])
            .env("LOTAR_IGNORE_HOME_CONFIG", "1")
            .current_dir(root)
            .output()
            .unwrap();
        assert!(all.status.success());
        assert!(wt.exists(), "--all must not authorize indeterminate cleanup");
        for raw in ["broken: [", "title: malformed\nstatus: Done\ncreated: []\n", "title: unknown\nstatus: Unconfigured\ncreated: now\n"] {
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
        git(root, &["show-ref", "--verify", &format!("refs/heads/{branch}")]);
        fs::write(root.join("other"), "main moved on").unwrap();
        git(root, &["add", "other"]);
        git(root, &["commit", "-m", "Other work"]);
        git(root, &["cherry-pick", &branch]);
        let output = cleanup(&fixture);
        assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
        assert!(!wt.exists(), "work landed rebased counts as merged");
        assert!(!branch_exists(root, &branch));
    }

    #[test]
    fn cli_worktree_list_accepts_a_relative_tasks_dir() {
        common::require_git();
        let (fixture, ticket) = setup("exit 0", true);
        let root = fixture.get_temp_path();
        let wt = root.join("worktrees").join(&ticket);
        let branch = format!("agent/{ticket}");
        git(root, &["worktree", "add", "-b", &branch, wt.to_str().unwrap()]);
        let relative = fixture.tasks_root.strip_prefix(root).unwrap();
        let output = Command::new(env!("CARGO_BIN_EXE_lotar"))
            .args(["--tasks-dir", relative.to_str().unwrap(), "agent", "worktree", "list"])
            .env("LOTAR_IGNORE_HOME_CONFIG", "1")
            .current_dir(root)
            .output()
            .unwrap();
        let stdout = String::from_utf8_lossy(&output.stdout);
        assert!(output.status.success(), "{stdout}{}", String::from_utf8_lossy(&output.stderr));
        assert!(stdout.contains(&branch), "{stdout}");
    }

    #[test]
    fn automatic_cleanup_preserves_missing_malformed_dirty_and_unmerged_worktrees() {
        common::require_git();
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
            let mut task: serde_yaml_ng::Value = serde_yaml_ng::from_str(&fs::read_to_string(&path).unwrap()).unwrap();
            task["status"] = "Done".into();
            fs::write(path, serde_yaml_ng::to_string(&task).unwrap()).unwrap();
            let job = AgentJobService::start_job_with_tasks_dir(request(&ticket), &fixture.tasks_root).unwrap();
            wait_idle();
            let job = AgentJobService::get_job(&job.id).unwrap();
            assert_eq!(job.status, "completed", "{body}: {:?}", job.last_message);
            assert!(Path::new(job.worktree_path.as_deref().unwrap()).exists(), "{body}");
            git(fixture.get_temp_path(), &["show-ref", "--verify", &format!("refs/heads/agent/{ticket}")]);
        }
    }

    #[test]
    fn cli_cleanup_accepts_clean_merged_worktree_with_project_done_status() {
        common::require_git();
        let (fixture, ticket) = setup("exit 0", true);
        let root = fixture.get_temp_path();
        let wt = root.join("worktrees").join(&ticket);
        let branch = format!("agent/{ticket}");
        git(root, &["worktree", "add", "-b", &branch, wt.to_str().unwrap()]);
        fixture.create_config_in_dir(&fixture.tasks_root.join("SAFE"), "project:\n  name: SAFE\nissue:\n  states: [Todo, Shipped]\n");
        let path = fixture.tasks_root.join("SAFE/1.yml");
        let mut task: serde_yaml_ng::Value = serde_yaml_ng::from_str(&fs::read_to_string(&path).unwrap()).unwrap();
        task["status"] = "Shipped".into();
        fs::write(path, serde_yaml_ng::to_string(&task).unwrap()).unwrap();
        let output = cleanup(&fixture);
        assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
        assert!(
            !wt.exists(),
            "configured done state should allow clean merged cleanup: {}{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(!Command::new("git").args(["show-ref", "--verify", &format!("refs/heads/{branch}")]).current_dir(root).output().unwrap().status.success());
    }

    /// DEV-351: every fixture's ticket is SAFE-1, and other test runs on the
    /// machine start wrappers for it. Only a wrapper for this tasks
    /// directory keeps the worktree (DEV-349).
    #[test]
    fn cli_cleanup_ignores_a_wrapper_running_for_another_tasks_dir() {
        common::require_git();
        let (fixture, ticket) = setup("exit 0", true);
        let root = fixture.get_temp_path();
        let wt = root.join("worktrees").join(&ticket);
        git(root, &["worktree", "add", "-b", &format!("agent/{ticket}"), wt.to_str().unwrap()]);
        let path = fixture.tasks_root.join("SAFE/1.yml");
        let mut task: serde_yaml_ng::Value = serde_yaml_ng::from_str(&fs::read_to_string(&path).unwrap()).unwrap();
        task["status"] = "Done".into();
        fs::write(path, serde_yaml_ng::to_string(&task).unwrap()).unwrap();
        let other = TestFixtures::new();
        let with_wrapper = |tasks_dir: &Path| {
            use std::os::unix::process::CommandExt;
            let mut wrapper = Command::new(env!("CARGO_BIN_EXE_lotar-agent-wrapper"))
                .args(["--job-id", "foreign", "--ticket-id", &ticket, "--runner", "claude", "--tasks-dir", tasks_dir.to_str().unwrap(), "--", "sleep", "30"])
                .process_group(0)
                .spawn()
                .unwrap();
            let output = cleanup(&fixture);
            // SAFETY: signals only the process group this test started.
            unsafe {
                libc::kill(-(wrapper.id() as i32), libc::SIGKILL);
            }
            wrapper.wait().unwrap();
            output
        };
        assert!(with_wrapper(&fixture.tasks_root).status.success());
        assert!(wt.exists(), "a wrapper for this tasks directory keeps the worktree");
        let output = with_wrapper(&other.tasks_root);
        assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
        assert!(!wt.exists(), "a wrapper for another tasks directory must not keep the worktree");
    }

    fn set_status(fixture: &TestFixtures, ticket: &str, status: &str, context: TaskUpdateContext) {
        let patch = TaskUpdate { status: Some(status.into()), ..Default::default() };
        TaskService::update_with_context(&mut fixture.create_storage(), ticket, patch, context).unwrap();
    }

    fn branch_exists(root: &Path, branch: &str) -> bool {
        Command::new("git").args(["show-ref", "--verify", "--quiet", &format!("refs/heads/{branch}")]).current_dir(root).status().unwrap().success()
    }

    /// DEV-343: the fix job ends with the ticket handed on, not Done; the
    /// merge lands its commit rebased. Done, set afterwards by whoever,
    /// removes the fix job's worktree and branch.
    #[test]
    fn done_removes_the_worktree_an_earlier_job_left_once_its_work_landed_rebased() {
        common::require_git();
        let (fixture, ticket) = setup("echo fix > fix.txt && git add fix.txt && git commit -q -m Fix", true);
        let root = fixture.get_temp_path();
        let branch = format!("agent/{ticket}");
        let job = AgentJobService::start_job_with_tasks_dir(request(&ticket), &fixture.tasks_root).unwrap();
        wait_idle();
        let job = AgentJobService::get_job(&job.id).unwrap();
        assert_eq!(job.status, "completed", "{:?}", job.last_message);
        let worktree = Path::new(job.worktree_path.as_deref().unwrap()).to_path_buf();
        assert!(worktree.exists(), "the ticket is not Done yet");

        fs::write(root.join("other.txt"), "main moved on").unwrap();
        git(root, &["add", "other.txt"]);
        git(root, &["commit", "-q", "-m", "Other work"]);
        set_status(&fixture, &ticket, "Done", TaskUpdateContext::default());
        assert!(worktree.exists(), "unlanded work keeps the worktree");

        git(root, &["cherry-pick", &branch]);
        let ancestor = Command::new("git").args(["merge-base", "--is-ancestor", &branch, "HEAD"]).current_dir(root).status().unwrap();
        assert!(!ancestor.success(), "the fix landed as a new commit");
        set_status(&fixture, &ticket, "Todo", TaskUpdateContext::default());
        set_status(&fixture, &ticket, "Done", TaskUpdateContext::default());
        assert!(!worktree.exists(), "Done removes the landed worktree");
        assert!(!branch_exists(root, &branch), "cleanup_delete_branches deletes the rebased branch");
    }

    /// DEV-366: the merge lands the fix reworked (other changes in the same
    /// commit, so `git cherry` sees no equivalent), and main changes the
    /// fixed lines again later. Done keeps the worktree while part of the
    /// branch's work is missing from main and removes it once all of it
    /// landed.
    #[test]
    fn done_removes_the_worktree_once_its_work_landed_reworked_and_keeps_missing_work() {
        common::require_git();
        let (fixture, ticket) = setup("echo fix > fix.txt && echo more > more.txt && git add . && git commit -q -m Fix", true);
        let root = fixture.get_temp_path();
        let branch = format!("agent/{ticket}");
        let job = AgentJobService::start_job_with_tasks_dir(request(&ticket), &fixture.tasks_root).unwrap();
        wait_idle();
        let job = AgentJobService::get_job(&job.id).unwrap();
        assert_eq!(job.status, "completed", "{:?}", job.last_message);
        let worktree = Path::new(job.worktree_path.as_deref().unwrap()).to_path_buf();
        let commit = |message: &str, files: &[(&str, &str)]| {
            for (name, content) in files {
                fs::write(root.join(name), content).unwrap();
                git(root, &["add", name]);
            }
            git(root, &["commit", "-q", "-m", message]);
        };

        commit("Land part of the fix", &[("fix.txt", "fix\n"), ("notes.txt", "reworked\n")]);
        set_status(&fixture, &ticket, "Done", TaskUpdateContext::default());
        assert!(worktree.exists(), "more.txt is missing from main, so the worktree stays");
        assert!(branch_exists(root, &branch));

        commit("Land the rest", &[("more.txt", "more\n")]);
        commit("Change the fix again", &[("fix.txt", "fixed again\n")]);
        set_status(&fixture, &ticket, "Todo", TaskUpdateContext::default());
        set_status(&fixture, &ticket, "Done", TaskUpdateContext::default());
        assert!(!worktree.exists(), "Done removes the worktree whose work landed reworked");
        assert!(!branch_exists(root, &branch));
    }

    /// DEV-343: Done set while a job still runs for the ticket waits for
    /// the job's end.
    #[test]
    fn done_during_a_job_leaves_the_worktree_to_the_jobs_end() {
        common::require_git();
        let (fixture, ticket) = setup("touch \"$LOTAR_TASKS_DIR/../started\"; sleep 2", true);
        let root = fixture.get_temp_path();
        let job = AgentJobService::start_job_with_tasks_dir(request(&ticket), &fixture.tasks_root).unwrap();
        wait_until(|| root.join("started").exists());
        let worktree = Path::new(AgentJobService::get_job(&job.id).unwrap().worktree_path.as_deref().unwrap()).to_path_buf();
        set_status(&fixture, &ticket, "Done", TaskUpdateContext::automation_disabled());
        assert!(worktree.exists(), "the running job keeps its worktree");
        wait_idle();
        assert!(!worktree.exists(), "the job's end removes it");
        assert!(!branch_exists(root, &format!("agent/{ticket}")));
    }

    #[test]
    fn inherited_and_profile_git_overrides_cannot_redirect_setup_or_runner() {
        common::require_git();
        let (fixture, ticket) = setup(
            "git rev-parse --show-toplevel > \"$LOTAR_TASKS_DIR/../runner-top\"\ngit rev-parse --path-format=absolute --git-common-dir > \"$LOTAR_TASKS_DIR/../runner-common\"",
            true,
        );
        let (other, _) = setup("exit 0", true);
        let other_root = other.get_temp_path();
        let overrides = [
            ("GIT_DIR", other_root.join(".git").to_string_lossy().to_string()),
            ("GIT_WORK_TREE", other_root.to_string_lossy().to_string()),
            ("GIT_COMMON_DIR", other_root.join(".git").to_string_lossy().to_string()),
            ("GIT_INDEX_FILE", other_root.join(".git/index").to_string_lossy().to_string()),
            ("GIT_CONFIG_COUNT", "1".into()),
            ("GIT_CONFIG_KEY_0", "core.worktree".into()),
            ("GIT_CONFIG_VALUE_0", other_root.to_string_lossy().to_string()),
        ];
        let path = fixture.tasks_root.join("config.yml");
        let mut config: serde_yaml_ng::Value = serde_yaml_ng::from_str(&fs::read_to_string(&path).unwrap()).unwrap();
        config["agents"]["safety"]["env"] = serde_yaml_ng::to_value(overrides.iter().cloned().collect::<std::collections::BTreeMap<_, _>>()).unwrap();
        fs::write(path, serde_yaml_ng::to_string(&config).unwrap()).unwrap();
        wait_worker(worker_command(&fixture, &ticket).envs(overrides).spawn().unwrap());
        let top = fs::read_to_string(fixture.get_temp_path().join("runner-top")).unwrap();
        let common = fs::read_to_string(fixture.get_temp_path().join("runner-common")).unwrap();
        assert_eq!(fs::canonicalize(top.trim()).unwrap(), fs::canonicalize(fixture.get_temp_path().join("worktrees").join(&ticket)).unwrap());
        assert_eq!(fs::canonicalize(common.trim()).unwrap(), fs::canonicalize(fixture.get_temp_path().join(".git")).unwrap());
        assert!(
            !Command::new("git")
                .args(["show-ref", "--verify", &format!("refs/heads/agent/{ticket}")])
                .current_dir(other_root)
                .output()
                .unwrap()
                .status
                .success()
        );
    }

    #[test]
    fn inherited_git_dir_cannot_make_unmerged_cleanup_compare_branch_to_itself() {
        common::require_git();
        let (fixture, ticket) = setup("exit 0", true);
        let root = fixture.get_temp_path();
        let wt = root.join("worktrees").join(&ticket);
        let branch = format!("agent/{ticket}");
        git(root, &["worktree", "add", "-b", &branch, wt.to_str().unwrap()]);
        fs::write(wt.join("unmerged"), "must survive").unwrap();
        git(&wt, &["add", "unmerged"]);
        git(&wt, &["commit", "-m", "Unmerged test work"]);
        let path = fixture.tasks_root.join("SAFE/1.yml");
        let mut task: serde_yaml_ng::Value = serde_yaml_ng::from_str(&fs::read_to_string(&path).unwrap()).unwrap();
        task["status"] = "Done".into();
        fs::write(path, serde_yaml_ng::to_string(&task).unwrap()).unwrap();
        let gitdir = Command::new("git").args(["rev-parse", "--absolute-git-dir"]).current_dir(&wt).output().unwrap();
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
        git(root, &["show-ref", "--verify", &format!("refs/heads/{branch}")]);
    }
}

/// DEV-207: build output that git ignores no longer keeps a finished
/// ticket's worktree (git worktree remove drops it too), unless
/// agent.worktree.cleanup_ignored_files is turned off. A kept worktree is
/// explained in the job log.
#[test]
fn automatic_cleanup_removes_worktrees_with_only_ignored_files_unless_disabled() {
    common::require_git();
    for strict in [false, true] {
        let (fixture, ticket) = setup("mkdir -p build && echo out > build/app.o", true);
        let root = fixture.get_temp_path();
        fs::write(root.join(".gitignore"), "build/\n").unwrap();
        git(root, &["add", ".gitignore"]);
        git(root, &["commit", "-m", "Ignore build output"]);
        if strict {
            let config_path = fixture.tasks_root.join("config.yml");
            let config = fs::read_to_string(&config_path)
                .unwrap()
                .replace("    cleanup_delete_branches: true\n", "    cleanup_delete_branches: true\n    cleanup_ignored_files: false\n");
            fs::write(&config_path, config).unwrap();
        }
        let path = fixture.tasks_root.join("SAFE/1.yml");
        let mut task: serde_yaml_ng::Value = serde_yaml_ng::from_str(&fs::read_to_string(&path).unwrap()).unwrap();
        task["status"] = "Done".into();
        fs::write(path, serde_yaml_ng::to_string(&task).unwrap()).unwrap();

        let job = AgentJobService::start_job_with_tasks_dir(request(&ticket), &fixture.tasks_root).unwrap();
        wait_idle();
        let job = AgentJobService::get_job(&job.id).unwrap();
        assert_eq!(job.status, "completed", "{:?}", job.last_message);
        let worktree = Path::new(job.worktree_path.as_deref().unwrap());
        let kept_note = AgentJobService::events_for(&job.id)
            .iter()
            .any(|event| event.message.as_deref().is_some_and(|m| m.contains("Kept the worktree") && m.contains("build/app.o")));
        if strict {
            assert!(worktree.exists(), "strict cleanup keeps ignored files");
            assert!(kept_note, "a kept worktree is explained in the log");
        } else {
            assert!(!worktree.exists(), "ignored build output must not block cleanup");
            assert!(!kept_note);
        }
    }
}

/// DEV-364: a job that writes build output, in a repository that ignores
/// `target/` and `keep/` and has `cleanup_paths: [target]` configured.
fn build_output_setup(body: &str) -> (TestFixtures, String) {
    let (fixture, ticket) = setup(&format!("mkdir -p target/debug keep && echo bin > target/debug/app && echo out > keep/log\n{body}"), true);
    let root = fixture.get_temp_path();
    fs::write(root.join(".gitignore"), "target/\nkeep/\n").unwrap();
    git(root, &["add", ".gitignore"]);
    git(root, &["commit", "-m", "Ignore build output"]);
    let config_path = fixture.tasks_root.join("config.yml");
    let config = fs::read_to_string(&config_path)
        .unwrap()
        .replace("states: [Todo, Done]", "states: [Todo, HelpNeeded, Done]")
        .replace("    cleanup_delete_branches: true\n", "    cleanup_delete_branches: true\n    cleanup_paths: [target, ../escape, .git]\n");
    fs::write(&config_path, config).unwrap();
    fs::create_dir_all(root.join("escape")).unwrap();
    (fixture, ticket)
}

fn run_build_output_job(fixture: &TestFixtures, ticket: &str) -> lotar::api_types::AgentJob {
    let job = AgentJobService::start_job_with_tasks_dir(request(ticket), &fixture.tasks_root).unwrap();
    wait_idle();
    let job = AgentJobService::get_job(&job.id).unwrap();
    assert!(Path::new(job.worktree_path.as_deref().unwrap()).join(".git").exists(), "the worktree stays");
    job
}

fn deleted_note(job: &lotar::api_types::AgentJob) -> bool {
    AgentJobService::events_for(&job.id).iter().any(|event| event.message.as_deref().is_some_and(|m| m.contains("Deleted build output `target`")))
}

/// Ending a job with the ticket still open deletes the configured build
/// output and nothing else: unlisted ignored paths, paths outside the
/// worktree and git's metadata stay.
#[test]
fn ending_a_job_deletes_configured_build_output_and_keeps_the_worktree() {
    common::require_git();
    let (fixture, ticket) = build_output_setup("exit 0");
    let job = run_build_output_job(&fixture, &ticket);
    assert_eq!(job.status, "completed", "{:?}", job.last_message);
    let worktree = Path::new(job.worktree_path.as_deref().unwrap());
    assert!(!worktree.join("target").exists(), "configured build output is deleted");
    assert!(worktree.join("keep/log").exists(), "unlisted ignored files stay");
    assert!(fixture.get_temp_path().join("escape").exists(), "paths outside the worktree stay");
    assert!(deleted_note(&job), "the job log says what was deleted");
    let git_dir = Command::new("git").args(["rev-parse", "--absolute-git-dir"]).current_dir(worktree).output().unwrap();
    let git_dir = String::from_utf8(git_dir.stdout).unwrap();
    assert!(fs::read_dir(git_dir.trim()).unwrap().flatten().all(|entry| !entry.file_name().to_string_lossy().starts_with("lotar-cleanup-")));
}

/// A job that hands its ticket back (HelpNeeded) keeps its worktree, but
/// not its build output; a tracked file under a configured path keeps it.
#[test]
fn a_job_ending_with_help_needed_deletes_build_output() {
    common::require_git();
    let lotar_bin = env!("CARGO_BIN_EXE_lotar");
    let (fixture, ticket) = build_output_setup("");
    let script = fixture.get_temp_path().join("runner.sh");
    let body = fs::read_to_string(&script).unwrap();
    fs::write(
        &script,
        format!(
            "{body}LOTAR_IGNORE_HOME_CONFIG=1 '{lotar_bin}' --tasks-dir '{}' status \"$LOTAR_TICKET_ID\" HelpNeeded >/dev/null\n",
            fixture.tasks_root.display()
        ),
    )
    .unwrap();
    let job = run_build_output_job(&fixture, &ticket);
    assert_eq!(job.status, "completed", "{:?}", job.last_message);
    let task = fs::read_to_string(fixture.tasks_root.join("SAFE/1.yml")).unwrap();
    assert!(task.contains("status: HelpNeeded"), "{task}");
    let worktree = Path::new(job.worktree_path.as_deref().unwrap());
    assert!(!worktree.join("target").exists());
    assert!(deleted_note(&job));
}

/// Build output stays while another job runs for the ticket: it shares the
/// worktree. So does a configured path that holds tracked files.
#[test]
fn build_output_stays_while_another_job_runs_in_the_worktree() {
    common::require_git();
    use lotar::services::agent_active_jobs::{self, ActiveJob};
    let (fixture, ticket) = build_output_setup("");
    let go = fixture.get_temp_path().join("GO");
    let script = fixture.get_temp_path().join("runner.sh");
    let body = fs::read_to_string(&script).unwrap();
    fs::write(&script, format!("{body}while [ ! -f '{}' ]; do sleep 0.05; done\n", go.display())).unwrap();
    let job = AgentJobService::start_job_with_tasks_dir(request(&ticket), &fixture.tasks_root).unwrap();
    // A job another (live) process runs for the ticket.
    let elsewhere = ActiveJob {
        job_id: "job-elsewhere".into(),
        ticket_id: ticket.clone(),
        runner: "claude".into(),
        agent: Some("safety".into()),
        owner_pid: 1,
        child_pid: 1,
        started_at: "2026-11-11T00:00:00Z".into(),
    };
    agent_active_jobs::register(&fixture.tasks_root, &elsewhere);
    fs::write(&go, "").unwrap();
    wait_idle();
    agent_active_jobs::unregister(&fixture.tasks_root, &elsewhere.job_id);
    let job = AgentJobService::get_job(&job.id).unwrap();
    let worktree = Path::new(job.worktree_path.as_deref().unwrap());
    assert!(worktree.join("target/debug/app").exists(), "a running job's build output stays");
    assert!(!deleted_note(&job));

    // With no other job, tracked files under a configured path keep it.
    fs::write(&script, format!("{body}git add -f target/debug/app && git -c commit.gpgsign=false commit -qm 'Track a build file'\n")).unwrap();
    let job = run_build_output_job(&fixture, &ticket);
    assert_eq!(job.status, "completed", "{:?}", job.last_message);
    assert!(Path::new(job.worktree_path.as_deref().unwrap()).join("target/debug/app").exists(), "tracked files are never deleted");
    assert!(!deleted_note(&job));
}

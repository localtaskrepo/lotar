//! DEV-510: harnessmux's t3 agent gives a job its thread's link, ends a turn that waits for an answer as such, and a
//! taken-over t3 job ends with the thread's run instead of a second turn. A shell script stands in for T3's CLI.
//! DEV-619: a continued job renames its thread through T3 Code's server, which a thread here stands in for.
#![cfg(unix)]
use jiff::SignedDuration;
use jiff::Timestamp;

use crate::common::TestFixtures;
use crate::common::env_mutex::EnvVarGuard;
use lotar::api_types::{AgentJobCreateRequest, TaskCreate};
use lotar::services::agent_job_service::{AgentJobService, AgentOrchestratorMode};
use lotar::services::task_service::TaskService;
use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;
use std::time::{Duration, Instant};

/// `t3 thread start` appends its prompt to `prompts`, names thread t1, prints "Started." and exits with `start-exit` after `start-sleep` seconds;
/// `thread send` prints "Sent."; `thread show` prints `show.json`; `thread list` prints `list.json` (or fails while
/// `list-fail` exists); `auth session issue` prints session s1's token. Every call goes to `calls`, with its arguments
/// to `args`. While `setup` names a lotar binary, `thread start` makes the thread's worktree in `worktree` and runs
/// `lotar agent worktree setup` there, as T3 Code's worktree script does.
const FAKE_T3: &str = r#"#!/bin/sh
dir=$(dirname "$0")
echo "$1 $2" >> "$dir/calls"
echo "$*" >> "$dir/args"
if [ "$1 $2" = "thread start" ] && [ -f "$dir/setup" ]; then
  prev=""
  for arg in "$@"; do case "$prev" in --worktree) base=$arg ;; --branch) branch=$arg ;; esac; prev=$arg; done
  git -C "$3" worktree add -q -b "$branch" "$dir/worktree" "$base"
  T3CODE_WORKTREE_PATH="$dir/worktree" "$(cat "$dir/setup")" agent worktree setup >> "$dir/setup-out" 2>&1
fi
case "$1 $2" in
  "thread start") cat >> "$dir/prompts"; echo "Thread t1" >&2; sleep "$(cat "$dir/start-sleep")"; echo "Started."; exit "$(cat "$dir/start-exit")" ;;
  "thread send") cat > /dev/null; echo "Sent." ;;
  "thread show") cat "$dir/show.json" ;;
  "auth session") if [ "$3" = issue ]; then echo '{"sessionId":"s1","token":"secret"}'; else echo "Revoked session $4."; fi ;;
  "thread list") if [ -f "$dir/list-fail" ]; then echo "Could not reach the T3 Code server" >&2; exit 1; fi; cat "$dir/list.json" ;;
esac
"#;

const LINK: &str = "https://app.t3.codes/threads/env/t1";

struct Harness {
    fixtures: TestFixtures,
    dir: PathBuf,
    _env: Vec<EnvVarGuard>,
}

fn harness(start_exit: u8, start_sleep: u32) -> Harness {
    let fixtures = TestFixtures::new();
    let root = fixtures.get_temp_path().to_path_buf();
    let dir = root.join("t3");
    std::fs::create_dir_all(&dir).unwrap();
    let script = dir.join("t3");
    std::fs::write(&script, FAKE_T3).unwrap();
    std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).unwrap();
    std::fs::write(dir.join("start-exit"), start_exit.to_string()).unwrap();
    std::fs::write(dir.join("start-sleep"), start_sleep.to_string()).unwrap();
    let home = root.join("harnessmux");
    std::fs::create_dir_all(home.join("config")).unwrap();
    std::fs::write(home.join("config/agents.toml"), format!("[agents.t3]\nkind = \"t3\"\ncommand = [\"{}\"]\n", script.display())).unwrap();
    fixtures.create_config_in_dir(
        &fixtures.tasks_root,
        "default:\n  project: TEST\nissue:\n  states: [Todo, InProgress, HelpNeeded, Done]\nagents:\n  fixer:\n    runner: t3\n    provider: claudeAgent\n    model: m\n  named:\n    runner: t3\n    thread_name: \"${{ticket.id}} fix: ${{ticket.title}}\"\n",
    );
    let guards = vec![
        EnvVarGuard::set("HARNESSMUX_HOME", home.to_string_lossy().as_ref()),
        EnvVarGuard::set("LOTAR_TASKS_DIR", fixtures.tasks_root.to_string_lossy().as_ref()),
        EnvVarGuard::set("LOTAR_TEST_SILENT", "1"),
        EnvVarGuard::set("T3CODE_HOME", root.join("t3home").to_string_lossy().as_ref()),
    ];
    let create = TaskCreate { title: "Thread job".into(), project: Some("TEST".into()), reporter: Some("owner".into()), ..Default::default() };
    TaskService::create(&mut fixtures.create_storage(), create).unwrap();
    Harness { fixtures, dir, _env: guards }
}

impl Harness {
    fn show(&self, status: &str, active: &str, pending: u64, reply: &str) {
        let json = format!(
            r#"{{"thread":{{"threadId":"t1","link":"{LINK}","status":"{status}","activeRunId":{active},"latestRunId":"r1","pendingRequestCount":{pending}}},"messages":[{{"role":"assistant","runId":"r1","text":"{reply}"}}]}}"#
        );
        std::fs::write(self.dir.join("show.json"), json).unwrap();
    }

    /// Whether harnessmux saved thread t1 as the session's agent session. Its t3 agent calls `thread show` before
    /// the session's owner saves the thread id, so a process killed at that call resumes without the thread.
    fn thread_saved(&self) -> bool {
        let sessions = self.fixtures.get_temp_path().join("harnessmux/state/sessions");
        std::fs::read_dir(sessions).into_iter().flatten().flatten().any(|entry| {
            let record = std::fs::read_to_string(entry.path()).unwrap_or_default();
            entry.path().extension().is_some_and(|ext| ext == "json")
                && serde_json::from_str::<serde_json::Value>(&record).is_ok_and(|record| record["agent_session"] == "t1")
        })
    }

    /// Remove the saved thread id from the session's record, as if lotar died after T3 made the thread and before
    /// harnessmux saved its id (the record then notes when the thread was asked for), and list the thread in T3 as made
    /// then.
    fn forget_thread(&self) {
        let sessions = self.fixtures.get_temp_path().join("harnessmux/state/sessions");
        for entry in std::fs::read_dir(sessions).unwrap().flatten().filter(|entry| entry.path().extension().is_some_and(|ext| ext == "json")) {
            let mut record: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(entry.path()).unwrap()).unwrap();
            if record["agent_session"] != "t1" {
                continue;
            }
            record.as_object_mut().unwrap().remove("agent_session");
            record["agent_session_asked"] = record["last_used"].clone();
            std::fs::write(entry.path(), record.to_string()).unwrap();
            let list = serde_json::json!([{"threadId": "t1", "title": "TEST-1 Thread job", "createdAt": record["last_used"]}]);
            std::fs::write(self.dir.join("list.json"), list.to_string()).unwrap();
        }
    }

    fn args(&self) -> String {
        std::fs::read_to_string(self.dir.join("args")).unwrap_or_default()
    }

    fn calls(&self) -> String {
        std::fs::read_to_string(self.dir.join("calls")).unwrap_or_default()
    }

    fn command(&self) -> assert_cmd::Command {
        let mut cmd = crate::common::lotar_cmd().unwrap();
        cmd.current_dir(self.fixtures.get_temp_path()).args(["agent", "run", "TEST-1", "Fix it", "--agent", "fixer", "--wait"]);
        cmd
    }

    fn task(&self) -> lotar::api_types::TaskDTO {
        TaskService::get(&self.fixtures.create_storage(), "TEST-1", None).unwrap()
    }

    fn linked(&self) -> bool {
        self.task().references.iter().any(|reference| reference.link.as_deref() == Some(LINK))
    }
}

fn wait_for(what: &str, mut done: impl FnMut() -> bool) {
    let deadline = Instant::now() + Duration::from_secs(60);
    while !done() {
        assert!(Instant::now() < deadline, "timed out waiting for {what}");
        std::thread::sleep(Duration::from_millis(25));
    }
}

#[test]
fn a_t3_job_links_its_thread_on_the_ticket() {
    let harness = harness(0, 0);
    harness.show("completed", "null", 0, "Started.");
    harness.command().assert().success();
    assert!(harness.linked(), "{:?}", harness.task().references);
}

/// DEV-621: a rework past `continue_within_minutes` starts a new thread, and its handoff names the old thread.
#[test]
fn a_t3_handoff_names_the_skipped_sessions_thread() {
    let harness = harness(0, 0);
    harness.show("completed", "null", 0, "Started.");
    harness.command().assert().success();
    assert!(harness.thread_saved());
    let sessions = harness.fixtures.get_temp_path().join("harnessmux/state/sessions");
    let path = std::fs::read_dir(&sessions)
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .find(|path| path.to_string_lossy().ends_with("-TEST-1-fixer.json"))
        .expect("the fixer session");
    let mut record: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
    let at = Timestamp::now() - SignedDuration::from_mins(56);
    record["last_used"] = serde_json::Value::String(at.strftime("%Y-%m-%dT%H:%M:%S%.3fZ").to_string());
    std::fs::write(&path, record.to_string()).unwrap();

    harness.command().args(["--session", "continue"]).assert().success();
    let prompts = std::fs::read_to_string(harness.dir.join("prompts")).unwrap_or_default();
    let handoff = prompts.split("Handoff:").nth(1).unwrap_or_default();
    assert!(handoff.contains("- Its T3 Code thread: t1\n"), "{prompts}");
    assert_eq!(harness.calls().matches("thread start").count(), 2, "{}", harness.calls());
}

#[test]
fn a_new_thread_is_named_id_and_title_without_thread_name() {
    let harness = harness(0, 0);
    harness.show("completed", "null", 0, "Started.");
    harness.command().assert().success();
    assert!(harness.args().contains("--title TEST-1 Thread job "), "{}", harness.args());
}

#[test]
fn a_new_thread_takes_the_profiles_thread_name_with_the_tickets_templates_filled_in() {
    let harness = harness(0, 0);
    harness.show("completed", "null", 0, "Started.");
    let mut cmd = crate::common::lotar_cmd().unwrap();
    cmd.current_dir(harness.fixtures.get_temp_path()).args(["agent", "run", "TEST-1", "Fix it", "--agent", "named", "--wait"]);
    cmd.assert().success();
    assert!(harness.args().contains("--title TEST-1 fix: Thread job "), "{}", harness.args());
}

#[test]
fn a_thread_that_waits_for_an_answer_hands_the_ticket_back_with_its_link() {
    let harness = harness(2, 0);
    harness.show("running", "\"r1\"", 1, "Which one?");
    harness.command().output().unwrap();
    let task = harness.task();
    assert_eq!(task.status.as_str(), "HelpNeeded");
    assert_eq!(task.assignee.as_deref(), Some("owner"));
    let message = format!("The t3 session waits for an answer: {LINK}");
    assert!(task.comments.iter().any(|comment| comment.text == message), "{:?}", task.comments);
    assert!(harness.linked());
}

#[test]
fn a_taken_over_t3_job_ends_with_the_threads_run_without_a_second_turn() {
    let (harness, job) = take_over(false, false);
    assert_eq!(job.status, "completed", "{job:?}");
    assert_eq!(job.summary.as_deref(), Some("Finished."));
    assert!(harness.linked());
}

#[test]
fn a_t3_job_taken_over_before_its_thread_id_was_saved_ends_with_that_thread() {
    let (harness, job) = take_over(true, false);
    assert_eq!(job.status, "completed", "{job:?}");
    assert_eq!(job.summary.as_deref(), Some("Finished."));
    assert!(harness.linked());
}

#[test]
fn a_t3_job_taken_over_before_its_thread_id_was_saved_fails_when_t3_cant_list_its_threads() {
    let (_harness, job) = take_over(true, true);
    assert_eq!(job.status, "failed", "{job:?}");
    let reason = job.summary.or(job.last_message).unwrap_or_default();
    assert!(reason.contains("can't tell whether that turn made a thread"), "{reason}");
}

/// Kill lotar during a t3 job's turn, finish the thread's run and take the job over; the job as it ended. No second
/// thread or turn either way. `forget_thread`: lotar died before harnessmux saved the thread's id; `list_fails`: T3
/// can't list its threads then.
fn take_over(forget_thread: bool, list_fails: bool) -> (Harness, lotar::api_types::AgentJob) {
    let harness = harness(0, 30);
    harness.show("running", "\"r1\"", 0, "Working.");
    let mut lotar = std::process::Command::new(crate::common::lotar_bin());
    lotar
        .env_remove("RUST_TEST_THREADS")
        .env("LOTAR_IGNORE_HOME_CONFIG", "1")
        .current_dir(harness.fixtures.get_temp_path())
        .args(["agent", "run", "TEST-1", "Fix it", "--agent", "fixer", "--wait"])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null());
    let mut lotar = lotar.spawn().unwrap();
    let tasks_dir = harness.fixtures.tasks_root.clone();
    let session_known = || lotar::services::agent_active_jobs::for_ticket(&tasks_dir, "TEST-1").is_some_and(|job| job.session_id.is_some());
    wait_for("the thread to start", || harness.thread_saved() && session_known());
    lotar.kill().unwrap();
    lotar.wait().unwrap();
    if forget_thread {
        harness.forget_thread();
    }
    if list_fails {
        std::fs::write(harness.dir.join("list-fail"), "").unwrap();
    }

    harness.show("completed", "null", 0, "Finished.");
    AgentJobService::recover_orphaned_jobs(&tasks_dir);
    let job_id = lotar::services::agent_active_jobs::list(&tasks_dir).first().map(|job| job.job_id.clone());
    let job_id = job_id.unwrap_or_default();
    // Settled: its worker has also dropped the job's queue files, which the process scratch cleanup removes (DEV-648).
    let ended = || AgentJobService::job_settled(&job_id) && AgentJobService::get_job(&job_id).is_some_and(|job| job.status != "running");
    let deadline = Instant::now() + Duration::from_secs(60);
    while !ended() {
        // DEV-670: a rare timeout here didn't recur; the job's state tells where it stopped next time.
        assert!(
            Instant::now() < deadline,
            "timed out waiting for job {job_id:?} to end: {:?}\nevents: {:#?}\nt3 calls:\n{}",
            AgentJobService::get_job(&job_id),
            AgentJobService::events_for(&job_id),
            harness.calls()
        );
        std::thread::sleep(Duration::from_millis(25));
    }
    let job = AgentJobService::get_job(&job_id).unwrap();
    assert!(!harness.calls().contains("thread send"), "{}", harness.calls());
    assert_eq!(harness.calls().matches("thread start").count(), 1, "{}", harness.calls());
    (harness, job)
}

/// DEV-593: lotar killed after `agent_job_started`, while a `job_started` rule runs and before the session starts,
/// leaves the job to the next worker or server, which fails it with that reason and runs its `job_failed` rules
/// once; `agent status` and `list-jobs` show it failed.
#[test]
fn a_t3_job_killed_before_its_session_started_fails_once_taken_over() {
    let harness = harness(0, 0);
    let _trust = EnvVarGuard::set("LOTAR_TRUST_ALL", "1");
    let (started, failed) = (harness.dir.join("rule-started"), harness.dir.join("rule-failed"));
    std::fs::write(
        harness.fixtures.tasks_root.join("automation.yml"),
        format!(
            "automation:\n  rules:\n    - name: Hold\n      on:\n        job_started:\n          run: \"sh -c 'touch {}; sleep 30'\"\n        job_failed:\n          run: \"sh -c 'echo x >> {}'\"\n",
            started.display(),
            failed.display()
        ),
    )
    .unwrap();
    let mut lotar = std::process::Command::new(crate::common::lotar_bin());
    lotar
        .env_remove("RUST_TEST_THREADS")
        .env("LOTAR_IGNORE_HOME_CONFIG", "1")
        .current_dir(harness.fixtures.get_temp_path())
        .args(["agent", "run", "TEST-1", "Fix it", "--agent", "fixer", "--wait"])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null());
    let mut lotar = lotar.spawn().unwrap();
    wait_for("the job_started rule", || started.exists());
    lotar.kill().unwrap();
    lotar.wait().unwrap();

    let tasks_dir = harness.fixtures.tasks_root.clone();
    let orphans = lotar::services::agent_active_jobs::orphaned(&tasks_dir);
    assert_eq!(orphans.len(), 1, "the killed job leaves its record");
    assert_eq!(orphans[0].session_id, None, "it names no session it never started");
    let job_id = orphans[0].job_id.clone();
    AgentJobService::recover_orphaned_jobs(&tasks_dir);
    wait_for("the job to end", || AgentJobService::job_settled(&job_id) && AgentJobService::get_job(&job_id).is_some_and(|job| job.status != "running"));
    let job = AgentJobService::get_job(&job_id).unwrap();
    assert_eq!(job.status, "failed", "{job:?}");
    let reason = job.summary.or(job.last_message).unwrap_or_default();
    assert!(reason.contains("exited before the job ended"), "{reason}");
    wait_for("the job_failed rule", || failed.exists());
    AgentJobService::recover_orphaned_jobs(&tasks_dir);
    assert_eq!(std::fs::read_to_string(&failed).unwrap_or_default(), "x\n", "the job_failed rule ran once");
    assert!(!harness.calls().contains("thread"), "no session was started or resumed: {}", harness.calls());

    let cli = |args: &[&str]| {
        let output = crate::common::lotar_cmd().unwrap().current_dir(harness.fixtures.get_temp_path()).args(args).output().unwrap();
        String::from_utf8_lossy(&output.stdout).to_string()
    };
    let status = cli(&["--format", "json", "agent", "status", &job_id]);
    assert!(status.contains("\"status\":\"failed\"") && status.contains("exited before the job ended"), "{status}");
    let jobs = cli(&["--format", "json", "agent", "list-jobs"]);
    assert!(jobs.contains("\"status\":\"failed\"") && jobs.contains("exited before the job ended"), "{jobs}");
}

/// Turns on agent worktrees and makes the fixture a git repository, so a job sets up its worktree first.
fn with_worktrees(harness: &Harness) {
    let root = harness.fixtures.get_temp_path();
    let config = harness.fixtures.tasks_root.join("config.yml");
    let text = std::fs::read_to_string(&config).unwrap();
    std::fs::write(&config, format!("{text}agent:\n  worktree:\n    enabled: true\n")).unwrap();
    for args in [
        &["init", "-q", "-b", "main"][..],
        &["config", "user.name", "Setup"],
        &["config", "user.email", "setup@example.invalid"],
        &["config", "commit.gpgsign", "false"],
        &["add", "."],
        &["commit", "-qm", "init"],
    ] {
        assert!(std::process::Command::new("git").args(args).current_dir(root).status().unwrap().success(), "git {args:?}");
    }
}

/// DEV-598: lotar killed while it sets up the job's worktree (before the job log and `agent_job_started`) leaves
/// the job's record, so the next worker or server fails the job with a reason that names the setup and runs its
/// `job_failed` rules once; `agent status` and `list-jobs` show it failed. A `git` on PATH holds the setup.
#[test]
fn a_t3_job_killed_during_its_worktree_setup_fails_once_taken_over() {
    let harness = harness(0, 0);
    with_worktrees(&harness);
    let _trust = EnvVarGuard::set("LOTAR_TRUST_ALL", "1");
    let (setup, failed) = (harness.dir.join("setup"), harness.dir.join("rule-failed"));
    std::fs::write(
        harness.fixtures.tasks_root.join("automation.yml"),
        format!("automation:\n  rules:\n    - name: Retry\n      on:\n        job_failed:\n          run: \"sh -c 'echo x >> {}'\"\n", failed.display()),
    )
    .unwrap();
    let real_git = String::from_utf8(std::process::Command::new("sh").args(["-c", "command -v git"]).output().unwrap().stdout).unwrap();
    let bin = harness.dir.join("bin");
    std::fs::create_dir_all(&bin).unwrap();
    std::fs::write(
        bin.join("git"),
        format!(
            "#!/bin/sh\nif [ \"$*\" = \"symbolic-ref --quiet --short HEAD\" ]; then touch '{}'; sleep 30; fi\nexec '{}' \"$@\"\n",
            setup.display(),
            real_git.trim()
        ),
    )
    .unwrap();
    std::fs::set_permissions(bin.join("git"), std::fs::Permissions::from_mode(0o755)).unwrap();
    let path = format!("{}:{}", bin.display(), std::env::var("PATH").unwrap_or_default());
    let mut lotar = std::process::Command::new(crate::common::lotar_bin());
    lotar
        .env_remove("RUST_TEST_THREADS")
        .env("LOTAR_IGNORE_HOME_CONFIG", "1")
        .env("PATH", path)
        .current_dir(harness.fixtures.get_temp_path())
        .args(["agent", "run", "TEST-1", "Fix it", "--agent", "fixer", "--wait"])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null());
    let mut lotar = lotar.spawn().unwrap();
    wait_for("the worktree setup", || setup.exists());
    lotar.kill().unwrap();
    lotar.wait().unwrap();

    let tasks_dir = harness.fixtures.tasks_root.clone();
    let orphans = lotar::services::agent_active_jobs::orphaned(&tasks_dir);
    assert_eq!(orphans.len(), 1, "the killed job leaves its record");
    assert!(orphans[0].in_setup && orphans[0].session_id.is_none(), "{:?}", orphans[0]);
    let job_id = orphans[0].job_id.clone();
    AgentJobService::recover_orphaned_jobs(&tasks_dir);
    wait_for("the job to end", || AgentJobService::job_settled(&job_id) && AgentJobService::get_job(&job_id).is_some_and(|job| job.status != "running"));
    let job = AgentJobService::get_job(&job_id).unwrap();
    assert_eq!(job.status, "failed", "{job:?}");
    let reason = job.summary.or(job.last_message).unwrap_or_default();
    assert!(reason.contains("exited while it set up the job's worktree"), "{reason}");
    wait_for("the job_failed rule", || failed.exists());
    AgentJobService::recover_orphaned_jobs(&tasks_dir);
    assert_eq!(std::fs::read_to_string(&failed).unwrap_or_default(), "x\n", "the job_failed rule ran once");
    assert!(!harness.calls().contains("thread"), "no session was started or resumed: {}", harness.calls());

    let cli = |args: &[&str]| {
        let output = crate::common::lotar_cmd().unwrap().current_dir(harness.fixtures.get_temp_path()).args(args).output().unwrap();
        String::from_utf8_lossy(&output.stdout).to_string()
    };
    let status = cli(&["--format", "json", "agent", "status", &job_id]);
    assert!(status.contains("\"status\":\"failed\"") && status.contains("set up the job's worktree"), "{status}");
    let jobs = cli(&["--format", "json", "agent", "list-jobs"]);
    assert!(jobs.contains("\"status\":\"failed\"") && jobs.contains("set up the job's worktree"), "{jobs}");
}

/// DEV-598: a worktree setup that fails (here: no git repository) leaves a job log, so `agent status` shows why.
#[test]
fn a_job_whose_worktree_setup_fails_logs_the_failure() {
    let harness = harness(0, 0);
    let config = harness.fixtures.tasks_root.join("config.yml");
    let text = std::fs::read_to_string(&config).unwrap();
    std::fs::write(&config, format!("{text}agent:\n  worktree:\n    enabled: true\n")).unwrap();
    harness.command().output().unwrap();
    let cli = |args: &[&str]| {
        let output = crate::common::lotar_cmd().unwrap().current_dir(harness.fixtures.get_temp_path()).args(args).output().unwrap();
        String::from_utf8_lossy(&output.stdout).to_string()
    };
    let jobs = cli(&["--format", "json", "agent", "list-jobs"]);
    assert!(jobs.contains("\"status\":\"failed\"") && jobs.contains("Worktree setup failed"), "{jobs}");
    assert!(!harness.calls().contains("thread"), "no session was started: {}", harness.calls());
}

/// DEV-618: a job whose profile sets `tools.disallowed` and `env` gets them into its thread's new worktree through
/// T3 Code's worktree script, its log says so, and no commit takes the file.
#[test]
fn a_t3_job_gives_its_thread_worktree_the_profiles_tools_and_env() {
    let harness = harness(0, 0);
    let config = harness.fixtures.tasks_root.join("config.yml");
    let text = std::fs::read_to_string(&config)
        .unwrap()
        .replace("    model: m\n", "    model: m\n    tools:\n      disallowed: [Artifact]\n    env:\n      FOO: bar\n");
    std::fs::write(&config, text).unwrap();
    with_worktrees(&harness);
    std::fs::write(harness.dir.join("setup"), crate::common::lotar_bin().to_string_lossy().as_ref()).unwrap();
    harness.show("completed", "null", 0, "Started.");
    harness.command().assert().success();

    let settings = std::fs::read_to_string(harness.dir.join("worktree/.claude/settings.local.json"))
        .unwrap_or_else(|err| panic!("{err}: {}", std::fs::read_to_string(harness.dir.join("setup-out")).unwrap_or_default()));
    let settings: serde_json::Value = serde_json::from_str(&settings).unwrap();
    assert_eq!(settings, serde_json::json!({"permissions": {"deny": ["Artifact"]}, "env": {"FOO": "bar"}}));
    let cli = |args: &[&str]| {
        let output = crate::common::lotar_cmd().unwrap().current_dir(harness.fixtures.get_temp_path()).args(args).output().unwrap();
        String::from_utf8_lossy(&output.stdout).to_string()
    };
    let jobs: serde_json::Value = serde_json::from_str(&cli(&["--format", "json", "agent", "list-jobs"])).unwrap();
    let id = jobs["jobs"][0]["id"].as_str().or_else(|| jobs[0]["id"].as_str()).unwrap_or_else(|| panic!("{jobs}")).to_string();
    let logs = cli(&["agent", "logs", &id]);
    assert!(logs.contains("Wrote .claude/settings.local.json"), "{logs}");
    let root = harness.fixtures.get_temp_path();
    let committed = std::process::Command::new("git").args(["log", "--name-only", "--format=", "main..agent/TEST-1"]).current_dir(root).output().unwrap();
    assert!(!String::from_utf8_lossy(&committed.stdout).contains(".claude"), "{}", String::from_utf8_lossy(&committed.stdout));
}

/// DEV-618: without the worktree script's `lotar agent worktree setup` the job says so; a profile without
/// `tools.disallowed` or `env` leaves no record and no note.
#[test]
fn a_t3_job_says_when_the_worktree_script_left_out_the_setup() {
    let harness = harness(0, 0);
    let config = harness.fixtures.tasks_root.join("config.yml");
    let text = std::fs::read_to_string(&config).unwrap().replace("    model: m\n", "    model: m\n    tools:\n      disallowed: [Artifact]\n");
    std::fs::write(&config, text).unwrap();
    with_worktrees(&harness);
    harness.show("completed", "null", 0, "Started.");
    harness.command().assert().success();
    let cli = |args: &[&str]| {
        let output = crate::common::lotar_cmd().unwrap().current_dir(harness.fixtures.get_temp_path()).args(args).output().unwrap();
        String::from_utf8_lossy(&output.stdout).to_string()
    };
    let jobs: serde_json::Value = serde_json::from_str(&cli(&["--format", "json", "agent", "list-jobs"])).unwrap();
    let id = jobs["jobs"][0]["id"].as_str().or_else(|| jobs[0]["id"].as_str()).unwrap_or_else(|| panic!("{jobs}")).to_string();
    let logs = cli(&["agent", "logs", &id]);
    assert!(logs.contains("doesn't run `lotar agent worktree setup`"), "{logs}");
    assert!(!harness.fixtures.get_temp_path().join(".git/lotar/t3-worktree-setup").read_dir().unwrap().any(|_| true), "the record is gone");
}

/// Starts a job as profile `named` in a new thread, then continues that thread in this process; returns the
/// continued job's progress notes.
fn continue_named(harness: &Harness) -> Vec<String> {
    harness.show("completed", "null", 0, "Started.");
    let mut cmd = crate::common::lotar_cmd().unwrap();
    cmd.current_dir(harness.fixtures.get_temp_path()).args(["agent", "run", "TEST-1", "Fix it", "--agent", "named", "--wait"]);
    cmd.assert().success();
    AgentJobService::set_orchestrator_mode(AgentOrchestratorMode::Server);
    let req = AgentJobCreateRequest {
        ticket_id: "TEST-1".into(),
        prompt: "Merge it".into(),
        agent: Some("named".into()),
        thread_name: Some("${{ticket.id}} merge: ${{ticket.title}}".into()),
        session: Some(lotar::config::types::AgentSession::Continue),
        ..Default::default()
    };
    let job = AgentJobService::start_job_with_tasks_dir(req, harness.fixtures.tasks_root.as_path()).unwrap().id;
    wait_for("the job's end", || AgentJobService::get_job(&job).is_some_and(|job| job.status == "completed"));
    AgentJobService::events_for(&job).into_iter().filter(|event| event.kind == "agent_job_progress").filter_map(|event| event.message).collect()
}

/// DEV-619: a continued thread is renamed to the run's thread_name before its turn is sent, with a session token that
/// lotar revokes afterwards.
#[test]
fn a_continued_thread_is_renamed_before_its_turn() {
    let harness = harness(0, 0);
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let home = harness.fixtures.get_temp_path().join("t3home/userdata");
    std::fs::create_dir_all(&home).unwrap();
    std::fs::write(home.join("server-runtime.json"), format!(r#"{{"origin":"http://{}"}}"#, listener.local_addr().unwrap())).unwrap();
    let calls_before_rename = harness.dir.join("calls");
    let server = std::thread::spawn(move || {
        let (stream, _) = listener.accept().unwrap();
        let calls = std::fs::read_to_string(calls_before_rename).unwrap_or_default();
        let mut socket = tungstenite::accept(stream).unwrap();
        let request: serde_json::Value = match socket.read().unwrap() {
            tungstenite::Message::Text(text) => serde_json::from_str(text.as_str()).unwrap(),
            other => panic!("expected the rename request, got {other:?}"),
        };
        let answer = serde_json::json!({"_tag": "Exit", "requestId": request["id"], "exit": {"_tag": "Success", "value": {"sequence": 1}}});
        socket.send(tungstenite::Message::text(answer.to_string())).unwrap();
        let _ = socket.read();
        (request, calls)
    });
    let notes = continue_named(&harness);
    let (request, calls) = server.join().unwrap();
    assert_eq!(request["payload"]["type"], "thread.metadata.update");
    assert_eq!(request["payload"]["threadId"], "t1");
    assert_eq!(request["payload"]["title"], "TEST-1 merge: Thread job");
    assert_eq!(calls.matches("thread send").count(), 0, "the rename comes before the continued turn: {calls}");
    assert!(notes.iter().any(|note| note == "Renamed the T3 Code thread t1 to \"TEST-1 merge: Thread job\"."), "{notes:?}");
    assert!(harness.args().contains("auth session revoke s1"), "{}", harness.args());
    assert!(harness.calls().contains("thread send"), "{}", harness.calls());
}

/// DEV-619: a rename that can't reach T3 Code's server is logged and the job runs on.
#[test]
fn a_continued_thread_that_cant_be_renamed_runs_on() {
    let harness = harness(0, 0);
    let notes = continue_named(&harness);
    let failed = "Could not rename the continued T3 Code thread to \"TEST-1 merge: Thread job\", so it keeps its name: T3 Code's server isn't running";
    assert!(notes.iter().any(|note| note.starts_with(failed)), "{notes:?}");
    assert!(harness.calls().contains("thread send"), "{}", harness.calls());
}

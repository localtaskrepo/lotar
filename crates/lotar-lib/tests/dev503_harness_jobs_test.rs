//! DEV-503: every runner but `command` runs as a harnessmux session. A shell script stands in for the agent's ACP
//! server (harnessmux's `agents.toml` under a scratch `HARNESSMUX_HOME` points the claude agent at it).
#![cfg(unix)]

use crate::common::TestFixtures;
use crate::common::env_mutex::EnvVarGuard;
use lotar::api_types::{AgentJobCreateRequest, TaskCreate};
use lotar::services::agent_job_service::{AgentJobService, AgentOrchestratorMode};
use lotar::services::task_service::TaskService;
use predicates::prelude::*;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

/// An ACP agent on stdio: it answers initialize, session/new, resume and load, and answers each prompt with
/// `$FAKE_REPLY` and a turn number, after a tool call with `$FAKE_TOOL`. It logs every line it reads to `$FAKE_LOG`. `$FAKE_ASK` asks for permission first
/// and says which option it got; `$FAKE_HOLD` (a file it creates) keeps the first prompt running until a cancel;
/// `$FAKE_SLOW` sleeps that many seconds before it answers; `$FAKE_RUN` is a shell command it runs in the session's
/// working directory before it answers.
const FAKE_ACP_AGENT: &str = r#"#!/bin/sh
turn=0
pending=""
respond() { printf '{"jsonrpc":"2.0","id":%s,"result":%s}\n' "$1" "$2"; }
reply() {
  turn=$((turn + 1))
  if [ -n "$FAKE_TOOL" ]; then
    printf '{"jsonrpc":"2.0","method":"session/update","params":{"sessionId":"fake-1","update":{"sessionUpdate":"tool_call","toolCallId":"t%s","title":"Read %s/notes.md","kind":"read","status":"pending"}}}\n' "$turn" "$cwd"
  fi
  printf '{"jsonrpc":"2.0","method":"session/update","params":{"sessionId":"fake-1","update":{"sessionUpdate":"agent_message_chunk","content":{"type":"text","text":"%s %s"}}}}\n' "${FAKE_REPLY:-Done}" "$turn"
  respond "$1" '{"stopReason":"end_turn","usage":{"inputTokens":10,"outputTokens":5,"cachedReadTokens":2,"totalTokens":17}}'
}
while IFS= read -r line; do
  printf '%s\n' "$line" >> "$FAKE_LOG"
  id=$(printf '%s' "$line" | sed -nE 's/.*"id":("[^"]*"|[0-9]+).*/\1/p')
  case "$line" in
    *'"method":"initialize"'*)
      respond "$id" '{"protocolVersion":1,"agentCapabilities":{"loadSession":true,"sessionCapabilities":{"resume":{}}},"agentInfo":{"name":"fake-acp","version":"1.0.0"},"authMethods":[]}' ;;
    *'"method":"session/new"'*)
      cwd=$(printf '%s' "$line" | sed -nE 's/.*"cwd":"([^"]*)".*/\1/p')
      respond "$id" '{"sessionId":"fake-1"}' ;;
    *'"method":"session/resume"'*|*'"method":"session/load"'*)
      cwd=$(printf '%s' "$line" | sed -nE 's/.*"cwd":"([^"]*)".*/\1/p')
      respond "$id" '{}' ;;
    *'"method":"session/cancel"'*)
      if [ -n "$pending" ]; then respond "$pending" '{"stopReason":"cancelled"}'; pending=""; fi ;;
    *'"method":"session/prompt"'*)
      if [ -n "$FAKE_HOLD" ] && [ ! -f "$FAKE_HOLD" ]; then
        pending=$id
        touch "$FAKE_HOLD"
        continue
      fi
      if [ -n "$FAKE_ASK" ]; then
        printf '{"jsonrpc":"2.0","id":900,"method":"session/request_permission","params":{"sessionId":"fake-1","toolCall":{"toolCallId":"edit-1","title":"Edit notes.md","kind":"edit"},"options":[{"optionId":"allow","name":"Allow","kind":"allow_once"},{"optionId":"reject","name":"Reject","kind":"reject_once"}]}}\n'
        IFS= read -r answer
        printf 'answer %s\n' "$answer" >> "$FAKE_LOG"
      fi
      if [ -n "$FAKE_SLOW" ]; then sleep "$FAKE_SLOW"; fi
      if [ -n "$FAKE_RUN" ]; then (cd "$cwd" && sh -c "$FAKE_RUN") >/dev/null 2>&1; fi
      reply "$id" ;;
    *'"method":'*) if [ -n "$id" ]; then respond "$id" '{}'; fi ;;
  esac
done
"#;

struct Harness {
    fixtures: TestFixtures,
    log: PathBuf,
    _env: Vec<EnvVarGuard>,
}

/// A TEST project with a `fixer` profile on the claude runner, run by the fake agent. `extra` adds profile keys
/// (four spaces deep), `env` the fake's settings.
fn harness(extra: &str, env: &[(&str, String)]) -> Harness {
    let fixtures = TestFixtures::new();
    let root = fixtures.get_temp_path().to_path_buf();
    let agent = root.join("fake-acp.sh");
    std::fs::write(&agent, FAKE_ACP_AGENT).unwrap();
    std::fs::set_permissions(&agent, std::fs::Permissions::from_mode(0o755)).unwrap();
    let home = root.join("harnessmux");
    std::fs::create_dir_all(home.join("config")).unwrap();
    std::fs::write(home.join("config/agents.toml"), format!("[agents.claude]\ncommand = [\"{}\"]\n", agent.display())).unwrap();
    let log = root.join("fake.log");
    let mut lines = format!("    env:\n      FAKE_LOG: \"{}\"\n", log.display());
    for (key, value) in env {
        lines.push_str(&format!("      {key}: \"{value}\"\n"));
    }
    fixtures.create_config_in_dir(&fixtures.tasks_root, &format!("default:\n  project: TEST\nagents:\n  fixer:\n    runner: claude\n{extra}{lines}"));
    let guards = vec![
        EnvVarGuard::set("HARNESSMUX_HOME", home.to_string_lossy().as_ref()),
        EnvVarGuard::set("LOTAR_TASKS_DIR", fixtures.tasks_root.to_string_lossy().as_ref()),
        EnvVarGuard::set("LOTAR_TEST_SILENT", "1"),
    ];
    TaskService::create(&mut fixtures.create_storage(), TaskCreate { title: "Session job".into(), project: Some("TEST".into()), ..Default::default() })
        .unwrap();
    Harness { fixtures, log, _env: guards }
}

impl Harness {
    fn root(&self) -> &Path {
        self.fixtures.get_temp_path()
    }

    fn log(&self) -> String {
        std::fs::read_to_string(&self.log).unwrap_or_default()
    }

    fn run(&self, extra: &[&str]) -> assert_cmd::assert::Assert {
        let mut cmd = crate::common::lotar_cmd().unwrap();
        cmd.current_dir(self.root()).args(["agent", "run", "TEST-1", "Fix it", "--agent", "fixer", "--wait"]).args(extra);
        cmd.assert()
    }

    fn start(&self) -> String {
        AgentJobService::set_orchestrator_mode(AgentOrchestratorMode::Server);
        let req = AgentJobCreateRequest { ticket_id: "TEST-1".into(), prompt: "Fix it".into(), agent: Some("fixer".into()), ..Default::default() };
        AgentJobService::start_job_with_tasks_dir(req, self.fixtures.tasks_root.as_path()).unwrap().id
    }
}

fn wait_for(what: &str, mut done: impl FnMut() -> bool) {
    // Ends as soon as it happens; the bound is for a busy machine.
    let deadline = Instant::now() + Duration::from_secs(60);
    while !done() {
        assert!(Instant::now() < deadline, "timed out waiting for {what}");
        std::thread::sleep(Duration::from_millis(25));
    }
}

fn status(job_id: &str) -> String {
    AgentJobService::get_job(job_id).map(|job| job.status).unwrap_or_default()
}

#[test]
fn a_job_runs_as_the_tickets_harnessmux_session_and_its_reply_is_the_summary() {
    let harness = harness("", &[("FAKE_REPLY", "Fixed it".into())]);
    harness.run(&[]).success().stdout(predicate::str::contains("Fixed it 1"));
    let log = harness.log();
    assert!(log.contains("\"method\":\"session/new\""), "{log}");
    assert!(log.contains("User request:"), "the job's prompt reaches the agent: {log}");
    let sessions = harness.root().join("harnessmux/state/sessions");
    let names: Vec<String> = std::fs::read_dir(&sessions).unwrap().map(|entry| entry.unwrap().file_name().to_string_lossy().to_string()).collect();
    assert!(names.iter().any(|name| name.contains("-TEST-1-fixer")), "named after repository, ticket and profile: {names:?}");
}

#[test]
fn session_continue_resumes_the_tickets_session_from_another_process() {
    let harness = harness("", &[]);
    harness.run(&[]).success();
    harness.run(&["--session", "continue"]).success().stdout(predicate::str::contains("Done 1"));
    let log = harness.log();
    assert_eq!(log.matches("\"method\":\"session/new\"").count(), 1, "{log}");
    assert!(log.contains("\"method\":\"session/resume\""), "{log}");
    harness.run(&[]).success();
    assert_eq!(harness.log().matches("\"method\":\"session/new\"").count(), 2, "without continue a job starts a new session");
}

/// The second review of DEV-503: session names are machine-wide, so two repositories with the same ticket and profile
/// must not share one. `continue` in the second checkout starts its own session.
#[test]
fn checkouts_with_the_same_ticket_and_profile_keep_their_own_sessions() {
    let first = harness("", &[]);
    first.run(&[]).success();
    // A second checkout with its own TEST-1 and fixer profile, on the same harnessmux home and fake agent.
    let second = TestFixtures::new();
    let log = second.get_temp_path().join("fake.log");
    second.create_config_in_dir(
        &second.tasks_root,
        &format!("default:\n  project: TEST\nagents:\n  fixer:\n    runner: claude\n    env:\n      FAKE_LOG: \"{}\"\n", log.display()),
    );
    let lotar = |args: &[&str]| {
        let mut cmd = crate::common::lotar_cmd().unwrap();
        cmd.current_dir(second.get_temp_path()).env("LOTAR_TASKS_DIR", &second.tasks_root).args(args);
        cmd.assert().success();
    };
    lotar(&["add", "Session job"]);
    lotar(&["agent", "run", "TEST-1", "Fix it", "--agent", "fixer", "--session", "continue", "--wait"]);
    let log = std::fs::read_to_string(&log).unwrap();
    assert!(log.contains("\"method\":\"session/new\"") && !log.contains("\"method\":\"session/resume\""), "{log}");
    let sessions = std::fs::read_dir(first.root().join("harnessmux/state/sessions"))
        .unwrap()
        .filter(|entry| entry.as_ref().unwrap().file_name().to_string_lossy().ends_with("-TEST-1-fixer.json"))
        .count();
    assert_eq!(sessions, 2, "one session per checkout");
}

#[test]
fn an_error_the_agent_sends_as_its_reply_fails_the_job() {
    let harness = harness("", &[("FAKE_REPLY", "Error: model_not_found".into())]);
    harness.run(&[]).failure();
}

#[test]
fn agent_profiles_take_no_command() {
    let harness = harness("    command: /opt/claude\n", &[]);
    harness.run(&[]).failure().stderr(predicate::str::contains("runner: command"));
}

#[test]
fn a_message_runs_as_the_next_turn_of_the_same_job() {
    let harness = harness("", &[("FAKE_SLOW", "1".into()), ("FAKE_TOOL", "1".into())]);
    let job = harness.start();
    wait_for("the first prompt", || harness.log().contains("\"method\":\"session/prompt\""));
    AgentJobService::send_message(&job, "pineapple").expect("message accepted");
    wait_for("the job's end", || status(&job) == "completed");
    let log = harness.log();
    assert_eq!(log.matches("\"method\":\"session/prompt\"").count(), 2, "{log}");
    assert!(log.contains("pineapple"));
    let job = AgentJobService::get_job(&job).unwrap();
    assert_eq!(job.last_message.as_deref(), Some("Done 2"));
    assert_eq!(job.usage.as_ref().and_then(|usage| usage.turns), Some(2));
    assert_eq!(job.usage.as_ref().and_then(|usage| usage.input_tokens), Some(24), "input counts cached tokens");
    let events = AgentJobService::events_for(&job.id);
    assert!(events.iter().any(|e| e.kind == "agent_job_input" && e.message.as_deref() == Some("pineapple")));
    let tools: Vec<_> = events.iter().filter(|e| e.kind == "agent_job_progress").filter_map(|e| e.message.clone()).collect();
    assert!(tools.iter().any(|message| message == "Tool: Read notes.md"), "tool paths are relative: {tools:?}");
}

#[test]
fn a_permission_request_is_a_question_in_the_job_until_it_is_answered() {
    let harness = harness("    on_permission: ask\n", &[("FAKE_ASK", "1".into())]);
    let job = harness.start();
    wait_for("the question", || AgentJobService::get_job(&job).is_some_and(|job| job.question.is_some()));
    let question = AgentJobService::get_job(&job).unwrap().question.unwrap();
    assert_eq!(question.title, "Edit notes.md");
    assert_eq!(question.options.iter().map(|option| option.id.as_str()).collect::<Vec<_>>(), ["allow", "reject"]);
    assert!(AgentJobService::answer_question(&job, &question.request, "maybe").is_err());
    AgentJobService::answer_question(&job, &question.request, "reject").unwrap();
    wait_for("the job's end", || status(&job) == "completed");
    assert!(harness.log().contains("\"optionId\":\"reject\""), "{}", harness.log());
    assert!(AgentJobService::get_job(&job).unwrap().question.is_none());
}

#[test]
fn cancelling_a_job_cancels_its_sessions_turn() {
    let hold = std::env::temp_dir().join(format!("lotar-dev503-hold-{}", std::process::id()));
    let _ = std::fs::remove_file(&hold);
    let harness = harness("", &[("FAKE_HOLD", hold.to_string_lossy().to_string())]);
    let job = harness.start();
    wait_for("the held prompt", || hold.exists());
    AgentJobService::cancel_job(&job).unwrap();
    wait_for("the cancel", || harness.log().contains("\"method\":\"session/cancel\""));
    assert_eq!(status(&job), "cancelled");
    wait_for("the job's end", || AgentJobService::job_settled(&job));
    let _ = std::fs::remove_file(&hold);
}

fn git(dir: &Path, args: &[&str]) {
    let status = std::process::Command::new("git").args(args).current_dir(dir).status().unwrap();
    assert!(status.success(), "git {args:?}");
}

/// The owner, 2026-11-14: an agent that continues a ticket another CLI worked on needs that work's context, as a T3
/// thread had it. A ticket goes fix (Claude) -> review (Codex, a new session) -> rework (Claude, session: continue)
/// -> merge (Claude, a new session), each step started by a rule. Every agent gets the earlier agents' replies in its
/// prompt and the other sessions' transcripts; the rework is the next turn of its own session.
#[test]
fn a_ticket_hands_its_context_from_harness_to_harness_through_its_states() {
    crate::common::require_git();
    let fixtures = TestFixtures::new();
    let root = fixtures.get_temp_path().to_path_buf();
    let agent = root.join("fake-acp.sh");
    std::fs::write(&agent, FAKE_ACP_AGENT).unwrap();
    std::fs::set_permissions(&agent, std::fs::Permissions::from_mode(0o755)).unwrap();
    let home = root.join("harnessmux");
    std::fs::create_dir_all(home.join("config")).unwrap();
    let command = format!("command = [\"{}\"]\n", agent.display());
    std::fs::write(home.join("config/agents.toml"), format!("[agents.claude]\n{command}[agents.codex]\n{command}")).unwrap();
    let log = |profile: &str| root.join(format!("{profile}.log"));
    let profile = |name: &str, runner: &str, reply: &str, run: &str| {
        format!(
            "  {name}:\n    runner: {runner}\n    env:\n      FAKE_LOG: \"{}\"\n      FAKE_REPLY: \"{reply}\"\n      FAKE_RUN: \"{run}\"\n",
            log(name).display()
        )
    };
    let commit = "echo fix > fix.txt && git add fix.txt && git -c commit.gpgsign=false commit -qm PIPE-1-fix";
    fixtures.create_config_in_dir(
        &fixtures.tasks_root,
        &format!(
            "default:\n  project: PIPE\nissue:\n  states: [Todo, InProgress, Review, Merging, Done]\nagent:\n  worktree:\n    enabled: true\n    dir: '{}'\nagents:\n{}{}{}",
            root.join("worktrees").display(),
            profile("fix", "claude", "Fixed: fix.txt holds the fix", commit),
            profile("review", "codex", "Review: keep fix.txt but say why", ""),
            profile("merge", "claude", "Merged", ""),
        ),
    );
    lotar::services::automation_service::AutomationService::set(
        &fixtures.tasks_root,
        None,
        r#"automation:
  rules:
    - name: Fixed
      when:
        all:
          - assignee: "@fix"
          - not: { tags: { any: [reviewed] } }
      on:
        job_completed:
          set: { status: Review, assignee: "@review" }
    - name: Reworked
      when:
        all:
          - assignee: "@fix"
          - tags: { any: [reviewed] }
      on:
        job_completed:
          set: { status: Merging, assignee: "@merge" }
    - name: Reviewed
      when:
        assignee: "@review"
      on:
        job_completed:
          set: { status: InProgress, assignee: "@fix" }
          add: { tags: [reviewed] }
          agent: { session: continue, prompt: "Rework after the review" }
    - name: Merged
      when:
        assignee: "@merge"
      on:
        job_completed:
          set: { status: Done, assignee: "@reporter" }
"#,
    )
    .expect("automation rules");
    git(&root, &["init", "-q", "-b", "main"]);
    git(&root, &["config", "user.name", "Pipeline"]);
    git(&root, &["config", "user.email", "pipeline@example.invalid"]);
    git(&root, &["add", "."]);
    git(&root, &["-c", "commit.gpgsign=false", "commit", "-qm", "init"]);
    let _env = [
        EnvVarGuard::set("HARNESSMUX_HOME", home.to_string_lossy().as_ref()),
        EnvVarGuard::set("LOTAR_TASKS_DIR", fixtures.tasks_root.to_string_lossy().as_ref()),
        EnvVarGuard::set("LOTAR_TEST_SILENT", "1"),
    ];
    AgentJobService::set_orchestrator_mode(AgentOrchestratorMode::Server);
    let mut storage = fixtures.create_storage();
    let task = TaskService::create(
        &mut storage,
        TaskCreate { title: "Pipeline".into(), project: Some("PIPE".into()), reporter: Some("owner".into()), ..Default::default() },
    )
    .unwrap();
    TaskService::update(&mut storage, &task.id, lotar::api_types::TaskUpdate { assignee: Some("@fix".into()), ..Default::default() }).unwrap();

    wait_for("the merged ticket", || TaskService::get(&storage, &task.id, None).is_ok_and(|task| task.status.to_string() == "Done"));
    let read = |profile: &str| std::fs::read_to_string(log(profile)).unwrap_or_default();
    let prompts = |profile: &str| read(profile).lines().filter(|line| line.contains("\"method\":\"session/prompt\"")).map(str::to_string).collect::<Vec<_>>();
    let (fix, review, merge) = (prompts("fix"), prompts("review"), prompts("merge"));
    assert_eq!((fix.len(), review.len(), merge.len()), (2, 1, 1), "fix twice, review and merge once");
    assert!(review[0].contains("Fixed: fix.txt holds the fix 1"), "the reviewer gets the fix agent's reply: {}", review[0]);
    assert!(review[0].contains("Earlier agent sessions on this ticket") && review[0].contains("-PIPE-1-fix (claude agent"), "and its session: {}", review[0]);
    assert_eq!(read("fix").matches("\"method\":\"session/new\"").count(), 1, "the rework is the next turn of the fix session");
    assert!(fix[1].contains("Review: keep fix.txt but say why 1"), "the rework gets the review: {}", fix[1]);
    assert!(
        merge[0].contains("Review: keep fix.txt but say why 1") && merge[0].contains("Fixed: fix.txt holds the fix 2"),
        "the merge agent gets both: {}",
        merge[0]
    );
    assert!(merge[0].contains("-PIPE-1-review (codex agent"), "and the review session: {}", merge[0]);
    assert!(root.join("fix.txt").is_file(), "the fix reached main");
}

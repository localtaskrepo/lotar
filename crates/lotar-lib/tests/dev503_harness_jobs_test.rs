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
/// working directory before it answers. `$FAKE_PREFACE` is a reply chunk it sends as soon as a prompt arrives;
/// `$FAKE_COST` the cost of each turn, reported as the session's total so far. It offers one config option,
/// `reasoning_effort` (`high` or `ultra`).
const FAKE_ACP_AGENT: &str = r#"#!/bin/sh
turn=0
pending=""
respond() { printf '{"jsonrpc":"2.0","id":%s,"result":%s}\n' "$1" "$2"; }
reply() {
  turn=$((turn + 1))
  if [ -n "$FAKE_COST" ]; then
    printf '{"jsonrpc":"2.0","method":"session/update","params":{"sessionId":"fake-1","update":{"sessionUpdate":"usage_update","used":10,"size":100,"cost":{"amount":%s,"currency":"USD"}}}}\n' "$(awk "BEGIN { print $turn * $FAKE_COST }")"
  fi
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
      respond "$id" '{"sessionId":"fake-1","configOptions":[{"id":"reasoning_effort","name":"Reasoning","type":"select","currentValue":"high","options":[{"value":"high","name":"High"},{"value":"ultra","name":"Ultra"}]}]}' ;;
    *'"method":"session/resume"'*|*'"method":"session/load"'*)
      cwd=$(printf '%s' "$line" | sed -nE 's/.*"cwd":"([^"]*)".*/\1/p')
      respond "$id" '{}' ;;
    *'"method":"session/set_config_option"'*)
      respond "$id" '{"configOptions":[{"id":"reasoning_effort","name":"Reasoning","type":"select","currentValue":"high","options":[{"value":"high","name":"High"},{"value":"ultra","name":"Ultra"}]}]}' ;;
    *'"method":"session/cancel"'*)
      if [ -n "$pending" ]; then respond "$pending" '{"stopReason":"cancelled"}'; pending=""; fi ;;
    *'"method":"session/prompt"'*)
      if [ -n "$FAKE_PREFACE" ]; then
        printf '{"jsonrpc":"2.0","method":"session/update","params":{"sessionId":"fake-1","update":{"sessionUpdate":"agent_message_chunk","content":{"type":"text","text":"%s"}}}}\n' "$FAKE_PREFACE"
      fi
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

/// DEV-512: a profile's `command` and `args` start its agent instead of the agent table's command.
#[test]
fn a_profile_command_starts_the_agent_with_its_args() {
    let scratch = tempfile::tempdir().unwrap();
    let own = scratch.path().join("own-agent.sh");
    let harness = harness(&format!("    command: {}\n    args: [\"from the profile\"]\n", own.display()), &[]);
    let fake = harness.root().join("fake-acp.sh");
    std::fs::write(&own, format!("#!/bin/sh\nFAKE_REPLY=\"Own agent, $1,\" exec \"{}\"\n", fake.display())).unwrap();
    std::fs::set_permissions(&own, std::fs::Permissions::from_mode(0o755)).unwrap();
    harness.run(&[]).success().stdout(predicate::str::contains("Own agent, from the profile, 1"));
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

/// DEV-511: a reply shows in the job log while the agent writes it, not only when its turn ends.
#[test]
fn a_reply_streams_into_the_job_log_as_the_agent_writes_it() {
    let hold = std::env::temp_dir().join(format!("lotar-dev511-hold-{}", std::process::id()));
    let _ = std::fs::remove_file(&hold);
    let held = harness("", &[("FAKE_PREFACE", "Looking at it.\\\\n\\\\n".into()), ("FAKE_HOLD", hold.to_string_lossy().to_string())]);
    let job = held.start();
    let messages =
        |job: &str| AgentJobService::events_for(job).into_iter().filter(|e| e.kind == "agent_job_message").filter_map(|e| e.message).collect::<Vec<_>>();
    wait_for("the first paragraph", || messages(&job) == ["Looking at it."]);
    assert_eq!(status(&job), "running", "the turn is still running");
    AgentJobService::cancel_job(&job).unwrap();
    wait_for("the job's end", || AgentJobService::job_settled(&job));
    let _ = std::fs::remove_file(&hold);
    drop(held);

    // A whole turn: the part before a tool streams, the rest comes with the end of the turn, nothing twice.
    let harness = harness("", &[("FAKE_PREFACE", "Looking at it.".into()), ("FAKE_TOOL", "1".into())]);
    let job = harness.start();
    wait_for("the job's end", || status(&job) == "completed");
    assert_eq!(messages(&job), ["Looking at it.", "Done 1"]);
    assert_eq!(AgentJobService::get_job(&job).unwrap().last_message.as_deref(), Some("Looking at it.Done 1"), "the whole reply");
}

/// DEV-511: agents report the session's cost so far; the job's cost is what it added, and a job that passes the
/// profile's max_budget_usd is cancelled.
#[test]
fn a_job_records_what_it_cost_and_stops_at_the_profiles_budget() {
    let two_turns = harness("", &[("FAKE_COST", "0.25".into()), ("FAKE_SLOW", "1".into())]);
    let job = two_turns.start();
    wait_for("the first prompt", || two_turns.log().contains("\"method\":\"session/prompt\""));
    AgentJobService::send_message(&job, "and the tests").expect("message accepted");
    wait_for("the job's end", || status(&job) == "completed");
    let usage = AgentJobService::get_job(&job).unwrap().usage.unwrap();
    assert_eq!((usage.turns, usage.cost_usd), (Some(2), Some(0.5)), "two turns of 0.25, not the totals 0.25 + 0.5");
    drop(two_turns);

    let harness = harness("    max_budget_usd: 0.4\n", &[("FAKE_COST", "0.5".into())]);
    let job = harness.start();
    wait_for("the job's failure", || status(&job) == "failed");
    let events = AgentJobService::events_for(&job);
    assert!(events.iter().any(|e| e.message.as_deref().is_some_and(|m| m.contains("$0.50, more than the profile's max_budget_usd of $0.40"))), "{events:?}");
}

/// DEV-511: `options` sets the agent's own config options, such as an effort outside low..max.
#[test]
fn profile_options_set_the_agents_own_config_options() {
    let ultra = harness("    options:\n      reasoning_effort: ultra\n", &[]);
    ultra.run(&[]).success();
    let log = ultra.log();
    let set = log.lines().find(|line| line.contains("\"method\":\"session/set_config_option\"")).unwrap_or_else(|| panic!("{log}"));
    assert!(set.contains("\"configId\":\"reasoning_effort\"") && set.contains("\"value\":\"ultra\""), "{set}");
    drop(ultra);
    let refused = harness("    options:\n      reasoning_effort: bogus\n", &[]);
    refused.run(&[]).failure().stdout(predicate::str::contains("bogus is not a value"));
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

/// `t3` as T3 Code's CLI: `thread start` runs each thread in a new worktree on a branch of its own (`t3/topic-<n>`),
/// made from the `--worktree` base, runs `$FAKE_RUN` there and logs `<base> <branch> <commit it started from>` to
/// `$FAKE_LOG`. `thread send thread-<n>` runs `$FAKE_RUN` in that thread's worktree and logs
/// `send thread-<n> <branch> <commit it started from>`. `thread show` answers with an empty thread.
const FAKE_T3: &str = r#"#!/bin/sh
if [ "$1 $2" = "thread send" ]; then
  cat > /dev/null
  tree="$FAKE_TREES/${3#thread-}"
  printf 'send %s %s %s\n' "$3" "$(git -C "$tree" symbolic-ref --short HEAD)" "$(git -C "$tree" rev-parse HEAD)" >> "$FAKE_LOG"
  (cd "$tree" && sh -c "$FAKE_RUN") >/dev/null 2>&1
  echo "Done in $3"
  exit 0
fi
[ "$1 $2" = "thread start" ] || { echo '{}'; exit 0; }
base=""
while [ $# -gt 0 ]; do
  if [ "$1" = "--worktree" ]; then base=$2; shift; fi
  shift
done
cat > /dev/null
mkdir -p "$FAKE_TREES"
n=$(ls "$FAKE_TREES" | wc -l | tr -d ' ')
tree="$FAKE_TREES/$n"
git worktree add -q -b "t3/topic-$n" "$tree" "$base" || exit 1
printf '%s t3/topic-%s %s\n' "$base" "$n" "$(git -C "$tree" rev-parse HEAD)" >> "$FAKE_LOG"
(cd "$tree" && sh -c "$FAKE_RUN") >/dev/null 2>&1
echo "Thread thread-$n" >&2
echo "Done in thread $n"
"#;

/// A repository whose TEST project runs the profiles `fix`, `rework` and `review` as fake T3 Code threads in worktrees.
fn t3_harness(rework_run: &str) -> (TestFixtures, Vec<EnvVarGuard>) {
    t3_harness_with(&[("fix", "echo fix > fix.txt && git add fix.txt && git commit -qm TEST-1-fix", ""), ("rework", rework_run, ""), ("review", "", "")])
}

/// A repository whose TEST project runs `profiles` (name, `$FAKE_RUN`, more profile keys four spaces deep) as fake
/// T3 Code threads in worktrees.
fn t3_harness_with(profiles: &[(&str, &str, &str)]) -> (TestFixtures, Vec<EnvVarGuard>) {
    crate::common::require_git();
    let fixtures = TestFixtures::new();
    let root = fixtures.get_temp_path().to_path_buf();
    let t3 = root.join("fake-t3.sh");
    std::fs::write(&t3, FAKE_T3).unwrap();
    std::fs::set_permissions(&t3, std::fs::Permissions::from_mode(0o755)).unwrap();
    let home = root.join("harnessmux");
    std::fs::create_dir_all(home.join("config")).unwrap();
    std::fs::write(home.join("config/agents.toml"), format!("[agents.t3]\ncommand = [\"{}\"]\n", t3.display())).unwrap();
    let profile = |(name, run, extra): &(&str, &str, &str)| {
        format!(
            "  {name}:\n    runner: t3\n{extra}    env:\n      FAKE_LOG: \"{}\"\n      FAKE_TREES: \"{}\"\n      FAKE_RUN: \"{run}\"\n",
            root.join("t3.log").display(),
            root.join("t3-trees").display()
        )
    };
    let agents: String = profiles.iter().map(profile).collect();
    fixtures.create_config_in_dir(&fixtures.tasks_root, &format!("default:\n  project: TEST\nagent:\n  worktree:\n    enabled: true\nagents:\n{agents}"));
    TaskService::create(&mut fixtures.create_storage(), TaskCreate { title: "Rework".into(), project: Some("TEST".into()), ..Default::default() }).unwrap();
    git(&root, &["init", "-q", "-b", "main"]);
    git(&root, &["config", "user.name", "Rework"]);
    git(&root, &["config", "user.email", "rework@example.invalid"]);
    git(&root, &["config", "commit.gpgsign", "false"]);
    git(&root, &["add", "."]);
    git(&root, &["commit", "-qm", "init"]);
    let guards = vec![
        EnvVarGuard::set("HARNESSMUX_HOME", home.to_string_lossy().as_ref()),
        EnvVarGuard::set("LOTAR_TASKS_DIR", fixtures.tasks_root.to_string_lossy().as_ref()),
        EnvVarGuard::set("LOTAR_TEST_SILENT", "1"),
    ];
    (fixtures, guards)
}

fn run_t3(fixtures: &TestFixtures, profile: &str) -> assert_cmd::assert::Assert {
    let mut cmd = crate::common::lotar_cmd().unwrap();
    cmd.current_dir(fixtures.get_temp_path()).args(["agent", "run", "TEST-1", "Work on it", "--agent", profile, "--wait"]);
    cmd.assert()
}

fn rev(dir: &Path, rev: &str) -> String {
    let output = std::process::Command::new("git").args(["rev-parse", rev]).current_dir(dir).output().unwrap();
    String::from_utf8_lossy(&output.stdout).trim().to_string()
}

/// DEV-468: T3 Code runs each thread on a branch of its own, so a rework thread's commits (here LoTaR's commit of
/// what the rework left uncommitted) missed `agent/TEST-1`, and the next `session: fresh` review read the first fix.
/// Each thread's branch now becomes the ticket's branch when its job ends.
#[test]
fn every_t3_thread_of_a_ticket_ends_on_and_starts_from_the_tickets_branch() {
    let (fixtures, _env) = t3_harness("echo rework > rework.txt");
    let root = fixtures.get_temp_path();
    run_t3(&fixtures, "fix").success();
    assert_eq!(rev(root, "agent/TEST-1"), rev(root, "t3/topic-0"), "the fix thread's commit is on the ticket's branch");
    run_t3(&fixtures, "rework").success();
    let rework = rev(root, "t3/topic-1");
    assert_eq!(rev(root, "agent/TEST-1"), rework, "so is LoTaR's commit of the rework");
    let tree = root.join("t3-trees/1");
    assert!(tree.join("rework.txt").is_file() && rev(&tree, "HEAD~1") == rev(root, "t3/topic-0"), "the rework was committed on the fix");
    run_t3(&fixtures, "review").success();
    let log = std::fs::read_to_string(root.join("t3.log")).unwrap();
    let review = log.lines().nth(2).unwrap_or_default();
    assert_eq!(review, format!("agent/TEST-1 t3/topic-2 {rework}"), "the fresh review starts from the rework: {log}");
}

/// A ticket branch that can't follow its thread (here it is checked out in the main checkout) fails the job and stays
/// where it was, so automation doesn't hand the ticket to a job that would read it.
#[test]
fn a_ticket_branch_that_cannot_follow_its_thread_fails_the_job() {
    let (fixtures, _env) = t3_harness("echo rework > rework.txt && git add rework.txt && git commit -qm TEST-1-rework");
    let root = fixtures.get_temp_path();
    run_t3(&fixtures, "fix").success();
    let fix = rev(root, "agent/TEST-1");
    git(root, &["checkout", "-q", "agent/TEST-1"]);
    run_t3(&fixtures, "rework")
        .failure()
        .stdout(predicate::str::contains("agent/TEST-1 stays where it is, not at t3/topic-1 of the T3 Code thread: git branch --force"));
    assert_eq!(rev(root, "agent/TEST-1"), fix, "the ticket's branch is unchanged");
    assert_ne!(rev(root, "t3/topic-1"), fix, "the rework stays on its thread's branch");
}

/// A `fix` thread that commits, then amends, and a `merge` thread that hands the ticket back; both continue their
/// threads.
fn amending_fix_and_merge() -> (TestFixtures, Vec<EnvVarGuard>) {
    t3_harness_with(&[
        (
            "fix",
            "if [ -f fix.txt ]; then echo again >> fix.txt && git commit -qa --amend --no-edit; else echo fix > fix.txt && git add fix.txt && git commit -qm TEST-1-fix; fi",
            "    session: continue\n",
        ),
        ("merge", "lotar assignee TEST-1 mallox", "    session: continue\n"),
    ])
}

/// DEV-550: a merge that continues an earlier merge thread ran on that thread's worktree branch, still at the fix the
/// rework had amended since. The continued rework recorded no worktree either, so the ticket's branch stayed at the
/// first fix. A continued thread's branch now starts at the ticket branch's commit, and its end moves the ticket
/// branch, amend included.
#[test]
fn a_continued_merge_thread_starts_from_the_rework_that_amended_the_ticket_branch() {
    let (fixtures, _env) = amending_fix_and_merge();
    let root = fixtures.get_temp_path();
    run_t3(&fixtures, "fix").success();
    let fix = rev(root, "agent/TEST-1");
    run_t3(&fixtures, "merge").success();
    assert_ne!(rev(root, "main"), fix, "the merge handed the ticket back and left main alone");
    run_t3(&fixtures, "fix").success();
    let rework = rev(root, "t3/topic-0");
    assert_ne!(rework, fix, "the rework amended the fix");
    assert_eq!(rev(root, "agent/TEST-1"), rework, "the ticket's branch follows the amended rework");
    run_t3(&fixtures, "merge").success();
    let log = std::fs::read_to_string(root.join("t3.log")).unwrap();
    let merge = log.lines().last().unwrap_or_default();
    assert_eq!(merge, format!("send thread-1 t3/topic-1 {rework}"), "the continued merge thread starts from the rework: {log}");
}

/// A continued thread whose branch holds a commit the ticket's branch lacks keeps it: the job starts a new thread on
/// the ticket's branch instead and says why.
#[test]
fn a_continued_thread_with_work_of_its_own_leaves_it_and_starts_a_new_thread() {
    let (fixtures, _env) = amending_fix_and_merge();
    let root = fixtures.get_temp_path();
    run_t3(&fixtures, "fix").success();
    run_t3(&fixtures, "merge").success();
    let tree = root.join("t3-trees/1");
    std::fs::write(tree.join("own.txt"), "own").unwrap();
    git(&tree, &["add", "own.txt"]);
    git(&tree, &["commit", "-qm", "own work"]);
    let own = rev(&tree, "HEAD");
    run_t3(&fixtures, "fix").success();
    let rework = rev(root, "agent/TEST-1");
    run_t3(&fixtures, "merge").success();
    let logs: String = std::fs::read_dir(fixtures.tasks_root.join("@agent-logs"))
        .unwrap()
        .map(|entry| std::fs::read_to_string(entry.unwrap().path()).unwrap_or_default())
        .collect();
    assert!(
        logs.contains("t3/topic-1 in the T3 Code thread's worktree") && logs.contains("this job starts a new T3 Code thread on agent/TEST-1"),
        "the job log says why: {logs}"
    );
    assert_eq!(rev(root, "t3/topic-1"), own, "the thread's own commit stays");
    let log = std::fs::read_to_string(root.join("t3.log")).unwrap();
    assert_eq!(log.lines().last().unwrap_or_default(), format!("agent/TEST-1 t3/topic-2 {rework}"), "a new thread starts from the rework: {log}");
}

/// A continued thread keeps the worktree LoTaR checked for it: one that appears elsewhere during its turn (another
/// job's thread) is not the job's, so LoTaR neither commits there nor points the ticket's branch at it.
#[test]
fn a_continued_thread_keeps_its_worktree_when_another_appears_during_its_turn() {
    let (fixtures, _env) = t3_harness_with(&[(
        "fix",
        "if [ -f fix.txt ]; then git worktree add -q -b other ../../other-tree && echo stray > ../../other-tree/stray.txt; else echo fix > fix.txt && git add fix.txt && git commit -qm TEST-1-fix; fi",
        "    session: continue\n",
    )]);
    let root = fixtures.get_temp_path();
    run_t3(&fixtures, "fix").success();
    let fix = rev(root, "agent/TEST-1");
    run_t3(&fixtures, "fix").success();
    assert!(root.join("other-tree/stray.txt").is_file(), "the other worktree appeared during the turn");
    assert_eq!(rev(root, "other"), fix, "LoTaR committed nothing in the other worktree");
    assert_eq!(rev(root, "agent/TEST-1"), fix, "the ticket's branch stays at the thread's work");
}

/// A session name outlives its thread: a job that starts a new one (`--session new`) replaces it under the same name.
/// When the latest job of the session (here as an older lotar logged it, without worktree) doesn't name the thread's
/// worktree, an earlier thread's worktree is not the continued thread's: the job starts a new thread on the ticket's
/// branch and leaves the earlier thread's branch alone.
#[test]
fn a_continued_thread_never_takes_the_worktree_of_the_thread_it_replaced() {
    let (fixtures, _env) = t3_harness_with(&[("fix", "echo work >> work.txt && git add work.txt && git commit -qm TEST-1-work", "    session: continue\n")]);
    let root = fixtures.get_temp_path();
    run_t3(&fixtures, "fix").success();
    let first = rev(root, "t3/topic-0");
    let mut replace = crate::common::lotar_cmd().unwrap();
    replace.current_dir(root).args(["agent", "run", "TEST-1", "Work on it", "--agent", "fix", "--session", "new", "--wait"]).assert().success();
    let logs = fixtures.tasks_root.join("@agent-logs");
    let mut files: Vec<_> =
        std::fs::read_dir(&logs).unwrap().map(|entry| entry.unwrap().path()).filter(|path| path.extension().is_some_and(|ext| ext == "jsonl")).collect();
    files.sort();
    let latest = files.last().unwrap();
    let older: String = std::fs::read_to_string(latest)
        .unwrap()
        .lines()
        .map(|line| {
            let mut value: serde_json::Value = serde_json::from_str(line).unwrap();
            if value["type"] == "status"
                && let Some(status) = value.as_object_mut()
            {
                for key in ["worktree_path", "worktree_branch", "ticket_commit", "agent_session"] {
                    status.remove(key);
                }
            }
            format!("{value}\n")
        })
        .collect();
    std::fs::write(latest, older).unwrap();
    run_t3(&fixtures, "fix").success();
    assert_eq!(rev(root, "t3/topic-0"), first, "the replaced thread's branch is unchanged");
    let log = std::fs::read_to_string(root.join("t3.log")).unwrap();
    let ticket = rev(root, "agent/TEST-1");
    assert_eq!(ticket, rev(root, "t3/topic-2"), "a new thread did the work, and the ticket's branch follows it: {log}");
    assert_eq!(rev(root, "t3/topic-2~1"), rev(root, "t3/topic-1"), "it started from the replacement thread's work: {log}");
}

/// `t3` whose server stops answering: `thread start` names thread-1 and exits 1 with T3's error, and `thread show`
/// fails the same way while `$FAKE_DOWN` holds a number above 0, then shows the thread completed.
const FAKE_T3_OUTAGE: &str = r#"#!/bin/sh
case "$1 $2" in
  "thread start") cat > /dev/null; echo "Thread thread-1" >&2; echo "Could not reach the T3 Code server at http://127.0.0.1:3773" >&2; exit 1 ;;
  "thread show")
    down=$(cat "$FAKE_DOWN" 2>/dev/null || echo 0)
    if [ "$down" -gt 0 ]; then
      echo $((down - 1)) > "$FAKE_DOWN"
      echo "Could not reach the T3 Code server at http://127.0.0.1:3773" >&2
      exit 1
    fi
    echo '{"thread":{"threadId":"thread-1","status":"completed","activeRunId":null,"latestRunId":"r1","pendingRequestCount":0},"messages":[{"role":"assistant","runId":"r1","text":"Fixed while the server was down"}]}' ;;
esac
"#;

/// DEV-467: jobs whose thread finished while the T3 Code server didn't answer failed, so their end rules never ran.
/// The job now follows the thread until the server answers and ends from it.
#[test]
fn a_t3_job_that_loses_the_server_completes_from_its_thread_and_runs_its_end_rules() {
    let fixtures = TestFixtures::new();
    let root = fixtures.get_temp_path().to_path_buf();
    let t3 = root.join("fake-t3.sh");
    std::fs::write(&t3, FAKE_T3_OUTAGE).unwrap();
    std::fs::set_permissions(&t3, std::fs::Permissions::from_mode(0o755)).unwrap();
    let home = root.join("harnessmux");
    std::fs::create_dir_all(home.join("config")).unwrap();
    std::fs::write(home.join("config/agents.toml"), format!("[agents.t3]\ncommand = [\"{}\"]\n", t3.display())).unwrap();
    let down = root.join("down");
    std::fs::write(&down, "2").unwrap();
    fixtures.create_config_in_dir(
        &fixtures.tasks_root,
        &format!("default:\n  project: TEST\nagents:\n  fix:\n    runner: t3\n    env:\n      FAKE_DOWN: \"{}\"\n", down.display()),
    );
    std::fs::write(
        fixtures.tasks_root.join("automation.yml"),
        "automation:\n  rules:\n    - name: Finish\n      on:\n        job_completed:\n          set:\n            status: Done\n",
    )
    .unwrap();
    TaskService::create(&mut fixtures.create_storage(), TaskCreate { title: "Outage".into(), project: Some("TEST".into()), ..Default::default() }).unwrap();
    let _env = [
        EnvVarGuard::set("HARNESSMUX_HOME", home.to_string_lossy().as_ref()),
        EnvVarGuard::set("LOTAR_TASKS_DIR", fixtures.tasks_root.to_string_lossy().as_ref()),
        EnvVarGuard::set("LOTAR_TEST_SILENT", "1"),
    ];
    run_t3(&fixtures, "fix").success().stdout(predicate::str::contains("Fixed while the server was down"));
    assert_eq!(std::fs::read_to_string(&down).unwrap().trim(), "0", "the job read the thread through the outage");
    let task = TaskService::get(&fixtures.create_storage(), "TEST-1", None).unwrap();
    assert_eq!(task.status.as_str(), "Done", "the job_completed rule ran");
}

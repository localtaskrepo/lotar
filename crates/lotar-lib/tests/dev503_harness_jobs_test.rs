//! DEV-503: every runner but `command` runs as a harnessmux session. A shell script stands in for the agent's ACP
//! server (harnessmux's `agents.toml` under a scratch `HARNESSMUX_HOME` points the claude agent at it).
#![cfg(unix)]
use jiff::SignedDuration;
use jiff::Timestamp;
use lotar::utils::time;

use crate::common::TestFixtures;
use crate::common::env_mutex::EnvVarGuard;
use lotar::api_types::{AgentJobCreateRequest, TaskCreate};
use lotar::services::agent_job_service::{AgentJobService, AgentOrchestratorMode};
use lotar::services::agent_queue_service::AgentQueueService;
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

impl Drop for Harness {
    fn drop(&mut self) {
        // A test that already failed only waits: a second panic while unwinding aborts the binary and hides its message.
        if std::thread::panicking() {
            settle(|| jobs_ended(&self.fixtures.tasks_root));
        } else {
            wait_for_jobs_to_end(&self.fixtures.tasks_root);
        }
    }
}

/// Wait until no job on the fixture runs, in this process or in a queue worker: a job still writes to the tasks folder
/// after its status reads completed, and a test that ends then has that folder made again after the fixture's removal.
fn wait_for_jobs_to_end(tasks_dir: &Path) {
    wait_for("the jobs' end", || jobs_ended(tasks_dir));
}

fn jobs_ended(tasks_dir: &Path) -> bool {
    AgentJobService::queue_stats().running == 0
        && lotar::services::agent_active_jobs::list(tasks_dir).is_empty()
        && !lotar::services::agent_queue_service::AgentQueueService::is_worker_running(tasks_dir)
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

fn wait_for(what: &str, done: impl FnMut() -> bool) {
    assert!(settle(done), "timed out waiting for {what}");
}

/// Whether `done` holds within the bound. It ends as soon as it happens; the bound is for a busy machine.
fn settle(mut done: impl FnMut() -> bool) -> bool {
    let deadline = Instant::now() + Duration::from_secs(60);
    while !done() {
        if Instant::now() >= deadline {
            return false;
        }
        std::thread::sleep(Duration::from_millis(25));
    }
    true
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

/// Sets `last_used` of the ticket's fixer session `minutes` ago, as if its last turn ended then.
fn age_session(harness: &Harness, minutes: i64) {
    let sessions = harness.root().join("harnessmux/state/sessions");
    let path = std::fs::read_dir(&sessions)
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .find(|path| path.to_string_lossy().ends_with("-TEST-1-fixer.json"))
        .expect("the fixer session");
    let mut record: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
    let at = Timestamp::now() - SignedDuration::from_mins(minutes);
    record["last_used"] = serde_json::Value::String(at.strftime("%Y-%m-%dT%H:%M:%S%.3fZ").to_string());
    std::fs::write(&path, serde_json::to_string_pretty(&record).unwrap()).unwrap();
}

/// Runs a `session: continue` job in this process and returns its job log's progress notes.
fn continue_job(harness: &Harness) -> Vec<String> {
    AgentJobService::set_orchestrator_mode(AgentOrchestratorMode::Server);
    let req = AgentJobCreateRequest {
        ticket_id: "TEST-1".into(),
        prompt: "Rework it".into(),
        agent: Some("fixer".into()),
        session: Some(lotar::config::types::AgentSession::Continue),
        ..Default::default()
    };
    let job = AgentJobService::start_job_with_tasks_dir(req, harness.fixtures.tasks_root.as_path()).unwrap().id;
    wait_for("the job's end", || status(&job) == "completed");
    // The ticket's active job ends after its status reads completed; the next job on it would be refused until then.
    wait_for_jobs_to_end(&harness.fixtures.tasks_root);
    AgentJobService::events_for(&job).into_iter().filter(|event| event.kind == "agent_job_progress").filter_map(|event| event.message).collect()
}

/// DEV-603: a session idle for less than `continue_within_minutes` (default 55) resumes, and the job log says so.
#[test]
fn session_continue_resumes_a_session_used_within_the_time() {
    let harness = harness("", &[]);
    harness.run(&[]).success();
    age_session(&harness, 50);
    let notes = continue_job(&harness);
    assert!(notes.iter().any(|note| note.contains("last used 50 minutes ago (within continue_within_minutes, 55)")), "{notes:?}");
    let log = harness.log();
    assert_eq!(log.matches("\"method\":\"session/new\"").count(), 1, "{log}");
    assert!(log.contains("\"method\":\"session/resume\"") && !log.contains("Handoff:"), "{log}");
}

/// The header of the job's log, where `lotar agent status`, the job list and the timeline read a job of another process.
fn log_header(harness: &Harness, job_id: &str) -> serde_json::Value {
    let logs = harness.fixtures.tasks_root.join("@agent-logs");
    let log = std::fs::read_dir(&logs)
        .unwrap()
        .map(|entry| std::fs::read_to_string(entry.unwrap().path()).unwrap_or_default())
        .find(|log| log.contains(job_id))
        .expect("the job's log");
    serde_json::from_str(log.lines().next().unwrap_or_default()).unwrap()
}

/// DEV-672: a merge that continued the reviewer's opus session showed the merge profile's sonnet. A job that resumes a
/// session shows the model and effort the session runs on; one that starts its own shows its profile's.
#[test]
fn a_continued_session_shows_the_model_and_effort_it_runs_on() {
    let harness = harness("    model: sonnet\n", &[]);
    harness.run(&["--model", "opus", "--effort", "high"]).success();
    let shown = |job: &str| AgentJobService::get_job(job).map(|job| (job.model, job.effort)).unwrap();
    let opus = (Some("opus".to_string()), Some("high".to_string()));
    AgentJobService::set_orchestrator_mode(AgentOrchestratorMode::Server);
    let req = AgentJobCreateRequest {
        ticket_id: "TEST-1".into(),
        prompt: "Merge it".into(),
        agent: Some("fixer".into()),
        session: Some(lotar::config::types::AgentSession::Continue),
        ..Default::default()
    };
    let continued = AgentJobService::start_job_with_tasks_dir(req, harness.fixtures.tasks_root.as_path()).unwrap().id;
    wait_for("the job's end", || status(&continued) == "completed");
    wait_for_jobs_to_end(&harness.fixtures.tasks_root);
    assert!(harness.log().contains("\"method\":\"session/resume\""), "{}", harness.log());
    assert_eq!(shown(&continued), opus, "the job shows the session's model, not its profile's");
    let header = log_header(&harness, &continued);
    assert_eq!((header["model"].as_str(), header["effort"].as_str()), (Some("opus"), Some("high")), "{header}");
    let own = harness.start();
    wait_for("the job's end", || status(&own) == "completed");
    assert_eq!(shown(&own), (Some("sonnet".to_string()), None), "a new session runs on the profile's model");
    assert_eq!(log_header(&harness, &own)["model"].as_str(), Some("sonnet"));
}

/// DEV-603: a session idle for longer than `continue_within_minutes` is not resumed: the job starts a new session and
/// its prompt carries a handoff with the ticket's newest comments.
#[test]
fn session_continue_starts_a_new_session_with_a_handoff_after_the_time() {
    let harness = harness("    continue_within_minutes: 30\n", &[]);
    harness.run(&[]).success();
    let mut storage = harness.fixtures.create_storage();
    TaskService::add_comment(&mut storage, "TEST-1", "Review: blocking, the test is missing").unwrap();
    age_session(&harness, 31);
    let notes = continue_job(&harness);
    let fresh = "last used 31 minutes ago, not within continue_within_minutes (30); this job starts a new one with a handoff";
    assert!(notes.iter().any(|note| note.contains(fresh)), "{notes:?}");
    let log = harness.log();
    assert_eq!(log.matches("\"method\":\"session/new\"").count(), 2, "{log}");
    assert!(!log.contains("\"method\":\"session/resume\""), "{log}");
    assert!(log.contains("Handoff:") && log.contains("Review: blocking, the test is missing"), "the handoff reaches the agent: {log}");
}

/// DEV-611: the new session takes the skipped one's name, so the skipped round's event log is kept under
/// `<name>.<last used>` first. The handoff names it, and the next job's earlier sessions list it.
#[test]
fn a_skipped_session_is_kept_and_the_handoff_names_its_event_log() {
    let harness = harness("    continue_within_minutes: 30\n", &[("FAKE_REPLY", "First round".into())]);
    harness.run(&[]).success();
    age_session(&harness, 31);
    continue_job(&harness);
    let sessions = harness.root().join("harnessmux/state/sessions");
    let kept = std::fs::read_dir(&sessions)
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .find(|path| path.to_string_lossy().strip_suffix(".events.jsonl").is_some_and(|name| name.contains("-TEST-1-fixer.")))
        .expect("the skipped session's event log is kept");
    assert!(std::fs::read_to_string(&kept).unwrap().contains("First round 1"), "the kept log holds the first round's reply");
    let kept_name = kept.file_name().unwrap().to_string_lossy().trim_end_matches(".events.jsonl").to_string();
    let record: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(sessions.join(format!("{kept_name}.json"))).unwrap()).unwrap();
    assert_eq!(record["state"], "ended", "nobody resumes a kept round: {record:#}");
    let kept_path = kept.display().to_string();
    let log = harness.log();
    assert!(
        log.contains("Handoff:")
            && log.contains(&format!(
                "Its event log, one JSON object per line (`message` events are the agent's replies, `tool.started` its tool calls): {kept_path}"
            )),
        "{log}"
    );

    // The next job, within the time, resumes the new session and lists the kept round among the earlier ones.
    continue_job(&harness);
    let log = harness.log();
    let last_prompt = log.lines().rfind(|line| line.contains("\"method\":\"session/prompt\"")).unwrap_or_default();
    assert!(last_prompt.contains("Earlier agent sessions on this ticket") && last_prompt.contains(&format!("{kept_name} (claude agent")), "{last_prompt}");
    assert!(!last_prompt.contains("Handoff:"), "{last_prompt}");
}

/// DEV-621: keeping the skipped session fails (its kept event log's name is taken by a folder), so the job fails and
/// starts no session, and the old session's record and event log stay where they are.
#[test]
fn a_failed_keep_fails_the_job_and_starts_no_session() {
    let harness = harness("    continue_within_minutes: 30\n", &[("FAKE_REPLY", "First round".into())]);
    harness.run(&[]).success();
    age_session(&harness, 31);
    let sessions = harness.root().join("harnessmux/state/sessions");
    let old = std::fs::read_dir(&sessions)
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .find(|path| path.to_string_lossy().ends_with("-TEST-1-fixer.json"))
        .expect("the fixer session");
    let record: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(&old).unwrap()).unwrap();
    let name = record["name"].as_str().unwrap().to_string();
    let stamp = time::parse_rfc3339(record["last_used"].as_str().unwrap()).unwrap().strftime("%Y%m%dT%H%M%SZ").to_string();
    let base: String = name.chars().take(64 - stamp.len() - 1).collect();
    std::fs::create_dir(sessions.join(format!("{base}.{stamp}.events.jsonl"))).unwrap();

    AgentJobService::set_orchestrator_mode(AgentOrchestratorMode::Server);
    let req = AgentJobCreateRequest {
        ticket_id: "TEST-1".into(),
        prompt: "Rework it".into(),
        agent: Some("fixer".into()),
        session: Some(lotar::config::types::AgentSession::Continue),
        ..Default::default()
    };
    let job = AgentJobService::start_job_with_tasks_dir(req, harness.fixtures.tasks_root.as_path()).unwrap().id;
    wait_for("the job's failure", || status(&job) == "failed");
    let message = AgentJobService::get_job(&job).and_then(|job| job.last_message).unwrap_or_default();
    assert!(message.starts_with(&format!("Could not keep the session {name} before a new one replaces it: ")), "{message}");
    assert_eq!(harness.log().matches("\"method\":\"session/new\"").count(), 1, "no new session starts");
    let records: Vec<PathBuf> =
        std::fs::read_dir(&sessions).unwrap().map(|entry| entry.unwrap().path()).filter(|path| path.extension().is_some_and(|ext| ext == "json")).collect();
    assert_eq!(records, vec![old.clone()], "no new or kept session record");
    let after: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(&old).unwrap()).unwrap();
    assert_eq!(after, record, "the old record is unchanged");
    let events = std::fs::read_to_string(sessions.join(format!("{name}.events.jsonl"))).unwrap();
    assert!(events.contains("First round 1"), "the old event log stays: {events}");
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

/// DEV-682: an agent that sends nothing for the profile's stall_timeout_seconds has its turn cancelled, and the job
/// fails as stalled, so `job_failed` rules decide.
#[test]
fn a_silent_agent_fails_its_job_as_stalled() {
    let root = tempfile::tempdir().unwrap();
    let hold = root.path().join("hold");
    let harness = harness("    stall_timeout_seconds: 1\n", &[("FAKE_HOLD", hold.to_string_lossy().to_string())]);
    let job = harness.start();
    wait_for("the job's failure", || status(&job) == "failed");
    assert!(hold.exists(), "the agent held its prompt");
    let summary = AgentJobService::get_job(&job).unwrap().summary.unwrap_or_default();
    assert!(summary.starts_with("Stalled: the agent sent nothing for 1s"), "{summary}");
    wait_for("the cancel", || harness.log().contains("\"method\":\"session/cancel\""));
}

/// DEV-695: a ticket set Done while its job's agent holds a prompt cancels the job once the grace is over
/// (`LOTAR_TEST_MOVED_ON_GRACE_MS` shortens it from 120s), and the job's log says so twice: when it was seen and
/// at the cancel.
#[test]
fn a_job_whose_ticket_goes_done_mid_run_ends_cancelled() {
    let root = tempfile::tempdir().unwrap();
    let hold = root.path().join("hold");
    let _grace = EnvVarGuard::set("LOTAR_TEST_MOVED_ON_GRACE_MS", "300");
    let harness = harness("", &[("FAKE_HOLD", hold.to_string_lossy().to_string())]);
    let job = harness.start();
    wait_for("the held prompt", || hold.exists());
    let update = lotar::api_types::TaskUpdate { status: Some("Done".into()), ..Default::default() };
    TaskService::update(&mut harness.fixtures.create_storage(), "TEST-1", update).unwrap();
    wait_for("the cancel", || status(&job) == "cancelled");
    wait_for("the agent's cancel", || harness.log().contains("\"method\":\"session/cancel\""));
    let notes: Vec<String> =
        AgentJobService::events_for(&job).into_iter().filter(|event| event.kind == "agent_job_progress").filter_map(|event| event.message).collect();
    assert!(notes.iter().any(|note| note.starts_with("TEST-1 went Done while the job ran; LoTaR cancels the job unless")), "{notes:?}");
    assert!(notes.iter().any(|note| note == "TEST-1 went Done while the job ran: LoTaR cancelled the job."), "{notes:?}");
    wait_for("the job's end", || AgentJobService::job_settled(&job));
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
    assert!(read("fix").contains("\"method\":\"session/resume\""), "resumed with the rework's environment (DEV-565)");
    assert!(fix[1].contains("Review: keep fix.txt but say why 1"), "the rework gets the review: {}", fix[1]);
    // DEV-620: the prompt carries the newest round only; the review reaches the merge through the ticket's
    // comments and the review session's log.
    assert!(
        merge[0].contains("Recent context (the newest 2 of 6 prompts and replies") && merge[0].contains("- assistant: Fixed: fix.txt holds the fix 1"),
        "the merge agent gets the rework: {}",
        merge[0]
    );
    assert!(merge[0].contains("-PIPE-1-review (codex agent"), "and the review session: {}", merge[0]);
    assert!(root.join("fix.txt").is_file(), "the fix reached main");
    wait_for_jobs_to_end(&fixtures.tasks_root);
}

/// `t3` as T3 Code's CLI, next to a [`fake_t3_server`] that keeps its threads in `t3-threads/<id>/` beside the
/// script. `thread show <id>` does a run the server took (a `t3_thread_launch` or `t3_thread_send`) when the call
/// carries a job's env (`$FAKE_LOG`): it logs `<id> <branch> <commit it started from>` for a launch, `send <id> ...`
/// for a send, to `$FAKE_LOG` (a launch's prompt to `$FAKE_LOG.prompt-<n>`), and runs `$FAKE_RUN` in the thread's
/// worktree. Without the env the run still runs. It answers with status `running` while a file `running-<id>` is next
/// to the script, an `activeRunId` while `active-<id>` is, as waiting for an answer while `waiting-<id>` is, and with
/// status `failed` while `failed-<id>` is, and as waiting on a background command (status `waiting`,
/// `pendingBackgroundTasks`) for as many more shows as the number in `background-<id>`; it logs `show <id>` to `t3-show.log` there.
/// While `list-threads` is there, `thread list` lists the threads with those statuses and whether they are settled. `thread start` (a session in the
/// main checkout, before DEV-652) makes a thread in the project and runs it the same way. `auth pairing create` prints
/// the pairing code `pair-1`.
const FAKE_T3: &str = r#"#!/bin/sh
dir=$(dirname "$0")
threads="$dir/t3-threads"
run_pending() {
  t="$threads/$1"
  [ -f "$t/pending" ] && [ -n "$FAKE_LOG" ] || return 0
  tree=$(cat "$t/tree")
  line="$1 $(git -C "$tree" symbolic-ref --short HEAD) $(git -C "$tree" rev-parse HEAD)"
  if [ "$(cat "$t/kind")" = launch ]; then
    echo "$line" >> "$FAKE_LOG"
    cp "$t/message" "$FAKE_LOG.prompt-${1#thread-}"
  else
    echo "send $line" >> "$FAKE_LOG"
  fi
  (cd "$tree" && sh -c "$FAKE_RUN") >/dev/null 2>&1
  mv "$t/pending" "$t/latest"
}
case "$1 $2" in
  "auth pairing") echo '{"credential":"pair-1","expiresAt":"2099-01-01T00:02:00.000Z","id":"p","label":"harnessmux","scopes":[]}'; exit 0 ;;
  "thread start")
    mkdir -p "$threads"
    id="thread-$(ls "$threads" | wc -l | tr -d ' ')"
    mkdir "$threads/$id"
    echo "$3" > "$threads/$id/tree"
    echo launch > "$threads/$id/kind"
    cat > "$threads/$id/message"
    echo r0 > "$threads/$id/pending"
    echo "Thread $id" >&2
    run_pending "$id"
    echo "Done in $id"
    exit 0 ;;
  "thread show") ;;
  "thread list")
    [ -f "$dir/list-threads" ] || { echo '{}'; exit 0; }
    printf '['; sep=''
    for t in "$threads"/*; do
      [ -d "$t" ] || continue
      id=$(basename "$t") status=completed settled=false
      [ -f "$t/pending" ] && status=running
      [ -f "$dir/running-$id" ] && status=running
      [ -f "$dir/failed-$id" ] && status=failed
      [ -f "$t/settled" ] && settled=true
      printf '%s{"threadId":"%s","link":"https://t3.test/%s","status":"%s","latestRunId":"%s","settled":%s}' "$sep" "$id" "$id" "$status" "$(cat "$t/latest" 2>/dev/null)" "$settled"
      sep=,
    done
    echo ']'; exit 0 ;;
  "auth session") if [ "$3" = issue ]; then echo '{"sessionId":"s1","token":"secret"}'; else echo "$*" >> "$dir/t3-auth.log"; fi; exit 0 ;;
  *) echo '{}'; exit 0 ;;
esac
id=$3
echo "show $id" >> "$dir/t3-show.log"
[ -d "$threads/$id" ] || { echo "No thread $id" >&2; exit 1; }
run_pending "$id"
status=completed active=null pending=0
run=$(cat "$threads/$id/latest" 2>/dev/null)
if [ -f "$threads/$id/pending" ]; then status=running; run=$(cat "$threads/$id/pending"); active="\"$run\""; fi
[ -f "$dir/running-$id" ] && status=running
[ -f "$dir/active-$id" ] && active='"r1"'
[ -f "$dir/waiting-$id" ] && status=running pending=1
[ -f "$dir/failed-$id" ] && status=failed
tasks='[]'
if [ -f "$dir/background-$id" ]; then
  left=$(cat "$dir/background-$id")
  if [ "$left" -gt 0 ]; then status=waiting tasks='["npm test"]'; echo $((left - 1)) > "$dir/background-$id"; else rm "$dir/background-$id"; fi
fi
printf '{"thread":{"threadId":"%s","link":"https://t3.test/%s","status":"%s","activeRunId":%s,"latestRunId":"%s","pendingRequestCount":%s,"pendingBackgroundTasks":%s,"worktreePath":"%s"},"messages":[{"role":"assistant","runId":"%s","text":"Done in %s"}]}\n' \
  "$id" "$id" "$status" "$active" "$run" "$pending" "$tasks" "$(cat "$threads/$id/tree")" "$run" "$id"
"#;

/// A fake T3 Code server for the fake `t3` in `dir`, with one project, the repository at `project`: the MCP OAuth
/// sign-in with the pairing code `pair-1`, and `/mcp`. `t3_thread_launch` makes thread `thread-<n>` in its
/// `existing_worktree` (the same thread for a repeated `clientRequestId`) and `t3_thread_send` gives a thread a run;
/// the fake `t3 thread show` does it. `t3_thread_organize` fails with the text in `organize-error` while that file is
/// there; else it marks the thread settled. Each tool call is logged to `t3-mcp.log` (`<tool> <arguments>`). A WebSocket on `/ws` takes T3 Code's
/// internal rename command (DEV-619), logs it there as `ws_rename <payload>` and answers success; while `ws-refuse` is
/// there, `/ws` answers 404. Returns the origin.
fn fake_t3_server(dir: &Path, project: &Path) -> String {
    use std::collections::{HashMap, HashSet};
    use std::io::{BufRead as _, Read as _, Write as _};
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let origin = format!("http://{}", listener.local_addr().unwrap());
    let (dir, project) = (dir.to_path_buf(), std::fs::canonicalize(project).unwrap());
    std::thread::spawn(move || {
        let threads = dir.join("t3-threads");
        let (mut tokens, mut codes, mut launched) = (HashSet::new(), HashSet::new(), HashMap::new());
        let mut runs = 0;
        for stream in listener.incoming() {
            let Ok(mut stream) = stream else { continue };
            let mut head = [0; 7];
            if stream.peek(&mut head).is_ok_and(|read| read == head.len()) && &head == b"GET /ws" && !dir.join("ws-refuse").exists() {
                let Ok(mut socket) = tungstenite::accept(stream) else { continue };
                if let Ok(tungstenite::Message::Text(text)) = socket.read() {
                    let request: serde_json::Value = serde_json::from_str(text.as_str()).unwrap();
                    let mut log = std::fs::OpenOptions::new().create(true).append(true).open(dir.join("t3-mcp.log")).unwrap();
                    writeln!(log, "ws_rename {}", request["payload"]).unwrap();
                    let answer = serde_json::json!({"_tag": "Exit", "requestId": request["id"], "exit": {"_tag": "Success", "value": {"sequence": 1}}});
                    let _ = socket.send(tungstenite::Message::text(answer.to_string()));
                    let _ = socket.read();
                }
                continue;
            }
            let mut reader = std::io::BufReader::new(stream.try_clone().unwrap());
            let mut request_line = String::new();
            reader.read_line(&mut request_line).unwrap();
            let path = request_line.split_whitespace().nth(1).unwrap_or_default().to_string();
            let (mut length, mut bearer) = (0, String::new());
            loop {
                let mut line = String::new();
                reader.read_line(&mut line).unwrap();
                let line = line.trim_end();
                if line.is_empty() {
                    break;
                }
                let (key, value) = line.split_once(':').unwrap();
                match key.to_ascii_lowercase().as_str() {
                    "content-length" => length = value.trim().parse().unwrap(),
                    "authorization" => bearer = value.trim().trim_start_matches("Bearer ").to_string(),
                    _ => {}
                }
            }
            let mut body = vec![0; length];
            reader.read_exact(&mut body).unwrap();
            let body = String::from_utf8(body).unwrap();
            let json = |status: u16, value: serde_json::Value| (status, value.to_string());
            let (status, answer) = match path.as_str() {
                "/oauth/mcp/register" => json(201, serde_json::json!({"client_id": "c1"})),
                "/oauth/mcp/decision" => {
                    let request: serde_json::Value = serde_json::from_str(&body).unwrap();
                    if request["decision"]["code"] != "pair-1" {
                        json(400, serde_json::json!({"error": "invalid_grant", "error_description": "Invalid pairing code"}))
                    } else {
                        let code = format!("ac{}", codes.len() + 1);
                        codes.insert(code.clone());
                        json(200, serde_json::json!({"redirectTo": format!("http://127.0.0.1/callback?code={code}&state=s")}))
                    }
                }
                "/oauth/mcp/token" => {
                    let token = format!("tok-{}", tokens.len() + 1);
                    tokens.insert(token.clone());
                    json(200, serde_json::json!({"access_token": token, "token_type": "Bearer", "expires_in": 2_592_000}))
                }
                "/mcp" => {
                    let request: serde_json::Value = serde_json::from_str(&body).unwrap();
                    if !tokens.contains(&bearer) {
                        json(401, serde_json::json!({"error": "invalid_mcp_credential"}))
                    } else if request.get("id").is_none() {
                        (202, String::new())
                    } else if request["method"] == "initialize" {
                        json(200, serde_json::json!({"jsonrpc": "2.0", "id": request["id"], "result": {"protocolVersion": "2025-06-18", "capabilities": {}}}))
                    } else {
                        let (tool, arguments) = (request["params"]["name"].as_str().unwrap(), &request["params"]["arguments"]);
                        let mut log = std::fs::OpenOptions::new().create(true).append(true).open(dir.join("t3-mcp.log")).unwrap();
                        writeln!(log, "{tool} {arguments}").unwrap();
                        let mut run = |id: &str, kind: &str, tree: Option<&str>| {
                            runs += 1;
                            let thread = threads.join(id);
                            std::fs::create_dir_all(&thread).unwrap();
                            if let Some(tree) = tree {
                                std::fs::write(thread.join("tree"), tree).unwrap();
                            }
                            std::fs::write(thread.join("kind"), kind).unwrap();
                            std::fs::write(thread.join("message"), arguments["message"].as_str().unwrap_or_default()).unwrap();
                            std::fs::write(thread.join("pending"), format!("r{runs}")).unwrap();
                            serde_json::json!({"threadId": id, "link": format!("https://t3.test/{id}"), "runId": format!("r{runs}"), "status": "starting"})
                        };
                        let result = match tool {
                            "t3_project_list" => Ok(serde_json::json!({"projects": [{"id": "p1", "workspaceRoot": project}], "nextCursor": null})),
                            "t3_thread_launch" => {
                                let request = arguments["clientRequestId"].as_str().unwrap().to_string();
                                match launched.get(&request) {
                                    Some(id) => Ok(serde_json::json!({"threadId": id, "link": format!("https://t3.test/{id}")})),
                                    None => {
                                        let id = format!("thread-{}", std::fs::read_dir(&threads).map_or(0, Iterator::count));
                                        launched.insert(request, id.clone());
                                        Ok(run(&id, "launch", arguments["workspaceStrategy"]["worktreePath"].as_str()))
                                    }
                                }
                            }
                            "t3_thread_send" => Ok(run(arguments["threadId"].as_str().unwrap(), "send", None)),
                            "t3_thread_organize" => std::fs::read_to_string(dir.join("organize-error")).map_or_else(
                                |_| {
                                    let _ = std::fs::write(threads.join(arguments["threadId"].as_str().unwrap_or_default()).join("settled"), "");
                                    Ok(serde_json::json!({"sequence": 1}))
                                },
                                Err,
                            ),
                            _ => Ok(serde_json::json!({"sequence": 1})),
                        };
                        let result = match result {
                            Ok(content) => {
                                serde_json::json!({"content": [{"type": "text", "text": content.to_string()}], "structuredContent": content, "isError": false})
                            }
                            Err(error) => serde_json::json!({"content": [{"type": "text", "text": error}], "isError": true}),
                        };
                        json(200, serde_json::json!({"jsonrpc": "2.0", "id": request["id"], "result": result}))
                    }
                }
                _ => (404, String::new()),
            };
            let _ = write!(
                stream,
                "HTTP/1.1 {status} X\r\ncontent-type: application/json\r\nmcp-session-id: s1\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{answer}",
                answer.len()
            );
        }
    });
    origin
}

/// The fake T3 Code server's calls of `tool`, with their arguments.
fn mcp_calls(root: &Path, tool: &str) -> Vec<serde_json::Value> {
    let calls = std::fs::read_to_string(root.join("t3-mcp.log")).unwrap_or_default();
    let prefix = format!("{tool} ");
    calls.lines().filter_map(|line| line.strip_prefix(&prefix)).map(|args| serde_json::from_str(args).unwrap()).collect()
}

/// A repository whose TEST project runs the profiles `fix`, `rework` and `review` as fake T3 Code threads in the
/// ticket's worktree.
fn t3_harness(rework_run: &str) -> (TestFixtures, Vec<EnvVarGuard>) {
    t3_harness_with(&[("fix", "echo fix > fix.txt && git add fix.txt && git commit -qm TEST-1-fix", ""), ("rework", rework_run, ""), ("review", "", "")])
}

/// A repository whose TEST project runs `profiles` (name, `$FAKE_RUN`, more profile keys four spaces deep) as fake
/// T3 Code threads in the ticket's worktree.
fn t3_harness_with(profiles: &[(&str, &str, &str)]) -> (TestFixtures, Vec<EnvVarGuard>) {
    t3_harness_and(profiles, "")
}

/// `t3_harness_with` plus `more_agents`, profiles on other runners (two spaces deep). The ticket's worktree is
/// `worktrees/TEST-1` in the repository ([`ticket_worktree`]).
fn t3_harness_and(profiles: &[(&str, &str, &str)], more_agents: &str) -> (TestFixtures, Vec<EnvVarGuard>) {
    crate::common::require_git();
    let fixtures = TestFixtures::new();
    let root = fixtures.get_temp_path().to_path_buf();
    let t3 = root.join("fake-t3.sh");
    std::fs::write(&t3, FAKE_T3).unwrap();
    std::fs::set_permissions(&t3, std::fs::Permissions::from_mode(0o755)).unwrap();
    let t3_home = root.join("t3home");
    std::fs::create_dir_all(t3_home.join("userdata")).unwrap();
    let origin = fake_t3_server(&root, &root);
    std::fs::write(t3_home.join("userdata/server-runtime.json"), format!(r#"{{"version":1,"origin":"{origin}"}}"#)).unwrap();
    let home = root.join("harnessmux");
    std::fs::create_dir_all(home.join("config")).unwrap();
    std::fs::write(
        home.join("config/agents.toml"),
        format!("[agents.t3]\ncommand = [{:?}]\nenv = {{ T3CODE_HOME = {:?} }}\n", t3.display().to_string(), t3_home.display().to_string()),
    )
    .unwrap();
    let profile = |(name, run, extra): &(&str, &str, &str)| {
        format!("  {name}:\n    runner: t3\n{extra}    env:\n      FAKE_LOG: \"{}\"\n      FAKE_RUN: \"{run}\"\n", root.join("t3.log").display())
    };
    let agents: String = profiles.iter().map(profile).collect::<String>() + more_agents;
    fixtures.create_config_in_dir(
        &fixtures.tasks_root,
        &format!(
            "default:\n  project: TEST\nissue:\n  states: [Todo, InProgress, HelpNeeded, Done]\nagent:\n  worktree:\n    enabled: true\n    dir: '{}'\nagents:\n{agents}",
            root.join("worktrees").display()
        ),
    );
    TaskService::create(&mut fixtures.create_storage(), TaskCreate { title: "Rework".into(), project: Some("TEST".into()), ..Default::default() }).unwrap();
    git(&root, &["init", "-q", "-b", "main"]);
    git(&root, &["config", "user.name", "Rework"]);
    git(&root, &["config", "user.email", "rework@example.invalid"]);
    git(&root, &["config", "commit.gpgsign", "false"]);
    std::fs::write(root.join(".gitignore"), "worktrees/\nt3-threads/\nt3home/\nharnessmux/\n*.log\n*.log.*\n").unwrap();
    git(&root, &["add", "."]);
    git(&root, &["commit", "-qm", "init"]);
    let guards = vec![
        EnvVarGuard::set("HARNESSMUX_HOME", home.to_string_lossy().as_ref()),
        EnvVarGuard::set("LOTAR_TASKS_DIR", fixtures.tasks_root.to_string_lossy().as_ref()),
        EnvVarGuard::set("LOTAR_TEST_SILENT", "1"),
    ];
    (fixtures, guards)
}

/// The ticket TEST-1's worktree, where LoTaR runs its jobs.
fn ticket_worktree(fixtures: &TestFixtures) -> PathBuf {
    fixtures.get_temp_path().join("worktrees/TEST-1")
}

fn run_t3(fixtures: &TestFixtures, profile: &str) -> assert_cmd::assert::Assert {
    run_t3_with(fixtures, profile, &[])
}

fn run_t3_with(fixtures: &TestFixtures, profile: &str, extra: &[&str]) -> assert_cmd::assert::Assert {
    let mut cmd = crate::common::lotar_cmd().unwrap();
    cmd.current_dir(fixtures.get_temp_path()).args(["agent", "run", "TEST-1", "Work on it", "--agent", profile, "--wait"]).args(extra);
    cmd.assert()
}

fn rev(dir: &Path, rev: &str) -> String {
    let output = std::process::Command::new("git").args(["rev-parse", rev]).current_dir(dir).output().unwrap();
    String::from_utf8_lossy(&output.stdout).trim().to_string()
}

/// The job logs of the fixture, in one string.
fn job_logs(fixtures: &TestFixtures) -> String {
    std::fs::read_dir(fixtures.tasks_root.join("@agent-logs"))
        .unwrap()
        .map(|entry| std::fs::read_to_string(entry.unwrap().path()).unwrap_or_default())
        .collect()
}

/// DEV-652: T3 Code made each thread a worktree of its own, on a `t3/*` branch, and LoTaR pointed the ticket's branch
/// at it when the job ended. A t3 job now runs in the ticket's worktree on its branch, as every runner does: the
/// thread starts there (`t3_thread_launch`, `existing_worktree`), each thread of the ticket finds the work of the one
/// before, and T3 Code makes no worktree.
#[test]
fn every_t3_thread_of_a_ticket_runs_in_its_worktree_on_its_branch() {
    let (fixtures, _env) = t3_harness("echo rework > rework.txt");
    let root = fixtures.get_temp_path();
    let start = rev(root, "main");
    run_t3(&fixtures, "fix").success();
    let tree = std::fs::canonicalize(ticket_worktree(&fixtures)).unwrap();
    let fix = rev(root, "agent/TEST-1");
    assert_ne!(fix, start, "the fix thread committed on the ticket's branch");
    run_t3(&fixtures, "rework").success();
    let rework = rev(root, "agent/TEST-1");
    assert!(tree.join("rework.txt").is_file() && rev(&tree, "HEAD~1") == fix, "LoTaR committed the rework on the fix");
    run_t3(&fixtures, "review").success();
    let log = std::fs::read_to_string(root.join("t3.log")).unwrap();
    assert_eq!(
        log.lines().collect::<Vec<_>>(),
        vec![format!("thread-0 agent/TEST-1 {start}"), format!("thread-1 agent/TEST-1 {fix}"), format!("thread-2 agent/TEST-1 {rework}")],
        "each thread starts from the work of the one before: {log}"
    );
    let launches = mcp_calls(root, "t3_thread_launch");
    assert_eq!(launches.len(), 3, "{launches:?}");
    for launch in &launches {
        assert_eq!(launch["workspaceStrategy"], serde_json::json!({"type": "existing_worktree", "worktreePath": tree.display().to_string()}), "{launch}");
        assert_eq!(launch["title"], "TEST-1 Rework", "{launch}");
        assert_eq!(launch["runtimeMode"], "full-access", "{launch}");
    }
    let worktrees = std::process::Command::new("git").args(["worktree", "list", "--porcelain"]).current_dir(root).output().unwrap();
    let worktrees = String::from_utf8_lossy(&worktrees.stdout).lines().filter(|line| line.starts_with("worktree ")).count();
    assert_eq!(worktrees, 2, "the main checkout and the ticket's worktree, nothing T3 Code made");
    let task = TaskService::get(&fixtures.create_storage(), "TEST-1", None).unwrap();
    for thread in ["thread-0", "thread-1", "thread-2"] {
        let link = format!("https://t3.test/{thread}");
        assert!(task.references.iter().any(|reference| reference.link.as_deref() == Some(link.as_str())), "{thread} is linked: {:?}", task.references);
    }
}

/// DEV-652: a `session: continue` job before DEV-652 ran in the main checkout, its thread in a worktree T3 Code
/// made. The next job doesn't continue it there: it starts a new thread in the ticket's worktree with a handoff.
#[test]
fn a_thread_from_before_ticket_worktrees_hands_off_to_a_new_one_in_the_worktree() {
    let (fixtures, _env) = t3_harness_with(&[("fix", "echo $$ >> work.txt", "    session: continue\n")]);
    let root = fixtures.get_temp_path();
    let config_path = fixtures.tasks_root.join("config.yml");
    let config = std::fs::read_to_string(&config_path).unwrap();
    std::fs::write(&config_path, config.replace("    enabled: true\n", "    enabled: false\n")).unwrap();
    run_t3(&fixtures, "fix").success();
    assert!(mcp_calls(root, "t3_thread_launch").is_empty(), "the old job's thread started from the main checkout");
    std::fs::write(&config_path, &config).unwrap();
    run_t3(&fixtures, "fix").success();
    let launches = mcp_calls(root, "t3_thread_launch");
    let tree = std::fs::canonicalize(ticket_worktree(&fixtures)).unwrap();
    assert_eq!(launches.len(), 1, "{launches:?}");
    assert_eq!(launches[0]["workspaceStrategy"]["worktreePath"], tree.display().to_string());
    assert!(
        job_logs(&fixtures).contains("T3 Code thread runs in a worktree T3 Code made; this job starts a new thread in the ticket's worktree, with a handoff."),
        "the job says why"
    );
    let prompt = std::fs::read_to_string(root.join("t3.log.prompt-1")).unwrap();
    assert!(prompt.contains("thread-0"), "the new thread hears of the old one: {prompt}");
    run_t3(&fixtures, "fix").success();
    let log = std::fs::read_to_string(root.join("t3.log")).unwrap();
    assert!(log.lines().last().unwrap_or_default().starts_with("send thread-1 agent/TEST-1 "), "the next job continues the new thread: {log}");
}

/// The owner decided 2026-11-17 (DEV-652): a t3 job that completed settles its thread in T3 Code once the end rules
/// moved the ticket on, so the sidebar keeps only the threads that still need someone.
#[test]
fn a_completed_t3_job_settles_its_thread_once_the_ticket_moves_on() {
    let (fixtures, _env) = t3_harness("");
    let root = fixtures.get_temp_path();
    lotar::services::automation_service::AutomationService::set(
        &fixtures.tasks_root,
        None,
        "automation:\n  rules:\n    - name: Fixed\n      on:\n        job_completed:\n          set: { status: Done, assignee: mallox }\n",
    )
    .expect("automation rules");
    run_t3(&fixtures, "fix").success();
    let settles = mcp_calls(root, "t3_thread_organize");
    assert_eq!(settles, vec![serde_json::json!({"threadId": "thread-0", "action": "settle"})], "a done ticket waits for nobody, whoever it is assigned to");
    assert!(job_logs(&fixtures).contains("Settled the T3 Code thread https://t3.test/thread-0"), "the job says so");
}

/// DEV-652: a thread whose ticket waits for a person, or whose job failed, stays active in T3 Code, and a settle T3
/// Code refuses is a note: the job still completes.
#[test]
fn a_t3_thread_stays_active_while_its_ticket_waits_or_its_job_failed() {
    let (fixtures, _env) = t3_harness("");
    let root = fixtures.get_temp_path();
    let rules = |set: &str| {
        lotar::services::automation_service::AutomationService::set(
            &fixtures.tasks_root,
            None,
            &format!("automation:\n  rules:\n    - name: End\n      on:\n        job_completed:\n          set: {set}\n"),
        )
        .expect("automation rules");
    };
    rules("{ status: InProgress, assignee: mallox }");
    run_t3(&fixtures, "review").success();
    rules("{ status: HelpNeeded }");
    run_t3(&fixtures, "review").success();
    rules("{ status: Done }");
    std::fs::write(root.join("failed-thread-2"), "").unwrap();
    run_t3(&fixtures, "review").failure();
    assert_eq!(mcp_calls(root, "t3_thread_organize"), Vec::<serde_json::Value>::new(), "no thread was settled");
    let logs = job_logs(&fixtures);
    assert!(logs.contains("The T3 Code thread stays active: TEST-1 is InProgress, assigned to mallox."), "{logs}");
    assert!(logs.contains("The T3 Code thread stays active: TEST-1 is HelpNeeded."), "{logs}");
    std::fs::write(root.join("organize-error"), "T3 Code refuses").unwrap();
    run_t3(&fixtures, "review").success();
    assert!(job_logs(&fixtures).contains("The T3 Code thread was not settled: "), "the refusal is a note");
}

/// The fake T3 Code server's settled threads, in the order it settled them.
fn settled_threads(root: &Path) -> Vec<String> {
    mcp_calls(root, "t3_thread_organize").iter().map(|call| call["threadId"].as_str().unwrap_or_default().to_string()).collect()
}

/// DEV-747: a failed job's thread stayed active for good, also after later jobs settled their own threads (DEV-732).
/// It is settled once a later job on the ticket completes and settles its own thread.
#[test]
fn a_failed_jobs_thread_is_settled_once_a_later_job_completes() {
    let (fixtures, _env) = t3_harness("");
    let root = fixtures.get_temp_path();
    std::fs::write(root.join("list-threads"), "").unwrap();
    std::fs::write(root.join("failed-thread-0"), "").unwrap();
    run_t3(&fixtures, "fix").failure();
    assert!(settled_threads(root).is_empty(), "a failed job keeps its thread active");
    // As DEV-729's threads did, the thread completes after its job failed.
    std::fs::remove_file(root.join("failed-thread-0")).unwrap();
    run_t3(&fixtures, "review").success();
    assert_eq!(settled_threads(root), vec!["thread-1", "thread-0"], "the later job settles its own thread, then the kept one");
    let logs = job_logs(&fixtures);
    assert!(logs.contains("Settled the T3 Code thread https://t3.test/thread-0"), "the failed job's log says so: {logs}");
}

/// DEV-747: a thread whose job ended while its ticket was HelpNeeded stays active until the ticket is done, however it
/// got there; the look `lotar serve` runs at its start and every 30 seconds settles it then.
#[test]
fn a_helpneeded_jobs_thread_is_settled_once_the_ticket_is_done() {
    let (fixtures, _env) = t3_harness("");
    let root = fixtures.get_temp_path();
    std::fs::write(root.join("list-threads"), "").unwrap();
    lotar::services::automation_service::AutomationService::set(
        &fixtures.tasks_root,
        None,
        "automation:\n  rules:\n    - name: Ask\n      on:\n        job_completed:\n          set: { status: HelpNeeded, assignee: mallox }\n",
    )
    .expect("automation rules");
    run_t3(&fixtures, "review").success();
    assert!(settled_threads(root).is_empty(), "the ticket waits for mallox");
    let update = lotar::api_types::TaskUpdate { status: Some("Done".into()), ..Default::default() };
    TaskService::update(&mut fixtures.create_storage(), "TEST-1", update).unwrap();
    AgentJobService::adopt_resumed_t3_threads(&fixtures.tasks_root);
    assert_eq!(settled_threads(root), vec!["thread-0"]);
    assert!(job_logs(&fixtures).contains("Settled the T3 Code thread https://t3.test/thread-0"), "the job's log says so");
}

/// DEV-754: a kept thread T3 Code refuses to settle is named by the link `t3 thread list` shows, not by its id.
#[test]
fn a_kept_thread_that_isnt_settled_is_named_by_its_link() {
    let (fixtures, _env) = t3_harness("");
    let root = fixtures.get_temp_path();
    std::fs::write(root.join("list-threads"), "").unwrap();
    std::fs::write(root.join("failed-thread-0"), "").unwrap();
    run_t3(&fixtures, "fix").failure();
    std::fs::remove_file(root.join("failed-thread-0")).unwrap();
    std::fs::write(root.join("organize-error"), "T3 Code refuses").unwrap();
    let update = lotar::api_types::TaskUpdate { status: Some("Done".into()), ..Default::default() };
    TaskService::update(&mut fixtures.create_storage(), "TEST-1", update).unwrap();
    AgentJobService::adopt_resumed_t3_threads(&fixtures.tasks_root);
    let logs = job_logs(&fixtures);
    assert!(logs.contains("The T3 Code thread https://t3.test/thread-0 was not settled: "), "{logs}");
}

/// DEV-747: a kept thread whose run still goes is left alone, also on a done ticket.
#[test]
fn a_kept_thread_that_still_runs_is_left_alone() {
    let (fixtures, _env) = t3_harness("");
    let root = fixtures.get_temp_path();
    std::fs::write(root.join("list-threads"), "").unwrap();
    std::fs::write(root.join("failed-thread-0"), "").unwrap();
    run_t3(&fixtures, "fix").failure();
    std::fs::remove_file(root.join("failed-thread-0")).unwrap();
    std::fs::write(root.join("running-thread-0"), "").unwrap();
    let update = lotar::api_types::TaskUpdate { status: Some("Done".into()), ..Default::default() };
    TaskService::update(&mut fixtures.create_storage(), "TEST-1", update).unwrap();
    AgentJobService::adopt_resumed_t3_threads(&fixtures.tasks_root);
    assert!(settled_threads(root).is_empty(), "the thread still runs");
}

/// DEV-729: a thread that ends a turn waiting on its own background command (status `waiting`,
/// `pendingBackgroundTasks`) still works; its job runs on until the thread's run completes, instead of failing with
/// "ended with status 'waiting'" and being retried while the thread still works.
#[test]
fn a_t3_job_runs_on_while_its_thread_waits_on_a_background_command() {
    let (fixtures, _env) = t3_harness("");
    let root = fixtures.get_temp_path();
    let background = root.join("background-thread-0");
    std::fs::write(&background, "2").unwrap();
    run_t3(&fixtures, "fix").success();
    assert!(!background.exists(), "the job followed the thread through its background wait");
    assert!(ticket_worktree(&fixtures).join("fix.txt").is_file(), "the thread's work landed");
    assert_eq!(mcp_calls(root, "t3_thread_launch").len(), 1, "no second thread started");
    let logs = job_logs(&fixtures);
    assert!(!logs.contains("ended with status 'waiting'") && !logs.contains("agent_job_failed"), "{logs}");
}

/// DEV-618, DEV-652: a job whose profile sets `tools.disallowed` and `env` writes them into the ticket's worktree
/// before its thread starts there; its log says so, and no commit takes the file. A profile on another provider than
/// Claude writes nothing.
#[test]
fn a_t3_job_gives_the_tickets_worktree_the_profiles_tools_and_env() {
    let (fixtures, _env) = t3_harness_with(&[
        ("fix", "echo fix > fix.txt", "    tools:\n      disallowed: [Artifact]\n"),
        ("other", "", "    provider: codex\n    model: m\n    tools:\n      disallowed: [Artifact]\n"),
    ]);
    let root = fixtures.get_temp_path();
    run_t3(&fixtures, "fix").success();
    let file = ticket_worktree(&fixtures).join(".claude/settings.local.json");
    let settings: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(&file).unwrap()).unwrap();
    assert_eq!(settings["permissions"], serde_json::json!({"deny": ["Artifact"]}));
    assert_eq!(settings["env"]["FAKE_RUN"], "echo fix > fix.txt", "{settings}");
    assert!(job_logs(&fixtures).contains("Wrote .claude/settings.local.json"), "{}", job_logs(&fixtures));
    let committed = std::process::Command::new("git").args(["log", "--name-only", "--format=", "main..agent/TEST-1"]).current_dir(root).output().unwrap();
    let committed = String::from_utf8_lossy(&committed.stdout).to_string();
    assert!(committed.contains("fix.txt") && !committed.contains(".claude"), "{committed}");

    std::fs::remove_file(&file).unwrap();
    run_t3_with(&fixtures, "other", &["--session", "new"]).success();
    assert!(!file.exists(), "a codex thread gets no Claude settings");
}

/// DEV-619: a continued thread is renamed to the profile's thread_name before its turn is sent, with a session token
/// that lotar revokes afterwards.
#[test]
fn a_continued_thread_is_renamed_before_its_turn() {
    let named = "    session: continue\n    thread_name: \"${{ticket.id}} fix: ${{ticket.title}}\"\n";
    let (fixtures, _env) = t3_harness_with(&[("fix", "", named)]);
    let root = fixtures.get_temp_path();
    run_t3(&fixtures, "fix").success();
    assert_eq!(mcp_calls(root, "t3_thread_launch")[0]["title"], "TEST-1 fix: Rework", "a new thread gets the name");
    assert!(mcp_calls(root, "ws_rename").is_empty(), "a new thread isn't renamed");
    run_t3(&fixtures, "fix").success();
    let rename = mcp_calls(root, "ws_rename");
    assert_eq!(rename.len(), 1, "{rename:?}");
    assert_eq!(rename[0]["type"], "thread.metadata.update");
    assert_eq!(rename[0]["threadId"], "thread-0");
    assert_eq!(rename[0]["title"], "TEST-1 fix: Rework");
    let log = std::fs::read_to_string(root.join("t3-mcp.log")).unwrap();
    let (renamed, sent) = (log.find("ws_rename").unwrap(), log.find("t3_thread_send").unwrap());
    assert!(renamed < sent, "the rename comes before the continued turn: {log}");
    assert!(job_logs(&fixtures).contains("Renamed the T3 Code thread thread-0 to \\\"TEST-1 fix: Rework\\\"."), "{}", job_logs(&fixtures));
    assert!(std::fs::read_to_string(root.join("t3-auth.log")).unwrap_or_default().contains("auth session revoke s1"));
}

/// DEV-619: a rename that T3 Code's server refuses is logged and the job runs on.
#[test]
fn a_continued_thread_that_cant_be_renamed_runs_on() {
    let named = "    session: continue\n    thread_name: \"${{ticket.id}} fix: ${{ticket.title}}\"\n";
    let (fixtures, _env) = t3_harness_with(&[("fix", "", named)]);
    let root = fixtures.get_temp_path();
    run_t3(&fixtures, "fix").success();
    std::fs::write(root.join("ws-refuse"), "").unwrap();
    run_t3(&fixtures, "fix").success();
    let logs = job_logs(&fixtures);
    assert!(logs.contains("Could not rename the continued T3 Code thread to \\\"TEST-1 fix: Rework\\\", so it keeps its name: "), "{logs}");
    assert_eq!(mcp_calls(root, "t3_thread_send").len(), 1, "the continued turn was sent");
}

/// DEV-594: a fix job's thread hit the usage limit, so the job failed, and T3 Code resumed the thread on its own when
/// the limit reset. The next fix job started beside it, and both committed. A job now waits while T3 Code still runs
/// an earlier job's thread on the ticket.
#[test]
fn no_job_starts_while_t3_code_still_runs_an_earlier_jobs_thread() {
    let (fixtures, _env) = t3_harness("");
    let root = fixtures.get_temp_path();
    run_t3(&fixtures, "fix").success();
    let fix = rev(root, "agent/TEST-1");
    let running = root.join("running-thread-0");
    std::fs::write(&running, "").unwrap();
    run_t3(&fixtures, "review").failure().stderr(
        predicate::str::contains("Ticket 'TEST-1' already has an active job: the T3 Code thread thread-0 of the job job-")
            .and(predicate::str::contains(" still runs")),
    );
    assert_eq!(std::fs::read_to_string(root.join("t3.log")).unwrap().lines().count(), 1, "no second thread started");
    std::fs::remove_file(&running).unwrap();
    run_t3(&fixtures, "review").success();
    let log = std::fs::read_to_string(root.join("t3.log")).unwrap();
    assert_eq!(log.lines().nth(1).unwrap_or_default(), format!("thread-1 agent/TEST-1 {fix}"), "the review starts from the fix: {log}");
}

/// DEV-583: a thread from before DEV-652 left uncommitted work in a worktree T3 Code made, on a branch of its own
/// from `agent/TEST-1`. The next job's prompt names it.
#[test]
fn a_t3_job_learns_where_an_earlier_threads_uncommitted_work_is() {
    let elsewhere = tempfile::tempdir().unwrap();
    let (fixtures, _env) = t3_harness_with(&[("fix", "", ""), ("review", "", ""), ("merge", "", "")]);
    let root = fixtures.get_temp_path();
    let earlier = elsewhere.path().join("earlier");
    git(root, &["branch", "agent/TEST-1"]);
    git(root, &["worktree", "add", "-q", "-b", "t3/topic-9", earlier.to_str().unwrap(), "agent/TEST-1"]);
    std::fs::write(earlier.join("half-done.txt"), "work").unwrap();
    run_t3(&fixtures, "fix").success();
    let prompt = std::fs::read_to_string(root.join("t3.log.prompt-0")).unwrap();
    assert!(prompt.contains("You work in a git worktree on branch `agent/TEST-1`, separate from the main checkout."), "{prompt}");
    let earlier = std::fs::canonicalize(&earlier).unwrap();
    assert!(prompt.contains(&format!("uncommitted changes in `{}` (branch `t3/topic-9`)", earlier.display())), "{prompt}");
    assert!(earlier.join("half-done.txt").is_file(), "the earlier work stays where it is");
    // DEV-608: the changes stay there after the retry, so later jobs learn they may be carried over or dropped already,
    // and a merge job doesn't hear of them.
    assert!(prompt.contains("A job that takes the work over (a fix or rework)"), "{prompt}");
    run_t3(&fixtures, "review").success();
    let prompt = std::fs::read_to_string(root.join("t3.log.prompt-1")).unwrap();
    assert!(prompt.contains("carried them over or dropped them already") && prompt.contains("A review job leaves them alone"), "{prompt}");
    run_t3(&fixtures, "merge");
    let prompt = std::fs::read_to_string(root.join("t3.log.prompt-2")).unwrap();
    assert!(prompt.contains("This is a merge job") && !prompt.contains("half-done") && !prompt.contains(&earlier.display().to_string()), "{prompt}");
}

/// DEV-600: harnessmux counts a thread with an `activeRunId` as running whatever its status says, and T3 Code shows a
/// thread that waits for an answer as `running` with a pending request. Only the status was read. Both hold the
/// ticket, and the error says which one it is.
#[test]
fn a_thread_with_an_active_run_or_a_question_holds_the_ticket() {
    let (fixtures, _env) = t3_harness("");
    let root = fixtures.get_temp_path();
    run_t3(&fixtures, "fix").success();
    let active = root.join("active-thread-0");
    std::fs::write(&active, "").unwrap();
    run_t3(&fixtures, "review")
        .failure()
        .stderr(predicate::str::contains("the T3 Code thread thread-0 of the job job-").and(predicate::str::contains(" still runs")));
    std::fs::remove_file(&active).unwrap();
    let waiting = root.join("waiting-thread-0");
    std::fs::write(&waiting, "").unwrap();
    run_t3(&fixtures, "review")
        .failure()
        .stderr(predicate::str::contains("the T3 Code thread thread-0 of the job job-").and(predicate::str::contains(" waits for an answer")));
    assert_eq!(std::fs::read_to_string(root.join("t3.log")).unwrap().lines().count(), 1, "no second thread started");
}

/// DEV-600: a queue entry held by an earlier job's T3 Code thread waited without a word, retrying every few hundred
/// milliseconds. Its ticket now gets one comment that says which thread holds it and what lets it go on.
#[test]
fn a_queue_entry_held_by_a_waiting_thread_says_so_once() {
    let (fixtures, _env) = t3_harness("");
    let root = fixtures.get_temp_path();
    run_t3(&fixtures, "fix").success();
    std::fs::write(root.join("waiting-thread-0"), "").unwrap();
    let _queue = EnvVarGuard::set("LOTAR_AGENT_QUEUE_DIR", root.join("queue").to_str().unwrap());
    let _no_worker = EnvVarGuard::set("LOTAR_AGENT_QUEUE_DISABLE_WORKER", "1");
    let tasks_dir = fixtures.tasks_root.clone();
    let req = AgentJobCreateRequest { ticket_id: "TEST-1".into(), prompt: "Review it".into(), agent: Some("review".into()), ..Default::default() };
    assert!(AgentQueueService::enqueue(&tasks_dir, req).unwrap());
    let comments = || {
        let task = TaskService::get(&fixtures.create_storage(), "TEST-1", None).unwrap();
        task.comments.into_iter().filter(|comment| comment.text.starts_with("The queued @review job waits")).map(|comment| comment.text).collect::<Vec<_>>()
    };
    let worker = std::thread::spawn({
        let tasks_dir = tasks_dir.clone();
        move || AgentQueueService::run_worker(&tasks_dir)
    });
    wait_for("the waiting entry's comment", || !comments().is_empty());
    // Several more retries of the entry.
    std::thread::sleep(Duration::from_millis(1500));
    // A remove also finds the entry while the worker holds it, and its retry then does not queue it again (DEV-661).
    wait_for("the waiting entry's removal", || AgentQueueService::remove(&tasks_dir, "TEST-1").unwrap());
    worker.join().unwrap().unwrap();
    let comments = comments();
    assert_eq!(comments.len(), 1, "{comments:?}");
    assert!(
        comments[0].contains("the T3 Code thread https://t3.test/thread-0 of the job job-")
            && comments[0].contains("waits for an answer. Answer it in T3 Code"),
        "{}",
        comments[0]
    );
    assert_eq!(std::fs::read_to_string(root.join("t3.log")).unwrap().lines().count(), 1, "no second thread started");
}

/// DEV-599: a queue entry waiting for the ticket's running job, or for nothing after its earlier threads ended, asked
/// T3 Code about each earlier thread on every retry. Now the active job is checked first, and the answer that no thread
/// runs counts for a while too.
#[test]
fn a_start_on_a_busy_ticket_or_after_an_idle_answer_asks_t3_code_nothing() {
    let (fixtures, _env) = t3_harness("");
    let root = fixtures.get_temp_path();
    run_t3(&fixtures, "fix").success();
    let shows = || std::fs::read_to_string(root.join("t3-show.log")).unwrap_or_default().lines().filter(|line| *line == "show thread-0").count();
    let before = shows();
    // No free slot: the job waits in this process's registry, and no thread starts.
    let config = std::fs::read_to_string(fixtures.tasks_root.join("config.yml")).unwrap();
    std::fs::write(fixtures.tasks_root.join("config.yml"), config.replace("    enabled: true\n", "    enabled: true\n    max_parallel_jobs: 0\n")).unwrap();
    AgentJobService::set_orchestrator_mode(AgentOrchestratorMode::Server);
    let start = || {
        let req = AgentJobCreateRequest { ticket_id: "TEST-1".into(), prompt: "Review it".into(), agent: Some("review".into()), ..Default::default() };
        AgentJobService::start_job_with_tasks_dir(req, fixtures.tasks_root.as_path())
    };
    let job = start().unwrap();
    assert_eq!(shows(), before + 1, "the first start asks about the fix's thread");
    let err = start().unwrap_err();
    assert!(err.to_string().contains("already has an active job"), "{err}");
    assert_eq!(shows(), before + 1, "a start on a ticket with an active job doesn't ask");
    AgentJobService::cancel_job(&job.id).unwrap();
    let job = start().unwrap();
    assert_eq!(shows(), before + 1, "a start soon after the idle answer doesn't ask again");
    AgentJobService::cancel_job(&job.id).unwrap();
}

/// DEV-609: the idle answer counts only for the threads it was found for. A T3 job that ends on the ticket after it
/// adds a thread, and the next start asks T3 Code again, well within `T3_THREAD_RECHECK`.
#[test]
fn a_t3_job_ending_on_the_ticket_drops_its_cached_idle_answer() {
    let (fixtures, _env) = t3_harness("");
    let root = fixtures.get_temp_path();
    run_t3(&fixtures, "fix").success();
    let shows = || std::fs::read_to_string(root.join("t3-show.log")).unwrap_or_default().lines().count();
    let config_path = fixtures.tasks_root.join("config.yml");
    let config = std::fs::read_to_string(&config_path).unwrap();
    // No free slot: an in-process start only queues its job, and no thread starts.
    let no_slots = || std::fs::write(&config_path, config.replace("    enabled: true\n", "    enabled: true\n    max_parallel_jobs: 0\n")).unwrap();
    AgentJobService::set_orchestrator_mode(AgentOrchestratorMode::Server);
    let start = || {
        let req = AgentJobCreateRequest { ticket_id: "TEST-1".into(), prompt: "Review it".into(), agent: Some("review".into()), ..Default::default() };
        AgentJobService::start_job_with_tasks_dir(req, fixtures.tasks_root.as_path()).unwrap()
    };
    no_slots();
    let before = shows();
    AgentJobService::cancel_job(&start().id).unwrap();
    assert_eq!(shows(), before + 1, "the first start asks about the fix's thread and caches the idle answer");
    std::fs::write(&config_path, &config).unwrap();
    run_t3(&fixtures, "review").success();
    no_slots();
    let before = shows();
    AgentJobService::cancel_job(&start().id).unwrap();
    assert_eq!(shows(), before + 2, "with the review's thread added, the next start asks about both threads again");
    AgentJobService::cancel_job(&start().id).unwrap();
    assert_eq!(shows(), before + 2, "and caches that answer");
}

/// DEV-581, DEV-652: headless jobs and T3 Code threads of a ticket share its worktree. The headless review and merge
/// run there, the rework continues the fix's thread there, and Done cleanup removes it.
#[test]
fn headless_jobs_and_t3_threads_of_a_ticket_share_its_worktree() {
    let fix = "if [ -f fix.txt ]; then echo again >> fix.txt && git commit -qam TEST-1-rework; else echo fix > fix.txt && git add fix.txt && git commit -qm TEST-1-fix; fi";
    let (fixtures, _env) = t3_harness_and(
        &[("fix", fix, "    session: continue\n")],
        "  review:\n    runner: command\n    command: /bin/sh\n    args: [\"-c\", \"pwd >> ../../review.log\"]\n  merge:\n    runner: command\n    command: /bin/sh\n    args: [\"-c\", \"pwd >> ../../review.log && git -C ../.. merge -q --ff-only agent/TEST-1\"]\n",
    );
    let root = fixtures.get_temp_path();
    let run = |profile: &str| {
        let mut cmd = crate::common::lotar_cmd().unwrap();
        cmd.current_dir(root).args(["agent", "run", "TEST-1", "Work on it", "--agent", profile, "--wait"]);
        cmd.assert()
    };
    run("fix").success();
    let tree = std::fs::canonicalize(ticket_worktree(&fixtures)).unwrap();
    assert_eq!(rev(&tree, "HEAD"), rev(root, "agent/TEST-1"), "the fix thread works on the ticket's branch");
    run("review").success();
    run("fix").success();
    let log = std::fs::read_to_string(root.join("t3.log")).unwrap();
    assert!(log.lines().last().unwrap_or_default().starts_with("send thread-0 agent/TEST-1 "), "the rework continues the fix thread in the worktree: {log}");
    assert!(std::fs::read_to_string(tree.join("fix.txt")).unwrap().contains("again"));
    run("merge").success();
    let pwds = std::fs::read_to_string(root.join("review.log")).unwrap();
    assert_eq!(pwds.lines().map(|line| std::fs::canonicalize(line).unwrap()).collect::<Vec<_>>(), vec![tree.clone(), tree.clone()], "{pwds}");
    assert_eq!(rev(root, "main"), rev(root, "agent/TEST-1"), "the merge fast-forwarded main");
    for args in [&["config", "set", "agent_worktree_cleanup_on_done", "true", "--global"][..], &["status", "TEST-1", "Done"]] {
        crate::common::lotar_cmd().unwrap().current_dir(root).args(args).assert().success();
    }
    assert!(!tree.exists(), "Done cleanup removed the ticket's worktree");
}

/// A `fix` thread that commits, then amends, and a `merge` thread that hands the ticket back; both continue their
/// threads. The first merge starts its own (`--session new`), else it would continue the fix's.
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

/// DEV-550: a merge that continues an earlier merge thread ran at the fix the rework had amended since. A continued
/// thread now runs in the ticket's worktree, at the amended rework.
#[test]
fn a_continued_merge_thread_starts_from_the_rework_that_amended_the_ticket_branch() {
    let (fixtures, _env) = amending_fix_and_merge();
    let root = fixtures.get_temp_path();
    run_t3(&fixtures, "fix").success();
    let fix = rev(root, "agent/TEST-1");
    run_t3_with(&fixtures, "merge", &["--session", "new"]).success();
    assert_ne!(rev(root, "main"), fix, "the merge handed the ticket back and left main alone");
    run_t3(&fixtures, "fix").success();
    let rework = rev(root, "agent/TEST-1");
    assert_ne!(rework, fix, "the rework amended the fix");
    run_t3(&fixtures, "merge").success();
    let log = std::fs::read_to_string(root.join("t3.log")).unwrap();
    let merge = log.lines().last().unwrap_or_default();
    assert_eq!(merge, format!("send thread-1 agent/TEST-1 {rework}"), "the continued merge thread starts from the rework: {log}");
}

/// DEV-565: the owner decided 2026-11-11 that the merge runs in the reviewer's thread (DEV-368). A `session: continue`
/// job without a session of its own profile takes the next turn of the ticket's newest session, whatever profile ran
/// it, as before harnessmux (DEV-503). The review's end rule starts the merge, and its turn runs with the merge
/// profile's env (its `FAKE_RUN`).
#[test]
fn a_continued_merge_without_a_session_of_its_own_takes_the_reviews_thread() {
    let (fixtures, _env) = t3_harness_with(&[("review", "", ""), ("merge", "touch merged", "    session: continue\n")]);
    let root = fixtures.get_temp_path();
    lotar::services::automation_service::AutomationService::set(
        &fixtures.tasks_root,
        None,
        "automation:\n  rules:\n    - name: Reviewed\n      when:\n        not: { assignee: \"@merge\" }\n      on:\n        job_completed:\n          set: { assignee: \"@merge\" }\n          agent: { session: continue, prompt: Merge it }\n",
    )
    .expect("automation rules");
    run_t3(&fixtures, "review").success();
    wait_for("the merge", || root.join("t3.log").is_file() && std::fs::read_to_string(root.join("t3.log")).unwrap().lines().count() == 2);
    let log = std::fs::read_to_string(root.join("t3.log")).unwrap();
    assert!(log.lines().last().unwrap_or_default().starts_with("send thread-0 "), "the merge continues the review's thread: {log}");
    wait_for("the merge's turn", || ticket_worktree(&fixtures).join("merged").is_file());
    wait_for_jobs_to_end(&fixtures.tasks_root);
}

/// DEV-671: a `session: continue` job of a claude profile on a ticket whose only session is a T3 Code thread doesn't
/// take the thread's next turn: it starts a claude session of its own and the job log says why.
#[test]
fn a_continued_job_on_another_runner_starts_a_session_of_its_own() {
    let (fixtures, _env) = t3_harness_and(&[("fix", "", "")], "  review:\n    runner: claude\n    session: continue\n    env:\n      FAKE_LOG: /dev/null\n");
    let root = fixtures.get_temp_path();
    let acp = root.join("fake-acp.sh");
    std::fs::write(&acp, FAKE_ACP_AGENT).unwrap();
    std::fs::set_permissions(&acp, std::fs::Permissions::from_mode(0o755)).unwrap();
    let agents = root.join("harnessmux/config/agents.toml");
    let t3 = std::fs::read_to_string(&agents).unwrap();
    std::fs::write(&agents, format!("{t3}[agents.claude]\ncommand = [\"{}\"]\n", acp.display())).unwrap();
    run_t3(&fixtures, "fix").success();
    run_t3(&fixtures, "review").success();
    let log = std::fs::read_to_string(root.join("t3.log")).unwrap();
    assert_eq!(log.lines().count(), 1, "the review sends the fix thread nothing: {log}");
    let review = std::fs::read_dir(fixtures.tasks_root.join("@agent-logs"))
        .unwrap()
        .map(|entry| std::fs::read_to_string(entry.unwrap().path()).unwrap_or_default())
        .find(|job| job.contains("\"agent_profile\":\"review\""))
        .expect("the review's job log");
    assert!(review.contains("\"runner\":\"claude\""), "the review runs on claude: {review}");
    assert!(review.contains("runs the t3 agent, not claude; this job starts a new one."), "the job log says why: {review}");
    wait_for_jobs_to_end(&fixtures.tasks_root);
}

/// DEV-565, in one process (`lotar serve`): the merge takes over the review's session while it is still open there, and
/// its turn runs with the merge profile's env, not the review's.
#[test]
fn a_continued_merge_in_the_reviews_process_runs_with_the_merge_profiles_env() {
    let (fixtures, _env) = t3_harness_with(&[("review", "", ""), ("merge", "touch merged", "    session: continue\n")]);
    let root = fixtures.get_temp_path();
    lotar::services::automation_service::AutomationService::set(
        &fixtures.tasks_root,
        None,
        "automation:\n  rules:\n    - name: Reviewed\n      when:\n        assignee: \"@review\"\n      on:\n        job_completed:\n          set: { assignee: \"@merge\" }\n",
    )
    .expect("automation rules");
    AgentJobService::set_orchestrator_mode(AgentOrchestratorMode::Server);
    let mut storage = fixtures.create_storage();
    TaskService::update(&mut storage, "TEST-1", lotar::api_types::TaskUpdate { assignee: Some("@review".into()), ..Default::default() }).unwrap();
    wait_for("the merge's turn", || ticket_worktree(&fixtures).join("merged").is_file());
    let log = std::fs::read_to_string(root.join("t3.log")).unwrap();
    assert!(log.lines().last().unwrap_or_default().starts_with("send thread-0 "), "the merge continues the review's thread: {log}");
    wait_for_jobs_to_end(&fixtures.tasks_root);
}

/// DEV-602 (standards STD-133): the fix thread committed on an earlier thread's branch `t3/x`, created from
/// `agent/TEST-1`, so the ticket's branch stayed at main. The review and the merge threads found nothing, and the
/// merge counted as landed with the fix not on main. A job now starts from the ticket's branch with the newest work
/// missing from main.
#[test]
fn review_and_merge_start_from_the_branch_that_holds_the_fix() {
    let elsewhere = tempfile::tempdir().unwrap();
    let earlier = elsewhere.path().join("earlier");
    let fix = format!("cd {} && echo fix > fix.txt && git add fix.txt && git commit -qm TEST-1-fix", earlier.display());
    let (fixtures, _env) = t3_harness_with(&[("fix", &fix, ""), ("review", "", ""), ("merge", "", "")]);
    let root = fixtures.get_temp_path();
    git(root, &["branch", "agent/TEST-1"]);
    git(root, &["worktree", "add", "-q", "-b", "t3/x", earlier.to_str().unwrap(), "agent/TEST-1"]);
    let start = rev(root, "main");
    run_t3(&fixtures, "fix").success();
    let fix = rev(root, "t3/x");
    assert_ne!(fix, start, "the fix committed on t3/x");
    run_t3(&fixtures, "review").success();
    let log = std::fs::read_to_string(root.join("t3.log")).unwrap();
    assert_eq!(log.lines().nth(1).unwrap_or_default(), format!("thread-1 agent/TEST-1 {fix}"), "the review starts from the fix: {log}");
    run_t3(&fixtures, "merge").success();
    assert_eq!(rev(root, "main"), fix, "the merge landed the fix on main");
}

/// DEV-602: a merge whose branch has nothing beyond main fails, naming the branch with the ticket's fix, so the
/// ticket can't go Done through the merge step without its fix on main.
#[test]
fn a_merge_that_brings_nothing_while_the_ticket_has_unlanded_work_fails() {
    let elsewhere = tempfile::tempdir().unwrap();
    let earlier = elsewhere.path().join("earlier");
    let fix = format!("cd {} && echo fix > fix.txt && git add fix.txt && git commit -qm TEST-1-fix", earlier.display());
    let (fixtures, _env) = t3_harness_with(&[("fix", &fix, ""), ("merge", "git reset -q --hard main", "")]);
    let root = fixtures.get_temp_path();
    git(root, &["branch", "agent/TEST-1"]);
    git(root, &["worktree", "add", "-q", "-b", "t3/x", earlier.to_str().unwrap(), "agent/TEST-1"]);
    let start = rev(root, "main");
    run_t3(&fixtures, "fix").success();
    run_t3(&fixtures, "merge").failure().stdout(predicate::str::contains(
        "Merge job finished, but agent/TEST-1 has nothing beyond the main checkout's branch, while t3/x holds TEST-1's work that isn't in it",
    ));
    assert_eq!(rev(root, "main"), start, "main stays where it was");
}

/// DEV-602, as the DEV-593 merge left it: main holds the fix rebased and amended under a new hash, and an earlier
/// thread's `t3/old` still holds a copy whose content differs. That copy is not the ticket's newest work: a job
/// leaves the ticket's branch on main, and a merge that brings nothing passes.
#[test]
fn a_leftover_copy_of_a_fix_that_main_supersedes_is_not_unlanded_work() {
    let (fixtures, _env) = t3_harness_with(&[("merge", "", "")]);
    let root = fixtures.get_temp_path();
    let commit = |message: &str, date: &str| {
        let status = std::process::Command::new("git").args(["commit", "-qam", message]).env("GIT_COMMITTER_DATE", date).current_dir(root).status().unwrap();
        assert!(status.success());
    };
    git(root, &["checkout", "-q", "-b", "agent/TEST-1"]);
    std::fs::write(root.join("fix.txt"), "fix\n").unwrap();
    git(root, &["add", "fix.txt"]);
    commit("TEST-1 fix", "2026-11-15T10:00:00Z");
    git(root, &["branch", "t3/old", "agent/TEST-1"]);
    git(root, &["checkout", "-q", "main"]);
    std::fs::write(root.join("fix.txt"), "fix, reworked in the merge\n").unwrap();
    git(root, &["add", "fix.txt"]);
    commit("TEST-1 fix", "2026-11-15T11:00:00Z");
    git(root, &["branch", "-f", "agent/TEST-1", "main"]);
    let main = rev(root, "main");
    run_t3(&fixtures, "merge").success();
    assert_eq!(rev(root, "agent/TEST-1"), main, "the ticket's branch stays on main, not at the old copy");
    assert_eq!(rev(root, "main"), main);
}

/// DEV-612: a fix job before DEV-652 logged its thread's `t3/topic-0`, which landed, and cleanup at Done removed it;
/// T3 Code gave the name to another ticket's thread. The reopened ticket's job log still names it, but it holds none
/// of the ticket's work: a merge leaves the ticket's branch where it is and passes.
#[test]
fn a_logged_branch_name_another_ticket_reuses_is_not_the_tickets_work() {
    let (fixtures, _env) = t3_harness_with(&[("fix", "echo fix > fix.txt && git add fix.txt && git commit -qm TEST-1-fix", ""), ("merge", "", "")]);
    let root = fixtures.get_temp_path();
    run_t3(&fixtures, "fix").success();
    // As a job before DEV-652 logged it: the thread's branch.
    for entry in std::fs::read_dir(fixtures.tasks_root.join("@agent-logs")).unwrap() {
        let path = entry.unwrap().path();
        if path.extension().is_some_and(|ext| ext == "jsonl") {
            let log = std::fs::read_to_string(&path).unwrap();
            std::fs::write(&path, log.replace("\"worktree_branch\":\"agent/TEST-1\"", "\"worktree_branch\":\"t3/topic-0\"")).unwrap();
        }
    }
    git(root, &["merge", "--ff-only", "-q", "agent/TEST-1"]);
    let elsewhere = tempfile::tempdir().unwrap();
    let tree = elsewhere.path().join("other");
    git(root, &["worktree", "add", "-q", "-b", "t3/topic-0", tree.to_str().unwrap(), "main"]);
    std::fs::write(tree.join("other.txt"), "other\n").unwrap();
    git(&tree, &["add", "other.txt"]);
    let status = std::process::Command::new("git")
        .args(["commit", "-qm", "TEST-10 other work"])
        .env("GIT_COMMITTER_DATE", "2099-01-01T00:00:00Z")
        .current_dir(&tree)
        .status()
        .unwrap();
    assert!(status.success());
    let main = rev(root, "main");
    run_t3(&fixtures, "merge").success();
    assert_eq!(rev(root, "agent/TEST-1"), main, "the ticket's branch doesn't follow the other ticket's work");
    assert_eq!(rev(root, "main"), main, "main stays where it was");
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

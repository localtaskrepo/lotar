//! DEV-584: a job that stops on a provider's usage limit pauses until the
//! reset instead of failing: no failure rule runs, its `job_paused` rules
//! do, the ticket keeps its status and assignee, and the job runs again on
//! the same profile after the reset. A shell script stands in for T3's CLI
//! and prints the provider's message as Codex and Claude do; another for
//! claude-agent-acp fails the turn with typed error data (DEV-711).
#![cfg(unix)]

use crate::common;
use common::TestFixtures;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, Instant};

use jiff::{Timestamp, ToSpan, tz::TimeZone};

const CODEX_LIMIT_2099: &str = "You've hit your usage limit. Upgrade to Pro or try again at Nov 20th, 2099 7:43 PM.";
const CLAUDE_LIMIT: &str = "Claude API rate limit reached. Try again later.";

/// `t3 thread start` counts its runs in `starts`; while that count is at
/// most `limited`'s, it prints `message` to stderr and exits 1 as T3's CLI
/// does when the provider refuses the turn. Otherwise the thread starts and
/// completes, or with a `fail` file prints it to stderr and exits 1.
const FAKE_T3: &str = r#"#!/bin/sh
dir=$(dirname "$0")
case "$1 $2" in
  "thread start")
    cat > /dev/null
    echo x >> "$dir/starts"
    if [ "$(wc -l < "$dir/starts")" -le "$(cat "$dir/limited")" ]; then cat "$dir/message" >&2; exit 1; fi
    echo "Thread t1" >&2
    if [ -f "$dir/fail" ]; then cat "$dir/fail" >&2; exit 1; fi
    echo "Started."; exit 0 ;;
  "thread send") cat > /dev/null; echo "Sent." ;;
  "thread show") cat "$dir/show.json" ;;
  "thread list") echo "[]" ;;
  "auth session") if [ "$3" = issue ]; then echo '{"sessionId":"s1","token":"secret"}'; else echo "Revoked session $4."; fi ;;
esac
"#;

/// An ACP agent on stdio that fails every prompt as claude-agent-acp does on
/// a usage limit: an internal error whose data names the kind and, in
/// `$dir/resets_at` (Unix seconds), the reset. The text names no limit, so
/// only the typed data can pause the job. Each prompt adds a line to
/// `$dir/starts`.
const FAKE_ACP: &str = r#"#!/bin/sh
dir=$(dirname "$0")
respond() { printf '{"jsonrpc":"2.0","id":%s,"result":%s}\n' "$1" "$2"; }
while IFS= read -r line; do
  id=$(printf '%s' "$line" | sed -nE 's/.*"id":("[^"]*"|[0-9]+).*/\1/p')
  case "$line" in
    *'"method":"initialize"'*)
      respond "$id" '{"protocolVersion":1,"agentCapabilities":{"loadSession":true},"agentInfo":{"name":"fake-acp","version":"1.0.0"},"authMethods":[]}' ;;
    *'"method":"session/new"'*) respond "$id" '{"sessionId":"fake-1"}' ;;
    *'"method":"session/prompt"'*)
      echo x >> "$dir/starts"
      printf '{"jsonrpc":"2.0","id":%s,"error":{"code":-32603,"message":"Internal error","data":{"message":"Claude stopped the turn","errorKind":"rate_limit","rateLimit":{"resetsAt":%s}}}}\n' "$id" "$(cat "$dir/resets_at")" ;;
    *'"method":'*) if [ -n "$id" ]; then respond "$id" '{}'; fi ;;
  esac
done
"#;

struct Workspace {
    _fixtures: TestFixtures,
    root: PathBuf,
    t3: PathBuf,
}

impl Drop for Workspace {
    /// Clear the queue and let the worker finish before the fixture goes.
    fn drop(&mut self) {
        let _ = lotar(self).args(["agent", "queue", "flush"]).output();
        let deadline = Instant::now() + Duration::from_secs(30);
        while Instant::now() < deadline {
            let out = lotar(self).args(["--format", "json", "agent", "queue"]).output().ok();
            let json: serde_json::Value = out.and_then(|out| serde_json::from_slice(&out.stdout).ok()).unwrap_or_default();
            if json["worker_state"] != "active" && json["running_jobs"] == 0 {
                return;
            }
            std::thread::sleep(Duration::from_millis(100));
        }
    }
}

/// Ticket TEST-1, assigned to `fixer`, whose t3 turns stop on `message`
/// `limited` times. Rules comment on the job's pause and failure and close
/// the ticket when the job completes.
fn workspace(message: &str, limited: u32) -> Workspace {
    let ws = workspace_on(FAKE_T3, "[agents.t3]\nkind = \"t3\"\n", "    runner: t3\n    provider: codex\n    model: gpt-5.5\n");
    std::fs::write(ws.t3.join("message"), format!("{message}\n")).unwrap();
    std::fs::write(ws.t3.join("limited"), limited.to_string()).unwrap();
    ws
}

/// `fake` as the harnessmux agent `agent` names (its `agents.toml` lines but
/// the command), for the `fixer` profile's `runner` lines.
fn workspace_on(fake: &str, agent: &str, runner: &str) -> Workspace {
    let fixtures = TestFixtures::new();
    let root = fixtures.get_temp_path().to_path_buf();
    let t3 = root.join("t3");
    std::fs::create_dir_all(&t3).unwrap();
    let script = t3.join("t3");
    std::fs::write(&script, fake).unwrap();
    std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).unwrap();
    std::fs::write(
        t3.join("show.json"),
        r#"{"thread":{"threadId":"t1","link":"https://app.t3.codes/threads/env/t1","status":"completed","activeRunId":null,"latestRunId":"r1","pendingRequestCount":0},"messages":[{"role":"assistant","runId":"r1","text":"Fixed."}]}"#,
    )
    .unwrap();
    let home = root.join("harnessmux");
    std::fs::create_dir_all(home.join("config")).unwrap();
    std::fs::write(home.join("config/agents.toml"), format!("{agent}command = [\"{}\"]\n", script.display())).unwrap();
    std::fs::write(
        fixtures.tasks_root.join("config.yml"),
        format!("default:\n  project: TEST\nissue:\n  states: [Todo, InProgress, HelpNeeded, Done]\nagents:\n  fixer:\n{runner}"),
    )
    .unwrap();
    std::fs::write(
        fixtures.tasks_root.join("automation.yml"),
        "automation:\n  rules:\n    - name: Paused\n      on:\n        job_paused:\n          comment: \"paused: ${{agent.runner}}\"\n    - name: Failed\n      on:\n        job_failed:\n          set:\n            status: HelpNeeded\n            assignee: owner\n          comment: \"failed\"\n    - name: Completed\n      on:\n        job_completed:\n          set:\n            status: Done\n",
    )
    .unwrap();
    let ws = Workspace { _fixtures: fixtures, root, t3 };
    assert!(lotar(&ws).args(["add", "Limited job", "--assignee", "fixer"]).status().unwrap().success());
    ws
}

fn lotar(ws: &Workspace) -> Command {
    let mut cmd = Command::new(common::lotar_bin());
    cmd.current_dir(&ws.root)
        .env_remove("RUST_TEST_THREADS")
        .env_remove("LOTAR_AGENT_QUEUE_DISABLE_WORKER")
        .env_remove("LOTAR_AGENT_JOB_ID")
        .env("LOTAR_IGNORE_HOME_CONFIG", "1")
        .env("LOTAR_AGENT_QUEUE_DIR", ws.root.join("queue"))
        .env("LOTAR_AGENT_USAGE_LIMIT_WAIT_SECONDS", "2")
        .env("HARNESSMUX_HOME", ws.root.join("harnessmux"))
        .env("T3CODE_HOME", ws.root.join("t3home"))
        .env("LOTAR_DEFAULT_REPORTER", "tester")
        .env("LOTAR_TEST_SILENT", "1");
    cmd
}

fn json(ws: &Workspace, args: &[&str]) -> serde_json::Value {
    let out = lotar(ws).arg("--format").arg("json").args(args).output().unwrap();
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
    serde_json::from_slice(&out.stdout).unwrap()
}

fn run(ws: &Workspace) {
    let out = lotar(ws).args(["agent", "run", "TEST-1", "Fix it", "--agent", "fixer"]).output().unwrap();
    assert!(out.status.success(), "{}{}", String::from_utf8_lossy(&out.stdout), String::from_utf8_lossy(&out.stderr));
}

fn wait_for(what: &str, done: impl Fn() -> bool) {
    let deadline = Instant::now() + Duration::from_secs(60);
    while !done() {
        assert!(Instant::now() < deadline, "timed out waiting for {what}");
        std::thread::sleep(Duration::from_millis(100));
    }
}

fn jobs(ws: &Workspace, status: &str) -> Vec<serde_json::Value> {
    let list = json(ws, &["agent", "list-jobs"]);
    list["jobs"].as_array().unwrap().iter().filter(|job| job["status"] == status).cloned().collect()
}

fn comments(ws: &Workspace) -> Vec<String> {
    let show = json(ws, &["show", "TEST-1"]);
    show["comments"].as_array().map(|comments| comments.iter().filter_map(|c| c["text"].as_str().map(str::to_string)).collect()).unwrap_or_default()
}

fn starts(t3: &Path) -> usize {
    std::fs::read_to_string(t3.join("starts")).unwrap_or_default().lines().count()
}

/// A reset 3 days from now, to the minute: the time and Codex's message
/// naming it in the machine's time zone, as Codex prints it.
fn codex_limit_in_3_days() -> (Timestamp, String) {
    let zone = TimeZone::system();
    let at = zone.to_datetime(Timestamp::now()).checked_add(3.days()).unwrap().with().second(0).subsec_nanosecond(0).build().unwrap();
    let message = format!("You've hit your usage limit. Upgrade to Pro or try again at {}.", at.strftime("%b %-d, %Y %-I:%M %p"));
    (zone.to_timestamp(at).unwrap(), message)
}

#[test]
fn codexs_usage_limit_pauses_the_job_until_the_reset_it_names() {
    let (reset, message) = codex_limit_in_3_days();
    let ws = workspace(&message, 1);
    run(&ws);
    wait_for("the job to pause", || !jobs(&ws, "paused").is_empty());
    let paused = jobs(&ws, "paused").remove(0);
    let resumes_at = paused["resumes_at"].as_str().unwrap_or_default().to_string();
    assert_eq!(resumes_at.parse::<Timestamp>().ok(), Some(reset), "it runs again at the reset Codex names: {paused}");
    let summary = paused["summary"].as_str().unwrap_or_default();
    assert!(summary.contains("Paused, not failed: codex hit its usage limit") && summary.contains("@fixer"), "{summary}");
    assert!(jobs(&ws, "failed").is_empty(), "no job failed");

    let show = json(&ws, &["show", "TEST-1"]);
    assert_eq!((&show["status"], &show["assignee"]), (&"Todo".into(), &"fixer".into()), "the ticket keeps its status and assignee: {show}");
    wait_for("the job_paused rule's comment", || comments(&ws).iter().any(|text| text == "paused: t3"));
    assert!(!comments(&ws).iter().any(|text| text == "failed"), "no failure rule ran: {:?}", comments(&ws));

    let queue = json(&ws, &["agent", "queue"]);
    let entry = &queue["pending"][0];
    assert_eq!((&entry["ticket_id"], &entry["agent"]), (&"TEST-1".into(), &"fixer".into()), "its retry is queued on the same profile: {queue}");
    assert_eq!(entry["not_before"].as_str(), Some(resumes_at.as_str()), "{queue}");
    assert!(queue["reason"].as_str().is_some_and(|reason| reason.contains("usage limit")), "{queue}");
    std::thread::sleep(Duration::from_secs(1));
    assert_eq!(starts(&ws.t3), 1, "the retry waits for the reset");

    let text = lotar(&ws).args(["agent", "list-jobs"]).output().unwrap();
    let text = String::from_utf8_lossy(&text.stdout);
    assert!(text.contains(&format!("paused (until {resumes_at})")), "{text}");
}

/// A reset more than 7 days away isn't believed (DEV-770): the job waits the
/// default, its note names the ignored time, and its retry completes.
#[test]
fn a_reset_more_than_7_days_away_waits_the_default_and_says_so() {
    let ws = workspace(CODEX_LIMIT_2099, 1);
    run(&ws);
    wait_for("the job to pause", || !jobs(&ws, "paused").is_empty());
    let paused = jobs(&ws, "paused").remove(0);
    let resumes_at = paused["resumes_at"].as_str().unwrap_or_default().parse::<Timestamp>().unwrap();
    assert!(resumes_at < Timestamp::now().saturating_add(1.hour()).unwrap(), "it waits the default, not until 2099: {paused}");
    let summary = paused["summary"].as_str().unwrap_or_default();
    assert!(summary.contains("after the default wait (ignored the named reset 2099-11-2") && summary.contains("more than 7 days away)"), "{summary}");
    wait_for("the retry to complete", || !jobs(&ws, "completed").is_empty());
    assert_eq!(starts(&ws.t3), 2);
}

/// A limit with no reset time waits the default; a retry that hits it again
/// pauses again, and the one after the reset completes as the job would have.
#[test]
fn claudes_rate_limit_waits_the_default_and_pauses_again_until_a_retry_completes() {
    let ws = workspace(CLAUDE_LIMIT, 2);
    run(&ws);
    wait_for("the job to complete", || !jobs(&ws, "completed").is_empty());
    assert_eq!(jobs(&ws, "paused").len(), 2, "both limited runs paused");
    assert!(jobs(&ws, "failed").is_empty(), "no job failed");
    assert_eq!(starts(&ws.t3), 3);
    let comments = comments(&ws);
    assert_eq!(comments.iter().filter(|text| *text == "paused: t3").count(), 2, "{comments:?}");
    assert!(!comments.iter().any(|text| text == "failed"), "{comments:?}");
    wait_for("the completed job's rule", || json(&ws, &["show", "TEST-1"])["status"] == "Done");
}

/// Only the runner's failure report names a limit: a job that fails for
/// another reason while its agent talked about a rate limit fails, and its
/// failure rules run.
#[test]
fn an_agent_that_only_talks_about_a_rate_limit_fails() {
    let ws = workspace(CLAUDE_LIMIT, 0);
    std::fs::write(
        ws.t3.join("show.json"),
        r#"{"thread":{"threadId":"t1","link":"https://app.t3.codes/threads/env/t1","status":"failed","activeRunId":null,"latestRunId":"r1","pendingRequestCount":0},"messages":[{"role":"assistant","runId":"r1","text":"I checked the rate limit code: too many requests, try again at 5pm."}]}"#,
    )
    .unwrap();
    std::fs::write(ws.t3.join("fail"), "error: the tests fail\n").unwrap();
    run(&ws);
    wait_for("the job to fail", || !jobs(&ws, "failed").is_empty());
    assert!(jobs(&ws, "paused").is_empty(), "nothing paused");
    wait_for("the job_failed rule", || json(&ws, &["show", "TEST-1"])["status"] == "HelpNeeded");
    wait_for("the job_failed rule's comment", || comments(&ws).iter().any(|text| text == "failed"));
    assert!(!comments(&ws).iter().any(|text| text.starts_with("paused")), "{:?}", comments(&ws));
}

/// harnessmux 0.2.0 types the failure (DEV-711): a rate limit the agent's
/// error data names pauses the job until its `resetsAt`, though the text
/// names no limit.
#[test]
fn a_typed_rate_limit_pauses_the_job_until_its_resets_at() {
    let ws = workspace_on(FAKE_ACP, "[agents.claude]\n", "    runner: claude\n");
    let reset = Timestamp::from_second(Timestamp::now().as_second() + 3 * 24 * 3600).unwrap();
    std::fs::write(ws.t3.join("resets_at"), reset.as_second().to_string()).unwrap();
    run(&ws);
    wait_for("the job to pause", || !jobs(&ws, "paused").is_empty());
    let paused = jobs(&ws, "paused").remove(0);
    let resumes_at = paused["resumes_at"].as_str().unwrap_or_default().to_string();
    assert_eq!(resumes_at.parse::<Timestamp>().ok(), Some(reset), "it runs again at the reset the error data names: {paused}");
    let summary = paused["summary"].as_str().unwrap_or_default();
    assert!(summary.contains("hit its usage limit (Claude stopped the turn)") && summary.contains("after the reset"), "{summary}");
    assert!(jobs(&ws, "failed").is_empty(), "no job failed");
    let queue = json(&ws, &["agent", "queue"]);
    assert_eq!(queue["pending"][0]["not_before"].as_str(), Some(resumes_at.as_str()), "{queue}");
    wait_for("the job_paused rule's comment", || comments(&ws).iter().any(|text| text == "paused: claude"));
    assert!(!comments(&ws).iter().any(|text| text == "failed"), "no failure rule ran: {:?}", comments(&ws));
    assert_eq!(starts(&ws.t3), 1, "the retry waits for the reset");
}

/// DEV-832: T3 Code resumes each thread it paused on a usage limit after the
/// reset, so a retry that started its own thread ran beside the paused one
/// in the same worktree. A t3 job's retry continues the paused job's thread,
/// also past `continue_within_minutes`: no second thread starts.
#[test]
fn a_t3_jobs_retry_after_a_usage_limit_continues_the_paused_thread() {
    let limited_thread = FAKE_T3.replace(r#"cat "$dir/message" >&2; exit 1; fi"#, r#"echo "Thread t1" >&2; cat "$dir/message" >&2; exit 1; fi"#);
    let ws = workspace_on(
        &limited_thread,
        "[agents.t3]\nkind = \"t3\"\n",
        "    runner: t3\n    provider: codex\n    model: gpt-5.5\n    session: continue\n    continue_within_minutes: 0\n",
    );
    std::fs::write(ws.t3.join("message"), format!("{CLAUDE_LIMIT}\n")).unwrap();
    std::fs::write(ws.t3.join("limited"), "1").unwrap();
    run(&ws);
    wait_for("the retry to end", || {
        let list = json(&ws, &["agent", "list-jobs"]);
        list["jobs"].as_array().unwrap().iter().any(|job| job["status"] == "completed" || job["status"] == "failed")
    });
    assert_eq!(jobs(&ws, "paused").len(), 1, "the first run paused");
    assert_eq!(starts(&ws.t3), 1, "the retry started no second thread");
    let retry = json(&ws, &["agent", "list-jobs"])["jobs"]
        .as_array()
        .unwrap()
        .iter()
        .find(|job| job["status"] != "paused")
        .and_then(|job| job["id"].as_str().map(str::to_string))
        .unwrap();
    let log = lotar(&ws).args(["agent", "logs", &retry]).output().unwrap();
    let log = String::from_utf8_lossy(&log.stdout);
    assert!(log.contains("this job retries one paused on a usage limit, in the same T3 Code thread"), "{log}");
}

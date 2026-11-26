//! The live state of a `t3` job's T3 Code thread for the job detail (DEV-849).
//!
//! The thread is the one the job's log records: the last `agent_job_init`
//! entry names it with its link (`t3 session started, <thread>: <link>`), and
//! an ended job's status line keeps it too. So each job of a ticket resolves
//! to its own thread, also when several jobs ran several threads. lotar asks
//! the T3 Code server on this machine with `t3 thread show`, at most once per
//! thread in [`RECHECK`] and for at most [`TIMEOUT`], and only reads: the job's
//! outcome and the ticket stay as they are.
//!
//! The milestones harnessmux reads from T3 Code's MCP events (DEV-877) are in
//! the job's log too. The newest one since the job's last prompt gives the
//! state when T3 Code doesn't answer, or when `t3 thread show` hasn't caught up
//! with it: it still shows the settled run as the active one, or a question
//! came after its last change. A thread it shows past the run (waiting on
//! background work, a newer run) keeps its own state.

use super::{AgentJobService, saved_job, t3_cli, t3_thread_waits};
use crate::api_types::{AgentJobEvent, AgentJobT3Thread};
use crate::config::types::AgentRunnerKind;
use std::collections::HashMap;
use std::path::Path;
use std::process::Command;
use std::sync::{LazyLock, Mutex};
use std::time::{Duration, Instant};

/// How long one `t3 thread show` may take.
const TIMEOUT: Duration = Duration::from_secs(10);

/// How long an answer counts: every open job detail polls, so several
/// viewers of one thread share one `t3` call.
const RECHECK: Duration = Duration::from_secs(10);

/// What lotar made of a `t3 thread show` answer, and the run it shows active.
#[derive(Clone, Default)]
struct Answer {
    thread: AgentJobT3Thread,
    active_run: Option<String>,
}

/// Per thread: when T3 Code answered and what lotar made of it.
static ANSWERS: LazyLock<Mutex<HashMap<String, (Instant, Answer)>>> = LazyLock::new(|| Mutex::new(HashMap::new()));

impl AgentJobService {
    /// The T3 Code thread of the job `id` and its state now. `None` for a job
    /// neither this process nor a log knows; `Some(Err)` for a job of another
    /// runner.
    pub fn t3_thread_in(tasks_dir: &Path, id: &str) -> Option<Result<AgentJobT3Thread, String>> {
        let (job, events) = Self::job_with_events_in(tasks_dir, id)?;
        if job.runner != AgentRunnerKind::T3.as_str() {
            return Some(Err(format!("The job {id} runs on {}, not in a T3 Code thread", job.runner)));
        }
        let recorded = recorded_thread(&events);
        let thread = recorded.as_ref().map(|(thread, _)| thread.clone()).or_else(|| saved_job(tasks_dir, id)?.status?.agent_session);
        let link = recorded.and_then(|(_, link)| link);
        let milestone = last_milestone(&events);
        let Some(thread) = thread else {
            return Some(Ok(unavailable(None, link, "The job recorded no T3 Code thread: it ended before its thread started, or its log has none.")));
        };
        if let Some((at, answer)) = ANSWERS.lock().ok().and_then(|answers| answers.get(&thread).cloned())
            && at.elapsed() < RECHECK
        {
            return Some(Ok(with_milestone(answer, milestone)));
        }
        let answer = match t3_cli(tasks_dir) {
            None => Answer::from(unavailable(Some(thread.clone()), link, "T3 Code's CLI (t3) isn't installed or harnessmux has no t3 agent.")),
            Some((program, env, cwd)) => {
                let mut command = Command::new(&program);
                command.args(["thread", "show", &thread, "--json", "--messages", "0"]).envs(env.iter().map(|(key, value)| (key, value))).current_dir(&cwd);
                ask(&mut command, &thread, link)
            }
        };
        if let Ok(mut answers) = ANSWERS.lock() {
            answers.retain(|_, (at, _)| at.elapsed() < RECHECK);
            answers.insert(thread, (Instant::now(), answer.clone()));
        }
        Some(Ok(with_milestone(answer, milestone)))
    }
}

impl From<AgentJobT3Thread> for Answer {
    fn from(thread: AgentJobT3Thread) -> Self {
        Self { thread, active_run: None }
    }
}

/// Runs `t3 thread show` and reads its answer.
fn ask(command: &mut Command, thread: &str, link: Option<String>) -> Answer {
    asked(command, thread, link.clone()).unwrap_or_else(|reason| unavailable(Some(thread.to_string()), link, &reason).into())
}

/// `t3 thread show`'s answer, or why there is none.
fn asked(command: &mut Command, thread: &str, link: Option<String>) -> Result<Answer, String> {
    let output = match crate::utils::git::output_within(command, TIMEOUT) {
        Ok(output) => output,
        Err(err) if err.kind() == std::io::ErrorKind::TimedOut => {
            return Err(format!("T3 Code didn't answer within {} seconds.", TIMEOUT.as_secs()));
        }
        Err(err) => return Err(format!("Could not run T3 Code's CLI: {err}")),
    };
    if !output.status.success() {
        let why = first_error_line(&String::from_utf8_lossy(&output.stderr)).unwrap_or_else(|| "it exited with an error".to_string());
        return Err(format!("T3 Code can't show the thread (is the T3 Code server running, and does the thread still exist?): {why}"));
    }
    let shown = serde_json::from_slice::<serde_json::Value>(&output.stdout).ok().and_then(|mut shown| shown.get_mut("thread").map(serde_json::Value::take));
    let shown = shown.ok_or("T3 Code's answer has no thread in it.")?;
    Ok(Answer { thread: from_shown(thread, link, &shown), active_run: active_run(&shown) })
}

/// The state of a thread `t3 thread show --json` describes. A question for
/// the owner wins over everything; a thread that ended its turn on
/// `t3_background_wait` (T3 shows it `waiting` or `completed`) still works
/// (DEV-857): its job ends after the wake run.
pub(super) fn from_shown(thread: &str, link: Option<String>, shown: &serde_json::Value) -> AgentJobT3Thread {
    let status = shown.get("status").and_then(serde_json::Value::as_str).map(str::to_string);
    let active = shown.get("activeRunId").is_some_and(|run| !run.is_null());
    let state = if t3_thread_waits(shown) {
        "waiting"
    } else if active || waits_on_background_work(shown) {
        "running"
    } else if status.as_deref() == Some("waiting") {
        "waiting"
    } else {
        match status.as_deref() {
            Some("running" | "preparing" | "queued" | "starting") => "running",
            Some("completed" | "idle") => "finished",
            Some("failed") => "failed",
            Some("interrupted" | "cancelled" | "rolled_back") => "stopped",
            _ => "unknown",
        }
    };
    let link = link.or_else(|| shown.get("link").and_then(serde_json::Value::as_str).filter(|link| link.starts_with("http")).map(str::to_string));
    AgentJobT3Thread {
        thread_id: Some(thread.to_string()),
        environment_id: link.as_deref().and_then(environment_of),
        link,
        state: state.to_string(),
        t3_status: status,
        reason: None,
        updated_at: shown.get("updatedAt").and_then(serde_json::Value::as_str).map(str::to_string),
        checked_at: crate::utils::time::now_rfc3339(),
    }
}

/// The run `t3 thread show` shows active.
fn active_run(shown: &serde_json::Value) -> Option<String> {
    shown.get("activeRunId").and_then(serde_json::Value::as_str).map(str::to_string)
}

/// Whether the thread waits on its background commands or on another thread.
fn waits_on_background_work(shown: &serde_json::Value) -> bool {
    shown.get("pendingBackgroundTasks").and_then(serde_json::Value::as_array).is_some_and(|tasks| !tasks.is_empty())
        || shown.get("blockedOnThreadWait").and_then(serde_json::Value::as_bool) == Some(true)
}

fn unavailable(thread: Option<String>, link: Option<String>, reason: &str) -> AgentJobT3Thread {
    AgentJobT3Thread {
        thread_id: thread,
        environment_id: link.as_deref().and_then(environment_of),
        link,
        state: "unavailable".to_string(),
        reason: Some(reason.to_string()),
        checked_at: crate::utils::time::now_rfc3339(),
        ..AgentJobT3Thread::default()
    }
}

/// The thread and link of the job's last session start in its log:
/// `<agent> session <how>, <thread>[: <link>]`.
pub(super) fn recorded_thread(events: &[AgentJobEvent]) -> Option<(String, Option<String>)> {
    events.iter().rev().filter(|event| event.kind == "agent_job_init").find_map(|event| {
        let message = event.message.as_deref()?;
        let (_, rest) = message.split_once(" session ")?;
        let (_, named) = rest.split_once(", ")?;
        let (thread, link) = match named.split_once(": ") {
            Some((thread, link)) if link.starts_with("http") => (thread, Some(link.trim().to_string())),
            _ => (named, None),
        };
        let thread = thread.trim();
        (!thread.is_empty() && !thread.contains(' ')).then(|| (thread.to_string(), link))
    })
}

/// How a milestone starts in the job log.
const MILESTONE: &str = "T3 Code milestone ";

/// The log line of a milestone: `T3 Code milestone <name>: run <run>, outcome <outcome>, request <request>, kind <kind>`.
pub(super) fn milestone_message(name: &str, run: Option<&str>, outcome: Option<&str>, request: Option<&str>, kind: Option<&str>) -> String {
    let details = [("run", run), ("outcome", outcome), ("request", request), ("kind", kind)]
        .into_iter()
        .filter_map(|(label, value)| value.map(|value| format!("{label} {value}")))
        .collect::<Vec<_>>();
    if details.is_empty() { format!("{MILESTONE}{name}") } else { format!("{MILESTONE}{name}: {}", details.join(", ")) }
}

/// The thread's state from a milestone, its run and the run's outcome, and when it came.
#[derive(Debug, PartialEq)]
struct Milestone {
    state: &'static str,
    run: Option<String>,
    outcome: Option<String>,
    at: String,
}

/// The newest milestone in the log since the job's last prompt.
fn last_milestone(events: &[AgentJobEvent]) -> Option<Milestone> {
    for event in events.iter().rev() {
        if event.kind == "agent_job_input" {
            return None;
        }
        let Some(rest) = event.message.as_deref().filter(|_| event.kind == "agent_job_progress").and_then(|message| message.strip_prefix(MILESTONE)) else {
            continue;
        };
        let (name, details) = rest.split_once(": ").unwrap_or((rest, ""));
        let detail = |label: &str| details.split(", ").find_map(|detail| detail.strip_prefix(label)).map(str::to_string);
        let (run, outcome) = (detail("run "), detail("outcome "));
        let state = match (name, outcome.as_deref()) {
            ("thread.needs_input", _) => "waiting",
            ("thread.run_settled", Some("completed")) => "finished",
            ("thread.run_settled", Some("failed")) => "failed",
            ("thread.run_settled", Some("interrupted" | "cancelled")) => "stopped",
            ("thread.run_settled", _) => "unknown",
            _ => continue,
        };
        return Some(Milestone { state, run, outcome, at: event.at.clone() });
    }
    None
}

/// The state `t3 thread show` gave, or the milestone's when T3 Code didn't answer or `t3 thread show` is behind it: it
/// still shows the settled run active, or the question came after the thread's last change there. A thread it shows
/// past the run (waiting on background work after it, a newer run) keeps its state (DEV-857).
fn with_milestone(answer: Answer, milestone: Option<Milestone>) -> AgentJobT3Thread {
    let Answer { thread, active_run } = answer;
    let Some(milestone) = milestone else {
        return thread;
    };
    let behind = if thread.state == "unavailable" {
        true
    } else if milestone.state == "waiting" {
        let changed = thread.updated_at.as_deref().and_then(|at| crate::utils::time::parse_rfc3339(at).ok());
        match (changed, crate::utils::time::parse_rfc3339(&milestone.at)) {
            (Some(changed), Ok(at)) => at > changed,
            (None, _) => true,
            (Some(_), Err(_)) => false,
        }
    } else {
        milestone.run.is_some() && milestone.run == active_run
    };
    if !behind {
        return thread;
    }
    AgentJobT3Thread { state: milestone.state.to_string(), t3_status: milestone.outcome, reason: None, updated_at: Some(milestone.at), ..thread }
}

/// The environment of a T3 Code thread link `https://…/threads/<environment>/<thread>`.
fn environment_of(link: &str) -> Option<String> {
    let (_, path) = link.split_once("/threads/")?;
    let environment = path.split('/').next()?;
    (!environment.is_empty() && path.contains('/')).then(|| environment.to_string())
}

/// The first sentence of `t3`'s error output that says something: not Electron's
/// start-up noise (`[1124/…:ERROR:electron/…]`); a longer one is cut at 200 characters.
fn first_error_line(stderr: &str) -> Option<String> {
    let line = stderr.lines().map(str::trim).find(|line| !line.is_empty() && !line.contains("electron/") && !line.starts_with("at "))?;
    let sentence = line.find(". ").map_or(line, |end| &line[..=end]);
    if sentence.chars().count() <= 200 {
        return Some(sentence.to_string());
    }
    Some(sentence.chars().take(200).chain(['…']).collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn init(message: &str) -> AgentJobEvent {
        AgentJobEvent { kind: "agent_job_init".to_string(), message: Some(message.to_string()), ..AgentJobEvent::default() }
    }

    #[test]
    fn the_last_session_start_names_the_thread_and_link() {
        let events = vec![
            init("t3 session started, mcp:launch:old: https://app.t3.codes/threads/env-1/mcp%3Alaunch%3Aold"),
            AgentJobEvent { kind: "agent_job_message".to_string(), message: Some("t3 session started, nope".to_string()), ..AgentJobEvent::default() },
            init("t3 session resumed, mcp:launch:new: https://app.t3.codes/threads/env-2/mcp%3Alaunch%3Anew"),
        ];
        let (thread, link) = recorded_thread(&events).unwrap();
        assert_eq!(thread, "mcp:launch:new");
        assert_eq!(link.as_deref(), Some("https://app.t3.codes/threads/env-2/mcp%3Alaunch%3Anew"));
        assert_eq!(environment_of(link.as_deref().unwrap()).as_deref(), Some("env-2"));
    }

    #[test]
    fn a_session_start_without_link_or_thread() {
        assert_eq!(recorded_thread(&[init("t3 session started, thread-1")]), Some(("thread-1".to_string(), None)));
        assert_eq!(recorded_thread(&[init("claude session started")]), None);
        assert_eq!(recorded_thread(&[]), None);
    }

    #[test]
    fn thread_states_map_to_running_waiting_finished_failed_and_stopped() {
        let state = |shown: serde_json::Value| from_shown("t", None, &shown).state;
        assert_eq!(state(serde_json::json!({"status": "running"})), "running");
        assert_eq!(state(serde_json::json!({"status": "idle", "activeRunId": "run:1"})), "running");
        assert_eq!(state(serde_json::json!({"status": "running", "pendingRequestCount": 1})), "waiting");
        assert_eq!(state(serde_json::json!({"status": "waiting"})), "waiting");
        assert_eq!(state(serde_json::json!({"status": "completed", "activeRunId": null})), "finished");
        assert_eq!(state(serde_json::json!({"status": "failed"})), "failed");
        assert_eq!(state(serde_json::json!({"status": "interrupted"})), "stopped");
        assert_eq!(state(serde_json::json!({"status": "something-new"})), "unknown");
    }

    fn logged(kind: &str, at: &str, message: &str) -> AgentJobEvent {
        AgentJobEvent { kind: kind.to_string(), at: at.to_string(), message: Some(message.to_string()), ..AgentJobEvent::default() }
    }

    #[test]
    fn the_newest_milestone_since_the_last_prompt_gives_the_state() {
        let settled = |outcome: &str| milestone_message("thread.run_settled", Some("run:1"), Some(outcome), None, None);
        assert_eq!(settled("completed"), "T3 Code milestone thread.run_settled: run run:1, outcome completed");
        let needs_input = milestone_message("thread.needs_input", None, None, Some("req-1"), Some("approval"));
        let events = vec![
            logged("agent_job_progress", "2026-11-26T10:00:00Z", &needs_input),
            logged("agent_job_message", "2026-11-26T10:00:01Z", "T3 Code milestone thread.run_settled: run x, outcome failed"),
            logged("agent_job_progress", "2026-11-26T10:00:02Z", &settled("completed")),
            logged("agent_job_progress", "2026-11-26T10:00:03Z", "The agent's summary: done"),
        ];
        let milestone = last_milestone(&events).unwrap();
        assert_eq!(
            milestone,
            Milestone { state: "finished", run: Some("run:1".to_string()), outcome: Some("completed".to_string()), at: "2026-11-26T10:00:02Z".to_string() }
        );
        assert_eq!(last_milestone(&events[..1]).unwrap().state, "waiting");
        for (outcome, state) in [("failed", "failed"), ("interrupted", "stopped"), ("cancelled", "stopped"), ("new", "unknown")] {
            assert_eq!(last_milestone(&[logged("agent_job_progress", "t", &settled(outcome))]).unwrap().state, state);
        }
        let mut prompted = events;
        prompted.push(logged("agent_job_input", "2026-11-26T10:05:00Z", "Go on"));
        assert_eq!(last_milestone(&prompted), None, "a milestone before the job's last prompt is old");
    }

    fn settled_milestone() -> Option<Milestone> {
        Some(Milestone { state: "finished", run: Some("run:1".to_string()), outcome: Some("completed".to_string()), at: "2026-11-26T10:00:02Z".to_string() })
    }

    fn shown(shown: &serde_json::Value) -> Answer {
        Answer { thread: from_shown("t", None, shown), active_run: active_run(shown) }
    }

    #[test]
    fn a_settled_milestone_wins_over_a_thread_show_still_running_its_run_and_an_unavailable_one() {
        let stale =
            with_milestone(shown(&serde_json::json!({"status": "running", "activeRunId": "run:1", "updatedAt": "2026-11-26T10:00:00Z"})), settled_milestone());
        assert_eq!((stale.state.as_str(), stale.t3_status.as_deref()), ("finished", Some("completed")));
        assert_eq!(stale.updated_at.as_deref(), Some("2026-11-26T10:00:02Z"));
        let newer_run =
            with_milestone(shown(&serde_json::json!({"status": "running", "activeRunId": "run:2", "updatedAt": "2026-11-26T10:00:00Z"})), settled_milestone());
        assert_eq!(newer_run.state, "running", "a newer run in T3 Code wins");
        let down = with_milestone(unavailable(Some("t".to_string()), None, "T3 Code didn't answer.").into(), settled_milestone());
        assert_eq!((down.state.as_str(), down.reason), ("finished", None));
        let running = shown(&serde_json::json!({"status": "running", "activeRunId": "run:1", "updatedAt": "2026-11-26T10:00:00Z"}));
        assert_eq!(with_milestone(running, None).state, "running");
    }

    #[test]
    fn a_thread_waiting_on_background_work_stays_running_after_its_run_settled() {
        for waits in [serde_json::json!({"pendingBackgroundTasks": [{"id": "bg-1"}]}), serde_json::json!({"blockedOnThreadWait": true})] {
            let mut thread = serde_json::json!({"status": "completed", "activeRunId": null, "updatedAt": "2026-11-26T10:00:00Z"});
            thread.as_object_mut().unwrap().extend(waits.as_object().unwrap().clone());
            assert_eq!(with_milestone(shown(&thread), settled_milestone()).state, "running", "DEV-857: its job ends after the wake run");
        }
    }

    #[test]
    fn a_question_wins_when_it_came_after_the_thread_s_last_change() {
        let question = || Some(Milestone { state: "waiting", run: None, outcome: None, at: "2026-11-26T10:00:02Z".to_string() });
        let running = |updated: &str| shown(&serde_json::json!({"status": "running", "activeRunId": "run:1", "updatedAt": updated}));
        assert_eq!(with_milestone(running("2026-11-26T10:00:00Z"), question()).state, "waiting");
        assert_eq!(with_milestone(running("2026-11-26T10:01:00Z"), question()).state, "running", "a later change in T3 Code wins");
    }

    #[test]
    fn a_thread_waiting_on_background_work_runs() {
        let state = |shown: serde_json::Value| from_shown("t", None, &shown).state;
        assert_eq!(state(serde_json::json!({"status": "completed", "activeRunId": null, "pendingBackgroundTasks": [{"id": "bg:1"}]})), "running");
        assert_eq!(state(serde_json::json!({"status": "completed", "activeRunId": null, "blockedOnThreadWait": true})), "running");
        assert_eq!(state(serde_json::json!({"status": "completed", "pendingBackgroundTasks": [], "blockedOnThreadWait": false})), "finished");
        assert_eq!(state(serde_json::json!({"status": "completed", "pendingRequestCount": 1, "blockedOnThreadWait": true})), "waiting");
        assert_eq!(state(serde_json::json!({"status": "waiting", "activeRunId": null, "pendingBackgroundTasks": ["npm test"]})), "running");
        assert_eq!(state(serde_json::json!({"status": "waiting", "activeRunId": null, "blockedOnThreadWait": true})), "running");
        assert_eq!(state(serde_json::json!({"status": "waiting", "pendingRequestCount": 1, "pendingBackgroundTasks": ["npm test"]})), "waiting");
        assert_eq!(state(serde_json::json!({"status": "waiting", "pendingBackgroundTasks": [], "blockedOnThreadWait": false})), "waiting");
    }

    #[test]
    fn the_recorded_link_wins_over_the_one_t3_shows() {
        let shown = serde_json::json!({"status": "completed", "link": "https://app.t3.codes/threads/env-b/t", "updatedAt": "2026-11-24T21:00:00Z"});
        let answer = from_shown("t", Some("https://app.t3.codes/threads/env-a/t".to_string()), &shown);
        assert_eq!(answer.link.as_deref(), Some("https://app.t3.codes/threads/env-a/t"));
        assert_eq!(answer.environment_id.as_deref(), Some("env-a"));
        assert_eq!(answer.updated_at.as_deref(), Some("2026-11-24T21:00:00Z"));
        let shown_only = from_shown("t", None, &serde_json::json!({"status": "completed", "link": "t3-thread://v2/env/t"}));
        assert_eq!(shown_only.link, None, "only a web link opens in a browser");
    }

    #[test]
    fn the_error_reason_is_its_first_sentence() {
        let gone = "Thread t is no longer available. Thread ids belong to one environment: for a thread on a linked one, pass its environmentId.";
        assert_eq!(first_error_line(gone).as_deref(), Some("Thread t is no longer available."));
        assert_eq!(first_error_line("connect ECONNREFUSED 127.0.0.1:3773").as_deref(), Some("connect ECONNREFUSED 127.0.0.1:3773"));
        let long = "x".repeat(300);
        assert_eq!(first_error_line(&long).map(|line| line.chars().count()), Some(201));
    }

    #[cfg(unix)]
    #[test]
    fn an_unreachable_server_or_a_hanging_cli_is_unavailable() {
        let failing = ask(Command::new("sh").args(["-c", "echo '[1124/1:ERROR:electron/x] noise' >&2; echo 'Thread not found' >&2; exit 1"]), "t", None).thread;
        assert_eq!(failing.state, "unavailable");
        assert!(failing.reason.as_deref().unwrap().ends_with("Thread not found"), "{failing:?}");
        assert_eq!(failing.thread_id.as_deref(), Some("t"));
        let garbage = ask(Command::new("sh").args(["-c", "echo not-json"]), "t", None).thread;
        assert_eq!(garbage.state, "unavailable");
        let shown = ask(Command::new("sh").args(["-c", r#"echo '{"thread":{"status":"running","activeRunId":"run:1"}}'"#]), "t", None);
        assert_eq!((shown.thread.state.as_str(), shown.active_run.as_deref()), ("running", Some("run:1")));
    }
}

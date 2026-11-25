//! The live state of a `t3` job's T3 Code thread for the job detail (DEV-849).
//!
//! The thread is the one the job's log records: the last `agent_job_init`
//! entry names it with its link (`t3 session started, <thread>: <link>`), and
//! an ended job's status line keeps it too. So each job of a ticket resolves
//! to its own thread, also when several jobs ran several threads. lotar asks
//! the T3 Code server on this machine with `t3 thread show`, at most once per
//! thread in [`RECHECK`] and for at most [`TIMEOUT`], and only reads: the job's
//! outcome and the ticket stay as they are.

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

/// Per thread: when T3 Code answered and what lotar made of it.
static ANSWERS: LazyLock<Mutex<HashMap<String, (Instant, AgentJobT3Thread)>>> = LazyLock::new(|| Mutex::new(HashMap::new()));

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
        let Some(thread) = thread else {
            return Some(Ok(unavailable(None, link, "The job recorded no T3 Code thread: it ended before its thread started, or its log has none.")));
        };
        if let Some((at, answer)) = ANSWERS.lock().ok().and_then(|answers| answers.get(&thread).cloned())
            && at.elapsed() < RECHECK
        {
            return Some(Ok(answer));
        }
        let answer = match t3_cli(tasks_dir) {
            None => unavailable(Some(thread.clone()), link, "T3 Code's CLI (t3) isn't installed or harnessmux has no t3 agent."),
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
        Some(Ok(answer))
    }
}

/// Runs `t3 thread show` and reads its answer.
fn ask(command: &mut Command, thread: &str, link: Option<String>) -> AgentJobT3Thread {
    let output = match crate::utils::git::output_within(command, TIMEOUT) {
        Ok(output) => output,
        Err(err) if err.kind() == std::io::ErrorKind::TimedOut => {
            return unavailable(Some(thread.to_string()), link, &format!("T3 Code didn't answer within {} seconds.", TIMEOUT.as_secs()));
        }
        Err(err) => return unavailable(Some(thread.to_string()), link, &format!("Could not run T3 Code's CLI: {err}")),
    };
    if !output.status.success() {
        let why = first_error_line(&String::from_utf8_lossy(&output.stderr)).unwrap_or_else(|| "it exited with an error".to_string());
        return unavailable(
            Some(thread.to_string()),
            link,
            &format!("T3 Code can't show the thread (is the T3 Code server running, and does the thread still exist?): {why}"),
        );
    }
    let shown = serde_json::from_slice::<serde_json::Value>(&output.stdout).ok().and_then(|mut shown| shown.get_mut("thread").map(serde_json::Value::take));
    match shown {
        Some(shown) => from_shown(thread, link, &shown),
        None => unavailable(Some(thread.to_string()), link, "T3 Code's answer has no thread in it."),
    }
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
        let failing = ask(Command::new("sh").args(["-c", "echo '[1124/1:ERROR:electron/x] noise' >&2; echo 'Thread not found' >&2; exit 1"]), "t", None);
        assert_eq!(failing.state, "unavailable");
        assert!(failing.reason.as_deref().unwrap().ends_with("Thread not found"), "{failing:?}");
        assert_eq!(failing.thread_id.as_deref(), Some("t"));
        let garbage = ask(Command::new("sh").args(["-c", "echo not-json"]), "t", None);
        assert_eq!(garbage.state, "unavailable");
        let shown = ask(Command::new("sh").args(["-c", r#"echo '{"thread":{"status":"running"}}'"#]), "t", None);
        assert_eq!(shown.state, "running");
    }
}

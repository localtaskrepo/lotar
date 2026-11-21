//! T3 Code threads lotar kept active (DEV-747). A job settles its own
//! thread only when it completes and its ticket waits for nobody, so the
//! threads of failed or cancelled jobs, and of jobs that ended while the
//! ticket waited for a person, stayed active for good. Such a thread is
//! settled once its ticket is done, or once the thread of a later job on the
//! ticket is settled: that job moved the ticket on.

use super::{resumed, settle_session, ticket_done_state, workspace_root_of};
use crate::config::types::AgentRunnerKind;
use crate::services::agent_log_service::{AgentLogService, JobLogEntry};
use crate::utils::time;
use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::Path;
use std::sync::{LazyLock, Mutex};

/// The threads whose settle failed in this process, so a failure is noted
/// once, not at every look.
static FAILED: LazyLock<Mutex<HashSet<String>>> = LazyLock::new(|| Mutex::new(HashSet::new()));

/// A thread of an ended `t3` job with the newest job that ran it.
pub(super) struct Thread {
    ticket_id: String,
    job_id: String,
    agent: Option<String>,
    thread: String,
    ended_at: String,
    logs_dir: String,
}

/// Each thread of the tasks directory's ended `t3` jobs, whatever their outcome, by ticket and oldest job first.
pub(super) fn threads(tasks_dir: &Path) -> BTreeMap<String, Vec<Thread>> {
    let root = workspace_root_of(tasks_dir);
    let mut by_thread: HashMap<String, Thread> = HashMap::new();
    for logs_dir in super::job_log_dirs(tasks_dir) {
        for job in AgentLogService::load_jobs(&root, &logs_dir).into_iter().filter(|job| job.header.runner == AgentRunnerKind::T3.as_str()) {
            let Some(status) = job.status else { continue };
            let Some(thread) = status.agent_session.clone() else { continue };
            if by_thread.get(&thread).is_some_and(|newer| newer.ended_at >= status.at) {
                continue;
            }
            let Ok(ticket) = crate::storage::TaskId::parse(&job.header.ticket_id) else { continue };
            let kept = Thread {
                ticket_id: ticket.canonical(),
                job_id: job.header.job_id,
                agent: job.header.agent_profile,
                thread: thread.clone(),
                ended_at: status.at,
                logs_dir: logs_dir.clone(),
            };
            by_thread.insert(thread, kept);
        }
    }
    let mut by_ticket: BTreeMap<String, Vec<Thread>> = BTreeMap::new();
    for thread in by_thread.into_values() {
        by_ticket.entry(thread.ticket_id.clone()).or_default().push(thread);
    }
    for threads in by_ticket.values_mut() {
        threads.sort_by(|a, b| a.ended_at.cmp(&b.ended_at));
    }
    by_ticket
}

/// Settle the kept threads now, asking T3 Code for its threads: as a job ends.
pub(super) fn settle_now(tasks_dir: &Path) {
    let threads = threads(tasks_dir);
    if threads.is_empty() {
        return;
    }
    let Some((program, env, cwd)) = super::t3_cli(tasks_dir) else { return };
    // T3 Code can't list its threads: the next look tries again.
    if let Some(listed) = resumed::list_threads(&program, &env, &cwd, tasks_dir) {
        settle(tasks_dir, threads, &listed);
    }
}

/// Settle each ticket's kept threads, those `listed` shows unsettled and
/// completed: all of them on a done ticket, else those of jobs that ended
/// before the job of a settled thread.
pub(super) fn settle(tasks_dir: &Path, threads: BTreeMap<String, Vec<Thread>>, listed: &HashMap<String, resumed::Listed>) {
    for (ticket_id, threads) in threads {
        let kept = |thread: &&Thread| listed.get(&thread.thread).is_some_and(|shown| !shown.settled && shown.status == "completed");
        if !threads.iter().any(|thread| kept(&thread)) {
            continue;
        }
        let upto = if ticket_done_state(tasks_dir, &ticket_id) == Some(true) {
            threads.len()
        } else {
            threads.iter().rposition(|thread| listed.get(&thread.thread).is_some_and(|shown| shown.settled)).unwrap_or(0)
        };
        let due: Vec<(&Thread, Option<&str>)> =
            threads[..upto].iter().filter(kept).map(|thread| (thread, listed.get(&thread.thread).and_then(|shown| shown.link.as_deref()))).collect();
        if !due.is_empty() {
            settle_ticket(tasks_dir, &ticket_id, &due);
        }
    }
}

/// Settle `threads` of the ticket, each with the link `t3 thread list` shows, under its lock, so one process at a time
/// settles them; each one's newest job says how it went.
fn settle_ticket(tasks_dir: &Path, ticket_id: &str, threads: &[(&Thread, Option<&str>)]) {
    let Some(_lock) = crate::services::agent_active_jobs::lock_resumed_threads(tasks_dir, ticket_id) else { return };
    let prefix = crate::storage::TaskId::parse(ticket_id).ok().map(|parsed| parsed.project);
    let config = crate::config::resolution::config_for_project(tasks_dir, prefix.as_deref()).ok();
    for &(thread, link) in threads {
        // A session a process follows runs the thread now.
        let record = resumed::session_of(&thread.thread);
        if record.as_ref().is_some_and(resumed::followed) {
            continue;
        }
        let settled = match (record, config.as_ref()) {
            (Some(record), Some(config)) => settle_session(&record, thread.agent.as_deref(), config),
            (None, _) => Err("no harnessmux session holds it".to_string()),
            (_, None) => Err(format!("{ticket_id}'s configuration can't be read")),
        };
        match settled {
            Ok(settled_link) => note(tasks_dir, thread, format!("Settled the T3 Code thread {settled_link}")),
            Err(err) if FAILED.lock().is_ok_and(|mut failed| failed.insert(thread.thread.clone())) => {
                note(tasks_dir, thread, format!("The T3 Code thread {} was not settled: {err}", link.unwrap_or(&thread.thread)));
            }
            Err(_) => {}
        }
    }
}

/// Add `message` to the log of the thread's job, which ended: a job this process still holds gets it as an event too.
fn note(tasks_dir: &Path, thread: &Thread, message: String) {
    let mut held = false;
    super::update_job(&thread.job_id, |state| {
        held = true;
        super::record_and_emit(state, "agent_job_progress", Some(message.clone()));
    });
    if held {
        return;
    }
    let root = workspace_root_of(tasks_dir);
    let sequence = AgentLogService::load_job(&root, &thread.logs_dir, &thread.job_id)
        .ok()
        .flatten()
        .map(|saved| super::saved_events(&saved).iter().map(|event| event.sequence).max().unwrap_or(0) + 1);
    let entry = JobLogEntry { kind: "agent_job_progress".to_string(), at: time::now_rfc3339(), message: Some(message), sequence };
    let _ = AgentLogService::append_event(&root, Some(&thread.logs_dir), &thread.job_id, entry);
}

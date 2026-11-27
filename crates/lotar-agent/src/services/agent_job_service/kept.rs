//! T3 Code threads lotar kept active (DEV-747). A job settles its own
//! thread only when it completes and its ticket waits for nobody, so the
//! threads of failed or cancelled jobs, and of jobs that ended while the
//! ticket waited for a person, stayed active for good. Such a thread is
//! settled once its ticket is done, or once the thread of a later job on the
//! ticket is settled: that job moved the ticket on. A new job settles the
//! threads of the ticket's earlier jobs, and a done ticket all of its
//! threads, whatever their run's outcome (DEV-876). A done ticket's job
//! thread no job log names any more is found by its title (DEV-810). A
//! thread whose run ended otherwise than `completed` is settled through T3
//! Code's internal API, as harnessmux settles only completed ones (DEV-917).

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

/// A thread of an ended `t3` job with the newest job that ran it, if a job log still names it.
pub(super) struct Thread {
    ticket_id: String,
    /// The newest job's id and log folder.
    job: Option<(String, String)>,
    agent: Option<String>,
    thread: String,
    ended_at: String,
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
                job: Some((job.header.job_id, logs_dir.clone())),
                agent: job.header.agent_profile,
                thread: thread.clone(),
                ended_at: status.at,
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

/// Whether a thread `t3 thread list` shows with `status` has no run going, so
/// settling it stops nothing: completed, failed, interrupted, cancelled, idle
/// (DEV-876).
fn ended(status: &str) -> bool {
    !matches!(status, "" | "running" | "waiting" | "queued" | "starting" | "preparing")
}

/// Whether a project of the tasks directory has a `t3` agent profile, so T3 Code may hold job threads no job log names.
pub(super) fn runs_t3(tasks_dir: &Path) -> bool {
    let mut projects = vec![None];
    projects.extend(crate::utils::filesystem::list_visible_subdirs(tasks_dir).into_iter().map(|(prefix, _)| Some(prefix)));
    projects.into_iter().any(|project| {
        crate::config::resolution::config_for_project(tasks_dir, project.as_deref())
            .is_ok_and(|config| config.agent_profiles.values().any(|profile| profile.runner == AgentRunnerKind::T3.as_str()))
    })
}

/// Settle the kept threads now, asking T3 Code for its threads: as a job ends.
pub(super) fn settle_now(tasks_dir: &Path) {
    let threads = threads(tasks_dir);
    if threads.is_empty() && !runs_t3(tasks_dir) {
        return;
    }
    let Some((program, env, cwd)) = super::t3_cli(tasks_dir) else { return };
    // T3 Code can't list its threads: the next look tries again.
    if let Some(listed) = resumed::list_threads(&program, &env, &cwd, tasks_dir) {
        settle(tasks_dir, threads, &listed);
    }
}

/// Settle the ended threads of the ticket's earlier jobs as a new `t3` job
/// starts on it, all but `own`, the thread the job continues; the notes for
/// its log. T3 Code resumes a thread a usage limit failed at the limit's
/// reset, also when a later job, such as the limited job's retry, took its
/// work over; a settled thread it never resumes (DEV-836). The new job is the
/// ticket's attempt now, so no earlier one stays in the sidebar (DEV-876).
/// A thread whose run still goes is left to the look ([`resumed::look`]).
pub(super) fn settle_superseded(tasks_dir: &Path, ticket_id: &str, own: Option<&str>) -> Vec<String> {
    let Some(earlier) = threads(tasks_dir).remove(ticket_id) else { return Vec::new() };
    let earlier: Vec<Thread> = earlier.into_iter().filter(|thread| Some(thread.thread.as_str()) != own).collect();
    if earlier.is_empty() {
        return Vec::new();
    }
    let Some((program, env, cwd)) = super::t3_cli(tasks_dir) else { return Vec::new() };
    let Some(listed) = resumed::list_threads(&program, &env, &cwd, tasks_dir) else { return Vec::new() };
    let mut notes = Vec::new();
    for thread in &earlier {
        let Some(shown) = listed.get(&thread.thread).filter(|shown| !shown.settled && ended(&shown.status)) else { continue };
        let link = shown.link.as_deref().unwrap_or(&thread.thread);
        let Some((job, _)) = thread.job.as_ref() else { continue };
        let status = &shown.status;
        let note = match crate::services::t3_thread_api::settle_thread(&program, &env, &cwd, &thread.thread) {
            Ok(()) => format!("Settled the T3 Code thread {link} of the earlier job {job} ({status}): this job is the ticket's attempt now."),
            Err(err) => format!("Could not settle the T3 Code thread {link} of the earlier job {job} ({status}), so it stays in T3 Code's sidebar: {err}"),
        };
        notes.push(note);
    }
    notes
}

/// Settle each ticket's kept threads, those `listed` shows unsettled: on a
/// done ticket all whose run ended, whatever its outcome (DEV-876), else the
/// completed ones of jobs that ended before the job of a settled thread, or
/// of one `listed` leaves out: T3 Code lists no older settled threads and no
/// archived ones (DEV-941). A done ticket's job threads no job log names count
/// too ([`unlogged`]).
pub(super) fn settle(tasks_dir: &Path, mut threads: BTreeMap<String, Vec<Thread>>, listed: &HashMap<String, resumed::Listed>) {
    for thread in unlogged(tasks_dir, &threads, listed) {
        threads.entry(thread.ticket_id.clone()).or_default().insert(0, thread);
    }
    for (ticket_id, threads) in threads {
        let unsettled = |thread: &Thread| listed.get(&thread.thread).filter(|shown| !shown.settled).map(|shown| shown.status.as_str());
        if !threads.iter().any(|thread| unsettled(thread).is_some_and(ended)) {
            continue;
        }
        let done = ticket_done_state(tasks_dir, &ticket_id) == Some(true);
        let kept = |thread: &&Thread| unsettled(thread).is_some_and(|status| if done { ended(status) } else { status == "completed" });
        let upto =
            if done { threads.len() } else { threads.iter().rposition(|thread| listed.get(&thread.thread).is_none_or(|shown| shown.settled)).unwrap_or(0) };
        let due: Vec<(&Thread, &resumed::Listed)> =
            threads[..upto].iter().filter(kept).filter_map(|thread| listed.get(&thread.thread).map(|shown| (thread, shown))).collect();
        if !due.is_empty() {
            settle_ticket(tasks_dir, &ticket_id, &due);
        }
    }
}

/// The job threads `listed` shows that no job log names: unsettled, their run
/// ended in any way ([`ended`], DEV-876), run in `full-access` as a job's thread is (the owner's own run in `auto`),
/// and titled with the ID of a done ticket of the tasks directory, as a job
/// names its thread (`<ID> <title>`, or `<ID> fix: <title>` by `thread_name`).
/// The title's second word names the profile when one of that name is configured.
fn unlogged(tasks_dir: &Path, logged: &BTreeMap<String, Vec<Thread>>, listed: &HashMap<String, resumed::Listed>) -> Vec<Thread> {
    let known: HashSet<&str> = logged.values().flatten().map(|thread| thread.thread.as_str()).collect();
    let mut done: HashMap<String, bool> = HashMap::new();
    let mut found = Vec::new();
    for (id, shown) in listed {
        if known.contains(id.as_str()) || shown.settled || !ended(&shown.status) || shown.runtime_mode != "full-access" {
            continue;
        }
        let mut words = shown.title.split_whitespace();
        let Some(ticket) = words.next().and_then(|word| crate::storage::TaskId::parse(word).ok()) else { continue };
        let ticket_id = ticket.canonical();
        if !*done.entry(ticket_id.clone()).or_insert_with(|| ticket_done_state(tasks_dir, &ticket_id) == Some(true)) {
            continue;
        }
        let profiles = crate::config::resolution::config_for_project(tasks_dir, Some(&ticket.project)).ok().map(|config| config.agent_profiles);
        let agent = words.next().and_then(|word| word.strip_suffix(':')).filter(|name| profiles.as_ref().is_some_and(|profiles| profiles.contains_key(*name)));
        found.push(Thread { ticket_id, job: None, agent: agent.map(str::to_string), thread: id.clone(), ended_at: String::new() });
    }
    found.sort_by(|a, b| a.thread.cmp(&b.thread));
    found
}

/// Settle `threads` of the ticket, each with what `t3 thread list` shows of it, under its lock, so one process at a time
/// settles them; each one's newest job says how it went. A `completed` thread is settled through harnessmux, which
/// checks `t3 thread show` first; one whose run ended otherwise through T3 Code's internal API, as harnessmux refuses
/// it (DEV-917).
fn settle_ticket(tasks_dir: &Path, ticket_id: &str, threads: &[(&Thread, &resumed::Listed)]) {
    let Some(_lock) = crate::services::agent_active_jobs::lock_resumed_threads(tasks_dir, ticket_id) else { return };
    let prefix = crate::storage::TaskId::parse(ticket_id).ok().map(|parsed| parsed.project);
    let config = crate::config::resolution::config_for_project(tasks_dir, prefix.as_deref()).ok();
    for &(thread, shown) in threads {
        // A session a process follows runs the thread now.
        let record = resumed::session_of(&thread.thread);
        if record.as_ref().is_some_and(resumed::followed) {
            continue;
        }
        let link = shown.link.as_deref();
        let settled = match config.as_ref() {
            _ if shown.status != "completed" => settle_ended(tasks_dir, &thread.thread).map(|()| link.unwrap_or(&thread.thread).to_string()),
            Some(config) => record_for(tasks_dir, &thread.thread, record, thread.agent.as_deref(), config)
                .and_then(|record| settle_session(&record, thread.agent.as_deref(), config)),
            None => Err(format!("{ticket_id}'s configuration can't be read")),
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

/// Settle `thread`, whose run ended otherwise than `completed`, through T3 Code's internal API (`thread.settle`).
fn settle_ended(tasks_dir: &Path, thread: &str) -> Result<(), String> {
    let (program, env, cwd) = super::t3_cli(tasks_dir).ok_or_else(|| "harnessmux has no t3 agent to reach T3 Code with".to_string())?;
    crate::services::t3_thread_api::settle_thread(&program, &env, &cwd, thread)
}

/// The harnessmux record to settle `thread` with: the session that holds it, else one made for it, as a ticket's next
/// job reuses its session's name and its record then holds only the newest thread (DEV-810). `t3` runs in the
/// repository's checkout when the session's directory is gone (a worktree the merge removed).
fn record_for(
    tasks_dir: &Path,
    thread: &str,
    record: Option<harnessmux::SessionRecord>,
    agent: Option<&str>,
    config: &crate::config::types::ResolvedConfig,
) -> Result<harnessmux::SessionRecord, String> {
    let checkout = || super::resolve_repo_root(tasks_dir).unwrap_or_else(|| workspace_root_of(tasks_dir));
    let mut record = match record {
        Some(record) => record,
        None => {
            let profile = match agent.and_then(|name| config.agent_profiles.get(name)) {
                Some(profile) => crate::services::agent_runner::session_profile(profile, HashMap::new()).map_err(|err| err.to_string())?,
                None => harnessmux::Profile::default(),
            };
            harnessmux::SessionRecord::new("lotar-settle", AgentRunnerKind::T3.as_str(), &checkout(), profile).map_err(|err| err.to_string())?
        }
    };
    record.agent_session = Some(thread.to_string());
    if !record.cwd.is_dir() {
        record.cwd = checkout();
    }
    Ok(record)
}

/// Write `message` to the process's log (the server's or queue worker's stderr) and, when a job log names the thread,
/// to the log of its job, which ended: a job this process still holds gets it as an event too.
fn note(tasks_dir: &Path, thread: &Thread, message: String) {
    if std::env::var("LOTAR_TEST_SILENT").as_deref() != Ok("1") {
        eprintln!("[lotar] {}: {message}", thread.ticket_id);
    }
    let Some((job_id, logs_dir)) = thread.job.as_ref() else { return };
    let mut held = false;
    super::update_job(job_id, |state| {
        held = true;
        super::record_and_emit(state, "agent_job_progress", Some(message.clone()));
    });
    if held {
        return;
    }
    let root = workspace_root_of(tasks_dir);
    let sequence = AgentLogService::load_job(&root, logs_dir, job_id)
        .ok()
        .flatten()
        .map(|saved| super::saved_events(&saved).iter().map(|event| event.sequence).max().unwrap_or(0) + 1);
    let entry = JobLogEntry {
        kind: "agent_job_progress".to_string(),
        at: time::now_rfc3339(),
        message: Some(message),
        sequence,
        tool: None,
        plan: None,
        transcript: None,
    };
    let _ = AgentLogService::append_event(&root, Some(logs_dir), job_id, entry);
}

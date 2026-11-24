//! T3 Code threads that run again after their job ended (DEV-626). T3 Code
//! resumes the turns a restart cut off, and a turn it paused for a usage
//! limit; an answer in a thread starts a new turn. No lotar job follows such
//! a thread, so its outcome never reached the ticket. lotar takes the newest
//! one of a ticket over as a job, which ends like any other, and stops the
//! others through T3 Code's internal API (DEV-836): at a usage limit's reset
//! T3 Code resumes every limited thread, and a ticket's threads share one
//! worktree. A Blocked ticket's threads are all stopped.

use super::{
    AgentJobRecord, AgentJobService, AgentJobState, AgentJobStatus, JOB_REGISTRY, RunningJobSlot, active_record_of, is_merge_job_candidate, make_job_id,
    prune_finished_jobs, record_and_emit, resume_session_to_end, saved_jobs, start_job, t3_cli, t3_thread_show, ticket_done_state, update_job,
};
use crate::config::types::{AgentProfileDetail, AgentRunnerKind};
use crate::services::task_service::{CommentContext, TaskService, TaskUpdateContext};
use crate::storage::manager::Storage;
use crate::utils::time;
use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::{LazyLock, Mutex};
use std::time::{Duration, Instant};

/// How often a process asks T3 Code for its threads: a worker calls in every
/// few hundred milliseconds, and each `t3` call takes about half a second.
const LOOK_EVERY: Duration = Duration::from_secs(30);

static LAST_LOOK: LazyLock<Mutex<HashMap<PathBuf, Instant>>> = LazyLock::new(|| Mutex::new(HashMap::new()));

/// An ended `t3` job whose thread T3 Code runs again.
struct Resumed {
    cancelled: bool,
    ticket_id: String,
    job_id: String,
    agent: Option<String>,
    thread: String,
    run: String,
    link: Option<String>,
    waiting: bool,
    ended_at: String,
    worktree_path: Option<String>,
    worktree_branch: Option<String>,
}

impl Resumed {
    fn shown(&self) -> &str {
        self.link.as_deref().unwrap_or(&self.thread)
    }
}

/// Take over or report the threads of the tasks directory's ended `t3` jobs
/// that T3 Code runs again, and settle the threads lotar kept active whose
/// time came ([`super::kept`]), at most every [`LOOK_EVERY`]. T3 Code is
/// asked nothing while no ended job has a thread.
pub(super) fn look(tasks_dir: &Path) {
    {
        let Ok(mut looks) = LAST_LOOK.lock() else {
            return;
        };
        if looks.get(tasks_dir).is_some_and(|at| at.elapsed() < LOOK_EVERY) {
            return;
        }
        looks.insert(tasks_dir.to_path_buf(), Instant::now());
    }
    let ended = ended_threads(tasks_dir);
    let kept = super::kept::threads(tasks_dir);
    if kept.is_empty() {
        return;
    }
    let Some((program, env, cwd)) = t3_cli(tasks_dir) else {
        return;
    };
    // T3 Code can't list its threads: the next look tries again.
    let Some(listed) = list_threads(&program, &env, &cwd, tasks_dir) else {
        return;
    };
    let mut by_ticket: BTreeMap<String, Vec<Resumed>> = BTreeMap::new();
    for mut resumed in ended {
        let Some(thread) = listed.get(&resumed.thread).filter(|thread| thread.status == "running" || thread.status == "waiting") else {
            continue;
        };
        resumed.run = thread.run.clone();
        resumed.link = thread.link.clone();
        resumed.waiting = thread.status == "waiting";
        by_ticket.entry(resumed.ticket_id.clone()).or_default().push(resumed);
    }
    for (ticket_id, mut threads) in by_ticket {
        threads.sort_by(|a, b| b.ended_at.cmp(&a.ended_at));
        on_ticket(tasks_dir, &ticket_id, threads, &program, &env, &cwd);
    }
    super::kept::settle(tasks_dir, kept, &listed);
}

/// Each thread of an ended `t3` job with the newest job that ran it, unless
/// that job was cancelled.
fn ended_threads(tasks_dir: &Path) -> Vec<Resumed> {
    let mut threads: HashMap<String, Resumed> = HashMap::new();
    for job in saved_jobs(tasks_dir).into_iter().filter(|job| job.header.runner == AgentRunnerKind::T3.as_str()) {
        let Some(status) = job.status else {
            continue;
        };
        let Some(thread) = status.agent_session.clone() else {
            continue;
        };
        if threads.get(&thread).is_some_and(|newer| newer.ended_at >= status.at) {
            continue;
        }
        let Ok(ticket) = crate::storage::TaskId::parse(&job.header.ticket_id) else {
            continue;
        };
        let resumed = Resumed {
            cancelled: status.status == AgentJobStatus::Cancelled.as_str(),
            ticket_id: ticket.canonical(),
            job_id: job.header.job_id,
            agent: job.header.agent_profile,
            thread: thread.clone(),
            run: String::new(),
            link: None,
            waiting: false,
            ended_at: status.at,
            worktree_path: status.worktree_path.or(job.header.worktree_path),
            worktree_branch: status.worktree_branch.or(job.header.worktree_branch),
        };
        threads.insert(thread, resumed);
    }
    // A cancelled job's thread runs on until it is stopped in T3 Code: the
    // cancel said no job follows it.
    threads.into_values().filter(|resumed| !resumed.cancelled).collect()
}

/// A thread as `t3 thread list --json` lists it.
pub(super) struct Listed {
    pub(super) status: String,
    run: String,
    pub(super) link: Option<String>,
    /// Settled in T3 Code's sidebar (DEV-747).
    pub(super) settled: bool,
}

/// The project's threads by id, from `t3 thread list --json`.
pub(super) fn list_threads(program: &Path, env: &[(String, String)], cwd: &Path, tasks_dir: &Path) -> Option<HashMap<String, Listed>> {
    let project = crate::services::agent_runner::session_project(tasks_dir);
    let output = Command::new(program)
        .args(["thread", "list", "--project"])
        .arg(&project)
        .arg("--json")
        .envs(env.iter().map(|(key, value)| (key, value)))
        .current_dir(cwd)
        .stdin(Stdio::null())
        .output()
        .ok()
        .filter(|output| output.status.success())?;
    let listed: serde_json::Value = serde_json::from_slice(&output.stdout).ok()?;
    let text = |thread: &serde_json::Value, key: &str| thread.get(key).and_then(|value| value.as_str()).unwrap_or_default().to_string();
    Some(
        listed
            .as_array()?
            .iter()
            .map(|thread| {
                let link = Some(text(thread, "link")).filter(|link| !link.is_empty());
                let settled = thread.get("settled").and_then(serde_json::Value::as_bool).unwrap_or(false);
                (text(thread, "threadId"), Listed { status: text(thread, "status"), run: text(thread, "latestRunId"), link, settled })
            })
            .collect(),
    )
}

/// The ticket's running threads, newest job's first: the first is taken over
/// when no job runs or waits for the ticket and no thread of it waits for an
/// answer, or left to the job queued for it; the others are stopped. On a Blocked ticket all are stopped. Under
/// the ticket's lock, so one process handles them, once per run.
fn on_ticket(tasks_dir: &Path, ticket_id: &str, threads: Vec<Resumed>, program: &Path, env: &[(String, String)], cwd: &Path) {
    let Some(_lock) = crate::services::agent_active_jobs::lock_resumed_threads(tasks_dir, ticket_id) else {
        return;
    };
    // A thread a running session follows belongs to a job; one that waits
    // for an answer holds the ticket until it is answered (DEV-600), so no
    // other thread of it is taken over (DEV-639).
    let (waiting, threads): (Vec<Resumed>, Vec<Resumed>) = threads
        .into_iter()
        .filter(|resumed| session_of(&resumed.thread).is_none_or(|record| !followed(&record)))
        .partition(|resumed| resumed.waiting || waits_for_answer(program, env, cwd, &resumed.thread));
    // The owner's own use of a done ticket's thread.
    if threads.is_empty() || ticket_done_state(tasks_dir, ticket_id) == Some(true) {
        return;
    }
    // A Blocked ticket waits for another one: none of its threads runs.
    if super::ticket_state(tasks_dir, ticket_id).is_some_and(|(status, _)| status.eq_ignore_ascii_case("blocked")) {
        for resumed in threads {
            if crate::services::agent_active_jobs::first_look_at_run(tasks_dir, &resumed.thread, &resumed.run) {
                stop(tasks_dir, ticket_id, &resumed, &format!("while {ticket_id} is Blocked"), program, env, cwd);
            }
        }
        return;
    }
    let busy = AgentJobService::has_active_job_in(tasks_dir, ticket_id)
        || crate::services::agent_active_jobs::orphaned(tasks_dir).iter().any(|job| crate::storage::identity::aliases_match(&job.ticket_id, ticket_id));
    // A queued job waits for the newest thread (DEV-600), and the others are
    // stopped; the owner or a rule chose it. A job's retry after a usage
    // limit doesn't: T3 Code resumed the paused job's own turn after the
    // reset, so the thread is taken over and the retry goes (DEV-584).
    let limit_retries = if busy { None } else { limit_retries(tasks_dir, ticket_id) };
    let waited_for = !busy && limit_retries.is_none() && queued(tasks_dir, ticket_id);
    let mut threads = threads.into_iter();
    let mut beside = waiting.first().filter(|_| !busy).map(|held| format!("the thread {} that waits for an answer", held.shown()));
    if waited_for
        && beside.is_none()
        && let Some(first) = threads.next()
    {
        beside = Some(format!("the thread {} that the ticket's queued job waits for", first.shown()));
    }
    if !busy
        && !waited_for
        && beside.is_none()
        && let Some(first) = threads.next()
    {
        if !crate::services::agent_active_jobs::first_look_at_run(tasks_dir, &first.thread, &first.run) {
            return;
        }
        let carried = limit_retries.as_ref().and_then(|entries| entries.iter().find_map(|entry| entry.carried_outcome.clone()));
        match adopt(tasks_dir, ticket_id, &first, carried) {
            Ok(job_id) => {
                for entry in limit_retries.iter().flatten() {
                    let _ = crate::services::agent_queue_service::AgentQueueService::remove(tasks_dir, &entry.ticket_id);
                }
                beside = Some(format!("the thread {} that the job {job_id} took over", first.shown()));
            }
            Err(why) => {
                comment(
                    tasks_dir,
                    ticket_id,
                    &format!(
                        "T3 Code runs the thread {} of the job {} again, with no lotar job, and lotar can't take it over: {why}. Stop it in T3 Code, or let it finish and move the ticket on yourself.",
                        first.shown(),
                        first.job_id
                    ),
                );
                beside = Some(format!("the thread {} that lotar could not take over", first.shown()));
            }
        }
    }
    let beside = beside.unwrap_or_else(|| {
        match AgentJobService::active_job_id(ticket_id).or_else(|| crate::services::agent_active_jobs::for_ticket(tasks_dir, ticket_id).map(|job| job.job_id)) {
            Some(job_id) => format!("the running job {job_id}"),
            None => "the ticket's running job".to_string(),
        }
    });
    for resumed in threads {
        if crate::services::agent_active_jobs::first_look_at_run(tasks_dir, &resumed.thread, &resumed.run) {
            stop(tasks_dir, ticket_id, &resumed, &format!("beside {beside}"), program, env, cwd);
        }
    }
}

/// Stop a thread T3 Code runs again `why` ("beside the job …"), and comment
/// what lotar did: a ticket runs one thread at a time (DEV-836).
fn stop(tasks_dir: &Path, ticket_id: &str, resumed: &Resumed, why: &str, program: &Path, env: &[(String, String)], cwd: &Path) {
    let reason = format!("lotar runs one thread of {ticket_id} at a time, none while it is Blocked");
    let text = match crate::services::t3_thread_api::stop_thread(program, env, cwd, &resumed.thread, &reason) {
        Ok(()) => format!(
            "T3 Code ran the thread {} of the job {} again, {why}. lotar stopped it: a ticket runs one thread at a time, none while it is Blocked.",
            resumed.shown(),
            resumed.job_id
        ),
        Err(err) => format!(
            "T3 Code runs the thread {} of the job {} again, {why}. lotar takes over only one thread per ticket and could not stop this one ({err}): stop it in T3 Code.",
            resumed.shown(),
            resumed.job_id
        ),
    };
    comment(tasks_dir, ticket_id, &text);
}

/// The harnessmux session whose agent session is `thread`.
pub(super) fn session_of(thread: &str) -> Option<harnessmux::SessionRecord> {
    let registry = crate::services::agent_runner::registry();
    registry.list(None).ok()?.into_iter().filter(|record| record.agent_session.as_deref() == Some(thread)).max_by(|a, b| a.last_used.cmp(&b.last_used))
}

/// Whether a process owns the session, following its thread: its owner
/// answers, as a pid saved before a restart may be another process's now.
pub(super) fn followed(record: &harnessmux::SessionRecord) -> bool {
    let registry = crate::services::agent_runner::registry();
    super::block_on(harnessmux::ipc::owner_running(&registry, &record.name))
}

fn waits_for_answer(program: &Path, env: &[(String, String)], cwd: &Path, thread: &str) -> bool {
    t3_thread_show(program, env, cwd, thread)
        .and_then(|shown| shown.get("pendingRequestCount").and_then(serde_json::Value::as_u64))
        .is_some_and(|count| count > 0)
}

/// The ticket's queued entries when every one is a job's retry after a
/// usage limit (`not_before`) and no job of it waits in this process.
fn limit_retries(tasks_dir: &Path, ticket_id: &str) -> Option<Vec<crate::services::agent_queue_service::AgentQueueEntry>> {
    let pending = crate::services::agent_queue_service::AgentQueueService::list_pending(tasks_dir).ok()?;
    let entries: Vec<_> = pending.into_iter().filter(|entry| crate::storage::identity::aliases_match(&entry.ticket_id, ticket_id)).collect();
    let in_memory = JOB_REGISTRY.lock().map_or(true, |registry| registry.pending_queue.iter().any(|job| job.ticket_id == ticket_id));
    (!entries.is_empty() && !in_memory && entries.iter().all(|entry| entry.not_before.is_some())).then_some(entries)
}

fn queued(tasks_dir: &Path, ticket_id: &str) -> bool {
    let pending = crate::services::agent_queue_service::AgentQueueService::list_pending(tasks_dir).unwrap_or_default();
    pending.iter().any(|entry| crate::storage::identity::aliases_match(&entry.ticket_id, ticket_id))
        || JOB_REGISTRY.lock().is_ok_and(|registry| registry.pending_queue.iter().any(|job| job.ticket_id == ticket_id))
}

fn comment(tasks_dir: &Path, ticket_id: &str, text: &str) {
    let _ = TaskService::add_comment_with_context(&mut Storage::new(tasks_dir), ticket_id, text, CommentContext::automation());
}

/// Take the thread over as a new job of its job's profile: the ticket goes
/// back to that profile, the `job_started` rules run, and the job follows the
/// thread's run and ends like any job. The new job's id, or why not.
fn adopt(
    tasks_dir: &Path,
    ticket_id: &str,
    resumed: &Resumed,
    carried_outcome: Option<crate::services::agent_queue_service::CarriedOutcome>,
) -> Result<String, String> {
    let parsed = crate::storage::TaskId::parse(ticket_id).map_err(|err| err.to_string())?;
    let config = crate::config::resolution::config_for_project(tasks_dir, Some(&parsed.project)).map_err(|err| err.to_string())?;
    let profile = match resumed.agent.as_deref() {
        Some(name) => config.agent_profiles.get(name).cloned().ok_or_else(|| format!("its profile {name} is no longer configured"))?,
        None => AgentProfileDetail { runner: AgentRunnerKind::T3.as_str().to_string(), ..Default::default() },
    };
    if profile.runner != AgentRunnerKind::T3.as_str() {
        return Err(format!("its profile {} runs on {} now", resumed.agent.as_deref().unwrap_or_default(), profile.runner));
    }
    let mut record = session_of(&resumed.thread).ok_or_else(|| "no harnessmux session has the thread".to_string())?;
    let name = record.name.clone();
    // The session's turn runs in T3 Code: its resume follows the thread's run
    // instead of sending a turn.
    record.state = harnessmux::SessionState::Running;
    record.owner_pid = None;
    crate::services::agent_runner::registry().save(&record).map_err(|err| err.to_string())?;

    let mut storage = Storage::new(tasks_dir);
    let task = TaskService::get(&storage, ticket_id, None).map_err(|err| err.to_string())?;
    let assignee = resumed.agent.as_deref().map(|agent| format!("@{agent}"));
    // The ticket goes back to the job's profile, so its rules match as they
    // did while the job ran; no assignment rule starts another job.
    if let Some(assignee) = assignee.as_ref().filter(|assignee| task.assignee.as_deref() != Some(assignee.as_str())) {
        let patch = crate::api_types::TaskUpdate { assignee: Some(assignee.clone()), ..Default::default() };
        TaskService::update_with_context(&mut storage, ticket_id, patch, TaskUpdateContext::automation_disabled()).map_err(|err| err.to_string())?;
    }

    let job_id = make_job_id();
    let started_at = time::now_rfc3339();
    let workspace_root = tasks_dir.parent().map(Path::to_path_buf).unwrap_or_else(|| tasks_dir.to_path_buf());
    {
        let mut registry = JOB_REGISTRY.lock().map_err(|_| "the job registry is unavailable".to_string())?;
        registry.active_by_ticket.insert(ticket_id.to_string(), job_id.clone());
        prune_finished_jobs(&mut registry);
        registry.jobs.insert(
            job_id.clone(),
            AgentJobState {
                record: AgentJobRecord {
                    id: job_id.clone(),
                    ticket_id: ticket_id.to_string(),
                    tasks_dir: tasks_dir.to_path_buf(),
                    workspace_root,
                    agent_logs_dir: crate::services::agent_log_service::effective_logs_dir(tasks_dir, config.agent_logs_dir.as_deref()),
                    runner: AgentRunnerKind::T3.as_str().to_string(),
                    agent_profile: resumed.agent.clone(),
                    status: AgentJobStatus::Queued,
                    created_at: started_at.clone(),
                    started_at: None,
                    finished_at: None,
                    exit_code: None,
                    last_message: None,
                    summary: None,
                    session_id: Some(name.clone()),
                    worktree_path: resumed.worktree_path.clone(),
                    worktree_branch: resumed.worktree_branch.clone(),
                    model: profile.model.clone(),
                    effort: profile.effort.clone(),
                    usage: None,
                    question: None,
                    is_merge_job: is_merge_job_candidate(&task, resumed.agent.as_deref()),
                },
                events: Vec::new(),
                next_sequence: 0,
                runtime: None,
                cancellation_finalized: false,
                runner_failed: false,
                runner_errors: Vec::new(),
                worker_running: true,
                main_head_at_start: None,
                assignee_at_start: None,
                request: None,
                commit_retry: None,
                carried_outcome: None,
                after_limit: false,
                commits: super::job_commits_with(&config, &profile, AgentRunnerKind::T3),
            },
        );
        // It counts toward max_parallel_jobs, so queued jobs wait for it.
        registry.running_count += 1;
        // Other processes see it before the ticket's lock goes; one that
        // takes it over after this process died follows the same thread.
        if let Some(state) = registry.jobs.get(&job_id) {
            crate::services::agent_active_jobs::register(tasks_dir, &active_record_of(state, ticket_id, &started_at, Some(name.clone())));
        }
    }
    let what = match assignee.as_deref() {
        Some(assignee) => format!("{ticket_id} goes back to {assignee}, and the job's end rules run when the thread's run ends"),
        None => "the job's end rules run when the thread's run ends".to_string(),
    };
    let text = format!(
        "T3 Code runs the thread {} of the job {} again, with no lotar job (it resumed a turn a restart or a usage limit cut off, or someone wrote in the thread). lotar takes it over as the job {job_id}: {what}.",
        resumed.shown(),
        resumed.job_id
    );
    comment(tasks_dir, ticket_id, &text);
    let (tasks_dir, ticket_id, old_job) = (tasks_dir.to_path_buf(), ticket_id.to_string(), resumed.job_id.clone());
    let new_job = job_id.clone();
    let worktree = (resumed.worktree_path.clone(), resumed.worktree_branch.clone());
    std::thread::spawn(move || {
        let job_id = new_job.as_str();
        let _slot = RunningJobSlot { job_id, ticket_id: &ticket_id, tasks_dir: &tasks_dir };
        if !start_job(job_id, AgentRunnerKind::T3, &profile, &tasks_dir, &ticket_id, &config, &started_at, Some(worktree)) {
            return;
        }
        // What the job paused on a usage limit recorded, as its retry would have.
        if let Some(carried) = carried_outcome {
            super::carry_outcome(job_id, &tasks_dir, &ticket_id, carried);
        }
        // Other processes see that this job follows the session.
        let active = JOB_REGISTRY
            .lock()
            .ok()
            .and_then(|registry| registry.jobs.get(job_id).map(|state| active_record_of(state, &ticket_id, &started_at, Some(name.clone()))));
        if let Some(active) = active.as_ref() {
            crate::services::agent_active_jobs::register(&tasks_dir, active);
        }
        update_job(job_id, |state| {
            let note = format!("T3 Code runs the thread of the job {old_job} again, with no lotar job; this job takes it over.");
            record_and_emit(state, "agent_job_progress", Some(note));
        });
        let why = format!("T3 Code runs the thread of the job {old_job} again");
        resume_session_to_end(job_id, AgentRunnerKind::T3, AgentRunnerKind::T3.as_str(), &name, &profile, &tasks_dir, &ticket_id, &config, &why);
    });
    Ok(job_id)
}

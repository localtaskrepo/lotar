use crate::api_types::AgentJobCreateRequest;
use crate::config::manager::ConfigManager;
use crate::errors::{LoTaRError, LoTaRResult};
use crate::services::agent_job_service::AgentJobService;
use crate::services::memory_gate::MemoryHold;
use crate::utils::time::{self, Rfc3339};
use fs2::FileExt;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::fs::{File, OpenOptions};
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread;
use std::time::Duration;

const QUEUE_VERSION: u32 = 1;
const WORKER_POLL_MS: u64 = 300;
/// Retries of an entry whose start failed, after the first try.
const MAX_ATTEMPTS: u32 = 3;
/// How long a paused queue waits before it tries one held job again (DEV-604).
const PAUSE_RETRY_SECONDS: i64 = 600;

#[derive(Debug, Clone, Serialize, Deserialize)]
struct AgentQueueState {
    version: u32,
    tasks_dir: String,
    updated_at: String,
    pending: Vec<AgentQueueEntry>,
    /// Entries a worker took and has not registered a job for yet: a worker
    /// that dies before that leaves them here for the next one (DEV-661).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    taken: Vec<AgentQueueEntry>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pause: Option<QueuePause>,
    /// Jobs the dependency check held back (DEV-530): each starts as it was
    /// once its ticket's prerequisites are done.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    parked: Vec<ParkedEntry>,
}

/// Why the queue holds jobs back (DEV-604).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PauseReason {
    /// Commits can't be signed: the jobs that commit wait.
    Signing,
    /// The disk is full: every job waits.
    Disk,
}

impl PauseReason {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Signing => "signing",
            Self::Disk => "disk",
        }
    }

    pub fn describe(self) -> &'static str {
        match self {
            Self::Signing => "commit signing fails",
            Self::Disk => "the disk is full",
        }
    }

    /// Whether this pause holds a job back; `commits` says whether the job commits.
    pub fn holds(self, commits: bool) -> bool {
        self == Self::Disk || commits
    }
}

/// A pause of the queue: a job failed on something every job like it
/// would fail on, so they wait, and one of them tries again at
/// `next_try_at` (DEV-604).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QueuePause {
    pub reason: PauseReason,
    /// The output line that showed it.
    pub message: String,
    pub since: String,
    pub next_try_at: String,
    /// The job that paused the queue, and its ticket.
    pub job_id: String,
    pub ticket_id: String,
    /// The ticket whose job paused the queue or took the last try. While
    /// that job runs, also to its end after the pause, no try is due: it
    /// can outlast the interval (DEV-742).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub trying: Option<String>,
}

impl QueuePause {
    pub fn retry_due(&self) -> bool {
        time::parse_rfc3339(&self.next_try_at).ok().is_none_or(|at| at <= jiff::Timestamp::now())
    }

    /// One line for `agent queue` and `agent list-jobs`.
    pub fn summary(&self) -> String {
        format!(
            "paused since {} because {} (job {} on {}: {}); one held job tries again at {}",
            self.since,
            self.reason.describe(),
            self.job_id,
            self.ticket_id,
            self.message,
            self.next_try_at
        )
    }

    /// The try's due and no job runs one: not `trying`'s, unless `ticket`
    /// asks (its job ended), nor one `running` says runs in this process.
    fn try_free(&self, tasks_dir: &Path, ticket: Option<&str>, running: impl Fn(&str) -> bool) -> bool {
        if !self.retry_due() {
            return false;
        }
        let Some(trying) = self.trying.as_deref().filter(|trying| Some(*trying) != ticket) else {
            return true;
        };
        !running(trying) && !crate::services::agent_active_jobs::list(tasks_dir).iter().any(|job| job.ticket_id == trying)
    }
}

/// What a job that ended on a commit it couldn't make retries: only that
/// commit, in the worktree that holds the work (DEV-604).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CommitRetry {
    /// The job whose commit failed.
    pub job_id: String,
    pub worktree_path: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub worktree_branch: Option<String>,
}

/// The status and assignee change a paused job recorded (DEV-320), carried
/// to its retry, with the ticket's status and assignee at the pause: a
/// field the ticket changed in since stays as it is (DEV-604).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CarriedOutcome {
    pub outcome: crate::api_types::JobOutcome,
    pub status_at_pause: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub assignee_at_pause: Option<String>,
}

/// A job the dependency check held back (DEV-530).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ParkedEntry {
    #[serde(flatten)]
    pub entry: AgentQueueEntry,
    /// The ticket's status before it was set to Blocked.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub status: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentQueueEntry {
    pub ticket_id: String,
    /// Agent profile name; empty when the entry names a bare runner.
    #[serde(default)]
    pub agent: String,
    /// Runner for entries queued without a profile (`agent run --runner`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub runner: Option<String>,
    pub prompt: String,
    pub queued_at: String,
    pub attempts: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub effort: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provider: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub session: Option<crate::config::types::AgentSession>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub thread_name: Option<String>,
    /// The T3 Code thread the entry waits for, once its ticket has a comment
    /// that says so (DEV-600).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub held_by: Option<String>,
    /// Set on a paused job's retry that only makes its commit (DEV-604).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub commit_retry: Option<CommitRetry>,
    /// What the paused job recorded for its ticket, applied when its retry ends.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub carried_outcome: Option<CarriedOutcome>,
    /// The entry starts no earlier than this (RFC 3339): a job paused on a
    /// provider's usage limit runs again after the reset (DEV-584).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub not_before: Option<String>,
}

impl AgentQueueEntry {
    /// When the entry still waits for its `not_before` time, that time.
    pub fn waiting_until(&self) -> Option<&str> {
        self.not_before.as_deref().filter(|at| time::parse_rfc3339(at).is_ok_and(|at| at > jiff::Timestamp::now()))
    }

    /// The entry that runs `req` again; `commit_retry` makes it retry only the commit.
    pub fn retry_of(req: &AgentJobCreateRequest, commit_retry: Option<CommitRetry>, carried_outcome: Option<CarriedOutcome>) -> Self {
        Self {
            ticket_id: req.ticket_id.clone(),
            agent: req.agent.clone().unwrap_or_default(),
            runner: req.runner.clone().filter(|_| req.agent.is_none()),
            prompt: req.prompt.clone(),
            queued_at: time::now_rfc3339(),
            attempts: 0,
            model: req.model.clone(),
            effort: req.effort.clone(),
            provider: req.provider.clone(),
            session: req.session,
            thread_name: req.thread_name.clone(),
            held_by: None,
            commit_retry,
            carried_outcome,
            not_before: None,
        }
    }

    fn request(&self) -> AgentJobCreateRequest {
        AgentJobCreateRequest {
            ticket_id: self.ticket_id.clone(),
            prompt: self.prompt.clone(),
            runner: self.runner.clone(),
            agent: (!self.agent.is_empty()).then(|| self.agent.clone()),
            model: self.model.clone(),
            effort: self.effort.clone(),
            provider: self.provider.clone(),
            session: self.session,
            thread_name: self.thread_name.clone(),
        }
    }
}

pub struct AgentQueueService;

/// What [`AgentQueueService::resume`] found and did.
#[derive(Debug, Clone)]
pub struct ResumeOutcome {
    pub pending: usize,
    /// A worker process was started.
    pub started: bool,
    /// Why no worker could start for the pending entries.
    pub error: Option<String>,
}

/// What the queue worker of a tasks directory does (DEV-592).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WorkerState {
    /// A worker process runs.
    Active,
    /// Entries are queued but no worker process runs.
    Stopped,
    /// No entries and no worker.
    Idle,
}

impl WorkerState {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Active => "active",
            Self::Stopped => "stopped",
            Self::Idle => "idle",
        }
    }
}

impl AgentQueueService {
    #[expect(clippy::needless_pass_by_value, reason = "the caller hands this value over and does not use it afterwards")]
    pub fn enqueue(tasks_dir: &Path, req: AgentJobCreateRequest) -> LoTaRResult<bool> {
        let entry = queue_entry(&req)?;
        let (ticket_id, agent, runner) = (entry.ticket_id.clone(), entry.agent.clone(), entry.runner.clone());

        let mut inserted = false;
        with_locked_queue(tasks_dir, |state| {
            // Alias-aware duplicate check: legacy padded entries already in
            // the persisted queue denote the same ticket.
            if state
                .pending
                .iter()
                .any(|item| crate::storage::identity::aliases_match(&item.ticket_id, &ticket_id) && item.agent == agent && item.runner == runner)
            {
                return;
            }
            state.pending.push(entry.clone());
            inserted = true;
        })?;

        if inserted {
            Self::ensure_worker(tasks_dir)?;
        }

        Ok(inserted)
    }

    /// Keep a job the dependency check held back (DEV-530), replacing one
    /// parked earlier for the ticket. The entry keeps what a retry carries
    /// (thread name, commit retry, carried outcome). `status` is the ticket's
    /// status before it was blocked; `None` keeps the one parked before.
    pub fn park(tasks_dir: &Path, entry: &AgentQueueEntry, status: Option<&str>) -> LoTaRResult<()> {
        let entry = AgentQueueEntry { queued_at: time::now_rfc3339(), attempts: 0, held_by: None, ..entry.clone() };
        with_locked_queue(tasks_dir, |state| {
            let earlier = state.parked.iter().position(|item| crate::storage::identity::aliases_match(&item.entry.ticket_id, &entry.ticket_id));
            let earlier = earlier.map(|index| state.parked.remove(index));
            let status = status.map(str::to_string).or_else(|| earlier.and_then(|item| item.status));
            state.parked.push(ParkedEntry { entry: entry.clone(), status });
        })
    }

    /// Move the job parked for `ticket_id` to the queue under one lock
    /// (DEV-530), so a worker that parks it again meanwhile isn't undone.
    /// `Ok(false)`: nothing is parked. A job the queue refuses stays parked.
    pub fn unpark(tasks_dir: &Path, ticket_id: &str) -> LoTaRResult<bool> {
        if !queue_file_path(tasks_dir).exists() {
            return Ok(false);
        }
        let mut moved = Ok(false);
        with_locked_queue(tasks_dir, |state| {
            let Some(index) = state.parked.iter().position(|item| crate::storage::identity::aliases_match(&item.entry.ticket_id, ticket_id)) else {
                return;
            };
            let parked = &state.parked[index].entry;
            let (commit_retry, carried_outcome) = (parked.commit_retry.clone(), parked.carried_outcome.clone());
            moved = queue_entry(&parked.request()).map(|entry| {
                let entry = AgentQueueEntry { commit_retry, carried_outcome, ..entry };
                state.parked.remove(index);
                if !state.pending.iter().any(|item| {
                    crate::storage::identity::aliases_match(&item.ticket_id, &entry.ticket_id) && item.agent == entry.agent && item.runner == entry.runner
                }) {
                    state.pending.push(entry);
                }
                true
            });
        })?;
        if moved.as_ref().is_ok_and(|moved| *moved) {
            Self::ensure_worker(tasks_dir)?;
        }
        moved
    }

    /// Take the job parked for `ticket_id` out of the parked list.
    pub fn take_parked(tasks_dir: &Path, ticket_id: &str) -> LoTaRResult<Option<ParkedEntry>> {
        if !queue_file_path(tasks_dir).exists() {
            return Ok(None);
        }
        let mut taken = None;
        with_locked_queue(tasks_dir, |state| {
            if let Some(index) = state.parked.iter().position(|item| crate::storage::identity::aliases_match(&item.entry.ticket_id, ticket_id)) {
                taken = Some(state.parked.remove(index));
            }
        })?;
        Ok(taken)
    }

    /// The jobs waiting for their tickets' prerequisites.
    pub fn list_parked(tasks_dir: &Path) -> LoTaRResult<Vec<ParkedEntry>> {
        if !queue_file_path(tasks_dir).exists() {
            return Ok(Vec::new());
        }
        let mut entries = Vec::new();
        with_locked_queue(tasks_dir, |state| {
            entries = state.parked.clone();
        })?;
        Ok(entries)
    }

    /// List all pending entries, also those a worker took and has not
    /// started yet, without modifying the queue.
    pub fn list_pending(tasks_dir: &Path) -> LoTaRResult<Vec<AgentQueueEntry>> {
        let queue_path = queue_file_path(tasks_dir);
        if !queue_path.exists() {
            return Ok(Vec::new());
        }
        let mut entries = Vec::new();
        with_locked_queue(tasks_dir, |state| {
            entries = state.taken.iter().chain(&state.pending).cloned().collect();
        })?;
        Ok(entries)
    }

    /// Remove all pending entries from the queue. Returns the number removed.
    pub fn flush(tasks_dir: &Path) -> LoTaRResult<usize> {
        let queue_path = queue_file_path(tasks_dir);
        if !queue_path.exists() {
            return Ok(0);
        }
        let mut count = 0;
        with_locked_queue(tasks_dir, |state| {
            count = state.pending.len() + state.taken.len() + state.parked.len();
            state.pending.clear();
            state.taken.clear();
            state.parked.clear();
        })?;
        Ok(count)
    }

    /// Remove a specific pending entry by ticket ID. Returns true if removed.
    pub fn remove(tasks_dir: &Path, ticket_id: &str) -> LoTaRResult<bool> {
        let mut removed = false;
        with_locked_queue(tasks_dir, |state| {
            let before = state.pending.len() + state.taken.len() + state.parked.len();
            state.pending.retain(|e| e.ticket_id != ticket_id);
            state.taken.retain(|e| e.ticket_id != ticket_id);
            state.parked.retain(|p| p.entry.ticket_id != ticket_id);
            removed = state.pending.len() + state.taken.len() + state.parked.len() < before;
        })?;
        Ok(removed)
    }

    /// Check if the worker process is running.
    pub fn is_worker_running(tasks_dir: &Path) -> bool {
        let Some(lock_path) = worker_lock_path(tasks_dir) else {
            return false;
        };
        if !lock_path.exists() {
            return false;
        }
        let Ok(file) = OpenOptions::new().read(true).open(&lock_path) else {
            return false;
        };
        // If we can acquire exclusive lock, worker is NOT running
        if file.try_lock_exclusive().is_ok() {
            let _ = file.unlock();
            return false;
        }
        true
    }

    /// Start the worker for entries already queued, as `lotar serve` starts
    /// and for `agent queue resume` (DEV-592): a worker that was killed (a
    /// restarted launch agent) leaves its entries behind. `Err` only when the
    /// queue can't be read; a worker that can't start is in the outcome.
    pub fn resume(tasks_dir: &Path) -> LoTaRResult<ResumeOutcome> {
        let pending = Self::list_pending(tasks_dir)?.len();
        let mut outcome = ResumeOutcome { pending, started: false, error: None };
        if pending == 0 {
            return Ok(outcome);
        }
        match worker_slot_free(tasks_dir).and_then(|free| if free { spawn_worker(tasks_dir).map(|()| true) } else { Ok(false) }) {
            Ok(started) => outcome.started = started,
            Err(err) => outcome.error = Some(err.to_string()),
        }
        Ok(outcome)
    }

    /// The worker's state and, with entries queued, what they wait for.
    /// `memory_hold` is the caller's one reading of the memory gate, so
    /// the reason and the hold it shows beside it agree (DEV-681).
    pub fn worker_state(tasks_dir: &Path, pending: &[AgentQueueEntry], memory_hold: Option<&MemoryHold>) -> (WorkerState, Option<String>) {
        let pause = Self::pause_state(tasks_dir);
        if !Self::is_worker_running(tasks_dir) {
            if pending.is_empty() {
                return (WorkerState::Idle, None);
            }
            return (WorkerState::Stopped, Some("no worker process runs; start one with `lotar agent queue resume`".to_string()));
        }
        if pending.is_empty() {
            return (WorkerState::Active, None);
        }
        if let Some(pause) = pause {
            return (WorkerState::Active, Some(format!("the queue is {}", pause.summary())));
        }
        if let Some(hold) = memory_hold {
            return (WorkerState::Active, Some(format!("waiting for memory: {}", hold.describe())));
        }
        if let Some(at) = pending.iter().map(AgentQueueEntry::waiting_until).collect::<Option<Vec<_>>>().and_then(|times| times.into_iter().min()) {
            return (WorkerState::Active, Some(format!("waiting for a provider's usage limit to reset; the next job runs at {at}")));
        }
        let reason = job_slots(tasks_dir, pending, AgentJobService::active_jobs())
            .ok()
            .and_then(|slots| pending.iter().find_map(|entry| slots.wait_reason(entry_project(entry).as_deref())))
            .unwrap_or_else(|| "waiting for the running jobs of their tickets to end".to_string());
        (WorkerState::Active, Some(reason))
    }

    /// Why a job for the ticket must wait for a free slot under
    /// `agent.worktree.max_parallel_jobs` (the global limit for all jobs
    /// together, its project's for that project's), or `None` when it may
    /// start. `own_jobs` are this process's jobs (ticket, job id); the
    /// records of the other processes are added (DEV-630).
    pub fn slot_wait_reason(tasks_dir: &Path, ticket_id: &str, own_jobs: Vec<(String, String)>) -> LoTaRResult<Option<String>> {
        let project = ticket_project(ticket_id);
        let slots = job_slots_for(tasks_dir, project.iter().cloned(), own_jobs)?;
        Ok(slots.wait_reason(project.as_deref()))
    }

    /// Pause the queue because a job failed on `reason`, and put that job's
    /// retry first in the queue (DEV-604). An open disk pause stays one:
    /// it holds every job, a signing pause only those that commit. An open
    /// pause keeps its start; its next try moves to an interval from now,
    /// and waits for the job to end.
    pub fn pause(tasks_dir: &Path, reason: PauseReason, message: &str, job_id: &str, retry: &AgentQueueEntry) -> LoTaRResult<QueuePause> {
        let now = jiff::Timestamp::now();
        let next_try_at = next_try_at_from(now);
        let mut paused = None;
        with_locked_queue(tasks_dir, |state| {
            let pause = match state.pause.take() {
                Some(mut open) if open.reason == PauseReason::Disk && reason == PauseReason::Signing => {
                    open.next_try_at = next_try_at.clone();
                    open.trying = Some(retry.ticket_id.clone());
                    open
                }
                open => QueuePause {
                    reason,
                    message: message.to_string(),
                    since: open.map_or_else(|| now.to_rfc3339(), |open| open.since),
                    next_try_at: next_try_at.clone(),
                    job_id: job_id.to_string(),
                    ticket_id: retry.ticket_id.clone(),
                    trying: Some(retry.ticket_id.clone()),
                },
            };
            state.pending.retain(|item| !same_entry(item, retry));
            state.pending.insert(0, retry.clone());
            paused = Some(pause.clone());
            state.pause = Some(pause);
        })?;
        Self::ensure_worker(tasks_dir)?;
        paused.ok_or_else(|| LoTaRError::ValidationError("The queue could not be paused".to_string()))
    }

    /// Queue the retry of a job that stopped on a provider's usage limit,
    /// first, to start at its `not_before` time (DEV-584). Other jobs run
    /// meanwhile; the queue itself stays open.
    pub fn requeue_after_limit(tasks_dir: &Path, retry: &AgentQueueEntry) -> LoTaRResult<()> {
        with_locked_queue(tasks_dir, |state| {
            state.pending.retain(|item| !same_entry(item, retry));
            state.pending.insert(0, retry.clone());
        })?;
        Self::ensure_worker(tasks_dir)
    }

    /// The queue's open pause, if any.
    pub fn pause_state(tasks_dir: &Path) -> Option<QueuePause> {
        read_state(tasks_dir)?.pause
    }

    /// The open pause that holds a job that `commits` (or not) back.
    pub fn holding(tasks_dir: &Path, commits: bool) -> Option<QueuePause> {
        Self::pause_state(tasks_dir).filter(|pause| pause.reason.holds(commits))
    }

    /// Give the pause's due try to a job that `commits` (or not) and waits
    /// outside the queue, in `lotar serve`, when no queue entry the pause
    /// holds waits for it: else nothing might ever end the pause (DEV-604).
    /// `running` says whether a ticket's job runs in this process.
    /// Taken under the queue lock, so one process gets it.
    pub fn take_held_try(tasks_dir: &Path, ticket_id: &str, commits: bool, running: impl Fn(&str) -> bool) -> bool {
        if !Self::holding(tasks_dir, commits).is_some_and(|pause| pause.retry_due()) {
            return false;
        }
        let mut taken = false;
        let _ = with_locked_queue(tasks_dir, |state| {
            let Some(pause) = state.pause.as_mut().filter(|pause| pause.reason.holds(commits) && pause.try_free(tasks_dir, Some(ticket_id), &running)) else {
                return;
            };
            let reason = pause.reason;
            if state.pending.iter().any(|entry| reason == PauseReason::Disk || reason.holds(entry_commits(tasks_dir, entry))) {
                return;
            }
            pause.next_try_at = next_try_at();
            pause.trying = Some(ticket_id.to_string());
            taken = true;
        });
        taken
    }

    /// End the pause a job that `commits` (or not) just proved over, by
    /// completing: it would have hit it. The held entries start.
    pub fn end_pause(tasks_dir: &Path, commits: bool) -> Option<QueuePause> {
        if !queue_file_path(tasks_dir).exists() {
            return None;
        }
        let mut ended = None;
        with_locked_queue(tasks_dir, |state| {
            if state.pause.as_ref().is_some_and(|pause| pause.reason.holds(commits)) {
                ended = state.pause.take();
            }
        })
        .ok()?;
        if ended.is_some() && !Self::list_pending(tasks_dir).unwrap_or_default().is_empty() {
            let _ = Self::ensure_worker(tasks_dir);
        }
        ended
    }

    pub fn ensure_worker(tasks_dir: &Path) -> LoTaRResult<()> {
        if worker_slot_free(tasks_dir)? {
            let _ = spawn_worker(tasks_dir);
        }
        Ok(())
    }

    /// The queue worker run in process, as tests do: it keeps PATH as it is.
    pub fn run_worker(tasks_dir: &Path) -> LoTaRResult<()> {
        Self::run_worker_with(tasks_dir, false)
    }

    /// The `lotar agent worker` process: before it starts any thread it adds
    /// the login shell's PATH entries, which its jobs inherit (DEV-731). In
    /// process, other threads may run, so `run_worker` leaves PATH alone
    /// (DEV-746).
    pub fn run_worker_process(tasks_dir: &Path) -> LoTaRResult<()> {
        Self::run_worker_with(tasks_dir, true)
    }

    fn run_worker_with(tasks_dir: &Path, login_path: bool) -> LoTaRResult<()> {
        let Some(lock_path) = worker_lock_path(tasks_dir) else {
            return Ok(());
        };
        if let Some(parent) = lock_path.parent() {
            std::fs::create_dir_all(parent).map_err(LoTaRError::IoError)?;
        }
        let lock_file = OpenOptions::new().read(true).write(true).create(true).truncate(false).open(&lock_path).map_err(LoTaRError::IoError)?;

        if lock_file.try_lock_exclusive().is_err() {
            return Ok(());
        }
        if login_path {
            complete_path_from_login_shell();
        }
        // Entries a worker that died took: this worker starts them.
        restore_taken(tasks_dir)?;

        // After an upgrade, take no new entries: once the running jobs end,
        // this worker exits and the next one starts from the new build
        // (DEV-361).
        let upgraded = Arc::new(AtomicBool::new(false));
        let flag = Arc::clone(&upgraded);
        crate::utils::binary_watch::spawn_watcher(move || flag.store(true, Ordering::SeqCst));
        let result = (|| {
            let mut exhausted_error = None;
            loop {
                let upgraded = upgraded.load(Ordering::SeqCst);
                // Jobs of a worker or server that exited before they ended
                // run here from now on (DEV-378).
                if !upgraded {
                    AgentJobService::recover_orphaned_jobs(tasks_dir);
                    // Threads T3 Code runs again with no job (DEV-626).
                    AgentJobService::adopt_resumed_t3_threads(tasks_dir);
                }
                // Short of memory: entries stay queued, unchanged (DEV-623).
                if !upgraded && pending_count(tasks_dir)? > 0 && crate::services::memory_gate::check(tasks_dir).is_none() {
                    let entries = dequeue_startable(tasks_dir)?;
                    for entry in entries {
                        match start_entry(tasks_dir, entry.clone()) {
                            Ok(()) => settle_taken(tasks_dir, &entry, None)?,
                            // The ticket's previous job is still running
                            // (maybe in another process): wait for it rather
                            // than spend retries (DEV-279).
                            Err(err) if is_ticket_busy(&err) => {
                                let mut entry = entry;
                                if err.to_string().contains("the T3 Code thread") {
                                    say_why_entry_waits(tasks_dir, &mut entry);
                                }
                                settle_taken(tasks_dir, &entry, Some(&entry))?;
                            }
                            Err(err) => {
                                if !requeue_entry(tasks_dir, entry, &err)? {
                                    exhausted_error = Some(err);
                                }
                            }
                        }
                    }
                }

                let stats = AgentJobService::queue_stats();
                // End rules a full disk kept from their checkpoint live only in this process (DEV-633).
                let unsaved_end_rules = crate::services::automation_service::has_unsaved_end_actions();
                if stats.running == 0 && stats.queued == 0 && !unsaved_end_rules && (upgraded || pending_count(tasks_dir)? == 0) {
                    break;
                }

                thread::sleep(Duration::from_millis(WORKER_POLL_MS));
            }

            exhausted_error.map_or(Ok(()), Err)
        })();
        // Even a queue/config I/O failure must not kill dispatched job threads.
        loop {
            let stats = AgentJobService::queue_stats();
            if stats.running == 0 && stats.queued == 0 {
                break;
            }
            thread::sleep(Duration::from_millis(WORKER_POLL_MS));
        }
        // Entries a failed queue write left taken go back for the next worker.
        let _ = restore_taken(tasks_dir);
        // An entry queued while this worker was exiting saw the lock held and
        // spawned no worker; start the next one now (DEV-279).
        let _ = lock_file.unlock();
        drop(lock_file);
        if pending_count(tasks_dir).unwrap_or(0) > 0 {
            let _ = Self::ensure_worker(tasks_dir);
        }
        result
    }
}

/// True when workers are enabled and no worker holds this tasks directory's lock.
fn worker_slot_free(tasks_dir: &Path) -> LoTaRResult<bool> {
    if std::env::var("LOTAR_AGENT_QUEUE_DISABLE_WORKER").is_ok() {
        return Ok(false);
    }
    let Some(lock_path) = worker_lock_path(tasks_dir) else {
        return Ok(false);
    };
    if let Some(parent) = lock_path.parent() {
        std::fs::create_dir_all(parent).map_err(LoTaRError::IoError)?;
    }
    let file = OpenOptions::new().read(true).write(true).create(true).truncate(false).open(&lock_path).map_err(LoTaRError::IoError)?;
    if file.try_lock_exclusive().is_err() {
        return Ok(false);
    }
    let _ = file.unlock();
    Ok(true)
}

fn start_entry(tasks_dir: &Path, entry: AgentQueueEntry) -> LoTaRResult<()> {
    let last_try = entry.attempts >= MAX_ATTEMPTS;
    let req = entry.request();
    if !crate::services::automation_service::queued_stage_allows_start(tasks_dir, &req) {
        return Ok(());
    }
    // A prerequisite can reopen while the job waits, and a check that failed
    // at assignment is tried again here (DEV-524). An open one parks the job
    // until it is done (DEV-530).
    if req.agent.is_some()
        && !crate::services::automation_service::prerequisites_allow_start(tasks_dir, &entry, last_try).map_err(LoTaRError::ValidationError)?
    {
        return Ok(());
    }
    let _ = AgentJobService::start_queued_job(req, tasks_dir, entry.commit_retry, entry.carried_outcome)?;
    Ok(())
}

/// The queue entry for `req`, checked and with the canonical ticket id, so
/// dequeued starts and duplicate checks share one identity with the job
/// registry.
fn queue_entry(req: &AgentJobCreateRequest) -> LoTaRResult<AgentQueueEntry> {
    let runner = req.runner.as_deref().map(str::trim).filter(|runner| !runner.is_empty()).map(str::to_string);
    let agent = match req.agent.clone() {
        Some(agent) => agent,
        None if runner.is_some() => String::new(),
        None => {
            return Err(LoTaRError::ValidationError("Agent profile or runner required".to_string()));
        }
    };
    let ticket_id = req.ticket_id.trim().to_string();
    if ticket_id.is_empty() {
        return Err(LoTaRError::ValidationError("Ticket id is required".to_string()));
    }
    let ticket_id =
        crate::storage::TaskId::parse(&ticket_id).map_err(|err| LoTaRError::ValidationError(format!("Invalid ticket id '{ticket_id}': {err}")))?.canonical();
    let prompt = req.prompt.trim().to_string();
    if prompt.is_empty() {
        return Err(LoTaRError::ValidationError("Prompt cannot be empty".to_string()));
    }
    Ok(AgentQueueEntry { ticket_id, agent, runner, prompt, ..AgentQueueEntry::retry_of(req, None, None) })
}

fn is_ticket_busy(err: &LoTaRError) -> bool {
    err.to_string().contains("already has an active job")
}

/// Comment on the ticket why its entry waits when an earlier job's T3 Code
/// thread holds it, once per entry and thread: the queue tries the entry
/// again every few hundred milliseconds (DEV-600).
fn say_why_entry_waits(tasks_dir: &Path, entry: &mut AgentQueueEntry) {
    let Some(hold) = crate::services::agent_job_service::recent_t3_thread_hold(&entry.ticket_id) else {
        return;
    };
    if entry.held_by.as_deref() == Some(hold.thread.as_str()) {
        return;
    }
    let who = entry_label(entry);
    let thread = hold.link.as_deref().unwrap_or(&hold.thread);
    let text = if hold.waiting {
        format!(
            "The queued {who} job waits: the T3 Code thread {thread} of the job {} waits for an answer. Answer it in T3 Code; the job starts once the thread's turn ends.",
            hold.job
        )
    } else {
        format!("The queued {who} job waits: T3 Code still runs the thread {thread} of the job {}. The job starts once the thread ends.", hold.job)
    };
    if comment_entry_ticket(tasks_dir, entry, &text) {
        entry.held_by = Some(hold.thread);
    }
}

fn entry_label(entry: &AgentQueueEntry) -> String {
    if entry.agent.is_empty() { entry.runner.clone().unwrap_or_default() } else { format!("@{}", entry.agent) }
}

fn comment_entry_ticket(tasks_dir: &Path, entry: &AgentQueueEntry, text: &str) -> bool {
    let mut storage = crate::storage::manager::Storage::new(tasks_dir);
    crate::services::task_service::TaskService::add_comment_with_context(
        &mut storage,
        &entry.ticket_id,
        text,
        crate::services::task_service::CommentContext::automation(),
    )
    .is_ok()
}

fn same_entry(a: &AgentQueueEntry, b: &AgentQueueEntry) -> bool {
    a.ticket_id == b.ticket_id && a.agent == b.agent && a.runner == b.runner
}

/// Drop a taken entry, once its job is registered or it is given up, and
/// queue `again` in its place unless the entry was removed meanwhile.
fn settle_taken(tasks_dir: &Path, entry: &AgentQueueEntry, again: Option<&AgentQueueEntry>) -> LoTaRResult<()> {
    with_locked_queue(tasks_dir, |state| {
        let Some(pos) = state.taken.iter().position(|item| same_entry(item, entry)) else {
            return;
        };
        state.taken.remove(pos);
        if let Some(again) = again
            && !state.pending.iter().any(|item| same_entry(item, again))
        {
            state.pending.push(again.clone());
        }
    })
}

/// Queue the taken entries again, ahead of the others.
fn restore_taken(tasks_dir: &Path) -> LoTaRResult<()> {
    with_locked_queue(tasks_dir, |state| {
        let mut restored: Vec<_> =
            std::mem::take(&mut state.taken).into_iter().filter(|entry| !state.pending.iter().any(|item| same_entry(item, entry))).collect();
        restored.append(&mut state.pending);
        state.pending = restored;
    })
}

/// Queue a failed entry again with one attempt more, or give it up after
/// its last retry and say why on its ticket (DEV-661). False when given up.
fn requeue_entry(tasks_dir: &Path, mut entry: AgentQueueEntry, err: &LoTaRError) -> LoTaRResult<bool> {
    entry.attempts = entry.attempts.saturating_add(1);
    if entry.attempts > MAX_ATTEMPTS {
        settle_taken(tasks_dir, &entry, None)?;
        let text = format!("The queued {} job did not start after {} tries and left the queue: {err}", entry_label(&entry), entry.attempts);
        let _ = comment_entry_ticket(tasks_dir, &entry, &text);
        return Ok(false);
    }
    settle_taken(tasks_dir, &entry, Some(&entry))?;
    Ok(true)
}

/// Take the entries that have a free slot, in queue order; the others stay
/// queued (DEV-630). While the queue is paused, the entries it holds stay,
/// except one when its next try is due (DEV-604).
fn dequeue_startable(tasks_dir: &Path) -> LoTaRResult<Vec<AgentQueueEntry>> {
    let mut entries = Ok(Vec::new());
    with_locked_queue(tasks_dir, |state| {
        let mut slots = match job_slots(tasks_dir, &state.pending, AgentJobService::active_jobs()) {
            Ok(slots) => slots,
            Err(err) => {
                entries = Err(err);
                return;
            }
        };
        let own: HashSet<String> =
            AgentJobService::active_jobs().into_iter().map(|(ticket, _)| ticket).chain(state.taken.iter().map(|entry| entry.ticket_id.clone())).collect();
        let mut retry_due = state.pause.as_ref().is_some_and(|pause| pause.try_free(tasks_dir, None, |ticket| own.contains(ticket)));
        let mut take = Vec::new();
        let mut keep = Vec::new();
        for entry in std::mem::take(&mut state.pending) {
            // A job paused on a usage limit waits for the reset (DEV-584).
            if entry.waiting_until().is_some() {
                keep.push(entry);
                continue;
            }
            // A disk pause holds every entry: no need to read their profiles.
            let held = state.pause.as_ref().is_some_and(|pause| pause.reason == PauseReason::Disk || pause.reason.holds(entry_commits(tasks_dir, &entry)));
            if held && !retry_due {
                keep.push(entry);
            } else if slots.take(&entry) {
                if held {
                    retry_due = false;
                    if let Some(pause) = state.pause.as_mut() {
                        pause.next_try_at = next_try_at();
                        pause.trying = Some(entry.ticket_id.clone());
                    }
                }
                take.push(entry);
            } else {
                keep.push(entry);
            }
        }
        state.pending = keep;
        state.taken.extend(take.iter().cloned());
        entries = Ok(take);
    })?;
    entries
}

/// A limit of `agent.worktree.max_parallel_jobs` and the jobs it counts.
#[derive(Debug, Clone, Copy)]
struct Slot {
    running: usize,
    limit: Option<usize>,
}

/// The global limit caps every project's jobs together, the limit of an
/// entry's project (its own, else the global one) that project's jobs.
#[derive(Debug)]
struct JobSlots {
    total: Slot,
    projects: HashMap<String, Slot>,
}

impl JobSlots {
    /// Take a slot for the entry when the global and its project's limit
    /// both have one free.
    fn take(&mut self, entry: &AgentQueueEntry) -> bool {
        let project = entry_project(entry);
        if self.wait_reason(project.as_deref()).is_some() {
            return false;
        }
        self.total.running += 1;
        if let Some(slot) = project.and_then(|project| self.projects.get_mut(&project)) {
            slot.running += 1;
        }
        true
    }

    /// Why a job of the project waits, the global limit first.
    fn wait_reason(&self, project: Option<&str>) -> Option<String> {
        if let Slot { running, limit: Some(max) } = self.total
            && running >= max
        {
            return Some(format!("waiting for a free slot ({running} of the global agent.worktree.max_parallel_jobs {max} running)"));
        }
        let project = project?;
        match self.projects.get(project) {
            Some(&Slot { running, limit: Some(max) }) if running >= max => {
                Some(format!("waiting for a free slot ({running} of {project}'s agent.worktree.max_parallel_jobs {max} running)"))
            }
            _ => None,
        }
    }
}

fn job_slots(tasks_dir: &Path, entries: &[AgentQueueEntry], own_jobs: Vec<(String, String)>) -> LoTaRResult<JobSlots> {
    job_slots_for(tasks_dir, entries.iter().filter_map(entry_project), own_jobs)
}

/// The slots for the projects. The jobs are counted in every process: this
/// process's own, also before they record themselves, and the records of
/// `lotar serve`, `agent run --wait` and taken-over jobs.
fn job_slots_for(tasks_dir: &Path, projects: impl Iterator<Item = String>, own_jobs: Vec<(String, String)>) -> LoTaRResult<JobSlots> {
    let mut jobs: HashSet<(String, String)> = own_jobs.into_iter().collect();
    jobs.extend(crate::services::agent_active_jobs::list(tasks_dir).into_iter().map(|job| (job.ticket_id, job.job_id)));
    let global = ConfigManager::new_manager_with_tasks_dir_readonly(tasks_dir).map_err(|e| LoTaRError::ValidationError(e.to_string()))?;
    let total = Slot { running: jobs.len(), limit: global.get_resolved_config().agent_worktree.max_parallel_jobs };
    let mut slots = HashMap::new();
    for project in projects {
        if slots.contains_key(&project) {
            continue;
        }
        let config = crate::config::resolution::config_for_project(tasks_dir, Some(&project)).map_err(|e| LoTaRError::ValidationError(e.to_string()))?;
        let running = jobs.iter().filter(|(ticket, _)| ticket_project(ticket).as_deref() == Some(project.as_str())).count();
        slots.insert(project, Slot { running, limit: config.agent_worktree.max_parallel_jobs });
    }
    Ok(JobSlots { total, projects: slots })
}

fn entry_project(entry: &AgentQueueEntry) -> Option<String> {
    ticket_project(&entry.ticket_id)
}

fn ticket_project(ticket_id: &str) -> Option<String> {
    crate::storage::TaskId::parse(ticket_id).ok().map(|id| id.project)
}

fn entry_commits(tasks_dir: &Path, entry: &AgentQueueEntry) -> bool {
    entry.commit_retry.is_some() || AgentJobService::job_commits(tasks_dir, &entry.request())
}

/// How long a pause waits before its next try: `LOTAR_AGENT_PAUSE_RETRY_SECONDS`,
/// else ten minutes.
fn retry_interval() -> jiff::SignedDuration {
    let seconds = std::env::var("LOTAR_AGENT_PAUSE_RETRY_SECONDS").ok().and_then(|value| value.trim().parse::<i64>().ok()).unwrap_or(PAUSE_RETRY_SECONDS);
    jiff::SignedDuration::from_secs(seconds.max(0))
}

/// When a pause's next try is due, an interval from `now`.
fn next_try_at_from(now: jiff::Timestamp) -> String {
    now.saturating_add(retry_interval()).unwrap_or(jiff::Timestamp::MAX).to_rfc3339()
}

fn next_try_at() -> String {
    next_try_at_from(jiff::Timestamp::now())
}

fn pending_count(tasks_dir: &Path) -> LoTaRResult<usize> {
    let mut count = 0;
    with_locked_queue(tasks_dir, |state| {
        count = state.pending.len();
    })?;
    Ok(count)
}

fn with_locked_queue<F>(tasks_dir: &Path, mut apply: F) -> LoTaRResult<()>
where
    F: FnMut(&mut AgentQueueState),
{
    let queue_path = queue_file_path(tasks_dir);
    if let Some(parent) = queue_path.parent() {
        std::fs::create_dir_all(parent).map_err(LoTaRError::IoError)?;
    }

    let mut file = OpenOptions::new().read(true).write(true).create(true).truncate(false).open(&queue_path).map_err(LoTaRError::IoError)?;

    file.lock_exclusive().map_err(LoTaRError::IoError)?;
    let mut state = load_state(&mut file, tasks_dir)?;
    apply(&mut state);
    save_state(&mut file, &state)?;
    file.unlock().map_err(LoTaRError::IoError)?;
    Ok(())
}

/// The queue's state, read without writing it back; `None` without a queue.
fn read_state(tasks_dir: &Path) -> Option<AgentQueueState> {
    let mut file = File::open(queue_file_path(tasks_dir)).ok()?;
    file.lock_shared().ok()?;
    let state = load_state(&mut file, tasks_dir).ok();
    let _ = file.unlock();
    state
}

fn load_state(file: &mut File, tasks_dir: &Path) -> LoTaRResult<AgentQueueState> {
    file.seek(SeekFrom::Start(0)).map_err(LoTaRError::IoError)?;
    let mut buf = String::new();
    file.read_to_string(&mut buf).map_err(LoTaRError::IoError)?;
    if buf.trim().is_empty() {
        return Ok(AgentQueueState {
            version: QUEUE_VERSION,
            tasks_dir: tasks_dir.to_string_lossy().to_string(),
            updated_at: time::now_rfc3339(),
            pending: Vec::new(),
            taken: Vec::new(),
            pause: None,
            parked: Vec::new(),
        });
    }
    let mut state: AgentQueueState = serde_json::from_str(&buf).map_err(|e| LoTaRError::SerializationError(e.to_string()))?;
    if state.version != QUEUE_VERSION {
        state.version = QUEUE_VERSION;
    }
    Ok(state)
}

fn save_state(file: &mut File, state: &AgentQueueState) -> LoTaRResult<()> {
    let mut state = state.clone();
    state.updated_at = time::now_rfc3339();
    let payload = serde_json::to_string_pretty(&state).map_err(|e| LoTaRError::SerializationError(e.to_string()))?;
    file.set_len(0).map_err(LoTaRError::IoError)?;
    file.seek(SeekFrom::Start(0)).map_err(LoTaRError::IoError)?;
    file.write_all(payload.as_bytes()).map_err(LoTaRError::IoError)?;
    file.flush().map_err(LoTaRError::IoError)?;
    Ok(())
}

/// The session of the agent whose lotar call starts a worker: its thread, its
/// messaging socket, its process. The worker outlives that session and runs
/// every later job, whose agents would act as that session (DEV-733).
const AGENT_SESSION_VARS: &[&str] = &[
    "AI_AGENT",
    "CLAUDECODE",
    "CLAUDE_PID",
    "CLAUDE_EFFORT",
    "CLAUDE_AGENT_SDK_VERSION",
    "CLAUDE_CODE_SESSION_ID",
    "CLAUDE_CODE_SESSION_ATTENDED",
    "CLAUDE_CODE_MESSAGING_SOCKET",
    "CLAUDE_CODE_MESSAGING_TOKEN",
    "CLAUDE_CODE_ENTRYPOINT",
    "CLAUDE_CODE_EXECPATH",
    "CLAUDE_CODE_CHILD_SESSION",
    "CLAUDE_CODE_CHILD_NOTES_DIR",
    "CLAUDE_CODE_AUTO_COMPACT_WINDOW",
    "CODEX_THREAD_ID",
    "CODEX_SANDBOX",
    "CODEX_SANDBOX_NETWORK_DISABLED",
    "CODEX_CI",
    "T3CODE_THREAD_ID",
];

fn spawn_worker(tasks_dir: &Path) -> LoTaRResult<()> {
    // The path lotar was started as, so an upgraded install starts the new
    // build (DEV-361).
    let exe = crate::utils::binary_watch::watched_path().ok_or_else(|| LoTaRError::ValidationError("cannot locate the lotar executable".to_string()))?;
    let tasks_dir = std::path::absolute(tasks_dir).map_err(LoTaRError::IoError)?;
    let mut cmd = Command::new(exe);
    cmd.arg("--tasks-dir").arg(&tasks_dir).arg("agent").arg("worker");
    cmd.env("LOTAR_TASKS_DIR", &tasks_dir);
    for key in AGENT_SESSION_VARS {
        cmd.env_remove(key);
    }
    // Not the starting command's folder: a job's worktree is removed while
    // the worker lives on, and every job it starts then fails (DEV-732).
    cmd.current_dir(tasks_dir.parent().unwrap_or(&tasks_dir));
    cmd.stdin(Stdio::null());
    // Its own process group: launchd kills a job's group when the job ends,
    // and the worker would die with the entries it took (DEV-660).
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        cmd.process_group(0);
    }

    if std::env::var("LOTAR_AGENT_WORKER_VERBOSE").is_ok() {
        cmd.stdout(Stdio::inherit());
        cmd.stderr(Stdio::inherit());
    } else {
        cmd.stdout(Stdio::null());
        cmd.stderr(Stdio::null());
    }

    cmd.spawn().map_err(LoTaRError::IoError)?;
    Ok(())
}

/// Marks where the login shell's PATH starts in its output, after anything
/// its profile prints.
const LOGIN_PATH_MARKER: &str = "__LOTAR_LOGIN_PATH__";
const LOGIN_SHELL_TIMEOUT: Duration = Duration::from_secs(5);

/// A worker runs every later job, whoever's `lotar` call started it: an
/// agent's shell or a launchd job can have a short PATH (DEV-731). The
/// entries of the user's login shell PATH that this one lacks go after it.
fn complete_path_from_login_shell() {
    if cfg!(windows) {
        return;
    }
    let Some(shell) = std::env::var_os("SHELL").filter(|shell| !shell.is_empty()) else {
        eprintln!("lotar agent worker: SHELL is not set; jobs keep PATH {}", std::env::var("PATH").unwrap_or_default());
        return;
    };
    let current = std::env::var("PATH").unwrap_or_default();
    match login_shell_path(&shell) {
        Some(login) => {
            let merged = merge_paths(&current, &login);
            if merged != current {
                // SAFETY: only the `lotar agent worker` process gets here,
                // before it starts a thread; the login shell's reader thread
                // has sent its output and only ends now; it doesn't read the
                // environment.
                unsafe { std::env::set_var("PATH", merged) };
            }
        }
        None => eprintln!("lotar agent worker: no PATH from login shell {}; jobs keep PATH {current}", shell.to_string_lossy()),
    }
}

/// The PATH `shell -l` sets, or `None` when it fails or takes too long.
fn login_shell_path(shell: &std::ffi::OsStr) -> Option<String> {
    let mut child = Command::new(shell)
        .args(["-l", "-c", &format!("printf '%s%s' '{LOGIN_PATH_MARKER}' \"$PATH\"")])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .ok()?;
    let mut stdout = child.stdout.take()?;
    // The pipe stays open while anything the profile started in the
    // background holds it (DEV-745), so the read gets the same deadline.
    let (sender, output) = std::sync::mpsc::channel();
    thread::spawn(move || {
        let mut out = String::new();
        let _ = stdout.read_to_string(&mut out);
        let _ = sender.send(out);
    });
    let deadline = std::time::Instant::now() + LOGIN_SHELL_TIMEOUT;
    let status = loop {
        if let Ok(Some(status)) = child.try_wait() {
            break status;
        }
        if std::time::Instant::now() >= deadline {
            let _ = child.kill();
            let _ = child.wait();
            return None;
        }
        thread::sleep(Duration::from_millis(10));
    };
    let out = output.recv_timeout(deadline.saturating_duration_since(std::time::Instant::now())).ok()?;
    let path = out.rsplit_once(LOGIN_PATH_MARKER)?.1.trim();
    (status.success() && !path.is_empty()).then(|| path.to_string())
}

/// `current` followed by the entries of `login` it lacks.
fn merge_paths(current: &str, login: &str) -> String {
    let mut entries: Vec<&str> = current.split(':').filter(|entry| !entry.is_empty()).collect();
    for entry in login.split(':') {
        if !entry.is_empty() && !entries.contains(&entry) {
            entries.push(entry);
        }
    }
    entries.join(":")
}

fn queue_base_dir() -> PathBuf {
    if let Ok(dir) = std::env::var("LOTAR_AGENT_QUEUE_DIR")
        && !dir.trim().is_empty()
    {
        return PathBuf::from(dir);
    }

    if let Ok(dir) = std::env::var("XDG_RUNTIME_DIR")
        && !dir.trim().is_empty()
    {
        return PathBuf::from(dir).join("lotar");
    }

    if let Some(cache) = dirs::cache_dir() {
        return cache.join("lotar");
    }

    std::env::temp_dir().join("lotar")
}

fn queue_file_path(tasks_dir: &Path) -> PathBuf {
    let hash = queue_hash(tasks_dir);
    queue_dir().join(format!("queue-{hash}.json"))
}

fn worker_lock_path(tasks_dir: &Path) -> Option<PathBuf> {
    let hash = queue_hash(tasks_dir);
    Some(queue_dir().join(format!("worker-{hash}.lock")))
}

pub(crate) fn queue_dir() -> PathBuf {
    queue_base_dir().join("agent-queue")
}

pub(crate) fn queue_hash(tasks_dir: &Path) -> String {
    let raw = tasks_dir.to_string_lossy();
    let hash = blake3::hash(raw.as_bytes());
    hash.to_hex().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::env;
    use tempfile::tempdir;

    #[test]
    fn merge_paths_appends_missing_login_entries() {
        assert_eq!(
            merge_paths("/opt/homebrew/bin:/opt/homebrew/bin:/usr/bin", "/a/.cargo/bin:/usr/bin::/opt/homebrew/bin"),
            "/opt/homebrew/bin:/opt/homebrew/bin:/usr/bin:/a/.cargo/bin"
        );
        assert_eq!(merge_paths("", "/usr/bin"), "/usr/bin");
    }

    struct EnvGuard {
        key: &'static str,
        prev: Option<String>,
    }

    impl EnvGuard {
        fn set(key: &'static str, value: &str) -> Self {
            let prev = env::var(key).ok();
            unsafe {
                env::set_var(key, value);
            }
            Self { key, prev }
        }
    }

    impl Drop for EnvGuard {
        fn drop(&mut self) {
            if let Some(prev) = self.prev.take() {
                unsafe {
                    env::set_var(self.key, prev);
                }
            } else {
                unsafe {
                    env::remove_var(self.key);
                }
            }
        }
    }

    #[test]
    fn queued_entries_keep_the_runs_model_and_effort() {
        let dir = tempdir().unwrap();
        let _guard = EnvGuard::set("LOTAR_AGENT_QUEUE_DIR", dir.path().to_str().unwrap());
        let _worker_guard = EnvGuard::set("LOTAR_AGENT_QUEUE_DISABLE_WORKER", "1");
        let tasks_dir = dir.path().join("tasks");
        std::fs::create_dir_all(&tasks_dir).unwrap();

        let req = AgentJobCreateRequest {
            ticket_id: "TEST-1".to_string(),
            prompt: "hello".to_string(),
            agent: Some("implement".to_string()),
            model: Some("haiku".to_string()),
            effort: Some("low".to_string()),
            ..Default::default()
        };
        assert!(AgentQueueService::enqueue(&tasks_dir, req).unwrap());
        let entries = dequeue_startable(&tasks_dir).unwrap();
        assert_eq!(entries[0].model.as_deref(), Some("haiku"));
        assert_eq!(entries[0].effort.as_deref(), Some("low"));

        // Entries written before these fields existed still load.
        let legacy: AgentQueueEntry = serde_json::from_str(r#"{"ticket_id":"TEST-2","agent":"a","prompt":"p","queued_at":"now","attempts":0}"#).unwrap();
        assert!(legacy.model.is_none() && legacy.effort.is_none());
    }

    fn entry(ticket: &str) -> AgentQueueEntry {
        AgentQueueEntry::retry_of(
            &AgentJobCreateRequest { ticket_id: ticket.into(), prompt: "p".into(), agent: Some("a".into()), ..Default::default() },
            None,
            None,
        )
    }

    /// DEV-604: a signing pause holds only jobs that commit, a disk pause
    /// every job; the worker starts one held entry, the first, when the next
    /// try is due; a job the pause would have held ends it
    /// by completing.
    #[test]
    fn a_pause_holds_jobs_in_its_scope_and_admits_one_try_when_due() {
        let dir = tempdir().unwrap();
        let _guard = EnvGuard::set("LOTAR_AGENT_QUEUE_DIR", dir.path().to_str().unwrap());
        let _worker_guard = EnvGuard::set("LOTAR_AGENT_QUEUE_DISABLE_WORKER", "1");
        let _interval = EnvGuard::set("LOTAR_AGENT_PAUSE_RETRY_SECONDS", "3600");
        let tasks_dir = dir.path().join("tasks");
        std::fs::create_dir_all(&tasks_dir).unwrap();
        assert!(AgentQueueService::holding(&tasks_dir, true).is_none(), "no queue, no pause");

        let pause = AgentQueueService::pause(&tasks_dir, PauseReason::Signing, "Couldn't find key in agent", "job-1", &entry("TEST-1")).unwrap();
        assert_eq!(pause.ticket_id, "TEST-1");
        assert!(AgentQueueService::holding(&tasks_dir, false).is_none(), "a job that doesn't commit starts");
        assert!(AgentQueueService::holding(&tasks_dir, true).is_some());
        assert!(AgentQueueService::end_pause(&tasks_dir, false).is_none(), "a job that doesn't commit proves nothing");

        // A disk pause isn't narrowed by a later signing failure.
        AgentQueueService::pause(&tasks_dir, PauseReason::Disk, "No space left on device", "job-2", &entry("TEST-2")).unwrap();
        AgentQueueService::pause(&tasks_dir, PauseReason::Signing, "gpg failed to sign the data", "job-3", &entry("TEST-3")).unwrap();
        let open = AgentQueueService::pause_state(&tasks_dir).unwrap();
        assert_eq!((open.reason, open.job_id.as_str(), open.since.as_str()), (PauseReason::Disk, "job-2", pause.since.as_str()));
        assert!(AgentQueueService::holding(&tasks_dir, false).is_some(), "a full disk holds every job");
        let order: Vec<String> = AgentQueueService::list_pending(&tasks_dir).unwrap().into_iter().map(|entry| entry.ticket_id).collect();
        assert_eq!(order, ["TEST-3", "TEST-2", "TEST-1"], "each paused job's retry goes first");
        assert!(dequeue_startable(&tasks_dir).unwrap().is_empty(), "the worker takes no held entry");

        // The next try is due: one entry, the first, starts, and the next
        // try moves on.
        let due = time::now_rfc3339();
        with_locked_queue(&tasks_dir, |state| state.pause.as_mut().unwrap().next_try_at = due.clone()).unwrap();
        let taken: Vec<String> = dequeue_startable(&tasks_dir).unwrap().into_iter().map(|entry| entry.ticket_id).collect();
        assert_eq!(taken, ["TEST-3"]);
        assert!(dequeue_startable(&tasks_dir).unwrap().is_empty(), "one try per interval");

        // DEV-742: a try that outlasts the interval keeps the next one back
        // until its job ends.
        settle_taken(&tasks_dir, &entry("TEST-3"), None).unwrap();
        let record: crate::services::agent_active_jobs::ActiveJob = serde_json::from_value(serde_json::json!({
            "job_id": "job-4", "ticket_id": "TEST-3", "runner": "command", "owner_pid": std::process::id(), "child_pid": 0, "started_at": due,
        }))
        .unwrap();
        crate::services::agent_active_jobs::register(&tasks_dir, &record);
        let due = time::now_rfc3339();
        with_locked_queue(&tasks_dir, |state| state.pause.as_mut().unwrap().next_try_at = due.clone()).unwrap();
        assert!(dequeue_startable(&tasks_dir).unwrap().is_empty(), "the try's job still runs");
        crate::services::agent_active_jobs::unregister(&tasks_dir, "job-4");
        let taken: Vec<String> = dequeue_startable(&tasks_dir).unwrap().into_iter().map(|entry| entry.ticket_id).collect();
        assert_eq!(taken, ["TEST-2"], "its job ended: the next try is due");

        assert_eq!(AgentQueueService::end_pause(&tasks_dir, false).map(|pause| pause.reason), Some(PauseReason::Disk));
        assert!(AgentQueueService::pause_state(&tasks_dir).is_none());
        assert_eq!(dequeue_startable(&tasks_dir).unwrap().len(), 1, "the held entries start");
    }

    #[test]
    fn enqueue_deduplicates() {
        let dir = tempdir().unwrap();
        let _guard = EnvGuard::set("LOTAR_AGENT_QUEUE_DIR", dir.path().to_str().unwrap());
        let _worker_guard = EnvGuard::set("LOTAR_AGENT_QUEUE_DISABLE_WORKER", "1");
        let tasks_dir = dir.path().join("tasks");
        std::fs::create_dir_all(&tasks_dir).unwrap();

        let req = AgentJobCreateRequest {
            ticket_id: "TEST-1".to_string(),
            prompt: "hello".to_string(),
            runner: None,
            agent: Some("implement".to_string()),
            ..Default::default()
        };

        assert!(AgentQueueService::enqueue(&tasks_dir, req.clone()).unwrap());
        assert!(!AgentQueueService::enqueue(&tasks_dir, req).unwrap());
        assert_eq!(pending_count(&tasks_dir).unwrap(), 1);
    }

    /// DEV-530: unpark moves the parked job to the queue in one step, as a
    /// fresh entry that keeps what a retry carries (thread name, commit
    /// retry, carried outcome); one the queue refuses stays parked.
    #[test]
    fn unpark_moves_the_parked_job_to_the_queue() {
        let dir = tempdir().unwrap();
        let _guard = EnvGuard::set("LOTAR_AGENT_QUEUE_DIR", dir.path().to_str().unwrap());
        let _worker_guard = EnvGuard::set("LOTAR_AGENT_QUEUE_DISABLE_WORKER", "1");
        let tasks_dir = dir.path().join("tasks");
        std::fs::create_dir_all(&tasks_dir).unwrap();
        let req = |ticket: &str, prompt: &str| AgentJobCreateRequest {
            ticket_id: ticket.to_string(),
            prompt: prompt.to_string(),
            agent: Some("review".to_string()),
            thread_name: Some(format!("{ticket} review")),
            ..Default::default()
        };
        let commit_retry = CommitRetry { job_id: "job-1".to_string(), worktree_path: "/tmp/wt".to_string(), worktree_branch: Some("agent/TEST-1".to_string()) };
        let carried = CarriedOutcome {
            outcome: crate::api_types::JobOutcome { job_id: "job-1".to_string(), status: Some("Merging".to_string()), ..Default::default() },
            status_at_pause: "Reviewing".to_string(),
            assignee_at_pause: Some("@review".to_string()),
        };
        let first =
            AgentQueueEntry { attempts: 2, ..AgentQueueEntry::retry_of(&req("TEST-1", "Review TEST-1"), Some(commit_retry.clone()), Some(carried.clone())) };
        AgentQueueService::park(&tasks_dir, &first, Some("Reviewing")).unwrap();
        AgentQueueService::park(&tasks_dir, &AgentQueueEntry::retry_of(&req("TEST-2", "  "), None, None), None).unwrap();

        assert!(AgentQueueService::unpark(&tasks_dir, "TEST-1").unwrap());
        assert!(!AgentQueueService::unpark(&tasks_dir, "TEST-1").unwrap(), "nothing parked any more");
        assert!(AgentQueueService::unpark(&tasks_dir, "TEST-2").is_err());
        let pending = AgentQueueService::list_pending(&tasks_dir).unwrap();
        assert_eq!(
            pending.iter().map(|entry| (entry.ticket_id.as_str(), entry.prompt.as_str(), entry.attempts)).collect::<Vec<_>>(),
            [("TEST-1", "Review TEST-1", 0)]
        );
        assert_eq!(pending[0].thread_name.as_deref(), Some("TEST-1 review"));
        assert_eq!(pending[0].commit_retry.as_ref(), Some(&commit_retry));
        assert_eq!(pending[0].carried_outcome.as_ref(), Some(&carried));
        let parked = AgentQueueService::list_parked(&tasks_dir).unwrap();
        assert_eq!(parked.iter().map(|item| item.entry.ticket_id.as_str()).collect::<Vec<_>>(), ["TEST-2"]);
    }

    #[test]
    fn enqueue_canonicalizes_padded_aliases() {
        let dir = tempdir().unwrap();
        let _guard = EnvGuard::set("LOTAR_AGENT_QUEUE_DIR", dir.path().to_str().unwrap());
        let _worker_guard = EnvGuard::set("LOTAR_AGENT_QUEUE_DISABLE_WORKER", "1");
        let tasks_dir = dir.path().join("tasks");
        std::fs::create_dir_all(&tasks_dir).unwrap();

        let req = AgentJobCreateRequest {
            ticket_id: "TEST-001".to_string(),
            prompt: "hello".to_string(),
            runner: None,
            agent: Some("implement".to_string()),
            ..Default::default()
        };
        assert!(AgentQueueService::enqueue(&tasks_dir, req).unwrap());
        let entries = dequeue_startable(&tasks_dir).unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].ticket_id, "TEST-1", "padded alias canonicalized");
    }

    #[test]
    fn enqueue_deduplicates_padded_alias_against_legacy_entries() {
        let dir = tempdir().unwrap();
        let _guard = EnvGuard::set("LOTAR_AGENT_QUEUE_DIR", dir.path().to_str().unwrap());
        let _worker_guard = EnvGuard::set("LOTAR_AGENT_QUEUE_DISABLE_WORKER", "1");
        let tasks_dir = dir.path().join("tasks");
        std::fs::create_dir_all(&tasks_dir).unwrap();

        // Legacy persisted entry carrying a padded spelling.
        AgentQueueService::enqueue(
            &tasks_dir,
            AgentJobCreateRequest {
                ticket_id: "LEG-0007".to_string(),
                prompt: "first".to_string(),
                runner: None,
                agent: Some("implement".to_string()),
                ..Default::default()
            },
        )
        .unwrap();
        // Canonical spelling of the same ticket must not duplicate it.
        let inserted = AgentQueueService::enqueue(
            &tasks_dir,
            AgentJobCreateRequest {
                ticket_id: "LEG-7".to_string(),
                prompt: "second".to_string(),
                runner: None,
                agent: Some("implement".to_string()),
                ..Default::default()
            },
        )
        .unwrap();
        assert!(!inserted, "canonical spelling deduplicated against padded legacy entry");
        assert_eq!(pending_count(&tasks_dir).unwrap(), 1);
    }

    #[test]
    fn dequeue_takes_entries_up_to_their_projects_limit() {
        let dir = tempdir().unwrap();
        let _guard = EnvGuard::set("LOTAR_AGENT_QUEUE_DIR", dir.path().to_str().unwrap());
        let _worker_guard = EnvGuard::set("LOTAR_AGENT_QUEUE_DISABLE_WORKER", "1");
        let tasks_dir = dir.path().join("tasks");
        std::fs::create_dir_all(&tasks_dir).unwrap();
        std::fs::write(tasks_dir.join("config.yml"), "agent:\n  worktree:\n    max_parallel_jobs: 2\n").unwrap();

        for idx in 1..=3 {
            let req = AgentJobCreateRequest {
                ticket_id: format!("DQL-{idx}"),
                prompt: "hello".to_string(),
                runner: None,
                agent: Some("implement".to_string()),
                ..Default::default()
            };
            let _ = AgentQueueService::enqueue(&tasks_dir, req).unwrap();
        }

        let drained = dequeue_startable(&tasks_dir).unwrap();
        assert_eq!(drained.len(), 2);
        assert_eq!(pending_count(&tasks_dir).unwrap(), 1);
    }

    #[test]
    fn the_global_limit_caps_all_projects_together_and_a_projects_limit_its_own() {
        let dir = tempdir().unwrap();
        let _guard = EnvGuard::set("LOTAR_AGENT_QUEUE_DIR", dir.path().to_str().unwrap());
        let _worker_guard = EnvGuard::set("LOTAR_AGENT_QUEUE_DISABLE_WORKER", "1");
        let tasks_dir = dir.path().join("tasks");
        std::fs::create_dir_all(tasks_dir.join("GLA")).unwrap();
        std::fs::write(tasks_dir.join("config.yml"), "agent:\n  worktree:\n    max_parallel_jobs: 3\n").unwrap();
        std::fs::write(tasks_dir.join("GLA/config.yml"), "agent:\n  worktree:\n    max_parallel_jobs: 1\n").unwrap();

        for ticket in ["GLA-1", "GLA-2", "GLB-1", "GLB-2", "GLB-3"] {
            let req = AgentJobCreateRequest {
                ticket_id: ticket.to_string(),
                prompt: "hello".to_string(),
                agent: Some("implement".to_string()),
                ..Default::default()
            };
            let _ = AgentQueueService::enqueue(&tasks_dir, req).unwrap();
        }

        let drained: Vec<_> = dequeue_startable(&tasks_dir).unwrap().into_iter().map(|entry| entry.ticket_id).collect();
        assert_eq!(drained, ["GLA-1", "GLB-1", "GLB-2"], "GLA gets one slot, the global limit 3 in all");
        assert_eq!(pending_count(&tasks_dir).unwrap(), 2);

        let running: Vec<_> = drained.iter().enumerate().map(|(idx, ticket)| (ticket.clone(), format!("job-{idx}"))).collect();
        let reason = AgentQueueService::slot_wait_reason(&tasks_dir, "GLC-1", running.clone()).unwrap();
        assert_eq!(reason.as_deref(), Some("waiting for a free slot (3 of the global agent.worktree.max_parallel_jobs 3 running)"));
        let reason = AgentQueueService::slot_wait_reason(&tasks_dir, "GLA-2", running[..1].to_vec()).unwrap();
        assert_eq!(reason.as_deref(), Some("waiting for a free slot (1 of GLA's agent.worktree.max_parallel_jobs 1 running)"));
        assert_eq!(AgentQueueService::slot_wait_reason(&tasks_dir, "GLB-3", running[..1].to_vec()).unwrap(), None);
    }

    #[test]
    fn a_taken_entry_stays_queued_until_its_job_is_registered() {
        let dir = tempdir().unwrap();
        let _guard = EnvGuard::set("LOTAR_AGENT_QUEUE_DIR", dir.path().to_str().unwrap());
        let _worker_guard = EnvGuard::set("LOTAR_AGENT_QUEUE_DISABLE_WORKER", "1");
        let tasks_dir = dir.path().join("tasks");
        std::fs::create_dir_all(&tasks_dir).unwrap();
        for ticket in ["TKN-1", "TKN-2"] {
            let req = AgentJobCreateRequest {
                ticket_id: ticket.to_string(),
                prompt: "hello".to_string(),
                agent: Some("implement".to_string()),
                ..Default::default()
            };
            assert!(AgentQueueService::enqueue(&tasks_dir, req).unwrap());
        }

        // A worker takes both and dies before it starts their jobs.
        assert_eq!(dequeue_startable(&tasks_dir).unwrap().len(), 2);
        let listed: Vec<_> = AgentQueueService::list_pending(&tasks_dir).unwrap().into_iter().map(|entry| entry.ticket_id).collect();
        assert_eq!(listed, ["TKN-1", "TKN-2"], "taken entries are still queued");

        // The next worker takes them again, in their order.
        restore_taken(&tasks_dir).unwrap();
        let entries = dequeue_startable(&tasks_dir).unwrap();
        assert_eq!(entries.iter().map(|entry| entry.ticket_id.as_str()).collect::<Vec<_>>(), ["TKN-1", "TKN-2"]);

        // A removed entry is not queued again by its retry.
        assert!(AgentQueueService::remove(&tasks_dir, "TKN-1").unwrap());
        settle_taken(&tasks_dir, &entries[0], Some(&entries[0])).unwrap();
        let listed: Vec<_> = AgentQueueService::list_pending(&tasks_dir).unwrap().into_iter().map(|entry| entry.ticket_id).collect();
        assert_eq!(listed, ["TKN-2"]);
    }

    #[test]
    fn an_entry_given_up_after_its_retries_says_why_on_its_ticket() {
        let dir = tempdir().unwrap();
        let _guard = EnvGuard::set("LOTAR_AGENT_QUEUE_DIR", dir.path().to_str().unwrap());
        let _worker_guard = EnvGuard::set("LOTAR_AGENT_QUEUE_DISABLE_WORKER", "1");
        let tasks_dir = dir.path().join("tasks");
        std::fs::create_dir_all(&tasks_dir).unwrap();
        std::fs::write(tasks_dir.join("config.yml"), "default:\n  project: GVU\n").unwrap();
        let mut storage = crate::storage::manager::Storage::new(&tasks_dir);
        crate::services::task_service::TaskService::create(
            &mut storage,
            crate::api_types::TaskCreate { title: "Given up".to_string(), project: Some("GVU".to_string()), ..Default::default() },
        )
        .unwrap();
        let req =
            AgentJobCreateRequest { ticket_id: "GVU-1".to_string(), prompt: "hello".to_string(), agent: Some("implement".to_string()), ..Default::default() };
        assert!(AgentQueueService::enqueue(&tasks_dir, req).unwrap());

        let err = LoTaRError::ValidationError("runner missing".to_string());
        for attempt in 0..=MAX_ATTEMPTS {
            let entry = dequeue_startable(&tasks_dir).unwrap().remove(0);
            assert_eq!(requeue_entry(&tasks_dir, entry, &err).unwrap(), attempt < MAX_ATTEMPTS);
        }

        assert!(AgentQueueService::list_pending(&tasks_dir).unwrap().is_empty());
        let task = crate::services::task_service::TaskService::get(&storage, "GVU-1", None).unwrap();
        let said: Vec<_> = task.comments.iter().map(|comment| comment.text.as_str()).collect();
        assert_eq!(said, ["The queued @implement job did not start after 4 tries and left the queue: Validation error: runner missing"]);
    }
}

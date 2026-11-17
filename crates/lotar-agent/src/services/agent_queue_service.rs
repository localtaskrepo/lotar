use crate::api_types::AgentJobCreateRequest;
use crate::config::manager::ConfigManager;
use crate::errors::{LoTaRError, LoTaRResult};
use crate::services::agent_job_service::{AgentJobService, JobStartMode};
use chrono::Utc;
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

#[derive(Debug, Clone, Serialize, Deserialize)]
struct AgentQueueState {
    version: u32,
    tasks_dir: String,
    updated_at: String,
    pending: Vec<AgentQueueEntry>,
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
    /// The T3 Code thread the entry waits for, once its ticket has a comment
    /// that says so (DEV-600).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub held_by: Option<String>,
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
        // Canonicalize at the queue boundary so dequeued starts and duplicate
        // checks share one identity with the job registry.
        let ticket_id = crate::storage::TaskId::parse(&ticket_id)
            .map_err(|err| LoTaRError::ValidationError(format!("Invalid ticket id '{ticket_id}': {err}")))?
            .canonical();
        let prompt = req.prompt.trim().to_string();
        if prompt.is_empty() {
            return Err(LoTaRError::ValidationError("Prompt cannot be empty".to_string()));
        }

        let entry = AgentQueueEntry {
            ticket_id: ticket_id.clone(),
            agent: agent.clone(),
            runner: runner.clone(),
            prompt,
            queued_at: Utc::now().to_rfc3339(),
            attempts: 0,
            model: req.model.clone(),
            effort: req.effort.clone(),
            provider: req.provider.clone(),
            session: req.session,
            held_by: None,
        };

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

    /// List all pending entries without modifying the queue.
    pub fn list_pending(tasks_dir: &Path) -> LoTaRResult<Vec<AgentQueueEntry>> {
        let queue_path = queue_file_path(tasks_dir);
        if !queue_path.exists() {
            return Ok(Vec::new());
        }
        let mut entries = Vec::new();
        with_locked_queue(tasks_dir, |state| {
            entries = state.pending.clone();
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
            count = state.pending.len();
            state.pending.clear();
        })?;
        Ok(count)
    }

    /// Remove a specific pending entry by ticket ID. Returns true if removed.
    pub fn remove(tasks_dir: &Path, ticket_id: &str) -> LoTaRResult<bool> {
        let mut removed = false;
        with_locked_queue(tasks_dir, |state| {
            let before = state.pending.len();
            state.pending.retain(|e| e.ticket_id != ticket_id);
            removed = state.pending.len() < before;
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
    pub fn worker_state(tasks_dir: &Path, pending: &[AgentQueueEntry]) -> (WorkerState, Option<String>) {
        if !Self::is_worker_running(tasks_dir) {
            if pending.is_empty() {
                return (WorkerState::Idle, None);
            }
            return (WorkerState::Stopped, Some("no worker process runs; start one with `lotar agent queue resume`".to_string()));
        }
        if pending.is_empty() {
            return (WorkerState::Active, None);
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

    pub fn ensure_worker(tasks_dir: &Path) -> LoTaRResult<()> {
        if worker_slot_free(tasks_dir)? {
            let _ = spawn_worker(tasks_dir);
        }
        Ok(())
    }

    pub fn run_worker(tasks_dir: &Path) -> LoTaRResult<()> {
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
                if !upgraded {
                    let entries = dequeue_startable(tasks_dir)?;
                    for entry in entries {
                        match start_entry(tasks_dir, entry.clone()) {
                            Ok(()) => {}
                            // The ticket's previous job is still running
                            // (maybe in another process): wait for it rather
                            // than spend retries (DEV-279).
                            Err(err) if is_ticket_busy(&err) => {
                                let mut entry = entry;
                                if err.to_string().contains("the T3 Code thread") {
                                    say_why_entry_waits(tasks_dir, &mut entry);
                                }
                                requeue_entry_unchanged(tasks_dir, &entry)?;
                            }
                            Err(err) => {
                                if !requeue_entry(tasks_dir, entry)? {
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
    let req = AgentJobCreateRequest {
        ticket_id: entry.ticket_id,
        prompt: entry.prompt,
        runner: entry.runner,
        agent: (!entry.agent.is_empty()).then_some(entry.agent),
        model: entry.model,
        effort: entry.effort,
        provider: entry.provider,
        session: entry.session,
    };
    if !crate::services::automation_service::queued_stage_allows_start(tasks_dir, &req) {
        return Ok(());
    }
    // A prerequisite can reopen while the job waits, and a check that failed
    // at assignment is tried again here (DEV-524).
    if req.agent.is_some() && !crate::services::automation_service::prerequisites_allow_start(tasks_dir, &req, last_try).map_err(LoTaRError::ValidationError)? {
        return Ok(());
    }
    let _ = AgentJobService::start_job_with_tasks_dir_mode(req, tasks_dir, JobStartMode::Immediate)?;
    Ok(())
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
    let who = if entry.agent.is_empty() { entry.runner.clone().unwrap_or_default() } else { format!("@{}", entry.agent) };
    let thread = hold.link.as_deref().unwrap_or(&hold.thread);
    let text = if hold.waiting {
        format!(
            "The queued {who} job waits: the T3 Code thread {thread} of the job {} waits for an answer. Answer it in T3 Code; the job starts once the thread's turn ends.",
            hold.job
        )
    } else {
        format!("The queued {who} job waits: T3 Code still runs the thread {thread} of the job {}. The job starts once the thread ends.", hold.job)
    };
    let mut storage = crate::storage::manager::Storage::new(tasks_dir);
    let commented = crate::services::task_service::TaskService::add_comment_with_context(
        &mut storage,
        &entry.ticket_id,
        &text,
        crate::services::task_service::CommentContext::automation(),
    );
    if commented.is_ok() {
        entry.held_by = Some(hold.thread);
    }
}

/// Put an entry back without counting an attempt.
fn requeue_entry_unchanged(tasks_dir: &Path, entry: &AgentQueueEntry) -> LoTaRResult<()> {
    with_locked_queue(tasks_dir, |state| {
        if !state.pending.iter().any(|item| item.ticket_id == entry.ticket_id && item.agent == entry.agent && item.runner == entry.runner) {
            state.pending.push(entry.clone());
        }
    })
}

fn requeue_entry(tasks_dir: &Path, mut entry: AgentQueueEntry) -> LoTaRResult<bool> {
    entry.attempts = entry.attempts.saturating_add(1);
    if entry.attempts > MAX_ATTEMPTS {
        return Ok(false);
    }

    with_locked_queue(tasks_dir, |state| {
        if !state.pending.iter().any(|item| item.ticket_id == entry.ticket_id && item.agent == entry.agent && item.runner == entry.runner) {
            state.pending.push(entry.clone());
        }
    })?;

    Ok(true)
}

/// Take the entries that have a free slot, in queue order; the others stay
/// queued (DEV-630).
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
        let (take, keep): (Vec<_>, Vec<_>) = std::mem::take(&mut state.pending).into_iter().partition(|entry| slots.take(entry));
        state.pending = keep;
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

fn load_state(file: &mut File, tasks_dir: &Path) -> LoTaRResult<AgentQueueState> {
    file.seek(SeekFrom::Start(0)).map_err(LoTaRError::IoError)?;
    let mut buf = String::new();
    file.read_to_string(&mut buf).map_err(LoTaRError::IoError)?;
    if buf.trim().is_empty() {
        return Ok(AgentQueueState {
            version: QUEUE_VERSION,
            tasks_dir: tasks_dir.to_string_lossy().to_string(),
            updated_at: Utc::now().to_rfc3339(),
            pending: Vec::new(),
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
    state.updated_at = Utc::now().to_rfc3339();
    let payload = serde_json::to_string_pretty(&state).map_err(|e| LoTaRError::SerializationError(e.to_string()))?;
    file.set_len(0).map_err(LoTaRError::IoError)?;
    file.seek(SeekFrom::Start(0)).map_err(LoTaRError::IoError)?;
    file.write_all(payload.as_bytes()).map_err(LoTaRError::IoError)?;
    file.flush().map_err(LoTaRError::IoError)?;
    Ok(())
}

fn spawn_worker(tasks_dir: &Path) -> LoTaRResult<()> {
    // The path lotar was started as, so an upgraded install starts the new
    // build (DEV-361).
    let exe = crate::utils::binary_watch::watched_path().ok_or_else(|| LoTaRError::ValidationError("cannot locate the lotar executable".to_string()))?;
    let mut cmd = Command::new(exe);
    cmd.arg("--tasks-dir").arg(tasks_dir).arg("agent").arg("worker");
    cmd.env("LOTAR_TASKS_DIR", tasks_dir);
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
}

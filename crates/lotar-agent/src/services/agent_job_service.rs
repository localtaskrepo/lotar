use crate::api_types::{AgentJob, AgentJobCreateRequest};
use crate::config::types::{AgentInstructionsConfig, AgentProfileDetail, AgentRunnerKind, AgentSession, ResolvedConfig};
use crate::errors::{LoTaRError, LoTaRResult};
use crate::services::agent_context_service::{AgentContextService, build_assistant_message, build_user_message};
use crate::services::agent_log_service::AgentLogService;
use crate::services::agent_runner::{block_on, build_command, harness_agent, session_name};
use crate::services::automation_service::{AutomationEvent, AutomationJobContext, AutomationService, build_lotar_env};
use crate::services::sprint_metrics::determine_done_statuses_from_config;
use crate::services::task_service::TaskService;
use crate::storage::manager::Storage;
use crate::utils::git::{clear_repository_env, git_command, verified_common_dir};
use crate::utils::time;
use crate::workspace::TasksDirectoryResolver;
use jiff::SignedDuration;
use jiff::Timestamp;
use serde_json::json;
use std::collections::{HashMap, HashSet, VecDeque};
use std::fs;
use std::io::Read;
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Condvar, LazyLock, Mutex, MutexGuard, PoisonError};
use std::thread;
use std::time::{Duration, Instant};

mod merge;
mod resumed;

const EVENT_LOG_LIMIT: usize = 200;
/// How long a runner's descendants may keep its output streams busy after it exits.
const OUTPUT_DRAIN_LIMIT: Duration = Duration::from_secs(5);
const DEFAULT_AGENT_INSTRUCTIONS: &str = include_str!(concat!(env!("LOTAR_DOCS_DIR"), "/help/agent-instructions.md"));

static JOB_COUNTER: AtomicU64 = AtomicU64::new(1);
static ORCHESTRATOR_MODE: LazyLock<Mutex<AgentOrchestratorMode>> = LazyLock::new(|| Mutex::new(AgentOrchestratorMode::Standalone));
static JOB_REGISTRY: LazyLock<Mutex<JobRegistry>> = LazyLock::new(|| {
    Mutex::new(JobRegistry {
        jobs: HashMap::new(),
        active_by_ticket: HashMap::new(),
        cleaning: HashSet::new(),
        pending_queue: VecDeque::new(),
        running_count: 0,
        max_parallel_jobs: None,
    })
});
/// Signalled when a `CleanupClaim` ends.
static CLEANUP_FINISHED: Condvar = Condvar::new();
/// Event log lines queued under `JOB_REGISTRY` in sequence order and written
/// once it is released, so file I/O never holds up other jobs (DEV-521).
static EVENT_LOG_QUEUE: Mutex<VecDeque<QueuedLogEvent>> = Mutex::new(VecDeque::new());
/// Held while queued lines are written, so they reach the files in order.
static EVENT_LOG_WRITER: Mutex<()> = Mutex::new(());

struct QueuedLogEvent {
    workspace_root: std::path::PathBuf,
    logs_dir: Option<String>,
    job_id: String,
    entry: crate::services::agent_log_service::JobLogEntry,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum AgentJobStatus {
    Queued,
    Running,
    Completed,
    Failed,
    Cancelled,
}

impl AgentJobStatus {
    fn as_str(&self) -> &'static str {
        match self {
            Self::Queued => "queued",
            Self::Running => "running",
            Self::Completed => "completed",
            Self::Failed => "failed",
            Self::Cancelled => "cancelled",
        }
    }
}

#[derive(Debug, Clone)]
struct AgentJobRecord {
    id: String,
    ticket_id: String,
    tasks_dir: std::path::PathBuf,
    /// Workspace root directory (parent of tasks_dir)
    workspace_root: std::path::PathBuf,
    /// Optional logs directory from config (agent_logs_dir)
    agent_logs_dir: Option<String>,
    runner: String,
    agent_profile: Option<String>,
    status: AgentJobStatus,
    created_at: String,
    started_at: Option<String>,
    finished_at: Option<String>,
    exit_code: Option<i32>,
    last_message: Option<String>,
    summary: Option<String>,
    session_id: Option<String>,
    worktree_path: Option<String>,
    worktree_branch: Option<String>,
    model: Option<String>,
    effort: Option<String>,
    usage: Option<crate::api_types::AgentJobUsage>,
    question: Option<crate::api_types::AgentJobQuestion>,
    is_merge_job: bool,
}

pub use crate::api_types::AgentJobEvent;

/// What a running job's agent runs as.
enum AgentJobRuntime {
    /// A `command` runner's process.
    Process(Arc<Mutex<Child>>),
    /// The harnessmux session of every other runner (DEV-503).
    Session(harnessmux::Session),
}

struct AgentJobState {
    record: AgentJobRecord,
    events: Vec<AgentJobEvent>,
    /// Per-job lifetime event counter; survives in-memory event truncation so
    /// sequences are never reused even after EVENT_LOG_LIMIT trimming.
    next_sequence: u64,
    runtime: Option<AgentJobRuntime>,
    cancellation_finalized: bool,
    /// The runner reported a failure in its output (an error result, a
    /// failed turn), so the job fails even if the process exits 0.
    runner_failed: bool,
    /// A worker thread runs the job; it clears this only after the job's
    /// end automation, context and worktree cleanup (DEV-322).
    worker_running: bool,
    /// The main checkout's commit when a merge job started, so its end
    /// knows which of main's commits the job landed (DEV-411).
    main_head_at_start: Option<String>,
    /// The ticket's assignee once the job's start rules ran, so its end
    /// sees whether the agent handed the ticket back (DEV-516) or something
    /// moved it on while it ran (DEV-566).
    assignee_at_start: Option<Option<String>>,
    /// The ticket branch's commit when a T3 Code job's thread started from
    /// it: a thread that rewrote that commit (an amend) moves the ticket's
    /// branch when nothing else moved it meanwhile (DEV-550).
    ticket_commit_at_start: Option<String>,
}

/// Data needed to start a queued job when a slot becomes available
struct PendingJob {
    job_id: String,
    runner_kind: AgentRunnerKind,
    profile: AgentProfileDetail,
    tasks_dir: std::path::PathBuf,
    ticket_id: String,
    prompt: String,
    user_prompt: String,
    config: ResolvedConfig,
    is_merge_job: bool,
}

struct JobRegistry {
    jobs: HashMap<String, AgentJobState>,
    active_by_ticket: HashMap<String, String>,
    /// Tickets whose worktrees a `CleanupClaim` holds.
    cleaning: HashSet<String>,
    pending_queue: VecDeque<PendingJob>,
    running_count: usize,
    max_parallel_jobs: Option<usize>,
}

pub struct AgentJobService;

/// Worktree cleanup runs git for tens of seconds under load, so it holds
/// a ticket's worktrees with this claim instead of `JOB_REGISTRY`
/// (DEV-647): a job for the ticket waits in `start` until it drops, other
/// jobs go on.
struct CleanupClaim(String);

impl CleanupClaim {
    /// Waits for another cleanup of the ticket to end, then claims it while
    /// `registry` is held.
    fn take(registry: MutexGuard<'static, JobRegistry>, ticket_id: &str) -> (MutexGuard<'static, JobRegistry>, Self) {
        let mut registry = wait_for_cleanup(registry, ticket_id);
        registry.cleaning.insert(ticket_id.to_string());
        (registry, Self(ticket_id.to_string()))
    }
}

impl Drop for CleanupClaim {
    fn drop(&mut self) {
        JOB_REGISTRY.lock().unwrap_or_else(PoisonError::into_inner).cleaning.remove(&self.0);
        CLEANUP_FINISHED.notify_all();
    }
}

fn wait_for_cleanup(registry: MutexGuard<'static, JobRegistry>, ticket_id: &str) -> MutexGuard<'static, JobRegistry> {
    CLEANUP_FINISHED.wait_while(registry, |registry| registry.cleaning.contains(ticket_id)).unwrap_or_else(PoisonError::into_inner)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AgentOrchestratorMode {
    Standalone,
    Server,
    Worker,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum JobStartMode {
    RespectLimits,
    Immediate,
}

impl AgentJobService {
    pub fn set_orchestrator_mode(mode: AgentOrchestratorMode) {
        if let Ok(mut guard) = ORCHESTRATOR_MODE.lock() {
            *guard = mode;
        }
    }

    pub fn orchestrator_mode() -> AgentOrchestratorMode {
        ORCHESTRATOR_MODE.lock().map(|guard| *guard).unwrap_or(AgentOrchestratorMode::Standalone)
    }

    pub fn start_job(req: AgentJobCreateRequest, resolver: &TasksDirectoryResolver) -> LoTaRResult<AgentJob> {
        Self::start_job_with_tasks_dir(req, &resolver.path)
    }

    pub fn start_job_with_tasks_dir(req: AgentJobCreateRequest, tasks_dir: &std::path::Path) -> LoTaRResult<AgentJob> {
        Self::start_job_with_tasks_dir_mode(req, tasks_dir, JobStartMode::RespectLimits)
    }

    /// Check what [`Self::start_job`] would reject (ticket id, profile or
    /// runner, existing task) without starting anything. Used before a job
    /// is handed to the detached queue worker, which cannot report errors
    /// back to the command that queued it (DEV-257).
    pub fn preflight(req: &AgentJobCreateRequest, tasks_dir: &std::path::Path) -> LoTaRResult<()> {
        let ticket_id = req.ticket_id.trim().to_string();
        if ticket_id.is_empty() {
            return Err(LoTaRError::ValidationError("Ticket id is required".to_string()));
        }
        if req.prompt.trim().is_empty() {
            return Err(LoTaRError::ValidationError("Prompt cannot be empty".to_string()));
        }
        let ticket_id = match crate::storage::moves::follow(tasks_dir, &ticket_id) {
            Ok(Some(current)) => current,
            _ => ticket_id,
        };
        let parsed_ticket =
            crate::storage::TaskId::parse(&ticket_id).map_err(|err| LoTaRError::ValidationError(format!("Invalid ticket id '{ticket_id}': {err}")))?;
        let config =
            crate::config::resolution::config_for_project(tasks_dir, Some(&parsed_ticket.project)).map_err(|e| LoTaRError::ValidationError(e.to_string()))?;
        let profile = resolve_profile(&config, req)?;
        ensure_profile_trusted(tasks_dir, req)?;
        profile.runner.parse::<AgentRunnerKind>().map_err(|_| {
            LoTaRError::ValidationError(format!("Unsupported runner '{}'. Expected {}", profile.runner, crate::config::operations::AGENT_RUNNERS.join(", ")))
        })?;
        let storage = Storage::new(tasks_dir);
        TaskService::get(&storage, &parsed_ticket.canonical(), None)?;
        Ok(())
    }

    pub fn start_job_with_tasks_dir_mode(req: AgentJobCreateRequest, tasks_dir: &std::path::Path, mode: JobStartMode) -> LoTaRResult<AgentJob> {
        let ticket_id = req.ticket_id.trim().to_string();
        if ticket_id.is_empty() {
            return Err(LoTaRError::ValidationError("Ticket id is required".to_string()));
        }
        // A job started on an old ID of a moved task works on the task it
        // became, under its current ID (DEV-183).
        let ticket_id = match crate::storage::moves::follow(tasks_dir, &ticket_id) {
            Ok(Some(current)) => current,
            _ => ticket_id,
        };

        let parsed_ticket =
            crate::storage::TaskId::parse(&ticket_id).map_err(|err| LoTaRError::ValidationError(format!("Invalid ticket id '{ticket_id}': {err}")))?;
        let project_prefix = parsed_ticket.project.clone();
        // Canonicalize ONCE at this creation boundary: the registry key, the
        // persisted record, the pending queue entry, the spawned runner env,
        // and every active-job lookup then share one identity, so a job
        // started through a padded alias (TP-001) locks the canonical ticket
        // (TP-1) and duplicate alias jobs cannot be registered.
        let ticket_id = parsed_ticket.canonical();
        let config = crate::config::resolution::config_for_project(tasks_dir, Some(&project_prefix)).map_err(|e| LoTaRError::ValidationError(e.to_string()))?;

        let profile = resolve_profile(&config, &req)?;
        ensure_profile_trusted(tasks_dir, &req)?;
        let runner_kind = profile.runner.parse::<AgentRunnerKind>().map_err(|_| {
            LoTaRError::ValidationError(format!("Unsupported runner '{}'. Expected {}", profile.runner, crate::config::operations::AGENT_RUNNERS.join(", ")))
        })?;

        let storage = Storage::new(tasks_dir);
        let task = TaskService::get(&storage, &ticket_id, None)?;
        // Refuse root ambiguity and cross-root tickets BEFORE any spawn, log,
        // or registry side effect: a job runs with this tasks_dir and must
        // not mutate (or write logs about) a task stored in another root.
        match crate::storage::identity::resolve(tasks_dir, &ticket_id) {
            Ok(location) if !location.is_in_root(tasks_dir) => {
                return Err(LoTaRError::ValidationError(format!(
                    "Ticket '{}' is stored in workspace tasks root {} and cannot be managed from {}; run the command inside that workspace",
                    ticket_id,
                    location.root.display(),
                    tasks_dir.display()
                )));
            }
            Ok(_) => {}
            Err(err) => {
                return Err(LoTaRError::ValidationError(format!("Ticket '{ticket_id}' could not be uniquely resolved in this workspace: {err}")));
            }
        }
        let is_merge_job = is_merge_job_candidate(&task, req.agent.as_deref());
        if let Some(message) = review_blocks_merge(&task, req.agent.as_deref()) {
            return Err(LoTaRError::ValidationError(message));
        }

        if is_merge_job && !config.agent_worktree.enabled {
            return Err(LoTaRError::ValidationError("Merge agent jobs require agent.worktree.enabled=true so merges run on isolated branches.".to_string()));
        }

        let user_prompt = req.prompt.trim().to_string();
        if user_prompt.is_empty() {
            return Err(LoTaRError::ValidationError("Prompt cannot be empty".to_string()));
        }
        let job_id = make_job_id();
        let context = AgentContextService::load(tasks_dir, &config, &ticket_id)?;
        let template = instruction_template(tasks_dir, &config, &task, &job_id, runner_kind, req.agent.as_deref());
        let instructions = resolve_agent_instructions(tasks_dir, &config, &profile)?.map(|text| template.expand(&text));
        // A T3 Code thread runs in its project's runtime mode, whatever the
        // profile's permissions say.
        let read_only = runner_kind != AgentRunnerKind::T3 && profile.permissions == Some(crate::config::types::AgentPermissions::ReadOnly);
        let lotar_mcp = profile.mcp.as_ref().is_some_and(|mcp| mcp.lotar) && crate::services::agent_runner::takes_mcp_servers(runner_kind);
        let t3_thread = runner_kind == AgentRunnerKind::T3;
        let mut notes = job_notes(tasks_dir, &config, &ticket_id, is_merge_job, read_only, lotar_mcp, t3_thread);
        if t3_thread {
            notes.push_str(&t3_thread_notes(tasks_dir, &config, &ticket_id, is_merge_job));
        }
        let earlier = harness_agent(runner_kind).and_then(|_| earlier_sessions_text(tasks_dir, &ticket_id, req.agent.as_deref()));
        let history = PromptHistory { full: profile.full_history.unwrap_or(false), entries: profile.context_entries.unwrap_or(DEFAULT_CONTEXT_ENTRIES) };
        let prompt = build_prompt(&task, &user_prompt, context.as_ref(), history, earlier.as_deref(), instructions.as_deref(), Some(&notes));

        let created_at = time::now_rfc3339();
        let tasks_dir_buf = tasks_dir.to_path_buf();
        let (job_model, job_effort) = (profile.model.clone(), profile.effort.clone());

        let has_active_job = |registry: &JobRegistry| {
            registry.active_by_ticket.contains_key(&ticket_id) || crate::services::agent_active_jobs::for_ticket_elsewhere(tasks_dir, &ticket_id).is_some()
        };
        let active_job_error = || LoTaRError::ValidationError(format!("Ticket '{}' already has an active job", ticket_id));
        // Before T3 Code is asked: a queue entry waiting for the ticket's
        // running job retries every few hundred milliseconds (DEV-599).
        if has_active_job(&*JOB_REGISTRY.lock().map_err(|_| LoTaRError::ValidationError("Job registry unavailable".to_string()))?) {
            return Err(active_job_error());
        }
        if let Some(hold) = running_t3_thread(tasks_dir, &ticket_id) {
            let T3ThreadHold { thread, job, waiting, .. } = hold;
            let what = if waiting { "waits for an answer" } else { "still runs" };
            return Err(LoTaRError::ValidationError(format!(
                "Ticket '{ticket_id}' already has an active job: the T3 Code thread {thread} of the job {job} {what}"
            )));
        }

        let should_queue: bool;
        {
            let registry = JOB_REGISTRY.lock().map_err(|_| LoTaRError::ValidationError("Job registry unavailable".to_string()))?;
            let mut registry = wait_for_cleanup(registry, &ticket_id);

            if has_active_job(&registry) {
                return Err(active_job_error());
            }

            // The limits are the queue worker's, counting every process's jobs (DEV-646).
            let slot_wait = match mode {
                JobStartMode::RespectLimits => slot_wait_reason(&registry, tasks_dir, &ticket_id)?,
                JobStartMode::Immediate => None,
            };
            let merge_slot_busy = is_merge_job && has_active_merge_job(&registry);

            should_queue = matches!(mode, JobStartMode::RespectLimits) && (slot_wait.is_some() || merge_slot_busy);
            registry.max_parallel_jobs =
                crate::config::resolution::config_for_project(tasks_dir, None).ok().and_then(|global| global.agent_worktree.max_parallel_jobs);

            // Compute workspace root (parent of tasks_dir)
            let workspace_root = tasks_dir_buf.parent().map(|p| p.to_path_buf()).unwrap_or_else(|| tasks_dir_buf.clone());

            registry.active_by_ticket.insert(ticket_id.clone(), job_id.clone());
            prune_finished_jobs(&mut registry);
            registry.jobs.insert(
                job_id.clone(),
                AgentJobState {
                    record: AgentJobRecord {
                        id: job_id.clone(),
                        ticket_id: ticket_id.clone(),
                        tasks_dir: tasks_dir_buf.clone(),
                        workspace_root,
                        agent_logs_dir: crate::services::agent_log_service::effective_logs_dir(tasks_dir, config.agent_logs_dir.as_deref()),
                        runner: runner_kind.as_str().to_string(),
                        agent_profile: req.agent.clone(),
                        status: AgentJobStatus::Queued,
                        created_at: created_at.clone(),
                        started_at: None,
                        finished_at: None,
                        exit_code: None,
                        last_message: None,
                        summary: None,
                        session_id: None,
                        worktree_path: None,
                        worktree_branch: None,
                        model: job_model.clone(),
                        effort: job_effort.clone(),
                        usage: None,
                        question: None,
                        is_merge_job,
                    },
                    events: Vec::new(),
                    next_sequence: 0,
                    runtime: None,
                    cancellation_finalized: false,
                    runner_failed: false,
                    worker_running: false,
                    main_head_at_start: None,
                    assignee_at_start: None,
                    ticket_commit_at_start: None,
                },
            );

            if should_queue {
                // Add to pending queue - will be started when a slot opens
                registry.pending_queue.push_back(PendingJob {
                    job_id: job_id.clone(),
                    runner_kind,
                    profile: profile.clone(),
                    tasks_dir: tasks_dir_buf.clone(),
                    ticket_id: ticket_id.clone(),
                    prompt: prompt.clone(),
                    user_prompt: user_prompt.clone(),
                    config: config.clone(),
                    is_merge_job,
                });
            } else {
                // Slot available - increment running count
                registry.running_count += 1;
            }
        }

        if should_queue {
            watch_pending_queue();
        } else {
            // Start job immediately
            let job_id_clone = job_id.clone();
            let user_prompt_clone = user_prompt.clone();
            thread::spawn(move || {
                run_job(job_id_clone, runner_kind, profile, tasks_dir_buf, ticket_id, prompt, user_prompt_clone, config);
            });
        }

        Ok(get_job_dto(&job_id).unwrap_or_else(|| AgentJob {
            id: job_id,
            ticket_id: req.ticket_id,
            runner: runner_kind.as_str().to_string(),
            agent: req.agent,
            status: AgentJobStatus::Queued.as_str().to_string(),
            created_at: created_at.clone(),
            started_at: None,
            finished_at: None,
            exit_code: None,
            last_message: None,
            summary: None,
            session_id: None,
            worktree_path: None,
            worktree_branch: None,
            model: job_model,
            effort: job_effort,
            usage: None,
            question: None,
        }))
    }

    /// Finish the jobs whose process exited before they ended, as a worker
    /// or server starts (DEV-378). Each one runs here from now on; see
    /// [`adopt_orphaned_job`]. Outcomes their ends left pending apply too (DEV-467).
    pub fn recover_orphaned_jobs(tasks_dir: &std::path::Path) {
        for orphan in crate::services::agent_active_jobs::orphaned(tasks_dir) {
            adopt_orphaned_job(tasks_dir, &orphan);
        }
        AutomationService::apply_pending_outcomes(tasks_dir);
    }

    /// Take over the threads of ended `t3` jobs that T3 Code runs again,
    /// with no job, after a restart (DEV-626); see [`resumed::look`].
    pub fn adopt_resumed_t3_threads(tasks_dir: &std::path::Path) {
        resumed::look(tasks_dir);
    }

    pub fn list_jobs() -> Vec<AgentJob> {
        let registry = JOB_REGISTRY.lock();
        if let Ok(registry) = registry {
            return registry.jobs.values().map(|state| state.record.to_dto()).collect();
        }
        Vec::new()
    }

    /// [`Self::list_jobs`], plus the jobs only the job logs hold (DEV-518): jobs of an earlier server run or of
    /// another process. A job this process holds wins over its log.
    pub fn list_jobs_in(tasks_dir: &std::path::Path) -> Vec<AgentJob> {
        let mut jobs = Self::list_jobs();
        let mut seen: std::collections::HashSet<String> = jobs.iter().map(|job| job.id.clone()).collect();
        let running: std::collections::HashSet<String> = crate::services::agent_active_jobs::list(tasks_dir).into_iter().map(|job| job.job_id).collect();
        for saved in saved_jobs(tasks_dir) {
            if seen.insert(saved.header.job_id.clone()) {
                jobs.push(saved_job_dto(&saved, running.contains(&saved.header.job_id)));
            }
        }
        jobs
    }

    /// The job and its whole thread (DEV-518): the job from this process while it holds it, else from its log;
    /// the events from the log joined with the ones this process holds.
    pub fn job_with_events_in(tasks_dir: &std::path::Path, id: &str) -> Option<(AgentJob, Vec<AgentJobEvent>)> {
        let saved = saved_job(tasks_dir, id);
        if let Some(job) = get_job_dto(id) {
            return Some((job, merged_thread(saved.as_ref(), Self::events_for(id))));
        }
        let saved = saved?;
        let running = crate::services::agent_active_jobs::find(tasks_dir, id).is_some();
        Some((saved_job_dto(&saved, running), saved_events(&saved)))
    }

    /// The job's whole thread for readers (DEV-518).
    pub fn full_thread_in(tasks_dir: &std::path::Path, id: &str) -> Vec<AgentJobEvent> {
        merged_thread(saved_job(tasks_dir, id).as_ref(), Self::events_for(id))
    }

    /// Get queue statistics
    pub fn queue_stats() -> crate::api_types::AgentQueueStats {
        if let Ok(registry) = JOB_REGISTRY.lock() {
            crate::api_types::AgentQueueStats {
                running: registry.running_count,
                queued: registry.pending_queue.len(),
                max_parallel: registry.max_parallel_jobs,
            }
        } else {
            crate::api_types::AgentQueueStats { running: 0, queued: 0, max_parallel: None }
        }
    }

    /// Why the job waits in this process's queue instead of running, or
    /// `None` once it runs (DEV-525). A waiting job has no log yet and
    /// `lotar agent queue` doesn't list it, so its ticket has to say so.
    pub fn waiting_reason(job_id: &str) -> Option<String> {
        let registry = JOB_REGISTRY.lock().ok()?;
        let pending = registry.pending_queue.iter().find(|pending| pending.job_id == job_id)?;
        let merge_busy = registry
            .jobs
            .iter()
            .any(|(id, state)| id != job_id && state.record.is_merge_job && matches!(state.record.status, AgentJobStatus::Queued | AgentJobStatus::Running));
        Some(match slot_wait_reason(&registry, &pending.tasks_dir, &pending.ticket_id) {
            Ok(Some(reason)) => format!("it is {reason}"),
            _ if pending.is_merge_job && merge_busy => "it waits for the other merge job to end".to_string(),
            _ => "it waits in the job queue".to_string(),
        })
    }

    /// The tickets and jobs this process runs (or queues).
    pub fn active_jobs() -> Vec<(String, String)> {
        JOB_REGISTRY.lock().map(|registry| registry.active_by_ticket.iter().map(|(ticket, job)| (ticket.clone(), job.clone())).collect()).unwrap_or_default()
    }

    /// The job this process runs (or queues) for the ticket.
    pub fn active_job_id(ticket_id: &str) -> Option<String> {
        JOB_REGISTRY.lock().ok().and_then(|registry| registry.active_by_ticket.get(ticket_id).cloned())
    }

    /// Whether no worker of this process still works on the job: its end
    /// automation has run (DEV-322). A job that never got a worker (queued,
    /// then cancelled) is settled too.
    pub fn job_settled(job_id: &str) -> bool {
        JOB_REGISTRY.lock().map_or(true, |registry| registry.jobs.get(job_id).is_none_or(|state| !state.worker_running))
    }

    pub fn has_active_job(ticket_id: &str) -> bool {
        if let Ok(registry) = JOB_REGISTRY.lock() {
            return registry.active_by_ticket.contains_key(ticket_id);
        }
        false
    }

    /// [`Self::has_active_job`], plus jobs other processes run for this
    /// tasks directory (DEV-279).
    pub fn has_active_job_in(tasks_dir: &std::path::Path, ticket_id: &str) -> bool {
        Self::has_active_job(ticket_id) || crate::services::agent_active_jobs::for_ticket_elsewhere(tasks_dir, ticket_id).is_some()
    }

    pub fn get_job(id: &str) -> Option<AgentJob> {
        get_job_dto(id)
    }

    pub fn cancel_job(id: &str) -> LoTaRResult<Option<AgentJob>> {
        let mut registry = JOB_REGISTRY.lock().map_err(|_| LoTaRError::ValidationError("Job registry unavailable".to_string()))?;
        let (job, cancelled, was_pending, process) = {
            let was_pending = registry.pending_queue.iter().any(|p| p.job_id == id);
            let Some(state) = registry.jobs.get_mut(id) else {
                return Ok(None);
            };
            if !matches!(state.record.status, AgentJobStatus::Queued | AgentJobStatus::Running) {
                return Ok(Some(state.record.to_dto()));
            }
            // A session's turn is cancelled by its job's thread, which watches the status.
            let process = match state.runtime.as_ref() {
                Some(AgentJobRuntime::Process(child)) => Some(Arc::clone(child)),
                _ => None,
            };
            let mut cancelled = false;
            if state.record.status != AgentJobStatus::Cancelled {
                state.record.status = AgentJobStatus::Cancelled;
                state.record.finished_at = Some(time::now_rfc3339());
                record_and_emit(state, "agent_job_cancelled", None);
                cancelled = true;
            }
            let job = state.record.to_dto_with_cancelled(cancelled);
            (job, cancelled, was_pending, process)
        };
        if cancelled {
            // Remove from pending queue if it was queued
            registry.pending_queue.retain(|p| p.job_id != id);
            // A dispatched job owns its slot until its thread (and child) has exited.
        }
        drop(registry);
        // Stopped outside the lock: listing the processes is slow. A runner
        // its thread already reaped is left alone, its pid may be reused.
        if let Some(child) = process
            && let Ok(mut child) = child.lock()
            && matches!(child.try_wait(), Ok(None))
        {
            terminate_child(&mut child);
        }
        flush_event_log();

        if cancelled && was_pending {
            finalize_cancelled_job(id);
            process_pending_queue();
        }
        Ok(Some(job))
    }

    pub fn cancel_all_jobs() -> LoTaRResult<Vec<AgentJob>> {
        let ids = {
            let registry = JOB_REGISTRY.lock().map_err(|_| LoTaRError::ValidationError("Job registry unavailable".to_string()))?;
            registry
                .jobs
                .iter()
                .filter_map(|(id, state)| matches!(state.record.status, AgentJobStatus::Queued | AgentJobStatus::Running).then_some(id.clone()))
                .collect::<Vec<_>>()
        };

        let mut cancelled = Vec::new();
        for id in ids {
            if let Some(job) = Self::cancel_job(&id)? {
                cancelled.push(job);
            }
        }

        Ok(cancelled)
    }
    /// Cancel every job of this process and wait (up to `timeout`) until
    /// they are finalized: cancelled status, job_cancelled automation,
    /// worktree handling. For processes that are about to exit.
    pub fn shutdown_jobs(timeout: Duration) {
        let had_jobs = Self::cancel_all_jobs().is_ok_and(|jobs| !jobs.is_empty());
        if !had_jobs {
            return;
        }
        let deadline = Instant::now() + timeout;
        while Instant::now() < deadline {
            let stats = Self::queue_stats();
            if stats.running == 0 && stats.queued == 0 {
                break;
            }
            thread::sleep(Duration::from_millis(100));
        }
    }

    /// The one upgrade reaction for long-running commands (`lotar mcp`, `lotar
    /// serve --exit-on-upgrade`): once the binary is replaced, stop running
    /// agent jobs and exit with status 0, so whatever started this process runs
    /// the new version. The note goes to stderr; stdout may carry a protocol.
    /// Returns false when there is nothing to watch.
    pub fn exit_when_binary_replaced() -> bool {
        crate::utils::binary_watch::spawn_watcher(|| {
            eprintln!("The lotar binary changed; exiting so the new version can start");
            Self::shutdown_jobs(Duration::from_secs(10));
            std::process::exit(0);
        })
    }

    pub fn events_for(id: &str) -> Vec<AgentJobEvent> {
        if let Ok(registry) = JOB_REGISTRY.lock()
            && let Some(state) = registry.jobs.get(id)
        {
            return state.events.clone();
        }
        Vec::new()
    }

    /// Queue a message as the next turn of the job's session; the job ends after it.
    pub fn send_message(id: &str, message: &str) -> LoTaRResult<AgentJob> {
        let trimmed = message.trim();
        if trimmed.is_empty() {
            return Err(LoTaRError::ValidationError("Message cannot be empty".to_string()));
        }
        let session = Self::running_session(id)?;
        block_on(session.send(trimmed.to_string())).map_err(|err| LoTaRError::ValidationError(format!("Could not send the message: {err}")))?;
        get_job_dto(id).ok_or_else(|| LoTaRError::ValidationError("Job not found".to_string()))
    }

    /// Answer the job's open question (a permission request) with one of its options.
    pub fn answer_question(id: &str, request: &str, option: &str) -> LoTaRResult<AgentJob> {
        let session = Self::running_session(id)?;
        let question = get_job_dto(id).and_then(|job| job.question);
        let Some(question) = question.filter(|question| question.request == request) else {
            return Err(LoTaRError::ValidationError("The job has no open question with this request id".to_string()));
        };
        if !question.options.iter().any(|choice| choice.id == option) {
            let ids = question.options.iter().map(|choice| choice.id.as_str()).collect::<Vec<_>>().join(", ");
            return Err(LoTaRError::ValidationError(format!("Unknown option '{option}'; use one of: {ids}")));
        }
        block_on(session.answer(request.to_string(), option.to_string())).map_err(|err| LoTaRError::ValidationError(format!("Could not answer: {err}")))?;
        get_job_dto(id).ok_or_else(|| LoTaRError::ValidationError("Job not found".to_string()))
    }

    fn running_session(id: &str) -> LoTaRResult<harnessmux::Session> {
        let registry = JOB_REGISTRY.lock().map_err(|_| LoTaRError::ValidationError("Job registry unavailable".to_string()))?;
        let state = registry.jobs.get(id).ok_or_else(|| LoTaRError::ValidationError("Job not found".to_string()))?;
        if state.record.status != AgentJobStatus::Running {
            return Err(LoTaRError::ValidationError("Job is not running".to_string()));
        }
        match state.runtime.as_ref() {
            Some(AgentJobRuntime::Session(session)) => Ok(session.clone()),
            Some(AgentJobRuntime::Process(_)) => Err(LoTaRError::ValidationError("The command runner does not take messages".to_string())),
            None => Err(LoTaRError::ValidationError("Job is not accepting messages".to_string())),
        }
    }

    #[cfg(test)]
    pub fn reset_for_tests() {
        if let Ok(mut registry) = JOB_REGISTRY.lock() {
            registry.jobs.clear();
            registry.active_by_ticket.clear();
            registry.pending_queue.clear();
            registry.running_count = 0;
            registry.max_parallel_jobs = None;
        }
    }
}

impl AgentJobRecord {
    fn to_dto(&self) -> AgentJob {
        AgentJob {
            id: self.id.clone(),
            ticket_id: self.ticket_id.clone(),
            runner: self.runner.clone(),
            agent: self.agent_profile.clone(),
            status: self.status.as_str().to_string(),
            created_at: self.created_at.clone(),
            started_at: self.started_at.clone(),
            finished_at: self.finished_at.clone(),
            exit_code: self.exit_code,
            last_message: self.last_message.clone(),
            summary: self.summary.clone(),
            session_id: self.session_id.clone(),
            worktree_path: self.worktree_path.clone(),
            worktree_branch: self.worktree_branch.clone(),
            model: self.model.clone(),
            effort: self.effort.clone(),
            usage: self.usage.clone(),
            question: self.question.clone(),
        }
    }

    fn to_dto_with_cancelled(&self, cancelled: bool) -> AgentJob {
        let mut dto = self.to_dto();
        if cancelled && dto.status != AgentJobStatus::Cancelled.as_str() {
            dto.status = AgentJobStatus::Cancelled.as_str().to_string();
        }
        dto
    }
}

fn get_job_dto(id: &str) -> Option<AgentJob> {
    let registry = JOB_REGISTRY.lock().ok()?;
    registry.jobs.get(id).map(|state| state.record.to_dto())
}

/// The log folders the tasks directory's projects write to (`agent_logs_dir` can differ per project).
fn job_log_dirs(tasks_dir: &std::path::Path) -> Vec<String> {
    let mut projects = vec![None];
    projects.extend(crate::utils::filesystem::list_visible_subdirs(tasks_dir).into_iter().map(|(prefix, _)| Some(prefix)));
    let mut dirs: Vec<String> = Vec::new();
    for project in projects {
        let configured = crate::config::resolution::config_for_project(tasks_dir, project.as_deref()).ok().and_then(|config| config.agent_logs_dir);
        if let Some(dir) = crate::services::agent_log_service::effective_logs_dir(tasks_dir, configured.as_deref())
            && !dirs.contains(&dir)
        {
            dirs.push(dir);
        }
    }
    dirs
}

fn workspace_root_of(tasks_dir: &std::path::Path) -> std::path::PathBuf {
    tasks_dir.parent().map(|p| p.to_path_buf()).unwrap_or_else(|| tasks_dir.to_path_buf())
}

fn saved_jobs(tasks_dir: &std::path::Path) -> Vec<crate::services::agent_log_service::SavedJob> {
    let root = workspace_root_of(tasks_dir);
    job_log_dirs(tasks_dir).iter().flat_map(|dir| AgentLogService::load_jobs(&root, dir)).collect()
}

fn saved_job(tasks_dir: &std::path::Path, id: &str) -> Option<crate::services::agent_log_service::SavedJob> {
    let root = workspace_root_of(tasks_dir);
    job_log_dirs(tasks_dir).iter().find_map(|dir| AgentLogService::load_job(&root, dir, id).ok().flatten())
}

/// A log's events with the sequence the live stream gave them; events of logs from before DEV-518 count by line.
fn saved_events(saved: &crate::services::agent_log_service::SavedJob) -> Vec<AgentJobEvent> {
    saved
        .events
        .iter()
        .zip(1u64..)
        .map(|(entry, line)| AgentJobEvent {
            kind: entry.kind.clone(),
            sequence: entry.sequence.unwrap_or(line),
            at: entry.at.clone(),
            message: entry.message.clone(),
        })
        .collect()
}

/// The job's log and the events this process holds, one per sequence (DEV-518): the log has every event since
/// it was opened, memory only the last [`EVENT_LOG_LIMIT`], including any from before the log was opened.
fn merged_thread(saved: Option<&crate::services::agent_log_service::SavedJob>, held: Vec<AgentJobEvent>) -> Vec<AgentJobEvent> {
    let mut by_sequence: std::collections::BTreeMap<u64, AgentJobEvent> =
        saved.map(saved_events).unwrap_or_default().into_iter().map(|event| (event.sequence, event)).collect();
    by_sequence.extend(held.into_iter().map(|event| (event.sequence, event)));
    by_sequence.into_values().collect()
}

/// A logged job. One without an end in its log is running when a live process holds it, else it ended with
/// the process that ran it.
fn saved_job_dto(saved: &crate::services::agent_log_service::SavedJob, running: bool) -> AgentJob {
    let header = &saved.header;
    let status = saved.status.as_ref();
    let fallback = if running { AgentJobStatus::Running } else { AgentJobStatus::Failed };
    AgentJob {
        id: header.job_id.clone(),
        ticket_id: header.ticket_id.clone(),
        runner: header.runner.clone(),
        agent: header.agent_profile.clone(),
        status: status.map_or_else(|| fallback.as_str().to_string(), |status| status.status.clone()),
        created_at: header.created_at.clone(),
        started_at: saved.events.iter().find(|entry| entry.kind == "agent_job_started").map(|entry| entry.at.clone()),
        finished_at: status.map(|status| status.at.clone()),
        exit_code: status.and_then(|status| status.exit_code),
        last_message: saved.events.iter().rev().find_map(|entry| entry.message.clone()),
        summary: status.and_then(|status| status.summary.clone()),
        session_id: status.and_then(|status| status.session_id.clone()),
        worktree_path: status.and_then(|status| status.worktree_path.clone()).or_else(|| header.worktree_path.clone()),
        worktree_branch: status.and_then(|status| status.worktree_branch.clone()).or_else(|| header.worktree_branch.clone()),
        model: header.model.clone(),
        effort: header.effort.clone(),
        usage: status.and_then(|status| status.usage.clone()),
        question: None,
    }
}

fn job_context_for(id: &str) -> Option<AutomationJobContext> {
    let registry = JOB_REGISTRY.lock().ok()?;
    let state = registry.jobs.get(id)?;
    Some(AutomationJobContext {
        job_id: state.record.id.clone(),
        runner: state.record.runner.clone(),
        agent: state.record.agent_profile.clone(),
        worktree_path: state.record.worktree_path.clone(),
        worktree_branch: state.record.worktree_branch.clone(),
        usage: state.record.usage.clone(),
        assignee_at_start: recorded_assignee(state.assignee_at_start.clone()),
    })
}

fn make_job_id() -> String {
    let stamp = Timestamp::now().strftime("%Y%m%dT%H%M%S%.3fZ").to_string();
    let counter = JOB_COUNTER.fetch_add(1, Ordering::Relaxed);
    format!("job-{stamp}-{counter}")
}

fn wrap_runner_command(
    command: crate::services::agent_runner::RunnerCommand,
    job_id: &str,
    ticket_id: &str,
    runner: &str,
    tasks_dir: &std::path::Path,
) -> crate::services::agent_runner::RunnerCommand {
    let Some(wrapper_program) = resolve_wrapper_program() else {
        return command;
    };

    let mut args = vec![
        "--job-id".to_string(),
        job_id.to_string(),
        "--ticket-id".to_string(),
        ticket_id.to_string(),
        "--runner".to_string(),
        runner.to_string(),
        "--tasks-dir".to_string(),
        tasks_dir.canonicalize().unwrap_or_else(|_| tasks_dir.to_path_buf()).to_string_lossy().to_string(),
        "--".to_string(),
        command.program.clone(),
    ];
    args.extend(command.args.iter().cloned());

    crate::services::agent_runner::RunnerCommand { program: wrapper_program, args, env: command.env }
}

fn is_executable_file(path: &std::path::Path) -> bool {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        path.metadata().map(|m| m.is_file() && (m.permissions().mode() & 0o111 != 0)).unwrap_or(false)
    }
    #[cfg(not(unix))]
    {
        path.is_file()
    }
}

fn resolve_wrapper_program() -> Option<String> {
    if let Ok(raw) = std::env::var("LOTAR_AGENT_WRAPPER") {
        let trimmed = raw.trim();
        if !trimmed.is_empty() {
            return Some(trimmed.to_string());
        }
    }

    if let Ok(exe) = std::env::current_exe()
        && let Some(dir) = exe.parent()
    {
        let candidate = dir.join(wrapper_executable_name());
        if is_executable_file(&candidate) {
            return Some(candidate.to_string_lossy().to_string());
        }
    }

    let path = std::env::var_os("PATH")?;
    for entry in std::env::split_paths(&path) {
        let candidate = entry.join(wrapper_executable_name());
        if is_executable_file(&candidate) {
            return Some(candidate.to_string_lossy().to_string());
        }
    }

    None
}

fn wrapper_executable_name() -> &'static str {
    if cfg!(windows) { "lotar-agent-wrapper.exe" } else { "lotar-agent-wrapper" }
}

/// Stop a runner and everything it started. Some CLIs (Codex, Copilot) run
/// shell commands in their own process group, which signalling the runner's
/// group misses, so every descendant and its group are signalled too.
fn terminate_child(child: &mut Child) {
    #[cfg(unix)]
    {
        let root = child.id();
        let descendants = descendant_pids(root);
        // SAFETY: plain signal delivery to processes LoTaR started.
        unsafe {
            let _ = libc::kill(-(root as i32), libc::SIGTERM);
            for pid in &descendants {
                let pid = *pid as i32;
                if libc::getpgid(pid) == pid {
                    let _ = libc::kill(-pid, libc::SIGTERM);
                }
                let _ = libc::kill(pid, libc::SIGTERM);
            }
        }
    }
    // Windows has no process groups here: end the wrapper's whole process
    // tree, or the agent CLI it started keeps running (DEV-279).
    #[cfg(windows)]
    {
        let _ = std::process::Command::new("taskkill")
            .args(["/PID", &child.id().to_string(), "/T", "/F"])
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status();
    }
    let _ = child.kill();
}

/// All processes below `root`, found through their parent links.
#[cfg(unix)]
fn descendant_pids(root: u32) -> Vec<u32> {
    // Without a process list only the root itself is stopped.
    let Ok(mut system) = sysinfo::System::new() else {
        return Vec::new();
    };
    system.refresh_processes(sysinfo::ProcessesToUpdate::All, true);
    let mut children: HashMap<u32, Vec<u32>> = HashMap::new();
    for (pid, process) in system.processes() {
        if let Some(parent) = process.parent() {
            children.entry(parent.as_u32()).or_default().push(pid.as_u32());
        }
    }
    let mut found = Vec::new();
    let mut queue = vec![root];
    while let Some(pid) = queue.pop() {
        for child in children.get(&pid).into_iter().flatten() {
            if !found.contains(child) {
                found.push(*child);
                queue.push(*child);
            }
        }
    }
    found
}

/// Agent profiles defined in repository config run programs the
/// repository chose; they need a trusted workspace (DEV-259). Bare
/// `--runner` jobs and home-config profiles are the user's own choice.
fn ensure_profile_trusted(tasks_dir: &std::path::Path, req: &AgentJobCreateRequest) -> LoTaRResult<()> {
    use crate::config::trust::TrustService;
    let Some(name) = req.agent.as_deref() else {
        return Ok(());
    };
    if TrustService::profile_from_repository(tasks_dir, name) && !TrustService::allows_commands(tasks_dir) {
        return Err(LoTaRError::ValidationError(TrustService::untrusted_message(tasks_dir, &format!("agent profile '{name}'"))));
    }
    Ok(())
}

fn resolve_profile(config: &ResolvedConfig, req: &AgentJobCreateRequest) -> LoTaRResult<AgentProfileDetail> {
    let mut profile = if let Some(name) = req.agent.as_deref() {
        config.agent_profiles.get(name).ok_or_else(|| LoTaRError::ValidationError(format!("Unknown agent profile '{}'.", name)))?.clone()
    } else if let Some(runner) = req.runner.as_deref() {
        AgentProfileDetail { runner: runner.to_string(), ..Default::default() }
    } else {
        return Err(LoTaRError::ValidationError("Either agent or runner must be provided".to_string()));
    };

    // A run's own model and effort win over the profile's (DEV-213).
    let run_model = non_blank(req.model.as_deref());
    let run_effort = non_blank(req.effort.as_deref());
    if let Some(model) = run_model {
        profile.model = Some(model);
    }
    if let Some(effort) = run_effort {
        profile.effort = Some(effort);
    }
    if let Some(provider) = non_blank(req.provider.as_deref()) {
        profile.provider = Some(provider);
    }
    if let Some(session) = req.session {
        profile.session = Some(session);
    }
    profile.model = non_blank(profile.model.as_deref());
    profile.effort = non_blank(profile.effort.as_deref());
    profile.provider = non_blank(profile.provider.as_deref());
    if let Some(provider) = profile.provider.as_deref() {
        crate::services::agent_runner::validate_run_setting("provider", provider)?;
    }
    if let Some(model) = profile.model.as_deref() {
        crate::services::agent_runner::validate_run_setting("model", model)?;
    }
    if let Some(effort) = profile.effort.as_deref() {
        crate::services::agent_runner::validate_run_setting("effort", effort)?;
    }
    if let Ok(kind) = profile.runner.parse::<AgentRunnerKind>() {
        crate::services::agent_runner::check_agent_profile(kind, &profile)?;
    }
    Ok(profile)
}

fn non_blank(value: Option<&str>) -> Option<String> {
    value.map(str::trim).filter(|value| !value.is_empty()).map(str::to_string)
}

/// The instructions for a job: the base text (`agent.instructions`, or the
/// built-in default when unset) followed by the profile's own instructions.
fn resolve_agent_instructions(tasks_dir: &std::path::Path, config: &ResolvedConfig, profile: &AgentProfileDetail) -> LoTaRResult<Option<String>> {
    let base = match config.agent_instructions.as_ref() {
        Some(configured) => read_instructions(tasks_dir, configured)?,
        None => Some(DEFAULT_AGENT_INSTRUCTIONS.trim().to_string()),
    };
    let role = match profile.instructions.as_ref() {
        Some(configured) => read_instructions(tasks_dir, configured)?,
        None => None,
    };
    Ok(match (base, role) {
        (Some(base), Some(role)) => Some(format!("{base}\n\n## Instructions for this agent\n\n{role}")),
        (base, role) => base.or(role),
    })
}

fn read_instructions(tasks_dir: &std::path::Path, configured: &AgentInstructionsConfig) -> LoTaRResult<Option<String>> {
    let text = match configured {
        AgentInstructionsConfig::Inline(raw) => raw.clone(),
        AgentInstructionsConfig::File { file } => {
            let path = resolve_agent_instructions_path(tasks_dir, file)?;
            fs::read_to_string(&path)
                .map_err(|err| LoTaRError::ValidationError(format!("Failed to read agent.instructions file '{}': {}", path.display(), err)))?
        }
    };
    Ok(non_blank(Some(&text)))
}

/// Variables for `${{...}}` templates in agent instructions: the automation
/// ones (`ticket.*`, `agent.*`) plus `ticket.project`, `lotar.bin` (the CLI
/// to call) and `workspace.root` (the main checkout). Instructions carry
/// literal values because some agent CLIs refuse commands with `$VARIABLES`.
fn instruction_template(
    tasks_dir: &std::path::Path,
    config: &ResolvedConfig,
    task: &crate::api_types::TaskDTO,
    job_id: &str,
    runner_kind: AgentRunnerKind,
    agent: Option<&str>,
) -> crate::automation::template::TemplateContext {
    let repo_root = resolve_repo_root(tasks_dir);
    let branch = config.agent_worktree.enabled.then(|| build_worktree_branch(&config.agent_worktree.branch_prefix, &sanitize_worktree_token(&task.id)));
    let worktree_path = branch.as_ref().and_then(|_| {
        let root = repo_root.as_ref()?;
        resolve_worktree_root(root, config.agent_worktree.dir.as_deref()).ok().map(|dir| dir.join(sanitize_worktree_token(&task.id)))
    });
    let job = AutomationJobContext {
        job_id: job_id.to_string(),
        runner: runner_kind.as_str().to_string(),
        agent: agent.map(str::to_string),
        worktree_path: worktree_path.map(|path| path.to_string_lossy().to_string()),
        worktree_branch: branch,
        usage: None,
        assignee_at_start: None,
    };
    let workspace_root = repo_root.unwrap_or_else(|| tasks_dir.parent().unwrap_or(tasks_dir).to_path_buf());
    let lotar_bin = current_lotar_binary().map(|bin| bin.to_string_lossy().to_string()).unwrap_or_else(|| "lotar".to_string());
    let project = task.id.rsplit_once('-').map(|(project, _)| project).unwrap_or_default();
    crate::automation::template::TemplateContext::from_task(task)
        .with_job(Some(&job))
        .with_var("ticket.project", project)
        .with_var("lotar.bin", &lotar_bin)
        .with_var("workspace.root", &workspace_root.to_string_lossy())
}

/// What LoTaR knows about how this job runs and ends, so the agent neither
/// leaves work uncommitted in a worktree nor fights automation over the
/// ticket's status. A T3 Code thread may run on a branch T3 Code made from
/// the ticket's branch (DEV-583), which LoTaR points the ticket's branch at
/// when the job ends.
fn job_notes(
    tasks_dir: &std::path::Path,
    config: &ResolvedConfig,
    ticket_id: &str,
    is_merge_job: bool,
    read_only: bool,
    lotar_mcp: bool,
    t3_thread: bool,
) -> String {
    if read_only {
        return "- This job is read-only: you can read files and search, but not edit files or run commands \
that change anything, including lotar. Don't try to update the ticket. Put your complete result in your \
final answer; LoTaR saves it as the job summary.\n"
            .to_string();
    }
    let mut notes = String::new();
    if lotar_mcp {
        notes.push_str(&format!(
            "- LoTaR's MCP server `lotar` is connected for this ticket. Prefer its tools ({}) to read and update the ticket.\n",
            crate::services::agent_runner::LOTAR_MCP_JOB_TOOLS.join(", ")
        ));
    }
    if let Some(bin) = current_lotar_binary() {
        notes.push_str(&format!("- Run the LoTaR CLI as `{}` (the build that started this job).\n", bin.display()));
    }
    if config.agent_worktree.enabled {
        let branch = build_worktree_branch(&config.agent_worktree.branch_prefix, &sanitize_worktree_token(ticket_id));
        if t3_thread {
            notes.push_str(&format!(
                "- You work in a git worktree on branch `{branch}` or one T3 Code made from it (`git branch --show-current`). \
Commit your work to the branch you are on before you finish: LoTaR points `{branch}` at it when the job ends. \
If git refuses (a sandbox), leave the changes: LoTaR commits them. Don't switch branches.\n"
            ));
        } else {
            notes.push_str(&format!(
                "- You work in a git worktree on branch `{branch}`, separate from the main checkout. Commit your work to it \
before you finish; if git refuses (a sandbox), leave the changes: LoTaR commits them when the job ends. Don't switch branches.\n"
            ));
        }
        if is_merge_job {
            notes.push_str(&format!(
                "- This is a merge job: it succeeds only if the branch ends up in the main checkout's current branch. \
If git refuses to fast-forward the main checkout (a sandbox), leave it: LoTaR fast-forwards it when the job ends, if the branch \
is rebased onto it. If you land the work another way (for example conflicts resolved in the main checkout), start each of those \
commits' messages with `{ticket_id}`, so LoTaR counts the branch as landed. Not landed (for example a check fails on the work): \
hand the ticket back as under \"Clarifications, blockers and feedback\"; LoTaR leaves the branch unmerged.\n"
            ));
        }
    } else {
        notes.push_str("- You work in the main checkout. Leave your changes uncommitted unless the ticket or your instructions ask for a commit.\n");
    }
    if AutomationService::handles_job_end(tasks_dir, ticket_id) {
        notes.push_str(
            "- When this job ends, LoTaR's automation rules move this ticket to the next step as if you finished. \
Finished: leave status and assignee, and comment what you did. Not finished (blocked, a failure you can't fix, a decision needed): \
hand the ticket back as under \"Clarifications, blockers and feedback\", so it doesn't move on.\n",
        );
    } else {
        notes.push_str(
            "- No automation rule updates this ticket when the job ends. When you finish, set a configured status that fits \
(for example a review status) and comment what you did.\n",
        );
    }
    notes
}

fn resolve_agent_instructions_path(tasks_dir: &std::path::Path, raw: &str) -> LoTaRResult<std::path::PathBuf> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Err(LoTaRError::ValidationError("agent.instructions file path cannot be empty".to_string()));
    }

    let configured = std::path::Path::new(trimmed);
    if configured.is_absolute() {
        return Ok(configured.to_path_buf());
    }

    if configured.components().any(|component| matches!(component, std::path::Component::ParentDir)) {
        return Err(LoTaRError::ValidationError("agent.instructions file path cannot contain '..'".to_string()));
    }

    Ok(tasks_dir.join(configured))
}

/// The ticket's other agent sessions in this repository, for a job's prompt (DEV-503): the agent of another phase
/// (another profile, often another CLI) can read what they did, which the recent context only sums up.
fn earlier_sessions_text(tasks_dir: &std::path::Path, ticket_id: &str, agent_profile: Option<&str>) -> Option<String> {
    let project = crate::services::agent_runner::session_project(tasks_dir);
    let current = session_name(&project, ticket_id, agent_profile);
    let sessions = crate::services::agent_runner::earlier_sessions(&project, ticket_id, &current);
    if sessions.is_empty() {
        return None;
    }
    let mut text = String::from(
        "Each file is a harnessmux event log, one JSON object per line: `message` events are that agent's replies, \
`tool.started` its tool calls, `turn.started` the prompts it got. Read one when the recent context leaves out \
something you need.\n",
    );
    for (record, events) in sessions {
        text.push_str(&format!("- {} ({} agent, last used {}): {}\n", record.name, record.agent, record.last_used, events.display()));
    }
    Some(text)
}

/// How much of the ticket's history a job's prompt carries (DEV-620).
#[derive(Clone, Copy)]
struct PromptHistory {
    /// As before DEV-620: every stored recent-context entry, uncut, and no comments.
    full: bool,
    /// The newest recent-context entries the prompt carries.
    entries: usize,
}

/// The previous job's prompt and reply: its comments carry the rest.
const DEFAULT_CONTEXT_ENTRIES: usize = 2;
/// Where a comment or recent-context entry in the prompt is cut.
const PROMPT_EXCERPT_CHARS: usize = 1500;

fn excerpt(text: &str) -> String {
    let text = text.trim();
    let mut out: String = text.chars().take(PROMPT_EXCERPT_CHARS).collect();
    if text.chars().count() > PROMPT_EXCERPT_CHARS {
        out.push_str(" …");
    }
    out
}

/// The comments a job's prompt carries (DEV-620), oldest first: the open
/// question (the newest comment before the ticket was last handed to a
/// person, not by that person, or the newest with a question), the answer
/// to it, and the newest fix and review comments. A comment's phase is the
/// profile the ticket was last assigned to when it was written (a person
/// working on it continues that profile's phase): `@review…` is a review,
/// `@merge…` neither, any other profile a fix. Automation's comments are
/// left out.
fn prompt_comments(task: &crate::api_types::TaskDTO) -> Vec<(&'static str, &crate::types::TaskComment)> {
    let assignments: Vec<(&str, &str)> = task
        .history
        .iter()
        .flat_map(|entry| {
            entry.changes.iter().filter(|change| change.field == "assignee").map(|change| (entry.at.as_str(), change.new.as_deref().unwrap_or("").trim()))
        })
        .collect();
    let is_person = |name: &str| !name.is_empty() && !name.starts_with('@');
    let phase_at = |date: &str| {
        let profile = assignments.iter().take_while(|(at, _)| *at <= date).filter(|(_, name)| name.starts_with('@')).last();
        profile.map(|(_, name)| name.to_ascii_lowercase()).unwrap_or_default()
    };

    let handed_back = assignments.iter().rev().find(|(_, name)| is_person(name));
    let before_hand_back = handed_back.and_then(|(at, name)| {
        task.comments.iter().rposition(|comment| comment.date.as_str() <= *at && comment.author.as_deref().map(str::trim) != Some(*name))
    });
    let with_question = task.comments.iter().rposition(|comment| comment.question.is_some());
    let question = before_hand_back.max(with_question);
    let answer = question.and_then(|index| {
        // The person the question's phase handed the ticket to: the hand-back can be logged just before the comment.
        let date = task.comments[index].date.as_str();
        let phase_start = assignments.iter().rposition(|(at, name)| *at <= date && name.starts_with('@')).unwrap_or(0);
        let asked = assignments.iter().skip(phase_start).find(|(_, name)| is_person(name)).map(|(_, name)| *name);
        task.comments
            .iter()
            .enumerate()
            .skip(index + 1)
            .find_map(|(i, comment)| (comment.answer.is_some() || (asked.is_some() && comment.author.as_deref().map(str::trim) == asked)).then_some(i))
    });
    let newest_in = |review: bool| {
        let in_phase = |comment: &crate::types::TaskComment| {
            let phase = phase_at(&comment.date);
            let is_review = comment.review.is_some() || phase.starts_with("@review");
            !phase.is_empty() && !phase.starts_with("@merge") && is_review == review && comment.author.as_deref() != Some("automation")
        };
        task.comments.iter().enumerate().rev().find(|(i, comment)| Some(*i) != question && in_phase(comment)).map(|(i, _)| i)
    };

    let mut picked: Vec<(usize, &'static str)> = Vec::new();
    let question_label = if answer.is_some() { "Question" } else { "Open question" };
    for (index, label) in [(question, question_label), (answer, "Answer"), (newest_in(false), "Newest fix comment"), (newest_in(true), "Newest review comment")]
    {
        if let Some(index) = index
            && !picked.iter().any(|(i, _)| *i == index)
        {
            picked.push((index, label));
        }
    }
    picked.sort_by_key(|(index, _)| *index);
    picked.into_iter().map(|(index, label)| (label, &task.comments[index])).collect()
}

fn build_prompt(
    task: &crate::api_types::TaskDTO,
    user_prompt: &str,
    context: Option<&crate::services::agent_context_service::AgentContextFile>,
    history: PromptHistory,
    earlier_sessions: Option<&str>,
    instructions: Option<&str>,
    job_notes: Option<&str>,
) -> String {
    let mut out = String::new();
    if let Some(instructions) = instructions {
        let trimmed = instructions.trim();
        if !trimmed.is_empty() {
            out.push_str("Agent instructions:\n");
            out.push_str(trimmed);
            out.push_str("\n\n");
        }
    }
    if let Some(notes) = job_notes.map(str::trim).filter(|notes| !notes.is_empty()) {
        out.push_str("This job:\n");
        out.push_str(notes);
        out.push_str("\n\n");
    }
    out.push_str("Ticket context:\n");
    out.push_str(&format!("- ID: {}\n", task.id));
    out.push_str(&format!("- Title: {}\n", task.title));
    out.push_str(&format!("- Status: {}\n", task.status));
    out.push_str(&format!("- Priority: {}\n", task.priority));
    out.push_str(&format!("- Type: {}\n", task.task_type));
    if !task.tags.is_empty() {
        out.push_str(&format!("- Tags: {}\n", task.tags.join(", ")));
    }
    if let Some(desc) = task.description.as_deref()
        && !desc.trim().is_empty()
    {
        out.push_str("- Description:\n");
        out.push_str(desc);
        out.push('\n');
    }

    if !task.relationships.is_empty() {
        out.push_str("\nRelationships:\n");
        if !task.relationships.depends_on.is_empty() {
            out.push_str(&format!("- Depends on: {}\n", task.relationships.depends_on.join(", ")));
        }
        if !task.relationships.blocks.is_empty() {
            out.push_str(&format!("- Blocks: {}\n", task.relationships.blocks.join(", ")));
        }
        if !task.relationships.related.is_empty() {
            out.push_str(&format!("- Related: {}\n", task.relationships.related.join(", ")));
        }
        if let Some(parent) = task.relationships.parent.as_ref() {
            out.push_str(&format!("- Parent: {}\n", parent));
        }
        if !task.relationships.children.is_empty() {
            out.push_str(&format!("- Children: {}\n", task.relationships.children.join(", ")));
        }
        if !task.relationships.fixes.is_empty() {
            out.push_str(&format!("- Fixes: {}\n", task.relationships.fixes.join(", ")));
        }
        if let Some(duplicate_of) = task.relationships.duplicate_of.as_ref() {
            out.push_str(&format!("- Duplicate of: {}\n", duplicate_of));
        }
    }

    if !history.full && !task.comments.is_empty() {
        out.push_str(&format!(
            "\nComments ({} on the ticket; below the open question and the newest fix and review comments, each cut at {PROMPT_EXCERPT_CHARS} characters. \
All of them: `lotar show {id}`; the history: `lotar task timeline {id}`):\n",
            task.comments.len(),
            id = task.id
        ));
        for (label, comment) in prompt_comments(task) {
            let author = comment.author.as_deref().unwrap_or("unknown");
            out.push_str(&format!("- {label}, by {author}, {}:\n{}\n", comment.date, excerpt(&comment.text)));
        }
    }

    if let Some(ctx) = context
        && !ctx.messages.is_empty()
    {
        if history.full {
            out.push_str("\nRecent context:\n");
            for msg in &ctx.messages {
                out.push_str(&format!("- {}: {}\n", msg.role, msg.content));
            }
        } else if history.entries > 0 {
            let start = ctx.messages.len().saturating_sub(history.entries);
            out.push_str(&format!(
                "\nRecent context (the newest {} of {} prompts and replies, each cut at {PROMPT_EXCERPT_CHARS} characters):\n",
                ctx.messages.len() - start,
                ctx.messages.len()
            ));
            for msg in &ctx.messages[start..] {
                out.push_str(&format!("- {}: {}\n", msg.role, excerpt(&msg.content)));
            }
        }
    }
    if let Some(sessions) = earlier_sessions {
        out.push_str("\nEarlier agent sessions on this ticket:\n");
        out.push_str(sessions);
    }

    out.push_str("\nUser request:\n");
    out.push_str(user_prompt.trim());
    out.push('\n');

    out
}

/// The running `lotar` executable, when this process is one (not a test
/// harness or another host embedding the library).
fn current_lotar_binary() -> Option<std::path::PathBuf> {
    let exe = std::env::current_exe().ok()?;
    let stem = exe.file_stem()?.to_str()?;
    (stem == "lotar").then_some(exe)
}

/// Directories outside the working directory that a workspace-permission
/// job may write: the tasks directory and the repository's git data (also
/// when `.git` is a file, as in a submodule). A merge job gets the main
/// checkout instead, since merging updates it.
fn extra_writable_dirs(working_dir: &std::path::Path, tasks_dir: &std::path::Path, is_merge_job: bool) -> Vec<std::path::PathBuf> {
    let canonical = |path: &std::path::Path| path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
    let work = canonical(working_dir);
    let mut dirs = vec![canonical(tasks_dir)];
    if let Some(repo_root) = resolve_repo_root(tasks_dir) {
        if is_merge_job {
            dirs.push(canonical(&repo_root));
        } else if let Ok(git_dir) = verified_common_dir(&repo_root) {
            dirs.push(canonical(&git_dir));
        }
    }
    dirs.retain(|dir| !dir.starts_with(&work));
    dirs.dedup();
    dirs
}

struct WorktreeContext {
    working_dir: std::path::PathBuf,
    worktree_path: Option<std::path::PathBuf>,
    worktree_branch: Option<String>,
    /// Why the job runs in this worktree, for the job log.
    note: Option<String>,
}

fn prepare_worktree(tasks_dir: &std::path::Path, ticket_id: &str, config: &ResolvedConfig) -> LoTaRResult<WorktreeContext> {
    let repo_root = resolve_repo_root(tasks_dir).ok_or_else(|| LoTaRError::ValidationError("Git repository not found".to_string()))?;

    let mut context = WorktreeContext { working_dir: repo_root.clone(), worktree_path: None, worktree_branch: None, note: None };

    if !config.agent_worktree.enabled {
        return Ok(context);
    }
    let repo_common = verified_common_dir(&repo_root).map_err(LoTaRError::IoError)?;

    let worktree_root = resolve_worktree_root(&repo_root, config.agent_worktree.dir.as_deref())?;
    let ticket_token = sanitize_worktree_token(ticket_id);
    let branch = build_worktree_branch(&config.agent_worktree.branch_prefix, &ticket_token);
    // Worktree is per-ticket, not per-job. All phases for a ticket share the same worktree.
    let worktree_path = worktree_root.join(&ticket_token);

    if !branch_exists(&repo_root, &branch) {
        run_git_command(&repo_root, &["branch", &branch])?;
    }

    // Check if worktree already exists (reuse from previous phase)
    if worktree_path.exists() && worktree_path.join(".git").exists() {
        let actual = fs::canonicalize(&worktree_path).map_err(LoTaRError::IoError)?;
        let root = fs::canonicalize(&repo_root).map_err(LoTaRError::IoError)?;
        let head = git_command(&actual).args(["symbolic-ref", "--quiet", "HEAD"]).current_dir(&actual).output().map_err(LoTaRError::IoError)?;
        if actual == root
            || verified_common_dir(&actual).map_err(LoTaRError::IoError)? != repo_common
            || !head.status.success()
            || String::from_utf8_lossy(&head.stdout).trim() != format!("refs/heads/{branch}")
        {
            return Err(LoTaRError::ValidationError("Existing worktree is not the isolated ticket branch".into()));
        }
        // Worktree already set up for this ticket, reuse it
        context.working_dir = worktree_path.clone();
        context.worktree_path = Some(worktree_path);
        context.worktree_branch = Some(branch);
        return Ok(context);
    }

    // The ticket's branch checked out in another worktree, a T3 Code
    // thread's (DEV-581): git checks a branch out in one worktree only, so
    // the job runs there. The thread can continue in it afterwards.
    if let Some(path) =
        linked_worktrees(&repo_root)?.into_iter().find_map(|(path, on)| (on.as_deref() == Some(branch.as_str()) && path.is_dir()).then_some(path))
    {
        if verified_common_dir(&path).map_err(LoTaRError::IoError)? != repo_common {
            return Err(LoTaRError::ValidationError("Worktree belongs to a different repository".into()));
        }
        context.note = Some(format!("{branch} is checked out in the worktree {}, so this job runs there.", path.display()));
        context.working_dir = path.clone();
        context.worktree_path = Some(path);
        context.worktree_branch = Some(branch);
        return Ok(context);
    }

    std::fs::create_dir_all(&worktree_root).map_err(LoTaRError::IoError)?;
    let worktree_arg = worktree_path.to_string_lossy().to_string();
    run_git_command(&repo_root, &["worktree", "add", &worktree_arg, &branch])?;
    if verified_common_dir(&worktree_path).map_err(LoTaRError::IoError)? != repo_common {
        return Err(LoTaRError::ValidationError("Worktree belongs to a different repository".into()));
    }
    context.note = Some(clone_build_output(&repo_root, &config.agent_worktree.branch_prefix, &branch, &worktree_path));

    context.working_dir = worktree_path.clone();
    context.worktree_path = Some(worktree_path);
    context.worktree_branch = Some(branch);
    Ok(context)
}

/// A new worktree starts from a copy-on-write clone of the ticket's newest
/// build (DEV-635): `target/` of an earlier worktree of the ticket (its T3
/// Code threads'), the one built last, else of the main checkout. Only
/// `clonefile(2)` clones; a full copy would cost the disk the clone saves,
/// so on failure the job builds cold. Returns the note for the job log.
fn clone_build_output(repo_root: &std::path::Path, prefix: &str, branch: &str, worktree: &std::path::Path) -> String {
    let build = |dir: &std::path::Path| dir.join("target");
    let has_build = |dir: &std::path::Path| fs::symlink_metadata(build(dir)).is_ok_and(|meta| meta.is_dir());
    // `target/` itself changes only when an entry is added; cargo rewrites
    // `.rustc_info.json` and the profile directories on every build.
    let built_at = |dir: &std::path::Path| {
        fs::read_dir(build(dir)).into_iter().flatten().flatten().filter_map(|entry| entry.metadata().and_then(|meta| meta.modified()).ok()).max()
    };
    let new = fs::canonicalize(worktree).unwrap_or_else(|_| worktree.to_path_buf());
    let earlier = agent_worktrees(repo_root, prefix)
        .unwrap_or_default()
        .into_iter()
        .filter(|tree| tree.ticket_branch == branch && fs::canonicalize(&tree.path).is_ok_and(|path| path != new) && has_build(&tree.path))
        .max_by_key(|tree| built_at(&tree.path))
        .map(|tree| tree.path);
    let Some(source) = earlier.or_else(|| has_build(repo_root).then(|| repo_root.to_path_buf())) else {
        return "No target/ to clone in an earlier worktree of the ticket or in the main checkout; this job builds cold.".to_string();
    };
    let to = build(worktree);
    if fs::symlink_metadata(&to).is_ok() {
        return "The new worktree has a target/ already; nothing cloned.".to_string();
    }
    match clonefile(&build(&source), &to) {
        Ok(()) => format!("Cloned target/ from {} (copy-on-write).", source.display()),
        Err(err) => {
            let _ = fs::remove_dir_all(&to);
            format!("Couldn't clone target/ from {}: {err}; this job builds cold.", source.display())
        }
    }
}

/// Clone a directory tree copy-on-write (APFS); other filesystems fail.
#[cfg(target_os = "macos")]
fn clonefile(from: &std::path::Path, to: &std::path::Path) -> std::io::Result<()> {
    use std::os::unix::ffi::OsStrExt;
    let from = std::ffi::CString::new(from.as_os_str().as_bytes())?;
    let to = std::ffi::CString::new(to.as_os_str().as_bytes())?;
    // SAFETY: both are NUL-terminated paths that outlive the call.
    if unsafe { libc::clonefile(from.as_ptr(), to.as_ptr(), 0) } == 0 { Ok(()) } else { Err(std::io::Error::last_os_error()) }
}

#[cfg(not(target_os = "macos"))]
fn clonefile(_from: &std::path::Path, _to: &std::path::Path) -> std::io::Result<()> {
    Err(std::io::Error::new(std::io::ErrorKind::Unsupported, "clonefile(2) isn't available on this system"))
}

/// Where a t3 job's thread starts: `t3 thread start --worktree <base> [--branch <branch>]`.
struct T3Worktree {
    base: String,
    branch: Option<String>,
}

/// A T3 Code thread runs on the T3 server and doesn't get the job's
/// environment (`LOTAR_TASKS_DIR`), so its lotar commands name the tasks
/// folder; in a worktree they would otherwise change the worktree's copy.
/// A retried or resumed job's thread starts in a new worktree (DEV-583), so
/// the notes name the ticket's worktrees that hold uncommitted work. They
/// stay there after a job carried them over or dropped them, so the note
/// leaves them to a job that takes the work over; a merge job doesn't get
/// it (DEV-608).
fn t3_thread_notes(tasks_dir: &std::path::Path, config: &ResolvedConfig, ticket_id: &str, is_merge_job: bool) -> String {
    let dir = fs::canonicalize(tasks_dir).unwrap_or_else(|_| tasks_dir.to_path_buf());
    let mut notes = format!(
        "- A T3 Code thread doesn't get LoTaR's environment: add `--tasks-dir \"{}\"` to every lotar command, \
so it changes the ticket in the main checkout, not a worktree copy.\n",
        dir.display()
    );
    if is_merge_job {
        return notes;
    }
    for (path, branch) in uncommitted_ticket_worktrees(tasks_dir, config, ticket_id) {
        let shown = path.display();
        notes.push_str(&format!(
            "- An earlier job on this ticket left uncommitted changes in `{shown}` (branch `{branch}`), outside your worktree. \
A job that takes the work over (a fix or rework) reads them there (`git -C \"{shown}\" status`, `git -C \"{shown}\" diff`) \
and carries over what it still needs; a later job may have carried them over or dropped them already, so check the ticket's \
commits and comments first. A review job leaves them alone. Leave that worktree as it is.\n"
        ));
    }
    notes
}

/// The worktrees of the ticket's branch and of the T3 Code thread branches
/// made from it that have uncommitted changes, with their branches.
fn uncommitted_ticket_worktrees(tasks_dir: &std::path::Path, config: &ResolvedConfig, ticket_id: &str) -> Vec<(std::path::PathBuf, String)> {
    if !config.agent_worktree.enabled {
        return Vec::new();
    }
    let Some(repo_root) = resolve_repo_root(tasks_dir) else {
        return Vec::new();
    };
    let ticket_branch = build_worktree_branch(&config.agent_worktree.branch_prefix, &sanitize_worktree_token(ticket_id));
    let worktrees = agent_worktrees(&repo_root, &config.agent_worktree.branch_prefix).unwrap_or_default();
    worktrees
        .into_iter()
        .filter(|worktree| worktree.ticket_branch == ticket_branch)
        .filter(|worktree| {
            let status = git_command(&worktree.path).args(["status", "--porcelain"]).output();
            status.is_ok_and(|status| status.status.success() && !status.stdout.is_empty())
        })
        .map(|worktree| (fs::canonicalize(&worktree.path).unwrap_or(worktree.path), worktree.branch))
        .collect()
}

/// The worktree a t3 job asks T3 Code for, as T3 Code runs a thread only in
/// its project (DEV-318): a new ticket branch from the main checkout's
/// current branch, or the existing ticket branch of an earlier phase. The
/// session runs in the main checkout (the T3 project).
fn plan_t3_worktree(tasks_dir: &std::path::Path, ticket_id: &str, config: &ResolvedConfig) -> LoTaRResult<(WorktreeContext, T3Worktree)> {
    let repo_root = resolve_repo_root(tasks_dir).ok_or_else(|| LoTaRError::ValidationError("Git repository not found".to_string()))?;
    let branch = build_worktree_branch(&config.agent_worktree.branch_prefix, &sanitize_worktree_token(ticket_id));
    let mut note = None;
    let plan = if branch_exists(&repo_root, &branch) {
        T3Worktree { base: branch.clone(), branch: None }
    } else {
        note = move_aside_detached_t3_worktree(&repo_root, &config.agent_worktree.branch_prefix, &branch)?;
        let head =
            git_command(&repo_root).args(["symbolic-ref", "--quiet", "--short", "HEAD"]).current_dir(&repo_root).output().map_err(LoTaRError::IoError)?;
        let base = String::from_utf8_lossy(&head.stdout).trim().to_string();
        T3Worktree { base: if head.status.success() && !base.is_empty() { base } else { "HEAD".to_string() }, branch: Some(branch.clone()) }
    };
    let context = WorktreeContext { working_dir: repo_root, worktree_path: None, worktree_branch: Some(branch), note };
    Ok((context, plan))
}

/// T3 Code puts a new branch's worktree at the same path every time:
/// `<repo>/<branch, / as ->-<hash of repository and branch>`. A worktree an
/// earlier thread left there, detached from the ticket's branch when a
/// later thread took the branch over (DEV-573), stays after the branch is
/// deleted, and every later thread then fails with `path_already_exists`
/// (DEV-596). It moves aside with its changes; Done cleanup finds it there.
fn move_aside_detached_t3_worktree(repo_root: &std::path::Path, prefix: &str, branch: &str) -> LoTaRResult<Option<String>> {
    let mut stem = String::new();
    for character in branch.replace('/', "-").chars() {
        if stem.len() + character.len_utf8() > 23 {
            break;
        }
        stem.push(character);
    }
    let repo_name = repo_root.file_name();
    let t3_path = |path: &std::path::Path| {
        path.parent().and_then(std::path::Path::file_name) == repo_name
            && path
                .file_name()
                .and_then(|name| name.to_str())
                .and_then(|name| name.strip_prefix(stem.as_str())?.strip_prefix('-'))
                .is_some_and(|hash| hash.len() == 16 && hash.bytes().all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte)))
    };
    let Some(stale) =
        agent_worktrees(repo_root, prefix)?.into_iter().find(|worktree| worktree.detached && worktree.ticket_branch == branch && t3_path(&worktree.path))
    else {
        return Ok(None);
    };
    let name = stale.path.file_name().map(|name| name.to_string_lossy().to_string()).unwrap_or_default();
    let target = (1..).map(|n| stale.path.with_file_name(format!("{name}-detached-{n}"))).find(|path| !path.exists()).unwrap_or_default();
    run_git_command(repo_root, &["worktree", "move", &stale.path.to_string_lossy(), &target.to_string_lossy()])?;
    Ok(Some(format!(
        "Moved the worktree {} that an earlier T3 Code thread left detached from {branch} to {}, so T3 Code can make the new thread's worktree there.",
        stale.path.display(),
        target.display()
    )))
}

/// The worktree of the T3 Code thread a `session: continue` job sends its
/// turn to, moved to the ticket branch's commit (DEV-550): T3 Code keeps a
/// thread on a branch of its own, which a later job's work on the ticket's
/// branch (an amended rework) leaves behind. The branch moves when it is
/// behind the ticket's branch, or still at the commit the ticket's branch
/// pointed at when the thread's last job ended. The worktree is the one
/// the session's latest job logged, and only when that job ended in
/// `thread`: a job that started a new thread replaced the session's
/// earlier one under the same name. `Ok` gives the worktree, its branch
/// and a note when the branch moved; `Err` says why the thread can't
/// continue, and LoTaR changes nothing then.
fn continued_thread_worktree(
    tasks_dir: &std::path::Path,
    ticket_id: &str,
    config: &ResolvedConfig,
    session: &str,
    thread: Option<&str>,
) -> Result<(std::path::PathBuf, String, Option<String>), String> {
    let ticket_branch = build_worktree_branch(&config.agent_worktree.branch_prefix, &sanitize_worktree_token(ticket_id));
    let refuse = |why: String| format!("{why}; this job starts a new T3 Code thread on {ticket_branch}.");
    let repo_root = resolve_repo_root(tasks_dir).ok_or_else(|| refuse("Git repository not found".to_string()))?;
    let latest = saved_jobs(tasks_dir)
        .into_iter()
        .filter_map(|job| job.status)
        .filter(|status| status.session_id.as_deref() == Some(session))
        .max_by(|a, b| a.at.cmp(&b.at))
        .filter(|status| thread.is_some() && status.agent_session.as_deref() == thread);
    let (path, branch, ticket_commit) = latest
        .and_then(|status| Some((std::path::PathBuf::from(status.worktree_path?), status.worktree_branch?, status.ticket_commit)))
        .ok_or_else(|| refuse(format!("The latest job of the session {session} recorded no worktree of its T3 Code thread")))?;
    let shown = path.display();
    if !path.is_dir() {
        return Err(refuse(format!("The T3 Code thread's worktree {shown} is gone")));
    }
    let git = |args: &[&str]| -> Result<std::process::Output, String> {
        git_command(&path).args(args).output().map_err(|err| refuse(format!("git {} in {shown}: {err}", args.join(" "))))
    };
    let on = git(&["symbolic-ref", "--quiet", "--short", "HEAD"])?;
    if !on.status.success() || String::from_utf8_lossy(&on.stdout).trim() != branch {
        return Err(refuse(format!("The T3 Code thread's worktree {shown} is no longer on {branch}")));
    }
    let changes = git(&["status", "--porcelain"])?;
    if !changes.status.success() || !changes.stdout.is_empty() {
        return Err(refuse(format!("The T3 Code thread's worktree {shown} has uncommitted changes, which LoTaR leaves alone")));
    }
    let target = rev_parse(&repo_root, &format!("refs/heads/{ticket_branch}")).ok_or_else(|| refuse(format!("{ticket_branch} doesn't exist")))?;
    let head = rev_parse(&path, "HEAD").ok_or_else(|| refuse(format!("{shown} has no commit")))?;
    if head == target {
        return Ok((path, branch, None));
    }
    let behind = git(&["merge-base", "--is-ancestor", &head, &target])?.status.success();
    if !behind && ticket_commit.as_deref() != Some(head.as_str()) {
        return Err(refuse(format!("{branch} in the T3 Code thread's worktree {shown} has commits {ticket_branch} lacks, which LoTaR leaves alone")));
    }
    let reset = git(&["reset", "--quiet", "--keep", &target])?;
    if !reset.status.success() {
        return Err(refuse(format!("git reset --keep {target} in {shown} failed: {}", String::from_utf8_lossy(&reset.stderr).trim())));
    }
    let short = |commit: &str| commit.get(..12).unwrap_or(commit).to_string();
    let note = format!(
        "Moved {branch} of the continued T3 Code thread from {} to {} ({ticket_branch}), so the thread works on the ticket's current commit.",
        short(&head),
        short(&target)
    );
    Ok((path, branch, Some(note)))
}

/// Record the worktree T3 Code made for a t3 job's thread: the one T3 Code
/// names for the thread, else the one that appeared while it ran, else of
/// those the one on the job's branch. Other jobs' threads make worktrees
/// meanwhile, and an earlier job's may hold the job's branch (DEV-573).
/// T3 Code may rename the branch it was given (`agent/DEV-1` becomes
/// `t3/<topic>`), so the branch is read from the worktree.
fn record_t3_worktree(job_id: &str, repo_root: &std::path::Path, before: &[std::path::PathBuf], named: Option<std::path::PathBuf>) {
    let Ok(now) = linked_worktrees(repo_root) else {
        return;
    };
    let branch = JOB_REGISTRY.lock().ok().and_then(|registry| registry.jobs.get(job_id).and_then(|state| state.record.worktree_branch.clone()));
    let new: Vec<_> = now.iter().filter(|(path, _)| !before.contains(path)).collect();
    let named = named.and_then(|named| fs::canonicalize(named).ok());
    let found = match (named.and_then(|named| new.iter().find(|(path, _)| fs::canonicalize(path).ok().as_ref() == Some(&named))), new.as_slice()) {
        (Some(thread), _) | (None, [thread]) => Some((*thread).clone()),
        _ => branch.as_deref().and_then(|branch| new.iter().find(|(_, on)| on.as_deref() == Some(branch)).map(|found| (*found).clone())),
    };
    if let Some((path, branch)) = found {
        update_job(job_id, |state| {
            state.record.worktree_path = Some(path.to_string_lossy().to_string());
            state.record.worktree_branch = branch.clone().or(state.record.worktree_branch.take());
        });
    }
}

/// Point the ticket's agent branch at the commit its thread ended on
/// (DEV-468): T3 Code runs a thread on a branch of its own (`t3/<topic>`),
/// and a later job that starts from the ticket's branch must read that
/// thread's work, not an earlier job's.
fn follow_thread_branch(job_id: &str, tasks_dir: &std::path::Path, ticket_id: &str, config: &ResolvedConfig) -> Result<(), String> {
    if !config.agent_worktree.enabled {
        return Ok(());
    }
    let thread = JOB_REGISTRY.lock().ok().and_then(|registry| {
        let state = registry.jobs.get(job_id)?;
        let record = &state.record;
        let branch = record.worktree_branch.clone().filter(|_| record.runner == AgentRunnerKind::T3.as_str() && record.worktree_path.is_some())?;
        Some((branch, state.ticket_commit_at_start.clone()))
    });
    let (Some((thread_branch, started_from)), Some(repo_root)) = (thread, resolve_repo_root(tasks_dir)) else {
        return Ok(());
    };
    let ticket_branch = build_worktree_branch(&config.agent_worktree.branch_prefix, &sanitize_worktree_token(ticket_id));
    if thread_branch == ticket_branch {
        return Ok(());
    }
    let moved = move_ticket_branch(tasks_dir, &repo_root, &ticket_branch, &thread_branch, started_from.as_deref(), Some(job_id))
        .map_err(|err| format!("{ticket_branch} stays where it is, not at {thread_branch} of the T3 Code thread: {err}"))?;
    let note = match moved {
        BranchMove::Already => None,
        BranchMove::Moved => {
            Some(format!("Pointed {ticket_branch} at {thread_branch}, where the T3 Code thread worked, so the ticket's next job starts from its work."))
        }
        BranchMove::Stayed(why) => Some(why),
    };
    if let Some(note) = note {
        update_job(job_id, |state| {
            record_and_emit(state, "agent_job_progress", Some(note.clone()));
        });
    }
    Ok(())
}

/// The branch of a ticket, other than `except`, whose work missing from the
/// main checkout's branch is newest: `ticket_branch`, a branch traced to it
/// (`BranchOwners`) or one a job of the ticket logged (T3 Code may rename
/// the branch it was given), newest tip first, the ticket's branch on a
/// tie. A branch whose work landed, also rebased, doesn't count, nor does
/// one main supersedes (`superseded_removal`): a merge that rebased and
/// amended the fix leaves earlier copies whose content differs. A branch
/// counted only for its logged name must still hold the ticket's work
/// (`holds_logged_work`): cleanup frees the name, and T3 Code may give it to
/// another ticket's thread (DEV-612).
fn unlanded_ticket_branch(tasks_dir: &std::path::Path, ticket_id: &str, prefix: &str, except: Option<&str>) -> Option<String> {
    let repo_root = resolve_repo_root(tasks_dir)?;
    let ticket_branch = build_worktree_branch(prefix, &sanitize_worktree_token(ticket_id));
    let refs = git_stdout(&repo_root, &["for-each-ref", "--format=%(committerdate:unix) %(refname:short)", "refs/heads/"]).ok()?;
    let main = git_stdout(&repo_root, &["symbolic-ref", "--quiet", "--short", "HEAD"]).ok().map(|branch| branch.trim().to_string());
    let owners = BranchOwners::load(&repo_root, prefix);
    let jobs: Vec<_> = saved_jobs(tasks_dir).into_iter().filter(|job| job.header.ticket_id == ticket_id).collect();
    let commits: Vec<&str> = jobs.iter().filter_map(|job| job.status.as_ref()?.ticket_commit.as_deref()).collect();
    let logged: Vec<&str> = jobs
        .iter()
        .flat_map(|job| [job.header.worktree_branch.as_deref(), job.status.as_ref().and_then(|status| status.worktree_branch.as_deref())])
        .flatten()
        .collect();
    refs.lines()
        .filter_map(|line| line.split_once(' '))
        .filter(|(_, branch)| main.as_deref() != Some(*branch) && except != Some(*branch))
        .filter(|(_, branch)| {
            *branch == ticket_branch
                || owners.ticket_branch_of(branch).as_deref() == Some(ticket_branch.as_str())
                || (logged.contains(branch) && holds_logged_work(&repo_root, branch, ticket_id, &commits))
        })
        .filter(|(_, branch)| !branch_landed(&repo_root, branch).unwrap_or(true))
        .filter(|(_, branch)| {
            rev_parse(&repo_root, &format!("refs/heads/{branch}")).is_some_and(|tip| superseded_removal(&repo_root, &tip, ticket_id, branch).is_none())
        })
        .max_by_key(|(date, branch)| (date.parse::<i64>().unwrap_or(0), *branch == ticket_branch))
        .map(|(_, branch)| branch.to_string())
}

/// Whether `branch`, a name a job of `ticket_id` logged, still holds the
/// ticket's work: its tip descends from a `commits` entry the jobs logged
/// that main lacks, or a commit main lacks names the ticket first in its
/// subject (`TEST-1 fix`, `TEST-1: fix`, `TEST-1-fix`). A state git can't
/// judge doesn't count.
fn holds_logged_work(repo_root: &std::path::Path, branch: &str, ticket_id: &str, commits: &[&str]) -> bool {
    let tip = format!("refs/heads/{branch}");
    let ancestor =
        |commit: &str, of: &str| git_command(repo_root).args(["merge-base", "--is-ancestor", commit, of]).status().is_ok_and(|status| status.success());
    if commits.iter().any(|commit| ancestor(commit, &tip) && !ancestor(commit, "HEAD")) {
        return true;
    }
    let names_ticket = |subject: &str| {
        subject.get(..ticket_id.len()).is_some_and(|start| start.eq_ignore_ascii_case(ticket_id))
            && !subject.get(ticket_id.len()..).and_then(|rest| rest.chars().next()).is_some_and(char::is_alphanumeric)
    };
    git_stdout(repo_root, &["log", "--format=%s", &format!("HEAD..{tip}")]).is_ok_and(|subjects| subjects.lines().any(names_ticket))
}

/// Point the ticket's branch at the ticket's branch with the newest work
/// missing from the main checkout (DEV-602), before a job plans its
/// worktree: a thread may commit on an earlier thread's branch (`t3/<topic>`)
/// while the ticket's branch stays behind, and the review and merge then
/// start from nothing. The ticket's branch stays when it has work of its
/// own that the newer branch doesn't build on. `Ok(Some(note))` when it
/// moved; `Err` when git refused to move it.
fn follow_ticket_work(tasks_dir: &std::path::Path, ticket_id: &str, config: &ResolvedConfig) -> Result<Option<String>, String> {
    if !config.agent_worktree.enabled {
        return Ok(None);
    }
    let Some(repo_root) = resolve_repo_root(tasks_dir) else {
        return Ok(None);
    };
    let prefix = &config.agent_worktree.branch_prefix;
    let ticket_branch = build_worktree_branch(prefix, &sanitize_worktree_token(ticket_id));
    let Some(holder) = unlanded_ticket_branch(tasks_dir, ticket_id, prefix, None).filter(|holder| *holder != ticket_branch) else {
        return Ok(None);
    };
    if let Some(ticket_tip) = rev_parse(&repo_root, &format!("refs/heads/{ticket_branch}")) {
        let builds_on = git_command(&repo_root).args(["merge-base", "--is-ancestor", &ticket_tip, &format!("refs/heads/{holder}")]).status();
        if !builds_on.is_ok_and(|status| status.success()) && !branch_landed(&repo_root, &ticket_branch).unwrap_or(false) {
            return Ok(None);
        }
    }
    let moved = move_ticket_branch(tasks_dir, &repo_root, &ticket_branch, &holder, None, None)
        .map_err(|err| format!("{ticket_branch} can't move to {holder}, which holds {ticket_id}'s newest work that isn't in the main checkout: {err}"))?;
    let tip = rev_parse(&repo_root, &format!("refs/heads/{holder}")).unwrap_or_default();
    Ok(match moved {
        BranchMove::Already => None,
        BranchMove::Moved => Some(format!(
            "Pointed {ticket_branch} at {holder} ({}), which holds {ticket_id}'s newest work that isn't in the main checkout's branch, so this job starts from it.",
            tip.get(..12).unwrap_or(&tip)
        )),
        BranchMove::Stayed(why) => Some(why),
    })
}

/// What `move_ticket_branch` did with the ticket's branch.
enum BranchMove {
    /// It was at the commit already.
    Already,
    Moved,
    /// It stays where it is, for the reason given: the job's work is in the
    /// main checkout.
    Stayed(String),
}

/// Move `ticket_branch` to the tip of `thread_branch`, or create it there.
/// `Already` when it is there already; an error when it holds a commit
/// the thread's branch lacks and the main checkout doesn't have, which
/// moving it would drop. Unless it is still at `started_from`, the commit
/// the thread started from: then the thread rewrote that commit (an amend).
/// A worktree of a finished job that still has `ticket_branch` checked out
/// (an earlier thread's, which T3 Code leaves when a later thread takes
/// over the branch, DEV-573) moves with it when clean, else is detached at
/// its commit first; `ending`, the job that moves it, doesn't count as one at
/// work there (DEV-673). Any other worktree on it may be an agent's at work and
/// stays as it is (owner 2026-11-15, DEV-577): the branch then stays too,
/// when the thread's work is in the main checkout, as no job needs it then,
/// and it is an error otherwise.
fn move_ticket_branch(
    tasks_dir: &std::path::Path,
    repo_root: &std::path::Path,
    ticket_branch: &str,
    thread_branch: &str,
    started_from: Option<&str>,
    ending: Option<&str>,
) -> Result<BranchMove, String> {
    let git_output = |args: &[&str]| -> Result<String, String> {
        let output = git_command(repo_root).args(args).output().map_err(|err| format!("git {}: {err}", args.join(" ")))?;
        if !output.status.success() {
            return Err(format!("git {} failed: {}", args.join(" "), String::from_utf8_lossy(&output.stderr).trim()));
        }
        Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
    };
    let tip = git_output(&["rev-parse", "--verify", &format!("refs/heads/{thread_branch}^{{commit}}")])?;
    if branch_exists(repo_root, ticket_branch) {
        let ticket_ref = format!("refs/heads/{ticket_branch}");
        let ticket_tip = git_output(&["rev-parse", &ticket_ref])?;
        if ticket_tip == tip {
            return Ok(BranchMove::Already);
        }
        let lacks = started_from != Some(ticket_tip.as_str()) && git_output(&["cherry", &tip, &ticket_ref])?.lines().any(|line| line.starts_with('+'));
        if lacks && !branch_landed(repo_root, ticket_branch).unwrap_or(false) {
            return Err(format!("it has commits that {thread_branch} lacks"));
        }
        let worktrees = linked_worktrees(repo_root).map_err(|err| err.to_string())?;
        if let Some((path, _)) = worktrees.iter().find(|(path, on)| on.as_deref() == Some(ticket_branch) && finished_job_worktree(tasks_dir, path, ending)) {
            let in_worktree = |args: &[&str]| -> Result<String, String> {
                let output = git_command(path).args(args).output().map_err(|err| format!("git {} in {}: {err}", args.join(" "), path.display()))?;
                if !output.status.success() {
                    return Err(format!("git {} in {} failed: {}", args.join(" "), path.display(), String::from_utf8_lossy(&output.stderr).trim()));
                }
                Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
            };
            // A clean worktree moves with the branch and keeps it checked
            // out, so a later thread on the branch finds it there (DEV-595).
            if in_worktree(&["status", "--porcelain"])?.is_empty() {
                in_worktree(&["reset", "--quiet", "--keep", &tip])?;
                return Ok(BranchMove::Moved);
            }
            in_worktree(&["checkout", "--quiet", "--detach"])?;
        }
    }
    if let Err(err) = git_output(&["branch", "--force", ticket_branch, &tip]) {
        if !commit_landed(repo_root, &tip, ticket_branch).unwrap_or(false) {
            return Err(err);
        }
        return Ok(BranchMove::Stayed(format!(
            "{ticket_branch} stays where it is, not at {thread_branch}: the T3 Code thread's work is in the main checkout, and {err}"
        )));
    }
    Ok(BranchMove::Moved)
}

/// Whether `path` is the worktree of a finished job: an ended job's log
/// names it, and no job still works there. A job works there while its log
/// has no end, a running job continues its session, or T3 Code still runs
/// its thread or waits for an answer in it (DEV-594). The job `ending` is
/// left out: it may continue the session of the job that left the worktree
/// while its own thread works elsewhere (DEV-673).
fn finished_job_worktree(tasks_dir: &std::path::Path, path: &std::path::Path, ending: Option<&str>) -> bool {
    let Ok(path) = fs::canonicalize(path) else {
        return false;
    };
    let names_it = |worktree: Option<&String>| worktree.and_then(|worktree| fs::canonicalize(worktree).ok()).is_some_and(|worktree| worktree == path);
    let jobs: Vec<_> = saved_jobs(tasks_dir)
        .into_iter()
        .filter(|job| Some(job.header.job_id.as_str()) != ending)
        .filter(|job| names_it(job.header.worktree_path.as_ref()) || names_it(job.status.as_ref().and_then(|status| status.worktree_path.as_ref())))
        .collect();
    if jobs.is_empty() || jobs.iter().any(|job| job.status.is_none()) {
        return false;
    }
    let running: Vec<String> = crate::services::agent_active_jobs::list(tasks_dir)
        .into_iter()
        .filter(|job| Some(job.job_id.as_str()) != ending)
        .filter_map(|job| job.session_id)
        .collect();
    let statuses: Vec<_> = jobs.iter().filter_map(|job| job.status.as_ref()).collect();
    if statuses.iter().any(|status| status.session_id.as_ref().is_some_and(|session| running.contains(session))) {
        return false;
    }
    let threads: Vec<&str> = statuses.iter().filter_map(|status| status.agent_session.as_deref()).collect();
    if threads.is_empty() {
        return true;
    }
    let Some((program, env)) = t3_launch() else {
        return true;
    };
    let cwd = resolve_repo_root(tasks_dir).unwrap_or_else(|| tasks_dir.to_path_buf());
    !threads.iter().any(|thread| t3_thread_show(&program, &env, &cwd, thread).is_some_and(|shown| t3_thread_busy(&shown)))
}

/// Take over a job whose process exited before it ended (DEV-378): it runs
/// in this process from now on, so its record, status and end rules work
/// as if it had started here.
fn adopt_orphaned_job(tasks_dir: &std::path::Path, orphan: &crate::services::agent_active_jobs::ActiveJob) {
    let Ok(parsed) = crate::storage::TaskId::parse(&orphan.ticket_id) else {
        crate::services::agent_active_jobs::unregister(tasks_dir, &orphan.job_id);
        return;
    };
    let Ok(config) = crate::config::resolution::config_for_project(tasks_dir, Some(&parsed.project)) else {
        return;
    };
    let Some(job) = crate::services::agent_active_jobs::claim(tasks_dir, &orphan.job_id) else {
        return;
    };
    let ticket_id = parsed.canonical();
    let runner_kind = job.runner.parse::<AgentRunnerKind>().unwrap_or(AgentRunnerKind::Command);
    let workspace_root = tasks_dir.parent().map(|p| p.to_path_buf()).unwrap_or_else(|| tasks_dir.to_path_buf());
    let agent_logs_dir = crate::services::agent_log_service::effective_logs_dir(tasks_dir, config.agent_logs_dir.as_deref());
    // A job whose log has its final status ended; its process exited after
    // that, before it dropped the record (DEV-544).
    let ended = agent_logs_dir.as_deref().is_some_and(|dir| AgentLogService::load_status(&workspace_root, dir, &job.job_id).ok().flatten().is_some());
    if ended {
        crate::services::agent_active_jobs::unregister(tasks_dir, &job.job_id);
        return;
    }
    let profile = job
        .agent
        .as_deref()
        .and_then(|name| config.agent_profiles.get(name).cloned())
        .unwrap_or_else(|| AgentProfileDetail { runner: job.runner.clone(), ..Default::default() });
    let mut header = agent_logs_dir.as_deref().and_then(|dir| AgentLogService::load_header(&workspace_root, dir, &job.job_id).ok().flatten());
    // Killed during its worktree setup, it has no log yet; the takeover
    // writes one with what the record knows (DEV-598).
    if job.in_setup && header.is_none() {
        let setup_header = crate::services::agent_log_service::JobLogHeader {
            job_id: job.job_id.clone(),
            ticket_id: ticket_id.clone(),
            runner: runner_kind.as_str().to_string(),
            created_at: job.started_at.clone(),
            worktree_path: None,
            worktree_branch: None,
            model: profile.model.clone(),
            effort: profile.effort.clone(),
            agent_profile: job.agent.clone(),
        };
        if AgentLogService::init_log(&workspace_root, agent_logs_dir.as_deref(), setup_header.clone()).is_ok() {
            header = Some(setup_header);
        }
    }
    // Its events go on after the logged ones, so the log keeps one event per sequence (DEV-518).
    let logged_sequence = agent_logs_dir
        .as_deref()
        .and_then(|dir| AgentLogService::load_job(&workspace_root, dir, &job.job_id).ok().flatten())
        .and_then(|saved| saved_events(&saved).iter().map(|event| event.sequence).max())
        .unwrap_or(0);
    let is_merge_job = takes_over_as_merge_job(&job);
    {
        let Ok(mut registry) = JOB_REGISTRY.lock() else {
            return;
        };
        if registry.jobs.contains_key(&job.job_id) {
            return;
        }
        registry.active_by_ticket.insert(ticket_id.clone(), job.job_id.clone());
        prune_finished_jobs(&mut registry);
        registry.jobs.insert(
            job.job_id.clone(),
            AgentJobState {
                record: AgentJobRecord {
                    id: job.job_id.clone(),
                    ticket_id: ticket_id.clone(),
                    tasks_dir: tasks_dir.to_path_buf(),
                    workspace_root,
                    agent_logs_dir,
                    runner: runner_kind.as_str().to_string(),
                    agent_profile: job.agent.clone(),
                    status: AgentJobStatus::Running,
                    created_at: header.as_ref().map(|header| header.created_at.clone()).unwrap_or_else(|| job.started_at.clone()),
                    started_at: Some(job.started_at.clone()),
                    finished_at: None,
                    exit_code: None,
                    last_message: None,
                    summary: None,
                    session_id: job.session_id.clone(),
                    worktree_path: header.as_ref().and_then(|header| header.worktree_path.clone()),
                    worktree_branch: header.as_ref().and_then(|header| header.worktree_branch.clone()),
                    model: header.as_ref().and_then(|header| header.model.clone()),
                    effort: header.as_ref().and_then(|header| header.effort.clone()),
                    usage: None,
                    question: None,
                    is_merge_job,
                },
                events: Vec::new(),
                next_sequence: logged_sequence,
                runtime: None,
                cancellation_finalized: false,
                runner_failed: false,
                worker_running: true,
                main_head_at_start: None,
                assignee_at_start: restored_assignee(job.assignee_at_start.clone()),
                ticket_commit_at_start: None,
            },
        );
        registry.running_count += 1;
    }
    update_job(&job.job_id, |state| {
        let note =
            format!("The lotar process that ran this job (pid {}) exited before it ended; process {} took it over.", orphan.owner_pid, std::process::id());
        record_and_emit(state, "agent_job_progress", Some(note));
    });
    let tasks_dir = tasks_dir.to_path_buf();
    thread::spawn(move || finish_orphaned_job(&job, runner_kind, &profile, &tasks_dir, &ticket_id, &config));
}

/// End a taken-over job (DEV-378). A session job resumes its session in this
/// process and asks the agent to go on, as its turn ended with the process
/// that owned it; a `command` job fails, as its result went with its process.
fn finish_orphaned_job(
    job: &crate::services::agent_active_jobs::ActiveJob,
    runner_kind: AgentRunnerKind,
    profile: &AgentProfileDetail,
    tasks_dir: &std::path::Path,
    ticket_id: &str,
    config: &ResolvedConfig,
) {
    let job_id = job.job_id.as_str();
    let _slot = RunningJobSlot { job_id, ticket_id, tasks_dir };
    // Its process exited during the worktree setup, before any runner (DEV-598).
    if job.in_setup {
        let message = "The lotar process running the job exited while it set up the job's worktree, before the job started.".to_string();
        update_job_failure(job_id, ticket_id, message, tasks_dir);
        return;
    }
    let Some(agent) = harness_agent(runner_kind) else {
        let message = format!("The lotar process running the job exited before the {} runner finished, so its result is unknown.", job.runner);
        update_job_failure(job_id, ticket_id, message, tasks_dir);
        return;
    };
    // Its process exited before it started the session (DEV-593).
    let Some(name) = job.session_id.as_deref() else {
        let message = format!("The lotar process running the job exited before the job ended, before it started the {} session.", job.runner);
        update_job_failure(job_id, ticket_id, message, tasks_dir);
        return;
    };
    let why = "The lotar process running the job exited before its turn ended";
    resume_session_to_end(job_id, runner_kind, agent, name, profile, tasks_dir, ticket_id, config, why);
}

/// Resume the session `name` in this process and end the job with its turn:
/// a `t3` session follows its thread's run without sending a turn, others
/// ask the agent to go on. `why` starts the failure when it can't be resumed.
#[expect(clippy::too_many_arguments, reason = "the job's independent inputs from both of its takeovers")]
fn resume_session_to_end(
    job_id: &str,
    runner_kind: AgentRunnerKind,
    agent: &str,
    name: &str,
    profile: &AgentProfileDetail,
    tasks_dir: &std::path::Path,
    ticket_id: &str,
    config: &ResolvedConfig,
    why: &str,
) {
    let budget = profile.max_budget_usd;
    let session = (|| {
        let table = crate::services::agent_runner::job_agent_table(agent, profile)?;
        // The turn it finishes keeps the environment the job gave it.
        let env = crate::services::agent_runner::registry().load(name).map(|record| record.profile.env.into_iter().collect()).unwrap_or_default();
        let profile = crate::services::agent_runner::session_profile(profile, env)?;
        let cwd = resolve_repo_root(tasks_dir).unwrap_or_else(|| tasks_dir.to_path_buf());
        crate::services::agent_runner::open_session(name, agent, &cwd, &table, profile, true).map_err(LoTaRError::ValidationError)
    })();
    let session = match session {
        Ok(session) => session,
        Err(err) => {
            let message = format!("{why}, and its session {name} could not be resumed: {err}");
            update_job_failure(job_id, ticket_id, message, tasks_dir);
            return;
        }
    };
    if !publish_session(job_id, &session) {
        return;
    }
    let prompt = "The process that ran this job stopped before your turn ended. Continue the work from where you stopped, and finish it.";
    // harnessmux's t3 agent follows the thread's run instead of sending the prompt, as that run may still go on (DEV-510).
    let note = if runner_kind == AgentRunnerKind::T3 {
        format!("Resumed the session {name}; the job ends with its T3 Code thread's run.")
    } else {
        format!("Resumed the session {name}; asked the agent to continue.")
    };
    update_job(job_id, |state| {
        record_and_emit(state, "agent_job_progress", Some(note.clone()));
    });
    let mut context_messages = Vec::new();
    let end = drive_session(job_id, tasks_dir, &session, prompt, &[], budget, &mut context_messages);
    end_job(job_id, ticket_id, end, &context_messages, tasks_dir, config);
}

/// Hand a running job a message (DEV-384) as its session's next turn, from
/// whichever process sent it. `false` when the job can't take it (a
/// `command` runner, or no process owns its session).
pub fn tell_running_job(job: &crate::services::agent_active_jobs::ActiveJob, message: &str) -> bool {
    if job.owner_pid == std::process::id() {
        return AgentJobService::send_message(&job.job_id, message).is_ok();
    }
    job.session_id.as_deref().is_some_and(|name| crate::services::agent_runner::send_to_session(name, message))
}

fn resolve_repo_root(tasks_dir: &std::path::Path) -> Option<std::path::PathBuf> {
    crate::utils::git::find_repo_root(tasks_dir)
}

fn resolve_worktree_root(repo_root: &std::path::Path, configured: Option<&str>) -> LoTaRResult<std::path::PathBuf> {
    let base = repo_root.parent().unwrap_or(repo_root);
    if let Some(raw) = configured {
        let trimmed = raw.trim();
        if trimmed.is_empty() {
            return Err(LoTaRError::ValidationError("agent.worktree.dir cannot be empty".to_string()));
        }
        let path = std::path::Path::new(trimmed);
        if path.is_absolute() {
            return Ok(path.to_path_buf());
        }
        if path.components().any(|component| matches!(component, std::path::Component::ParentDir)) {
            return Err(LoTaRError::ValidationError("agent.worktree.dir cannot contain '..'".to_string()));
        }
        return Ok(base.join(path));
    }

    let repo_name = repo_root.file_name().and_then(|name| name.to_str()).unwrap_or("repo");
    Ok(base.join(".lotar-worktrees").join(repo_name))
}

fn sanitize_worktree_token(raw: &str) -> String {
    let mut out = String::new();
    for ch in raw.trim().chars() {
        if ch.is_ascii_alphanumeric() || ch == '-' || ch == '_' {
            out.push(ch);
        } else if ch.is_whitespace() || ch == '/' || ch == '\\' {
            out.push('-');
        }
    }
    if out.is_empty() { "ticket".to_string() } else { out }
}

fn build_worktree_branch(prefix: &str, suffix: &str) -> String {
    let trimmed = prefix.trim();
    let mut normalized = if trimmed.is_empty() { "agent/".to_string() } else { trimmed.to_string() };
    if !normalized.ends_with('/') {
        normalized.push('/');
    }
    normalized.push_str(suffix);
    normalized
}

fn branch_exists(repo_root: &std::path::Path, branch: &str) -> bool {
    let reference = format!("refs/heads/{}", branch);
    git_command(repo_root)
        .arg("show-ref")
        .arg("--verify")
        .arg("--quiet")
        .arg(reference)
        .current_dir(repo_root)
        .status()
        .map(|status| status.success())
        .unwrap_or(false)
}

fn run_git_command(repo_root: &std::path::Path, args: &[&str]) -> LoTaRResult<()> {
    git_stdout(repo_root, args).map(drop)
}

/// What a git command prints, or why it failed.
fn git_stdout(repo_root: &std::path::Path, args: &[&str]) -> LoTaRResult<String> {
    let output = git_command(repo_root).args(args).current_dir(repo_root).output().map_err(LoTaRError::IoError)?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(LoTaRError::ValidationError(format!("git {} failed: {}", args.join(" "), stderr.trim())));
    }
    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}

#[derive(Debug, Clone)]
struct WorktreeCleanupTarget {
    path: std::path::PathBuf,
    branch: Option<String>,
}

fn resolve_job_worktree(job_id: &str) -> Option<WorktreeCleanupTarget> {
    let registry = JOB_REGISTRY.lock().ok()?;
    let state = registry.jobs.get(job_id)?;
    let path = state.record.worktree_path.as_deref()?.trim();
    if path.is_empty() {
        return None;
    }
    Some(WorktreeCleanupTarget { path: std::path::PathBuf::from(path), branch: state.record.worktree_branch.clone() })
}

pub fn ticket_done_state(tasks_dir: &std::path::Path, ticket_id: &str) -> Option<bool> {
    let (prefix, number) = ticket_id.rsplit_once('-')?;
    if crate::storage::safety::validate_project_prefix(prefix).is_err() || number.parse::<u64>().is_err() {
        return None;
    }
    // DEV-21 review F1: classify against the policy of the tasks root that
    // ACTUALLY holds the ticket, not the issuing workspace — homonymous
    // prefixes across sibling roots carry different completion policies.
    // A missing or ambiguous identity stays indeterminate (None), which
    // conservatively skips cleanup; every destructive guard is unchanged.
    let storage = crate::storage::manager::Storage::new(tasks_dir);
    let root = match storage.resolve_task_location(ticket_id) {
        Ok(location) => location.root,
        Err(_) => return None,
    };
    let config = crate::config::resolution::config_for_project(root.as_path(), Some(prefix)).ok()?;
    // Cleanup must not use the tolerant task parser: damaged files are indeterminate.
    let path = crate::storage::operations::StorageOperations::get_file_path_for_id(&root.join(prefix), ticket_id)?;
    let raw = fs::read_to_string(path).ok()?;
    let task = serde_yaml_ng::from_str::<crate::storage::task::Task>(&raw).ok()?;
    if !config.issue_states.values.iter().any(|status| status.eq_ignore_case(task.status.as_str())) {
        return None;
    }
    Some(determine_done_statuses_from_config(&config).contains(&task.status.as_str().to_ascii_lowercase()))
}

/// Whether a finished ticket's worktree can be removed safely: it must be a
/// worktree of this repository, on its own branch, merged into the main
/// checkout, and hold no untracked or modified files. Git-ignored files
/// (build output) only keep it when `allow_ignored` is false.
pub(crate) fn check_worktree_cleanup(repo_root: &std::path::Path, path: &std::path::Path, branch: Option<&str>, allow_ignored: bool) -> LoTaRResult<()> {
    let (root, target) = check_worktree_clean(repo_root, path, allow_ignored)?;
    let branch = branch.ok_or_else(|| LoTaRError::ValidationError("Worktree branch is unknown".into()))?;
    let head = git_command(&target).args(["symbolic-ref", "--quiet", "HEAD"]).current_dir(&target).output().map_err(LoTaRError::IoError)?;
    if !head.status.success() || String::from_utf8_lossy(&head.stdout).trim() != format!("refs/heads/{branch}") {
        return Err(LoTaRError::ValidationError("Worktree branch changed or is unknown".into()));
    }
    if branch_landed(&root, branch)? {
        Ok(())
    } else {
        Err(LoTaRError::ValidationError(format!("{branch} has commits that are not in the main checkout's current branch")))
    }
}

/// Whether an agent worktree can be removed safely: on its branch as
/// `check_worktree_cleanup` says, or detached from it (DEV-476) with its
/// commit's work in the main checkout.
pub fn check_agent_worktree_cleanup(repo_root: &std::path::Path, worktree: &AgentWorktree, allow_ignored: bool) -> LoTaRResult<()> {
    if !worktree.detached {
        return check_worktree_cleanup(repo_root, &worktree.path, Some(&worktree.branch), allow_ignored);
    }
    let (root, target) = check_worktree_clean(repo_root, &worktree.path, allow_ignored)?;
    let head = git_command(&target).args(["symbolic-ref", "--quiet", "HEAD"]).current_dir(&target).output().map_err(LoTaRError::IoError)?;
    let commit = rev_parse(&target, "HEAD").filter(|_| !head.status.success());
    let commit = commit.ok_or_else(|| LoTaRError::ValidationError("Worktree is no longer detached".into()))?;
    if commit_landed(&root, &commit, &worktree.branch)? {
        Ok(())
    } else {
        Err(LoTaRError::ValidationError(format!("its commit {commit} has work that is not in the main checkout's current branch")))
    }
}

/// Why cleanup may remove a Done ticket's agent worktree or branch: its
/// work is in the main checkout, or main supersedes it (DEV-541).
pub enum Removal {
    Landed,
    /// A version of the ticket's work that differs from what landed, kept
    /// under `reference` before it goes.
    Superseded {
        reference: String,
        commit: String,
    },
}

impl Removal {
    /// Keep a superseded version's commit under `refs/archive/`.
    pub fn archive(&self, repo_root: &std::path::Path) -> LoTaRResult<()> {
        match self {
            Self::Landed => Ok(()),
            Self::Superseded { reference, commit } => run_git_command(repo_root, &["update-ref", reference, commit]),
        }
    }
}

/// How cleanup may remove an agent worktree of the Done ticket `ticket_id`
/// that no process runs in: as `check_agent_worktree_cleanup` allows, or as
/// a superseded version (`superseded_removal`) when it is clean and still on
/// its branch or detached commit.
pub fn check_done_worktree_cleanup(repo_root: &std::path::Path, worktree: &AgentWorktree, ticket_id: &str, allow_ignored: bool) -> LoTaRResult<Removal> {
    let removal = done_worktree_removal(repo_root, worktree, ticket_id, allow_ignored)?;
    if fs::canonicalize(&worktree.path).map_or(true, |target| process_runs_in(&target)) {
        return Err(LoTaRError::ValidationError("a process runs in it".into()));
    }
    Ok(removal)
}

fn done_worktree_removal(repo_root: &std::path::Path, worktree: &AgentWorktree, ticket_id: &str, allow_ignored: bool) -> LoTaRResult<Removal> {
    let unlanded = match check_agent_worktree_cleanup(repo_root, worktree, allow_ignored) {
        Ok(()) => return Ok(Removal::Landed),
        Err(err) => err,
    };
    let (root, target) = check_worktree_clean(repo_root, &worktree.path, allow_ignored)?;
    let head = git_command(&target).args(["symbolic-ref", "--quiet", "HEAD"]).current_dir(&target).output().map_err(LoTaRError::IoError)?;
    let unchanged = if worktree.detached {
        !head.status.success()
    } else {
        head.status.success() && String::from_utf8_lossy(&head.stdout).trim() == format!("refs/heads/{}", worktree.branch)
    };
    let commit = rev_parse(&target, "HEAD").filter(|_| unchanged);
    let label = if worktree.detached { "detached" } else { worktree.branch.as_str() };
    commit.and_then(|commit| superseded_removal(&root, &commit, ticket_id, label)).ok_or(unlanded)
}

/// How cleanup may delete `branch`, an agent branch no worktree has
/// checked out, with `done_ticket` naming its ticket when that is Done;
/// `None` keeps it.
pub fn branch_removal(repo_root: &std::path::Path, branch: &str, done_ticket: Option<&str>) -> Option<Removal> {
    if branch_landed(repo_root, branch).unwrap_or(false) {
        return Some(Removal::Landed);
    }
    superseded_removal(repo_root, &rev_parse(repo_root, &format!("refs/heads/{branch}"))?, done_ticket?, branch)
}

/// Whether main supersedes `commit`, an earlier version of the Done
/// ticket's work that never landed as it is (the merge agent rebased or
/// reworked it): a commit of the checkout's current branch since they
/// forked names the ticket first in its subject, as the merge agent's
/// commits do, and was committed later. Work committed since stays. It is
/// archived as `refs/archive/<ticket>/<label>/<commit>`.
fn superseded_removal(repo_root: &std::path::Path, commit: &str, ticket_id: &str, label: &str) -> Option<Removal> {
    let committed: u64 = git_stdout(repo_root, &["log", "-1", "--format=%ct", commit]).ok()?.trim().parse().ok()?;
    let base = git_stdout(repo_root, &["merge-base", "HEAD", commit]).ok()?;
    let log = git_stdout(repo_root, &["log", "--format=%ct %s", &format!("{}..HEAD", base.trim())]).ok()?;
    let landed = log.lines().filter_map(|line| line.split_once(' ')).any(|(time, subject)| {
        time.parse::<u64>().is_ok_and(|time| time > committed) && subject.split_whitespace().next().is_some_and(|word| word.eq_ignore_ascii_case(ticket_id))
    });
    landed.then(|| Removal::Superseded { reference: format!("refs/archive/{ticket_id}/{label}/{commit}"), commit: commit.to_string() })
}

/// Whether a process has its working directory in `dir` (canonical). Without
/// a process list, one is assumed to.
fn process_runs_in(dir: &std::path::Path) -> bool {
    let Ok(mut system) = sysinfo::System::new() else {
        return true;
    };
    let kind = sysinfo::ProcessRefreshKind::nothing().with_cwd(sysinfo::UpdateKind::Always);
    system.refresh_processes_specifics(sysinfo::ProcessesToUpdate::All, true, kind);
    system.processes().values().any(|process| process.cwd().is_some_and(|cwd| cwd.starts_with(dir)))
}

/// The repository root and the worktree, canonical, when the worktree is
/// another worktree of this repository without untracked or modified files.
fn check_worktree_clean(repo_root: &std::path::Path, path: &std::path::Path, allow_ignored: bool) -> LoTaRResult<(std::path::PathBuf, std::path::PathBuf)> {
    let root = fs::canonicalize(repo_root).map_err(LoTaRError::IoError)?;
    let target = fs::canonicalize(path).map_err(LoTaRError::IoError)?;
    if target == root {
        return Err(LoTaRError::ValidationError("Refusing to remove the repository checkout".into()));
    }
    if verified_common_dir(&root).map_err(LoTaRError::IoError)? != verified_common_dir(&target).map_err(LoTaRError::IoError)? {
        return Err(LoTaRError::ValidationError("Worktree belongs to a different repository".into()));
    }
    let mut status_args = vec!["status", "--porcelain", "--untracked-files=all"];
    if !allow_ignored {
        status_args.push("--ignored");
    }
    let status = git_command(&target).args(&status_args).current_dir(&target).output().map_err(LoTaRError::IoError)?;
    if !status.status.success() {
        return Err(LoTaRError::ValidationError("git status failed in the worktree".into()));
    }
    if !status.stdout.is_empty() {
        let first = String::from_utf8_lossy(&status.stdout).lines().next().unwrap_or_default().to_string();
        return Err(LoTaRError::ValidationError(format!(
            "it has uncommitted or {} files (for example `{}`)",
            if allow_ignored { "untracked" } else { "untracked or ignored" },
            first.get(3..).unwrap_or(&first)
        )));
    }
    Ok((root, target))
}

/// Whether the work on `branch` is in the checkout's current branch: merged,
/// landed rebased or cherry-picked (DEV-343), or landed reworked (DEV-366).
/// `git cherry` marks a commit HEAD has no equivalent of with `+`. Failing
/// that, the branch landed when a merge job recorded its current commit as
/// landed in a commit of HEAD (DEV-411), or when merging it into HEAD, or
/// into a commit since the fork that touched the branch's files, adds
/// nothing: that commit
/// already holds every change the branch makes. Main may have changed those
/// lines again since, which a merge into HEAD alone would see as a conflict.
fn branch_landed(repo_root: &std::path::Path, branch: &str) -> LoTaRResult<bool> {
    commit_landed(repo_root, &format!("refs/heads/{branch}"), branch)
}

/// Whether the work of `rev`, the branch or a commit a worktree detached
/// from `branch` holds, is in the checkout's current branch, as
/// `branch_landed` says. `branch`'s landing records count for the commit
/// they name.
fn commit_landed(repo_root: &std::path::Path, rev: &str, branch: &str) -> LoTaRResult<bool> {
    let git_output = |args: &[&str]| git_stdout(repo_root, args);
    if !git_output(&["cherry", "HEAD", rev])?.lines().any(|line| line.starts_with('+')) {
        return Ok(true);
    }
    if landing_recorded(repo_root, branch, rev) {
        return Ok(true);
    }
    let base = git_output(&["merge-base", "HEAD", rev])?;
    let base = base.trim();
    let changed = git_output(&["diff", "--name-only", "--no-renames", "-z", base, rev])?;
    let excluded = format!("^{base}");
    let mut args = vec!["rev-list", "HEAD", excluded.as_str(), "--"];
    args.extend(changed.split('\0').filter(|path| !path.is_empty()));
    let candidates = git_output(&args)?;
    Ok(std::iter::once("HEAD").chain(candidates.lines()).any(|commit| merge_adds_nothing(repo_root, commit, rev)))
}

/// Whether the changes `branch` makes outside `excluded` (a path relative to
/// the repository root) are in the checkout's current branch: merging the
/// branch into HEAD, or into a commit since the fork that touched those
/// files, changes nothing outside `excluded`. Conflicts inside it don't
/// count; a branch that changes nothing outside it hasn't landed.
fn branch_landed_outside(repo_root: &std::path::Path, branch: &str, excluded: &std::path::Path) -> LoTaRResult<bool> {
    let git_output = |args: &[&str]| git_stdout(repo_root, args);
    let branch_ref = format!("refs/heads/{branch}");
    let outside = format!(":(exclude){}", excluded.display());
    let base = git_output(&["merge-base", "HEAD", &branch_ref])?;
    let base = base.trim();
    let changed = git_output(&["diff", "--name-only", "--no-renames", "-z", base, &branch_ref, "--", ".", &outside])?;
    let changed: Vec<&str> = changed.split('\0').filter(|path| !path.is_empty()).collect();
    if changed.is_empty() {
        return Ok(false);
    }
    let excluded_base = format!("^{base}");
    let mut args = vec!["rev-list", "HEAD", excluded_base.as_str(), "--"];
    args.extend(&changed);
    let candidates = git_output(&args)?;
    Ok(std::iter::once("HEAD").chain(candidates.lines()).any(|commit| {
        // `merge-tree` exits 1 on a conflict and still writes the tree, with
        // conflict markers in the conflicted files.
        let merged = git_command(repo_root)
            .args(["merge-tree", "--write-tree", "--no-messages", commit, &branch_ref])
            .current_dir(repo_root)
            .output()
            .ok()
            .filter(|output| matches!(output.status.code(), Some(0 | 1)))
            .and_then(|output| String::from_utf8_lossy(&output.stdout).lines().next().map(str::to_string));
        merged.is_some_and(|tree| {
            git_command(repo_root)
                .args(["diff", "--quiet", commit, &tree, "--", ".", &outside])
                .current_dir(repo_root)
                .status()
                .is_ok_and(|status| status.success())
        })
    }))
}

/// Whether merging `branch` into `commit` yields `commit`'s own tree. A
/// conflict, or a git without `merge-tree --write-tree`, counts as no.
fn merge_adds_nothing(repo_root: &std::path::Path, commit: &str, branch: &str) -> bool {
    let tree = |args: &[&str]| {
        git_command(repo_root)
            .args(args)
            .current_dir(repo_root)
            .output()
            .ok()
            .filter(|output| output.status.success())
            .and_then(|output| String::from_utf8_lossy(&output.stdout).lines().next().map(str::to_string))
    };
    let merged = tree(&["merge-tree", "--write-tree", "--no-messages", commit, branch]);
    merged.is_some() && merged == tree(&["rev-parse", &format!("{commit}^{{tree}}")])
}

/// The commit `rev` names, if git knows it.
fn rev_parse(repo_root: &std::path::Path, rev: &str) -> Option<String> {
    let output = git_command(repo_root).args(["rev-parse", "--verify", "--quiet", rev]).current_dir(repo_root).output().ok()?;
    output.status.success().then(|| String::from_utf8_lossy(&output.stdout).trim().to_string()).filter(|commit| !commit.is_empty())
}

/// Where a merge job records that `branch` landed (DEV-411):
/// `refs/lotar/landed/<branch>/<branch commit>` points at main's commit
/// once the job ended. Named after the commit, so a branch that moved
/// since has no record.
fn landing_ref(branch: &str, commit: &str) -> String {
    format!("refs/lotar/landed/{branch}/{commit}")
}

/// Whether a merge job recorded `rev`'s commit as a landed commit of
/// `branch`, and the main commit it landed as is in the checkout's current
/// branch. Work a merge job reworked so it conflicts with the branch
/// (DEV-343's branch) counts as landed this way only.
fn landing_recorded(repo_root: &std::path::Path, branch: &str, rev: &str) -> bool {
    let Some(tip) = rev_parse(repo_root, rev) else {
        return false;
    };
    let Some(landed) = rev_parse(repo_root, &landing_ref(branch, &tip)) else {
        return false;
    };
    git_command(repo_root).args(["merge-base", "--is-ancestor", &landed, "HEAD"]).current_dir(repo_root).status().is_ok_and(|status| status.success())
}

/// Record that the branch's current commit landed as main's `HEAD`.
fn record_landing(repo_root: &std::path::Path, branch: &str) -> Option<String> {
    let tip = rev_parse(repo_root, &format!("refs/heads/{branch}"))?;
    let head = rev_parse(repo_root, "HEAD")?;
    run_git_command(repo_root, &["update-ref", &landing_ref(branch, &tip), &head]).ok()?;
    Some(head)
}

/// Drop the landing records of a deleted branch.
pub(crate) fn forget_landings(repo_root: &std::path::Path, branch: &str) {
    let Ok(output) =
        git_command(repo_root).args(["for-each-ref", "--format=%(refname)", &format!("refs/lotar/landed/{branch}/")]).current_dir(repo_root).output()
    else {
        return;
    };
    for reference in String::from_utf8_lossy(&output.stdout).lines() {
        let _ = run_git_command(repo_root, &["update-ref", "-d", reference]);
    }
}

/// The commits a merge job added to main since `start` that land the
/// ticket's branch reworked: their subject starts with the ticket ID, and
/// together they change every file the branch changes. Empty when any
/// file is left out, or when git can't tell.
fn reworked_landing(repo_root: &std::path::Path, start: &str, branch: &str, ticket_id: &str) -> Vec<String> {
    let git_output = |args: &[&str]| git_stdout(repo_root, args).ok();
    let branch_ref = format!("refs/heads/{branch}");
    let Some(base) = git_output(&["merge-base", "HEAD", &branch_ref]) else {
        return Vec::new();
    };
    let Some(changed) = git_output(&["diff", "--name-only", "--no-renames", "-z", base.trim(), &branch_ref]) else {
        return Vec::new();
    };
    let range = format!("{start}..HEAD");
    let Some(log) = git_output(&["log", "--format=%H %s", &range]) else {
        return Vec::new();
    };
    let commits: Vec<String> = log
        .lines()
        .filter_map(|line| line.split_once(' '))
        .filter(|(_, subject)| subject.split_whitespace().next().is_some_and(|word| word.eq_ignore_ascii_case(ticket_id)))
        .map(|(commit, _)| commit.to_string())
        .collect();
    let mut touched = std::collections::HashSet::new();
    for commit in &commits {
        let Some(files) = git_output(&["diff-tree", "--no-commit-id", "--name-only", "--no-renames", "-r", "-z", commit]) else {
            return Vec::new();
        };
        touched.extend(files.split('\0').filter(|path| !path.is_empty()).map(str::to_string));
    }
    let mut changed = changed.split('\0').filter(|path| !path.is_empty()).peekable();
    if changed.peek().is_none() || !changed.all(|path| touched.contains(path)) {
        return Vec::new();
    }
    commits
}

/// Remove a worktree `check_worktree_cleanup` passed, and its branch when
/// `delete_branch` is set. The branch's commits are in the main checkout,
/// possibly rebased, which `git branch -d` would refuse.
fn remove_worktree(repo_root: &std::path::Path, path: &std::path::Path, branch: Option<&str>, delete_branch: bool, prefix: &str) {
    let worktree_arg = path.to_string_lossy().to_string();
    if run_git_command(repo_root, &["worktree", "remove", &worktree_arg]).is_ok()
        && delete_branch
        && let Some(branch) = branch
    {
        let _ = delete_landed_branch(repo_root, prefix, branch);
    }
}

/// A worktree of an agent ticket branch (DEV-476): it has the ticket
/// branch or a T3 Code thread branch created from it checked out, or was
/// detached from one when a later thread took over the branch.
#[derive(Debug, Clone)]
pub struct AgentWorktree {
    pub path: std::path::PathBuf,
    /// The branch it has checked out, or the one it was detached from.
    pub branch: String,
    pub detached: bool,
    /// The ticket branch (`agent/DEV-1`) `branch` belongs to.
    pub ticket_branch: String,
}

/// Which ticket branch each local branch belongs to (DEV-476). T3 Code
/// creates a thread's branch (`t3/<topic>`) from the ticket branch or from
/// an earlier thread's branch, which the branch reflogs record (`branch:
/// Created from agent/DEV-1`); renaming a branch keeps its reflog. Deleting
/// a branch deletes its reflog, so before cleanup deletes one, the ticket
/// of each branch traced through it is recorded in the branch's config
/// (`branch.<name>.lotarticket`), which git drops with the branch.
struct BranchOwners {
    prefix: String,
    /// The branch each branch was created from.
    parents: HashMap<String, String>,
    /// The recorded ticket branches.
    recorded: HashMap<String, String>,
}

impl BranchOwners {
    fn load(repo_root: &std::path::Path, prefix: &str) -> Self {
        let git_lines = |args: &[&str]| {
            let output = git_command(repo_root).args(args).current_dir(repo_root).output();
            output.map(|output| String::from_utf8_lossy(&output.stdout).into_owned()).unwrap_or_default()
        };
        let mut parents = HashMap::new();
        for line in git_lines(&["log", "--walk-reflogs", "--format=%gD%x09%gs", "--branches"]).lines() {
            let Some((selector, subject)) = line.split_once('\t') else {
                continue;
            };
            if let (Some((branch, _)), Some(parent)) = (selector.rsplit_once("@{"), subject.strip_prefix("branch: Created from ")) {
                parents.insert(branch.to_string(), parent.trim().to_string());
            }
        }
        let recorded = git_lines(&["config", "--get-regexp", r"^branch\..*\.lotarticket$"])
            .lines()
            .filter_map(|line| {
                let (key, ticket) = line.split_once(' ')?;
                let branch = key.strip_prefix("branch.")?.strip_suffix(".lotarticket")?;
                Some((branch.to_string(), ticket.trim().to_string()))
            })
            .collect();
        Self { prefix: build_worktree_branch(prefix, ""), parents, recorded }
    }

    /// The ticket branch `branch` belongs to: itself when it has the agent
    /// branch prefix, the recorded one, or the first ticket branch it was
    /// created from; with the branches traced to get there, `branch` first.
    fn trace(&self, branch: &str) -> Option<(String, Vec<String>)> {
        let mut current = branch;
        let mut traced = Vec::new();
        for _ in 0..=self.parents.len() {
            if current.starts_with(&self.prefix) {
                return Some((current.to_string(), traced));
            }
            traced.push(current.to_string());
            if let Some(ticket) = self.recorded.get(current) {
                return Some((ticket.clone(), traced));
            }
            current = self.parents.get(current)?;
        }
        None
    }

    fn ticket_branch_of(&self, branch: &str) -> Option<String> {
        self.trace(branch).map(|(ticket, _)| ticket)
    }
}

/// The branch a detached worktree was on: the newest `checkout: moving
/// from <branch> to` entry of its HEAD reflog.
fn detached_from(worktree: &std::path::Path) -> Option<String> {
    let output = git_command(worktree).args(["log", "--walk-reflogs", "--format=%gs", "HEAD"]).current_dir(worktree).output().ok()?;
    let log = String::from_utf8_lossy(&output.stdout);
    let (branch, _) = log.lines().find_map(|line| line.strip_prefix("checkout: moving from "))?.split_once(" to ")?;
    Some(branch.to_string())
}

/// Where the ticket branch of a detached worktree is recorded once the
/// branch it was detached from is deleted: a file in the worktree's own
/// git directory, which git removes with the worktree.
fn detached_ticket_record(worktree: &std::path::Path) -> Option<std::path::PathBuf> {
    let output = git_command(worktree).args(["rev-parse", "--absolute-git-dir"]).current_dir(worktree).output().ok()?;
    output.status.success().then(|| std::path::PathBuf::from(String::from_utf8_lossy(&output.stdout).trim()).join("lotar-ticket-branch"))
}

/// The repository's worktrees other than the main checkout, each with the
/// branch it has checked out, or `None` when detached.
fn linked_worktrees(repo_root: &std::path::Path) -> LoTaRResult<Vec<(std::path::PathBuf, Option<String>)>> {
    let output = git_command(repo_root).args(["worktree", "list", "--porcelain"]).current_dir(repo_root).output().map_err(LoTaRError::IoError)?;
    if !output.status.success() {
        return Err(LoTaRError::ValidationError(format!("git worktree list failed: {}", String::from_utf8_lossy(&output.stderr).trim())));
    }
    let mut worktrees = Vec::new();
    for block in String::from_utf8_lossy(&output.stdout).split("\n\n").skip(1) {
        let mut lines = block.lines();
        let Some(path) = lines.next().and_then(|line| line.strip_prefix("worktree ")) else {
            continue;
        };
        let mut branch = None;
        let mut detached = false;
        for line in lines {
            branch = branch.or_else(|| line.strip_prefix("branch refs/heads/").map(str::to_string));
            detached |= line == "detached";
        }
        if branch.is_some() || detached {
            worktrees.push((std::path::PathBuf::from(path), branch));
        }
    }
    Ok(worktrees)
}

/// The worktrees of agent ticket branches (`prefix` names them), detached
/// ones included.
pub fn agent_worktrees(repo_root: &std::path::Path, prefix: &str) -> LoTaRResult<Vec<AgentWorktree>> {
    let owners = BranchOwners::load(repo_root, prefix);
    Ok(linked_worktrees(repo_root)?
        .into_iter()
        .filter_map(|(path, branch)| {
            let detached = branch.is_none();
            let branch = branch.or_else(|| detached_from(&path))?;
            let recorded = detached.then(|| detached_ticket_record(&path)).flatten().and_then(|record| fs::read_to_string(record).ok());
            let ticket_branch = recorded.map(|ticket| ticket.trim().to_string()).or_else(|| owners.ticket_branch_of(&branch))?;
            Some(AgentWorktree { path, branch, detached, ticket_branch })
        })
        .collect())
}

/// The agent branches no worktree has checked out, as (ticket branch,
/// branch): ticket branches and the T3 Code thread branches created from
/// them (DEV-476). With `ticket_branch`, only that ticket's.
pub fn idle_agent_branches(repo_root: &std::path::Path, prefix: &str, ticket_branch: Option<&str>) -> Vec<(String, String)> {
    let Ok(worktrees) = linked_worktrees(repo_root) else {
        return Vec::new();
    };
    let Ok(output) = git_command(repo_root).args(["for-each-ref", "--format=%(refname:short)", "refs/heads/"]).current_dir(repo_root).output() else {
        return Vec::new();
    };
    let main = git_command(repo_root).args(["symbolic-ref", "--quiet", "--short", "HEAD"]).current_dir(repo_root).output().ok();
    let main = main.map(|output| String::from_utf8_lossy(&output.stdout).trim().to_string());
    let owners = BranchOwners::load(repo_root, prefix);
    String::from_utf8_lossy(&output.stdout)
        .lines()
        .filter(|branch| main.as_deref() != Some(*branch) && !worktrees.iter().any(|(_, checked_out)| checked_out.as_deref() == Some(*branch)))
        .filter_map(|branch| Some((owners.ticket_branch_of(branch)?, branch.to_string())))
        .filter(|(ticket, _)| ticket_branch.is_none_or(|wanted| wanted == ticket))
        .collect()
}

/// Delete a branch whose work is in the main checkout, possibly rebased,
/// which `git branch -d` would refuse, and its landing records. The
/// branches and detached worktrees traced to their ticket through it get
/// their ticket branch recorded first, so a later cleanup still finds them
/// (`BranchOwners`).
pub fn delete_landed_branch(repo_root: &std::path::Path, prefix: &str, branch: &str) -> LoTaRResult<()> {
    let owners = BranchOwners::load(repo_root, prefix);
    let through = |traced: &str| owners.trace(traced).filter(|(_, path)| path.iter().any(|step| step == branch)).map(|(ticket, _)| ticket);
    for (other, ticket) in owners.parents.keys().filter(|other| *other != branch).filter_map(|other| Some((other, through(other)?))) {
        run_git_command(repo_root, &["config", &format!("branch.{other}.lotarticket"), &ticket])?;
    }
    for (path, _) in linked_worktrees(repo_root)?.into_iter().filter(|(_, checked_out)| checked_out.is_none()) {
        if let (Some(record), Some(ticket)) = (detached_ticket_record(&path), detached_from(&path).and_then(|from| through(&from)))
            && !record.exists()
        {
            fs::write(record, ticket).map_err(LoTaRError::IoError)?;
        }
    }
    run_git_command(repo_root, &["branch", "-D", branch])?;
    forget_landings(repo_root, branch);
    Ok(())
}

/// A ticket that reached Done loses its agent worktrees (DEV-343), whoever
/// set Done: an earlier phase ends with the ticket handed on, and the
/// merge job runs in a worktree of its own, so cleanup at the end of a job
/// alone leaves the ticket branch's worktree behind. That includes the
/// worktrees of its T3 Code thread branches and those detached when a
/// later thread took over the branch, and the branches themselves
/// (DEV-476). Versions main supersedes go too, archived (DEV-541); any
/// other work missing from main, uncommitted changes or a process running
/// in a worktree keep it. Waits while a job runs for the ticket; its end
/// calls this again.
pub(crate) fn cleanup_done_ticket_worktree(tasks_dir: &std::path::Path, ticket_id: &str) {
    if ticket_done_state(tasks_dir, ticket_id) != Some(true) {
        return;
    }
    let Some((prefix, _)) = ticket_id.rsplit_once('-') else {
        return;
    };
    let Ok(config) = crate::config::resolution::config_for_project(tasks_dir, Some(prefix)) else {
        return;
    };
    if !config.agent_worktree.cleanup_on_done {
        return;
    }
    let Some(repo_root) = resolve_repo_root(tasks_dir) else {
        return;
    };
    let branch_prefix = &config.agent_worktree.branch_prefix;
    let branch = build_worktree_branch(branch_prefix, &sanitize_worktree_token(ticket_id));
    let Ok(worktrees) = agent_worktrees(&repo_root, branch_prefix) else {
        return;
    };
    // The claim keeps a job from starting in the worktree meanwhile.
    let Ok(registry) = JOB_REGISTRY.lock() else {
        return;
    };
    let (registry, _claim) = CleanupClaim::take(registry, ticket_id);
    let busy = registry.active_by_ticket.contains_key(ticket_id) || crate::services::agent_active_jobs::for_ticket_elsewhere(tasks_dir, ticket_id).is_some();
    drop(registry);
    if busy {
        return;
    }
    let delete_branches = config.agent_worktree.cleanup_delete_branches;
    for worktree in worktrees.iter().filter(|worktree| worktree.ticket_branch == branch) {
        let Ok(removal) = check_done_worktree_cleanup(&repo_root, worktree, ticket_id, config.agent_worktree.cleanup_ignored_files) else {
            continue;
        };
        // A superseded branch kept without its worktree still holds its commit.
        let keeps_commit = !worktree.detached && !delete_branches;
        if keeps_commit || removal.archive(&repo_root).is_ok() {
            remove_worktree(&repo_root, &worktree.path, (!worktree.detached).then_some(worktree.branch.as_str()), delete_branches, branch_prefix);
        }
    }
    if delete_branches {
        for (_, idle) in idle_agent_branches(&repo_root, branch_prefix, Some(&branch)) {
            if branch_removal(&repo_root, &idle, Some(ticket_id)).is_some_and(|removal| removal.archive(&repo_root).is_ok()) {
                let _ = delete_landed_branch(&repo_root, branch_prefix, &idle);
            }
        }
    }
}

/// A job ended: retry `cleanup_done_ticket_worktree` for every Done ticket
/// that still has an agent worktree (DEV-574). A T3 Code thread's process
/// often still runs in a worktree when its ticket goes Done, which keeps
/// the worktree and its build output; nothing else would retry it.
fn cleanup_done_worktrees(tasks_dir: &std::path::Path, config: &ResolvedConfig) {
    let Some(repo_root) = resolve_repo_root(tasks_dir) else {
        return;
    };
    let prefix = &config.agent_worktree.branch_prefix;
    let Ok(worktrees) = agent_worktrees(&repo_root, prefix) else {
        return;
    };
    let tickets: std::collections::BTreeSet<_> = worktrees
        .iter()
        .map(|worktree| worktree.ticket_branch.strip_prefix(prefix.as_str()).unwrap_or(&worktree.ticket_branch).trim_start_matches('/').to_string())
        .collect();
    for ticket_id in tickets {
        cleanup_done_ticket_worktree(tasks_dir, &ticket_id);
    }
}

enum JobOutcome {
    Success,
    Failure,
    Cancelled,
}

#[expect(clippy::needless_pass_by_value, reason = "the caller hands this value over and does not use it afterwards")]
fn maybe_cleanup_worktree(job_id: &str, ticket_id: &str, tasks_dir: &std::path::Path, config: &ResolvedConfig, outcome: JobOutcome) {
    let should_cleanup = match outcome {
        JobOutcome::Success => config.agent_worktree.cleanup_on_done,
        JobOutcome::Failure => config.agent_worktree.cleanup_on_failure,
        JobOutcome::Cancelled => config.agent_worktree.cleanup_on_cancel,
    };
    if !should_cleanup || ticket_done_state(tasks_dir, ticket_id) != Some(true) {
        return;
    }
    let Some(target) = resolve_job_worktree(job_id).filter(|target| target.path.exists()) else {
        cleanup_done_ticket_worktree(tasks_dir, ticket_id);
        return;
    };
    let repo_root = match resolve_repo_root(tasks_dir) {
        Some(root) => root,
        None => return,
    };
    // Do not race a follow-up phase that now owns this ticket's worktree.
    let Ok(registry) = JOB_REGISTRY.lock() else {
        return;
    };
    let (registry, claim) = CleanupClaim::take(registry, ticket_id);
    let taken_over = registry.active_by_ticket.get(ticket_id).is_some_and(|id| id != job_id);
    drop(registry);
    if taken_over {
        return;
    }
    if let Err(err) = check_worktree_cleanup(&repo_root, &target.path, target.branch.as_deref(), config.agent_worktree.cleanup_ignored_files) {
        let reason = match err {
            LoTaRError::ValidationError(reason) => reason,
            other => other.to_string(),
        };
        let note = format!("Kept the worktree {}: {reason}. Remove it with `lotar agent worktree cleanup` once it is safe.", target.path.display());
        if let Ok(mut registry) = JOB_REGISTRY.lock()
            && let Some(state) = registry.jobs.get_mut(job_id)
        {
            record_and_emit(state, "agent_job_progress", Some(note));
        }
    } else {
        remove_worktree(
            &repo_root,
            &target.path,
            target.branch.as_deref(),
            config.agent_worktree.cleanup_delete_branches,
            &config.agent_worktree.branch_prefix,
        );
    }
    drop(claim);
    flush_event_log();
    cleanup_done_ticket_worktree(tasks_dir, ticket_id);
}

/// A job ended: delete `agent.worktree.cleanup_paths` (build output such as
/// `target`) from its worktree, which stays (DEV-364). Finished worktrees
/// otherwise keep gigabytes each until cleanup removes them, and a job that
/// ends with its ticket open keeps its worktree on purpose. Skipped while
/// another job runs for the ticket, as it shares the worktree. Only paths
/// git ignores and tracks nothing under are deleted.
fn remove_build_output(job_id: &str, ticket_id: &str, tasks_dir: &std::path::Path, config: &ResolvedConfig) {
    let paths = &config.agent_worktree.cleanup_paths;
    if paths.is_empty() {
        return;
    }
    let Some(target) = resolve_job_worktree(job_id).filter(|target| target.path.exists()) else {
        return;
    };
    let Some(repo_root) = resolve_repo_root(tasks_dir) else {
        return;
    };
    let (Ok(root), Ok(worktree)) = (fs::canonicalize(&repo_root), fs::canonicalize(&target.path)) else {
        return;
    };
    if worktree == root || verified_common_dir(&root).ok().is_none_or(|common| verified_common_dir(&worktree).ok() != Some(common)) {
        return;
    }
    let Some(git_dir) = git_command(&worktree)
        .args(["rev-parse", "--absolute-git-dir"])
        .current_dir(&worktree)
        .output()
        .ok()
        .filter(|output| output.status.success())
        .map(|output| std::path::PathBuf::from(String::from_utf8_lossy(&output.stdout).trim()))
    else {
        return;
    };
    // The claim keeps a job from starting in the worktree while its build
    // output moves out; the slow delete runs after the release.
    let Ok(registry) = JOB_REGISTRY.lock() else {
        return;
    };
    let (registry, claim) = CleanupClaim::take(registry, ticket_id);
    let busy = registry.active_by_ticket.get(ticket_id).is_some_and(|id| id != job_id)
        || crate::services::agent_active_jobs::for_ticket_elsewhere(tasks_dir, ticket_id).is_some();
    drop(registry);
    if busy {
        return;
    }
    let mut moved = Vec::new();
    let mut removed = Vec::new();
    for (index, path) in paths.iter().enumerate() {
        if crate::config::types::validate_cleanup_path(path).is_err() || !build_output_removable(&worktree, path) {
            continue;
        }
        let full = worktree.join(path);
        // Moved into the worktree's git dir: same file system, invisible to
        // git and to a job that starts next.
        let parked = git_dir.join(format!("lotar-cleanup-{job_id}-{index}"));
        if fs::rename(&full, &parked).is_ok() {
            moved.push(parked);
            removed.push(path.as_str());
        } else if remove_path(&full).is_ok() {
            removed.push(path.as_str());
        }
    }
    drop(claim);
    if !removed.is_empty()
        && let Ok(mut registry) = JOB_REGISTRY.lock()
        && let Some(state) = registry.jobs.get_mut(job_id)
    {
        let list = removed.iter().map(|path| format!("`{path}`")).collect::<Vec<_>>().join(", ");
        record_and_emit(state, "agent_job_progress", Some(format!("Deleted build output {list} from the worktree {}.", target.path.display())));
    }
    flush_event_log();
    for parked in moved {
        let _ = remove_path(&parked);
    }
}

/// Whether `path` in the worktree exists, is not a symlink, is ignored by
/// git and holds no tracked files.
fn build_output_removable(worktree: &std::path::Path, path: &str) -> bool {
    let Ok(metadata) = fs::symlink_metadata(worktree.join(path)) else {
        return false;
    };
    if metadata.file_type().is_symlink() {
        return false;
    }
    let git = |args: &[&str]| git_command(worktree).args(args).current_dir(worktree).output().ok();
    let ignored = git(&["check-ignore", "--quiet", "--", path]).is_some_and(|output| output.status.success());
    ignored && git(&["ls-files", "--", path]).is_some_and(|output| output.status.success() && output.stdout.is_empty())
}

fn remove_path(path: &std::path::Path) -> std::io::Result<()> {
    if fs::symlink_metadata(path)?.is_dir() { fs::remove_dir_all(path) } else { fs::remove_file(path) }
}

/// Initializes the job's persistent log file (if logging is enabled).
#[expect(clippy::too_many_arguments, reason = "the log header's independent fields from run_job")]
fn init_job_log(
    job_id: &str,
    runner_kind: AgentRunnerKind,
    profile: &AgentProfileDetail,
    tasks_dir: &std::path::Path,
    ticket_id: &str,
    config: &ResolvedConfig,
    created_at: &str,
    worktree_path: Option<String>,
    worktree_branch: Option<String>,
) {
    let workspace_root = tasks_dir.parent().map(|p| p.to_path_buf()).unwrap_or_else(|| tasks_dir.to_path_buf());
    let logs_dir = crate::services::agent_log_service::effective_logs_dir(tasks_dir, config.agent_logs_dir.as_deref());
    let _ = AgentLogService::init_log(
        &workspace_root,
        logs_dir.as_deref(),
        crate::services::agent_log_service::JobLogHeader {
            job_id: job_id.to_string(),
            ticket_id: ticket_id.to_string(),
            runner: runner_kind.as_str().to_string(),
            created_at: created_at.to_string(),
            worktree_path,
            worktree_branch,
            model: profile.model.clone(),
            effort: profile.effort.clone(),
            agent_profile: JOB_REGISTRY.lock().ok().and_then(|registry| registry.jobs.get(job_id).and_then(|state| state.record.agent_profile.clone())),
        },
    );
}

#[expect(
    clippy::too_many_arguments,
    clippy::needless_pass_by_value,
    reason = "job state moved into the worker thread; independent inputs from the single caller; bundling them only for this lint would not clarify the call"
)]
fn run_job(
    job_id: String,
    runner_kind: AgentRunnerKind,
    mut profile: AgentProfileDetail,
    tasks_dir: std::path::PathBuf,
    ticket_id: String,
    mut prompt: String,
    mut user_prompt: String,
    config: ResolvedConfig,
) {
    update_job(&job_id, |state| state.worker_running = true);
    let _slot = RunningJobSlot { job_id: &job_id, ticket_id: &ticket_id, tasks_dir: &tasks_dir };
    if should_stop_job(&job_id) {
        return;
    }
    // A process killed during the worktree setup leaves this record, so the
    // next worker or server ends the job (DEV-598).
    let setup_record = JOB_REGISTRY.lock().ok().and_then(|registry| {
        registry
            .jobs
            .get(&job_id)
            .map(|state| crate::services::agent_active_jobs::ActiveJob { in_setup: true, ..active_record_of(state, &ticket_id, &time::now_rfc3339(), None) })
    });
    if let Some(record) = setup_record.as_ref() {
        crate::services::agent_active_jobs::register(&tasks_dir, record);
    }
    let mut followed_note = match follow_ticket_work(&tasks_dir, &ticket_id, &config) {
        Ok(note) => note,
        Err(err) => {
            update_job_failure(&job_id, &ticket_id, format!("Worktree setup failed: {err}"), &tasks_dir);
            return;
        }
    };
    // A merge job of a repository with `agent.merge` merges without an
    // agent first (DEV-605); the agent starts only on a hand-off, in a new
    // session that reads why.
    let mut merge_started_at = None;
    if let Some(settings) = config.agent_merge.as_ref().filter(|_| merge::is_merge_job(&job_id)) {
        if crate::config::trust::TrustService::allows_commands(&tasks_dir) {
            let started_at = time::now_rfc3339();
            if !start_job(&job_id, runner_kind, &profile, &tasks_dir, &ticket_id, &config, &started_at, None) {
                return;
            }
            if let Some(note) = followed_note.take() {
                update_job(&job_id, |state| {
                    record_and_emit(state, "agent_job_progress", Some(note.clone()));
                });
            }
            let summary = match merge::run(&job_id, &tasks_dir, &ticket_id, &config, settings) {
                merge::Outcome::Cancelled => return,
                merge::Outcome::Completed(summary) => {
                    let branch = build_worktree_branch(&config.agent_worktree.branch_prefix, &sanitize_worktree_token(&ticket_id));
                    update_job(&job_id, |state| state.record.worktree_branch = Some(branch.clone()));
                    comment_merge(&job_id, &tasks_dir, &ticket_id, &summary);
                    finalize_job(&job_id, &ticket_id, Some(0), &[], &tasks_dir, &config);
                    return;
                }
                merge::Outcome::HandOff(summary) => summary,
            };
            comment_merge(&job_id, &tasks_dir, &ticket_id, &summary);
            let hand_off = merge::hand_off_prompt(&summary);
            prompt.insert_str(0, &hand_off);
            user_prompt.insert_str(0, &hand_off);
            profile.session = Some(AgentSession::Fresh);
            merge_started_at = Some(started_at);
        } else {
            let note = crate::config::trust::TrustService::untrusted_message(&tasks_dir, "lotar's own merge (agent.merge)");
            update_job(&job_id, |state| {
                record_and_emit(state, "agent_job_progress", Some(note.clone()));
            });
        }
    }
    let agent = harness_agent(runner_kind);
    let agent_profile = JOB_REGISTRY.lock().ok().and_then(|registry| registry.jobs.get(&job_id).and_then(|state| state.record.agent_profile.clone()));
    let project = crate::services::agent_runner::session_project(&tasks_dir);
    let own_name = session_name(&project, &ticket_id, agent_profile.as_deref());
    // The session a `session: continue` job takes its next turn in, or why
    // it starts a new one (for the job log).
    let (mut continued, mut session_reason) = match (profile.session == Some(AgentSession::Continue), agent) {
        (false, _) => (None, None),
        (true, None) => (None, Some("The command runner has no session to continue; this job starts the command as usual.".to_string())),
        (true, Some(agent)) => match crate::services::agent_runner::continuable_session(
            &crate::services::agent_runner::session_to_continue(&project, &ticket_id, agent_profile.as_deref()),
            agent,
            &tasks_dir,
        ) {
            Ok(record) => (Some(record), None),
            Err(reason) => (None, Some(reason)),
        },
    };
    // A session idle past the agent's prompt cache costs a full cache write
    // to resume; a new one with a handoff is cheaper (DEV-603).
    let mut handoff_from = None;
    if let Some(record) = continued.as_ref()
        && let Some(idle) = idle_since(&record.last_used)
    {
        let limit = profile.continue_within_minutes.unwrap_or(DEFAULT_CONTINUE_WITHIN_MINUTES);
        let minutes = idle.as_mins();
        if idle >= SignedDuration::from_mins(i64::try_from(limit).unwrap_or(i64::MAX)) {
            session_reason = Some(format!(
                "The session {} was last used {minutes} minutes ago, not within continue_within_minutes ({limit}); this job starts a new one with a handoff.",
                record.name
            ));
            handoff_from = continued.take();
        } else {
            session_reason =
                Some(format!("Continuing the session {}, last used {minutes} minutes ago (within continue_within_minutes, {limit}).", record.name));
        }
    }
    let t3_worktrees = runner_kind == AgentRunnerKind::T3 && config.agent_worktree.enabled;
    let mut thread_worktree = None;
    if t3_worktrees && continued.is_some() {
        let (session, thread) = continued.as_ref().map(|record| (record.name.clone(), record.agent_session.clone())).unwrap_or_default();
        match continued_thread_worktree(&tasks_dir, &ticket_id, &config, &session, thread.as_deref()) {
            Ok(found) => thread_worktree = Some(found),
            Err(reason) => {
                continued = None;
                session_reason = Some(reason);
            }
        }
    }
    // A resumed session runs on the model and effort it started with, also
    // a merge that continues the reviewer's (DEV-672): the job shows those.
    if let Some(record) = continued.as_ref() {
        profile.model = record.profile.model.clone();
        profile.effort = record.profile.effort.map(|effort| effort.as_str().to_string());
        update_job(&job_id, |state| {
            state.record.model = profile.model.clone();
            state.record.effort = profile.effort.clone();
        });
    }
    // A job that starts a new session starts its profile's own, never
    // replacing another profile's.
    let name = continued.as_ref().map_or(own_name, |record| record.name.clone());
    // A t3 job asks T3 Code for the worktree (DEV-318); a continued session
    // stays where it ran, a continued T3 Code thread in its own worktree.
    let mut t3_plan = None;
    let mut moved_note = None;
    let prepared = if let Some(record) = continued.as_ref() {
        let mut context = worktree_context_at(&tasks_dir, &record.cwd);
        if let Some((path, branch, note)) = thread_worktree {
            context.worktree_path = Some(path);
            context.worktree_branch = Some(branch);
            moved_note = note;
        }
        Ok(context)
    } else if t3_worktrees {
        plan_t3_worktree(&tasks_dir, &ticket_id, &config).map(|(context, plan)| {
            t3_plan = Some(plan);
            context
        })
    } else {
        prepare_worktree(&tasks_dir, &ticket_id, &config)
    };
    let worktree_context = match prepared {
        Ok(context) => {
            update_job(&job_id, |state| {
                state.record.worktree_path = context.worktree_path.as_ref().map(|path| path.to_string_lossy().to_string());
                state.record.worktree_branch = context.worktree_branch.clone();
            });
            if let Some(path) = context.worktree_path.as_ref() {
                let msg = format!("Worktree: {}", path.to_string_lossy());
                update_job(&job_id, |state| {
                    record_and_emit(state, "agent_job_progress", Some(msg.clone()));
                });
            }
            context
        }
        Err(err) => {
            if config.agent_worktree.enabled {
                // The failure goes to the job's log, so `agent status` shows it.
                init_job_log(&job_id, runner_kind, &profile, &tasks_dir, &ticket_id, &config, &time::now_rfc3339(), None, None);
                update_job_failure(&job_id, &ticket_id, format!("Worktree setup failed: {err}"), &tasks_dir);
                return;
            }
            let msg = format!("Worktree setup skipped: {}", err);
            update_job(&job_id, |state| {
                record_and_emit(state, "agent_job_progress", Some(msg.clone()));
            });
            WorktreeContext {
                working_dir: resolve_repo_root(&tasks_dir).unwrap_or_else(|| tasks_dir.clone()),
                worktree_path: None,
                worktree_branch: None,
                note: None,
            }
        }
    };

    let started_at = match merge_started_at {
        Some(started_at) => started_at,
        None => {
            let started_at = time::now_rfc3339();
            let worktree = (worktree_context.worktree_path.as_ref().map(|p| p.to_string_lossy().to_string()), worktree_context.worktree_branch.clone());
            if !start_job(&job_id, runner_kind, &profile, &tasks_dir, &ticket_id, &config, &started_at, Some(worktree)) {
                return;
            }
            started_at
        }
    };
    let is_merge_job = merge::is_merge_job(&job_id);
    for note in followed_note.into_iter().chain(session_reason).chain(moved_note).chain(worktree_context.note.clone()) {
        update_job(&job_id, |state| {
            record_and_emit(state, "agent_job_progress", Some(note.clone()));
        });
    }
    let ticket_ref = format!("refs/heads/{}", build_worktree_branch(&config.agent_worktree.branch_prefix, &sanitize_worktree_token(&ticket_id)));
    if t3_worktrees && let Some(commit) = resolve_repo_root(&tasks_dir).and_then(|root| rev_parse(&root, &ticket_ref)) {
        update_job(&job_id, |state| state.ticket_commit_at_start = Some(commit.clone()));
    }

    // Other processes see the job, and can ask to cancel it or send it a
    // message (DEV-279). The record names the session before the runner
    // starts it, and a wrapper without a record refuses ticket changes
    // (DEV-360).
    let active_record = JOB_REGISTRY
        .lock()
        .ok()
        .and_then(|registry| registry.jobs.get(&job_id).map(|state| active_record_of(state, &ticket_id, &started_at, agent.map(|_| name.clone()))));
    if let Some(record) = active_record.as_ref() {
        crate::services::agent_active_jobs::register(&tasks_dir, record);
    }

    let mut context_messages = vec![build_user_message(&user_prompt)];
    // The new session replaces a skipped one of its own name: keep that
    // round's log for the handoff and later jobs (DEV-611).
    let handoff_from = match handoff_from {
        Some(record) if record.name == name => match crate::services::agent_runner::keep_session(&name) {
            Ok(kept) => Some(kept),
            Err(err) => {
                let end = RunEnd::Failed(format!("Could not keep the session {name} before a new one replaces it: {err}"));
                end_job(&job_id, &ticket_id, end, &context_messages, &tasks_dir, &config);
                return;
            }
        },
        Some(record) => {
            let events = crate::services::agent_runner::registry().events_path(&record.name);
            Some((record, events))
        }
        None => None,
    };
    let prompt = match handoff_from.as_ref() {
        Some((record, events)) => format!("{prompt}{}", handoff_text(&tasks_dir, &ticket_id, &ticket_ref, record, events)),
        None => prompt,
    };
    let env = job_env(&job_id, &tasks_dir, &ticket_id, &config);
    let end = match agent {
        None => run_command(&job_id, &profile, &prompt, env, &worktree_context.working_dir, &tasks_dir, &ticket_id, active_record),
        Some(agent) => {
            let launch = SessionLaunch {
                agent,
                runner_kind,
                name: &name,
                resume: continued.is_some(),
                cwd: &worktree_context.working_dir,
                writable_dirs: extra_writable_dirs(&worktree_context.working_dir, &tasks_dir, is_merge_job),
                t3_plan,
            };
            run_session(&job_id, &launch, &profile, &prompt, env, &tasks_dir, &ticket_id, &mut context_messages)
        }
    };
    end_job(&job_id, &ticket_id, end, &context_messages, &tasks_dir, &config);
}

/// How long a `session: continue` job resumes its session after the
/// session's last turn ended, unless its profile says otherwise: under the
/// agent's one-hour prompt cache (DEV-603).
const DEFAULT_CONTINUE_WITHIN_MINUTES: u64 = 55;

/// How long ago a session's `last_used` was; `None` when it doesn't parse.
fn idle_since(last_used: &str) -> Option<SignedDuration> {
    let at = time::parse_rfc3339(last_used).ok()?;
    Some(Timestamp::now().duration_since(at).max(SignedDuration::ZERO))
}

/// What a job that starts a new session instead of resuming an idle one
/// (DEV-603) tells its agent: the session it didn't continue, the ticket's
/// branch and newest commit, and the ticket's newest comments, where the
/// earlier rounds' handoffs and reviews are. `events` is that session's
/// event log, kept when the new session takes its name (DEV-611).
fn handoff_text(tasks_dir: &std::path::Path, ticket_id: &str, ticket_ref: &str, record: &harnessmux::SessionRecord, events: &std::path::Path) -> String {
    const COMMENTS: usize = 3;
    const COMMENT_CHARS: usize = 1500;
    let mut out = format!(
        "\nHandoff:\nThis job would have continued the session {} ({} agent, last used {}), but it was idle too long, so it starts a new one. Pick up from the earlier rounds below.\n",
        record.name, record.agent, record.last_used
    );
    if events.is_file() {
        out.push_str(&format!(
            "- Its event log, one JSON object per line (`message` events are the agent's replies, `tool.started` its tool calls): {}\n",
            events.display()
        ));
    }
    if record.agent == AgentRunnerKind::T3.as_str()
        && let Some(thread) = record.agent_session.as_deref()
    {
        out.push_str(&format!("- Its T3 Code thread: {thread}\n"));
    }
    let branch = ticket_ref.trim_start_matches("refs/heads/");
    let newest = resolve_repo_root(tasks_dir).and_then(|root| {
        let output = git_command(&root).args(["log", "-1", "--format=%h %s", ticket_ref, "--"]).current_dir(&root).output().ok()?;
        output.status.success().then(|| String::from_utf8_lossy(&output.stdout).trim().to_string()).filter(|line| !line.is_empty())
    });
    if let Some(newest) = newest {
        out.push_str(&format!("- Branch {branch}, newest commit {newest}\n"));
    }
    let comments =
        Storage::try_open(tasks_dir).and_then(|storage| TaskService::get(&storage, ticket_id, None).ok()).map(|task| task.comments).unwrap_or_default();
    let start = comments.len().saturating_sub(COMMENTS);
    for comment in comments.iter().skip(start) {
        let mut text: String = comment.text.trim().chars().take(COMMENT_CHARS).collect();
        if comment.text.trim().chars().count() > COMMENT_CHARS {
            text.push_str(" …");
        }
        let author = comment.author.as_deref().unwrap_or("unknown");
        out.push_str(&format!("- Comment by {author}, {}:\n{}\n", comment.date, text));
    }
    out
}

/// Write the job's log header, mark it running and apply the `job_started`
/// rules; `false` when it was cancelled meanwhile. `worktree` is the log
/// header's (path, branch). A job records the ticket's assignee as it
/// starts, a merge job also the main checkout's commit.
#[expect(clippy::too_many_arguments, reason = "the job's independent inputs from run_job")]
fn start_job(
    job_id: &str,
    runner_kind: AgentRunnerKind,
    profile: &AgentProfileDetail,
    tasks_dir: &std::path::Path,
    ticket_id: &str,
    config: &ResolvedConfig,
    started_at: &str,
    worktree: Option<(Option<String>, Option<String>)>,
) -> bool {
    let (worktree_path, worktree_branch) = worktree.unwrap_or_default();
    init_job_log(job_id, runner_kind, profile, tasks_dir, ticket_id, config, started_at, worktree_path, worktree_branch);

    let mut cancelled = false;
    update_job(job_id, |state| {
        if state.record.status == AgentJobStatus::Cancelled {
            cancelled = true;
            return;
        }
        mark_started(state, tasks_dir, ticket_id, started_at);
    });
    if cancelled {
        return false;
    }

    let _ = AutomationService::apply_job_event(tasks_dir, ticket_id, AutomationEvent::JobStarted, job_context_for(job_id));

    if merge::is_merge_job(job_id)
        && let Some(head) = resolve_repo_root(tasks_dir).and_then(|root| rev_parse(&root, "HEAD"))
    {
        update_job(job_id, |state| state.main_head_at_start = Some(head.clone()));
    }
    if let Some((_, assignee)) = ticket_state(tasks_dir, ticket_id) {
        update_job(job_id, |state| state.assignee_at_start = Some(assignee.clone()));
    }
    true
}

/// Comment lotar's merge outcome on the ticket, and keep it as the job's
/// last message.
fn comment_merge(job_id: &str, tasks_dir: &std::path::Path, ticket_id: &str, summary: &merge::Summary) {
    let mut storage = Storage::new(tasks_dir);
    let context =
        crate::services::task_service::CommentContext { report: Some(summary.report.clone()), ..crate::services::task_service::CommentContext::automation() };
    if let Err(err) = TaskService::add_comment_with_context(&mut storage, ticket_id, &summary.text, context) {
        update_job(job_id, |state| {
            record_and_emit(state, "agent_job_progress", Some(format!("Could not comment the merge on {ticket_id}: {err}")));
        });
    }
    let first = summary.text.lines().next().unwrap_or_default().to_string();
    update_job(job_id, |state| {
        state.record.last_message = Some(first.clone());
        record_and_emit(state, "agent_job_progress", Some(summary.text.clone()));
    });
}

/// How a job's runner ended.
enum RunEnd {
    /// The runner finished with this exit code (a session: 0 when its last
    /// turn ended normally, 1 when it failed).
    Exited(Option<i32>),
    /// The job was cancelled.
    Stopped,
    /// The runner couldn't run or its output broke.
    Failed(String),
    /// The agent waits for an answer in its own app; the message names where.
    WaitingForAnswer(String),
}

fn end_job(
    job_id: &str,
    ticket_id: &str,
    end: RunEnd,
    context_messages: &[crate::services::agent_context_service::AgentContextMessage],
    tasks_dir: &std::path::Path,
    config: &ResolvedConfig,
) {
    match end {
        RunEnd::Exited(exit_code) if !should_stop_job(job_id) => finalize_job(job_id, ticket_id, exit_code, context_messages, tasks_dir, config),
        RunEnd::Failed(message) if !should_stop_job(job_id) => {
            let _ = AgentContextService::append_messages(tasks_dir, config, ticket_id, context_messages.to_vec(), None);
            update_job_failure(job_id, ticket_id, message, tasks_dir);
        }
        RunEnd::WaitingForAnswer(message) if !should_stop_job(job_id) => {
            hand_back_waiting_job(job_id, ticket_id, &message, tasks_dir, config);
            finalize_job(job_id, ticket_id, Some(WAITS_FOR_ANSWER), context_messages, tasks_dir, config);
        }
        _ => {
            let _ = AgentContextService::append_messages(tasks_dir, config, ticket_id, context_messages.to_vec(), None);
        }
    }
}

/// A job's exit code when its agent waits for an answer.
const WAITS_FOR_ANSWER: i32 = 2;

/// The agent waits for an answer (a T3 Code thread's question): hand the
/// ticket back to its reporter (status HelpNeeded where the project has it)
/// with where to answer, and let the job fail, so `job_completed` rules
/// don't treat it as done.
fn hand_back_waiting_job(job_id: &str, ticket_id: &str, message: &str, tasks_dir: &std::path::Path, config: &ResolvedConfig) {
    let mut storage = Storage::new(tasks_dir);
    if let Ok(task) = TaskService::get(&storage, ticket_id, None) {
        let mut patch = crate::api_types::TaskUpdate::default();
        if let Ok(status) = crate::types::TaskStatus::parse_with_config("HelpNeeded", config) {
            patch.status = Some(status.to_string());
        }
        patch.assignee = task.reporter.clone();
        let _ = TaskService::update_with_context(&mut storage, ticket_id, patch, crate::services::task_service::TaskUpdateContext::automation_disabled());
        let _ = TaskService::add_comment_with_context(&mut storage, ticket_id, message, crate::services::task_service::CommentContext::automation());
    }
    update_job(job_id, |state| {
        state.record.last_message = Some(message.to_string());
        record_and_emit(state, "agent_job_progress", Some(message.to_string()));
    });
}

/// Link the agent's session (a T3 Code thread) on the ticket, so the owner
/// and other agents find it.
fn link_session(job_id: &str, tasks_dir: &std::path::Path, ticket_id: &str, link: &str) {
    let mut storage = Storage::new(tasks_dir);
    if let Err(err) = crate::services::reference_service::ReferenceService::attach_link_reference(&mut storage, ticket_id, link) {
        update_job(job_id, |state| {
            record_and_emit(state, "agent_job_progress", Some(format!("Could not link the session {link} on {ticket_id}: {err}")));
        });
    }
}

/// The link of the latest `session.started` in a session's events.
fn last_session_link(events: &[harnessmux::Event]) -> Option<&str> {
    events.iter().rev().find_map(|event| match &event.body {
        harnessmux::EventBody::SessionStarted { link, .. } => Some(link.as_deref()),
        _ => None,
    })?
}

/// The job's place in `dir`: a continued session's working directory, the
/// main checkout or one of its worktrees.
fn worktree_context_at(tasks_dir: &std::path::Path, dir: &std::path::Path) -> WorktreeContext {
    let canonical = |path: &std::path::Path| fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
    let in_main = resolve_repo_root(tasks_dir).is_none_or(|root| canonical(&root) == canonical(dir));
    if in_main {
        return WorktreeContext { working_dir: dir.to_path_buf(), worktree_path: None, worktree_branch: None, note: None };
    }
    let head = git_command(dir).args(["symbolic-ref", "--quiet", "--short", "HEAD"]).current_dir(dir).output().ok();
    let branch = head.filter(|output| output.status.success()).map(|output| String::from_utf8_lossy(&output.stdout).trim().to_string());
    WorktreeContext {
        working_dir: dir.to_path_buf(),
        worktree_path: Some(dir.to_path_buf()),
        worktree_branch: branch.filter(|branch| !branch.is_empty()),
        note: None,
    }
}

/// The environment of a job's agent: LOTAR_* variables with the task it
/// works on, and this lotar build first on PATH.
fn job_env(job_id: &str, tasks_dir: &std::path::Path, ticket_id: &str, config: &ResolvedConfig) -> HashMap<String, String> {
    let mut env = HashMap::new();
    if let Some(storage) = Storage::try_open(tasks_dir)
        && let Ok(task) = TaskService::get(&storage, ticket_id, None)
    {
        env.extend(build_lotar_env(&task, config, tasks_dir, None, job_context_for(job_id).as_ref()));
    }
    // Not an automation `run` command, also when this lotar was started by
    // one: the agent's status and assignee changes are the job's outcome
    // (DEV-531).
    env.insert("LOTAR_AUTOMATION_EVENT".to_string(), String::new());
    // The agent calls the CLI itself; point it at this build, not whatever
    // `lotar` is first on its PATH (login shells and some CLIs reorder PATH).
    // The prompt names this binary by its absolute path (Claude Code refuses
    // a variable as the command), and its directory goes first on PATH.
    if let Some(bin) = current_lotar_binary() {
        if let Some(dir) = bin.parent() {
            let path = std::env::var_os("PATH").unwrap_or_default();
            let mut dirs = vec![dir.to_path_buf()];
            dirs.extend(std::env::split_paths(&path));
            if let Ok(joined) = std::env::join_paths(dirs) {
                env.insert("PATH".to_string(), joined.to_string_lossy().to_string());
            }
        }
        env.insert("LOTAR_BIN".to_string(), bin.to_string_lossy().to_string());
    }
    env
}

/// Run a `command` runner's program through the wrapper and log its output
/// lines.
#[expect(clippy::too_many_arguments, reason = "the job's independent inputs from run_job")]
fn run_command(
    job_id: &str,
    profile: &AgentProfileDetail,
    prompt: &str,
    env: HashMap<String, String>,
    working_dir: &std::path::Path,
    tasks_dir: &std::path::Path,
    ticket_id: &str,
    mut active_record: Option<crate::services::agent_active_jobs::ActiveJob>,
) -> RunEnd {
    let prompt_file = match crate::services::agent_active_jobs::write_prompt(tasks_dir, job_id, prompt) {
        Ok(path) => path,
        Err(err) => return RunEnd::Failed(format!("Failed to write the prompt file: {err}")),
    };
    let mut command = match build_command(profile, prompt, &prompt_file) {
        Ok(command) => wrap_runner_command(command, job_id, ticket_id, AgentRunnerKind::Command.as_str(), tasks_dir),
        Err(err) => return RunEnd::Failed(err.to_string()),
    };
    command.env.extend(env);
    let mut cmd = Command::new(&command.program);
    cmd.args(&command.args).envs(command.env).stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::piped());
    cmd.current_dir(working_dir);
    clear_repository_env(&mut cmd);

    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        unsafe {
            cmd.pre_exec(|| {
                if libc::setpgid(0, 0) != 0 {
                    return Err(std::io::Error::last_os_error());
                }
                Ok(())
            });
        }
    }

    // Serialize cancellation with spawn and runtime publication.
    let Ok(mut registry) = JOB_REGISTRY.lock() else {
        return RunEnd::Stopped;
    };
    if registry.jobs.get(job_id).is_none_or(|state| state.record.status == AgentJobStatus::Cancelled) {
        return RunEnd::Stopped;
    }
    let child = match cmd.spawn() {
        Ok(child) => child,
        Err(err) => {
            drop(registry);
            return RunEnd::Failed(format!("Failed to spawn runner: {}", err));
        }
    };

    let child = Arc::new(Mutex::new(child));
    let stdout = child.lock().ok().and_then(|mut c| c.stdout.take());
    let stderr = child.lock().ok().and_then(|mut c| c.stderr.take());
    if let Some(state) = registry.jobs.get_mut(job_id) {
        state.runtime = Some(AgentJobRuntime::Process(Arc::clone(&child)));
    }
    drop(registry);
    if let Some(record) = active_record.as_mut() {
        record.child_pid = child.lock().map(|c| c.id()).unwrap_or_default();
        crate::services::agent_active_jobs::register(tasks_dir, record);
    }

    let exited = Arc::new(AtomicBool::new(false));
    let prefixes = working_dir_prefixes(working_dir);
    let readers: Vec<_> =
        [stdout.map(|pipe| spawn_line_reader(pipe, job_id, &exited, &prefixes)), stderr.map(|pipe| spawn_line_reader(pipe, job_id, &exited, &prefixes))]
            .into_iter()
            .flatten()
            .collect();

    let status = loop {
        // A cancel asked for by another process (`lotar agent cancel`).
        if crate::services::agent_active_jobs::cancel_requested(tasks_dir, job_id) {
            let _ = AgentJobService::cancel_job(job_id);
        }
        if should_stop_job(job_id)
            && let Ok(mut c) = child.lock()
        {
            terminate_child(&mut c);
            break c.wait();
        }
        let status = child.lock().map_err(|_| std::io::Error::other("Job process unavailable")).and_then(|mut c| c.try_wait());
        match status {
            Ok(Some(status)) => break Ok(status),
            Ok(None) => {}
            Err(err) => {
                if let Ok(mut c) = child.lock() {
                    terminate_child(&mut c);
                    let _ = c.wait();
                }
                break Err(err);
            }
        }
        thread::sleep(Duration::from_millis(200));
    };
    exited.store(true, Ordering::Release);
    // Never join while holding the registry or child lock: readers publish events.
    let mut output_error = None;
    for reader in readers {
        let result = reader.join().unwrap_or_else(|_| Err(std::io::Error::other("Output reader panicked")));
        output_error = output_error.or(result.err());
    }
    if should_stop_job(job_id) {
        return RunEnd::Stopped;
    }
    match (status, output_error) {
        (Ok(status), None) => RunEnd::Exited(status.code()),
        (Err(error), _) | (Ok(_), Some(error)) => RunEnd::Failed(format!("Runner output/process failed: {error}")),
    }
}

/// Log each output line of a `command` runner as progress.
fn spawn_line_reader(
    pipe: impl JobOutputPipe + Send + 'static,
    job_id: &str,
    exited: &Arc<AtomicBool>,
    prefixes: &[String],
) -> thread::JoinHandle<std::io::Result<()>> {
    let job_id = job_id.to_string();
    let exited = Arc::clone(exited);
    let prefixes = prefixes.to_vec();
    thread::spawn(move || {
        read_job_output(pipe, &exited, OUTPUT_DRAIN_LIMIT, |line| {
            let msg = strip_prefixes(line.trim(), &prefixes);
            if !msg.is_empty() {
                update_job(&job_id, |state| {
                    state.record.last_message = Some(msg.clone());
                    record_and_emit(state, "agent_job_progress", Some(msg.clone()));
                });
            }
        })
    })
}

/// Tool lines name files relative to the job's working directory. Agents
/// may report either spelling of it (macOS /var vs /private/var).
fn working_dir_prefixes(working_dir: &std::path::Path) -> Vec<String> {
    let mut prefixes = vec![format!("{}/", working_dir.display())];
    if let Ok(canonical) = working_dir.canonicalize() {
        prefixes.push(format!("{}/", canonical.display()));
    }
    prefixes.sort_by_key(|prefix| std::cmp::Reverse(prefix.len()));
    prefixes.dedup();
    prefixes
}

fn strip_prefixes(text: &str, prefixes: &[String]) -> String {
    prefixes.iter().fold(text.to_string(), |text, prefix| text.replace(prefix.as_str(), ""))
}

/// How a job's harnessmux session starts.
struct SessionLaunch<'a> {
    agent: &'a str,
    runner_kind: AgentRunnerKind,
    name: &'a str,
    /// Continue the ticket's session instead of starting a new one.
    resume: bool,
    cwd: &'a std::path::Path,
    writable_dirs: Vec<std::path::PathBuf>,
    t3_plan: Option<T3Worktree>,
}

/// Run the job as a turn of its harnessmux session (DEV-503).
#[expect(clippy::too_many_arguments, reason = "the job's independent inputs from run_job")]
fn run_session(
    job_id: &str,
    launch: &SessionLaunch<'_>,
    profile: &AgentProfileDetail,
    prompt: &str,
    env: HashMap<String, String>,
    tasks_dir: &std::path::Path,
    ticket_id: &str,
    context_messages: &mut Vec<crate::services::agent_context_service::AgentContextMessage>,
) -> RunEnd {
    let mut session_profile = match crate::services::agent_runner::session_profile(profile, env) {
        Ok(converted) => converted,
        Err(err) => return RunEnd::Failed(err.to_string()),
    };
    let mut notes = Vec::new();
    if session_profile.permissions == harnessmux::Permissions::Workspace {
        session_profile.add_dirs = launch.writable_dirs.clone();
    }
    if profile.mcp.as_ref().is_some_and(|mcp| mcp.lotar) {
        if crate::services::agent_runner::takes_mcp_servers(launch.runner_kind) {
            let bin = current_lotar_binary().map(|bin| bin.to_string_lossy().to_string()).unwrap_or_else(|| "lotar".to_string());
            session_profile.mcp_servers.insert("lotar".to_string(), crate::services::agent_runner::lotar_mcp_server(&bin, tasks_dir));
        } else {
            notes.push(format!("The {} runner can't be given LoTaR's MCP server; the agent uses the lotar CLI instead.", launch.agent));
        }
    }
    let repo_root = resolve_repo_root(tasks_dir);
    let mut worktrees_before = None;
    let mut t3_setup_recorded = false;
    if launch.runner_kind == AgentRunnerKind::T3 {
        let title =
            Storage::try_open(tasks_dir).and_then(|storage| TaskService::get(&storage, ticket_id, None).ok()).map(|task| task.title).unwrap_or_default();
        session_profile.title = Some(format!("{ticket_id} {title}").trim().to_string());
        if let Some(plan) = launch.t3_plan.as_ref() {
            session_profile.worktree_base = Some(plan.base.clone());
            session_profile.branch = plan.branch.clone();
            if !launch.resume
                && let Some(root) = repo_root.as_deref()
            {
                t3_setup_recorded = record_t3_worktree_setup(job_id, root, ticket_id, plan, profile, session_profile.provider.as_deref());
            }
        }
        // A continued thread makes no worktree: it keeps the one run_job
        // checked for it, not one another job's thread made meanwhile.
        if !launch.resume {
            worktrees_before =
                repo_root.as_deref().map(|root| linked_worktrees(root).unwrap_or_default().into_iter().map(|(path, _)| path).collect::<Vec<_>>());
        }
    }
    for note in notes {
        update_job(job_id, |state| {
            record_and_emit(state, "agent_job_progress", Some(note.clone()));
        });
    }
    let table = match crate::services::agent_runner::job_agent_table(launch.agent, profile) {
        Ok(table) => table,
        Err(err) => return RunEnd::Failed(err.to_string()),
    };
    let t3_env: Vec<(String, String)> =
        table.get(launch.agent).map(|spec| spec.env.clone()).unwrap_or_default().into_iter().chain(session_profile.env.clone()).collect();
    let session = match crate::services::agent_runner::open_session(launch.name, launch.agent, launch.cwd, &table, session_profile, launch.resume) {
        Ok(session) => session,
        Err(err) => return RunEnd::Failed(format!("Could not start the {} session {}: {err}", launch.agent, launch.name)),
    };
    if !publish_session(job_id, &session) {
        let _ = block_on(session.cancel());
        return RunEnd::Stopped;
    }
    let how = if launch.resume { "Continuing the session" } else { "Session" };
    update_job(job_id, |state| {
        let note = format!("{how} {} (`harnessmux attach {}` follows it).", launch.name, launch.name);
        record_and_emit(state, "agent_job_progress", Some(note));
    });
    let end = drive_session(job_id, tasks_dir, &session, prompt, &working_dir_prefixes(launch.cwd), profile.max_budget_usd, context_messages);
    if t3_setup_recorded && let Some(root) = repo_root.as_deref() {
        let note = crate::services::t3_worktree_setup::finish(root, job_id);
        update_job(job_id, |state| {
            record_and_emit(state, "agent_job_progress", Some(note.clone()));
        });
    }
    if let (Some(root), Some(before)) = (repo_root.as_deref(), worktrees_before.as_deref()) {
        let thread = crate::services::agent_runner::registry().load(launch.name).ok().and_then(|record| record.agent_session);
        let program = table.get(launch.agent).ok().and_then(|spec| block_on(spec.launch_command()).ok()).map(|(program, _)| program);
        let named = thread.zip(program).and_then(|(thread, program)| t3_thread_worktree(&program, &t3_env, root, &thread));
        record_t3_worktree(job_id, root, before, named);
    }
    end
}

/// Leave the profile's `tools.disallowed` and `env` for the T3 project's
/// worktree script, which writes them into the new thread's Claude settings
/// (DEV-618). Only Claude threads read them: provider `claudeAgent`, or none
/// (the project's default model). A failure is logged and the thread starts
/// without them.
fn record_t3_worktree_setup(
    job_id: &str,
    repo_root: &std::path::Path,
    ticket_id: &str,
    plan: &T3Worktree,
    profile: &AgentProfileDetail,
    provider: Option<&str>,
) -> bool {
    if provider.is_some_and(|provider| !provider.to_ascii_lowercase().contains("claude")) {
        return false;
    }
    let deny = profile.tools.as_ref().map(|tools| tools.disallowed.clone()).unwrap_or_default();
    let env = crate::services::agent_runner::expand_env_map(&profile.env).into_iter().collect();
    match crate::services::t3_worktree_setup::record(repo_root, job_id, ticket_id, &plan.base, plan.branch.as_deref(), &deny, env) {
        Ok(recorded) => recorded,
        Err(err) => {
            let note = format!("The thread starts without the profile's tools.disallowed and env: {err}");
            update_job(job_id, |state| {
                record_and_emit(state, "agent_job_progress", Some(note.clone()));
            });
            false
        }
    }
}

/// The worktree T3 Code runs `thread` in, from `t3 thread show <thread> --json`.
fn t3_thread_worktree(program: &std::path::Path, env: &[(String, String)], cwd: &std::path::Path, thread: &str) -> Option<std::path::PathBuf> {
    let shown = t3_thread_show(program, env, cwd, thread)?;
    shown.get("worktreePath")?.as_str().filter(|path| !path.is_empty()).map(std::path::PathBuf::from)
}

/// The `thread` object of `t3 thread show <thread> --json`.
fn t3_thread_show(program: &std::path::Path, env: &[(String, String)], cwd: &std::path::Path, thread: &str) -> Option<serde_json::Value> {
    let output = Command::new(program)
        .args(["thread", "show", thread, "--json", "--messages", "0"])
        .envs(env.iter().map(|(key, value)| (key, value)))
        .current_dir(cwd)
        .stdin(Stdio::null())
        .output()
        .ok()
        .filter(|output| output.status.success())?;
    let mut shown: serde_json::Value = serde_json::from_slice(&output.stdout).ok()?;
    shown.get_mut("thread").map(serde_json::Value::take)
}

type T3Cli = (std::path::PathBuf, Vec<(String, String)>, std::path::PathBuf);

/// T3's CLI as harnessmux's t3 agent runs it, its environment, and the
/// directory to run it in (the tasks directory's repository).
fn t3_cli(tasks_dir: &std::path::Path) -> Option<T3Cli> {
    let (program, env) = t3_launch()?;
    let cwd = resolve_repo_root(tasks_dir).unwrap_or_else(|| tasks_dir.to_path_buf());
    Some((program, env, cwd))
}

/// How long the answer for a ticket counts before T3 Code is asked again,
/// while the ticket's earlier threads stay the same: the queue tries a waiting
/// ticket every few hundred milliseconds, and each `t3` call takes about half
/// a second (DEV-599).
const T3_THREAD_RECHECK: std::time::Duration = std::time::Duration::from_secs(30);

/// A T3 Code thread of an earlier job that holds later jobs on its ticket.
#[derive(Debug, Clone)]
pub(crate) struct T3ThreadHold {
    pub thread: String,
    pub job: String,
    /// The thread's link in T3 Code, when `t3 thread show` names one.
    pub link: Option<String>,
    /// The thread waits for an answer rather than runs a turn (DEV-600).
    pub waiting: bool,
}

/// Per ticket: when T3 Code was asked, about which earlier threads, and the
/// thread that holds the ticket, if any.
type T3ThreadAnswers = HashMap<String, (std::time::Instant, Vec<String>, Option<T3ThreadHold>)>;

static T3_THREAD_ANSWERS: LazyLock<Mutex<T3ThreadAnswers>> = LazyLock::new(|| Mutex::new(HashMap::new()));

/// A T3 Code thread of an earlier job on the ticket that T3 Code still runs,
/// with that job's id (DEV-594). A thread can run on after its job ended:
/// T3 Code resumes a turn it paused for a usage limit, which the job counted
/// as failed, and the owner can answer in a thread. A second job would work
/// beside it, its own thread on a branch of its own, as T3 Code checks the
/// ticket's branch out in one worktree only.
///
/// A thread runs while its status is `running` or it has an `activeRunId`, as
/// harnessmux counts it. A thread that waits for an answer holds the ticket
/// too, so an answer given later can't start a turn beside the new job: the
/// owner decided so (DEV-600).
fn running_t3_thread(tasks_dir: &std::path::Path, ticket_id: &str) -> Option<T3ThreadHold> {
    let mut threads: Vec<_> = saved_jobs(tasks_dir)
        .into_iter()
        .filter(|job| job.header.runner == AgentRunnerKind::T3.as_str() && crate::storage::identity::aliases_match(&job.header.ticket_id, ticket_id))
        .filter_map(|job| job.status.and_then(|status| Some((status.at, status.agent_session?, job.header.job_id))))
        .collect();
    if threads.is_empty() {
        return None;
    }
    threads.sort_by(|a, b| b.0.cmp(&a.0));
    let mut asked: Vec<String> = Vec::new();
    for (_, thread, _) in &threads {
        if !asked.contains(thread) {
            asked.push(thread.clone());
        }
    }
    if let Some((at, known, answer)) = T3_THREAD_ANSWERS.lock().ok()?.get(ticket_id)
        && at.elapsed() < T3_THREAD_RECHECK
        && *known == asked
    {
        return answer.clone();
    }
    // Unlocked while `t3` runs, so other tickets' starts don't wait for it.
    let (program, env, cwd) = t3_cli(tasks_dir)?;
    let answer = asked.iter().find_map(|thread| {
        let shown = t3_thread_show(&program, &env, &cwd, thread)?;
        let job = threads.iter().find(|(_, known, _)| known == thread).map(|(_, _, job)| job.clone())?;
        let waiting = t3_thread_waits(&shown);
        let link = shown.get("link").and_then(|link| link.as_str()).filter(|link| !link.is_empty()).map(str::to_string);
        t3_thread_busy(&shown).then(|| T3ThreadHold { thread: thread.clone(), job, link, waiting })
    });
    T3_THREAD_ANSWERS.lock().ok()?.insert(ticket_id.to_string(), (std::time::Instant::now(), asked, answer.clone()));
    answer
}

/// The `t3` program and env of the agent table, to ask T3 Code about threads.
fn t3_launch() -> Option<(std::path::PathBuf, Vec<(String, String)>)> {
    let table = crate::services::agent_runner::agent_table().ok()?;
    let spec = table.get("t3").ok()?;
    let (program, _) = block_on(spec.launch_command()).ok()?;
    Some((program, spec.env.clone().into_iter().collect()))
}

/// Whether a thread `t3 thread show` describes runs or waits for an answer.
fn t3_thread_busy(shown: &serde_json::Value) -> bool {
    shown.get("status").and_then(|status| status.as_str()) == Some("running")
        || shown.get("activeRunId").is_some_and(|run| !run.is_null())
        || t3_thread_waits(shown)
}

fn t3_thread_waits(shown: &serde_json::Value) -> bool {
    shown.get("pendingRequestCount").and_then(serde_json::Value::as_u64).is_some_and(|count| count > 0)
}

/// The thread that held the ticket at its last start, as T3 Code answered
/// within [`T3_THREAD_RECHECK`]; asks T3 Code nothing.
pub(crate) fn recent_t3_thread_hold(ticket_id: &str) -> Option<T3ThreadHold> {
    let answers = T3_THREAD_ANSWERS.lock().ok()?;
    let (at, _, hold) = answers.get(ticket_id)?;
    hold.clone().filter(|_| at.elapsed() < T3_THREAD_RECHECK)
}

/// Make the session the job's runtime, unless the job was cancelled meanwhile.
fn publish_session(job_id: &str, session: &harnessmux::Session) -> bool {
    let Ok(mut registry) = JOB_REGISTRY.lock() else {
        return false;
    };
    let Some(state) = registry.jobs.get_mut(job_id).filter(|state| state.record.status != AgentJobStatus::Cancelled) else {
        return false;
    };
    state.record.session_id = Some(session.name().to_string());
    state.runtime = Some(AgentJobRuntime::Session(session.clone()));
    true
}

/// Turns one job runs at most: a rule that tells the agent about its own
/// comments would otherwise keep it going.
const MAX_TURNS: u64 = 10;

/// The agent's reply in the running turn as it streams in (DEV-511): whole
/// paragraphs outside code blocks go to the job log as they arrive, the rest
/// when a tool starts, the agent asks or the turn ends.
#[derive(Default)]
struct ReplyStream {
    pending: String,
    /// Part of this turn's reply is in the job log already.
    streamed: bool,
}

impl ReplyStream {
    fn push(&mut self, delta: &str) -> Option<String> {
        self.pending.push_str(delta);
        let pending = &self.pending;
        let cut = pending.match_indices("\n\n").map(|(at, _)| at + 2).filter(|&end| pending[..end].matches("```").count().is_multiple_of(2)).last()?;
        let part: String = self.pending.drain(..cut).collect();
        self.part(&part)
    }

    fn flush(&mut self) -> Option<String> {
        let part = std::mem::take(&mut self.pending);
        self.part(&part)
    }

    fn part(&mut self, part: &str) -> Option<String> {
        let part = part.trim();
        if part.is_empty() {
            return None;
        }
        self.streamed = true;
        Some(part.to_string())
    }
}

/// What a job's agent costs (DEV-511). Agents report the session's total so
/// far (ACP `usage_update`), which starts over when the agent process starts
/// again (`session.started`).
#[derive(Default)]
struct CostMeter {
    /// The session's total the agent reported last.
    total: f64,
    job: f64,
    turn: Option<f64>,
}

impl CostMeter {
    /// Starts after what the session cost before the job: the last total in its event log since its agent started.
    fn after(earlier: &[harnessmux::Event]) -> Self {
        let mut meter = Self::default();
        for event in earlier {
            match &event.body {
                harnessmux::EventBody::SessionStarted { .. } => meter.total = 0.0,
                harnessmux::EventBody::Usage(usage) => meter.total = usage.cost_usd.unwrap_or(meter.total),
                _ => {}
            }
        }
        meter
    }

    fn report(&mut self, total: f64) {
        // A total below the last one is a new agent process whose start the job didn't see.
        let spent = if total >= self.total { total - self.total } else { total };
        self.total = total;
        self.job += spent;
        *self.turn.get_or_insert(0.0) += spent;
    }
}

/// Run `prompt` as the next turn of `session`, then the turns that messages
/// queue meanwhile (the message box, `harnessmux send`, DEV-384), and record
/// their events on the job. The job ends with the last of them, or when its
/// cost passes `budget`.
fn drive_session(
    job_id: &str,
    tasks_dir: &std::path::Path,
    session: &harnessmux::Session,
    prompt: &str,
    prefixes: &[String],
    budget: Option<f64>,
    context_messages: &mut Vec<crate::services::agent_context_service::AgentContextMessage>,
) -> RunEnd {
    let status = block_on(session.status()).map_err(|err| err.to_string());
    let agent = status.as_ref().map(|status| status.agent.clone()).unwrap_or_default();
    let (mut events, earlier) = SessionEvents::open(status.map(|status| status.events), || session.events());
    // A resumed session starts before the job listens: its start is only in the log.
    let link = last_session_link(&earlier);
    let mut meter = CostMeter::after(&earlier);
    // A job taken over from a stopped process has cost something already.
    meter.job = AgentJobService::get_job(job_id).and_then(|job| job.usage).and_then(|usage| usage.cost_usd).unwrap_or(0.0);
    let first = match block_on(session.send(prompt.to_string())) {
        Ok(turn) => turn,
        Err(err) => return RunEnd::Failed(format!("Could not send the prompt to the session {}: {err}", session.name())),
    };
    let settings = TurnSettings { first, prefixes, budget, agent: &agent, link };
    drive_turns(job_id, tasks_dir, session, &mut events, settings, meter, context_messages)
}

/// What a job's turns run with.
#[derive(Clone, Copy)]
struct TurnSettings<'a> {
    /// The job's first turn.
    first: u64,
    prefixes: &'a [String],
    budget: Option<f64>,
    /// The session's agent and link when the job started.
    agent: &'a str,
    link: Option<&'a str>,
}

/// What the job loop asks of a session besides its events.
trait TurnControl {
    fn name(&self) -> &str;
    fn cancel(&self);
    /// No turn runs and none is queued.
    fn idle(&self) -> bool;
}

impl TurnControl for harnessmux::Session {
    fn name(&self) -> &str {
        harnessmux::Session::name(self)
    }

    fn cancel(&self) {
        let _ = block_on(harnessmux::Session::cancel(self));
    }

    fn idle(&self) -> bool {
        block_on(self.status()).is_ok_and(|status| status.turn.is_none() && status.queued == 0)
    }
}

/// Record the events of turn `first` and the turns after it until the session is idle.
fn drive_turns(
    job_id: &str,
    tasks_dir: &std::path::Path,
    session: &impl TurnControl,
    events: &mut SessionEvents,
    settings: TurnSettings<'_>,
    mut meter: CostMeter,
    context_messages: &mut Vec<crate::services::agent_context_service::AgentContextMessage>,
) -> RunEnd {
    use harnessmux::EventBody;
    let TurnSettings { first, prefixes, budget, agent, link } = settings;
    let mut reply = ReplyStream::default();
    let progress = |message: String| {
        update_job(job_id, |state| {
            state.record.last_message = Some(message.clone());
            record_and_emit(state, "agent_job_progress", Some(message.clone()));
        });
    };
    let reply_part = |part: String| {
        update_job(job_id, |state| {
            state.record.last_message = Some(part.clone());
            record_and_emit(state, "agent_job_message", Some(part.clone()));
        });
    };
    let finish = |exit_code: i32, reported_cost: bool| {
        if budget.is_some() && !reported_cost {
            progress("The agent reported no cost, so the profile's max_budget_usd was not checked.".to_string());
        }
        RunEnd::Exited(Some(exit_code))
    };
    let mut turns = 0;
    let mut exit_code = 0;
    let mut reported_cost = false;
    let ticket_id = JOB_REGISTRY.lock().ok().and_then(|registry| registry.jobs.get(job_id).map(|state| state.record.ticket_id.clone())).unwrap_or_default();
    let ticket_id = ticket_id.as_str();
    let mut agent_name = agent.to_string();
    let mut link = link.map(str::to_string);
    if let Some(link) = link.as_deref() {
        link_session(job_id, tasks_dir, ticket_id, link);
    }
    let mut waiting = false;
    loop {
        if crate::services::agent_active_jobs::cancel_requested(tasks_dir, job_id) {
            let _ = AgentJobService::cancel_job(job_id);
        }
        if should_stop_job(job_id) {
            session.cancel();
            return RunEnd::Stopped;
        }
        let event = match events.next(Duration::from_millis(200)) {
            Ok(None) => continue,
            Err(why) => return RunEnd::Failed(format!("The session {} {why}", session.name())),
            Ok(Some(event)) => event,
        };
        if event.turn.is_some_and(|turn| turn < first) {
            continue;
        }
        let ended = event.body.ends_turn();
        if matches!(event.body, EventBody::ToolStarted { .. } | EventBody::PermissionRequested { .. })
            && let Some(part) = reply.flush()
        {
            reply_part(part);
        }
        match event.body {
            EventBody::SessionStarted { agent, how, agent_session, link: session_link, .. } => {
                meter.total = 0.0;
                let id = agent_session.map(|id| format!(", {id}")).unwrap_or_default();
                let at = session_link.as_deref().map(|link| format!(": {link}")).unwrap_or_default();
                update_job(job_id, |state| {
                    record_and_emit(state, "agent_job_init", Some(format!("{agent} session {how}{id}{at}")));
                });
                if let Some(new) = session_link.filter(|new| link.as_ref() != Some(new)) {
                    link_session(job_id, tasks_dir, ticket_id, &new);
                    link = Some(new);
                }
                agent_name = agent;
            }
            EventBody::TurnStarted { prompt } if event.turn != Some(first) => {
                context_messages.push(build_user_message(&prompt));
                update_job(job_id, |state| {
                    record_and_emit(state, "agent_job_input", Some(prompt.clone()));
                });
            }
            EventBody::MessageDelta { text } => {
                if let Some(part) = reply.push(&text) {
                    reply_part(part);
                }
            }
            EventBody::Message { text } if !text.trim().is_empty() => {
                let text = text.trim().to_string();
                context_messages.push(build_assistant_message(&text));
                // What streamed is in the log already; the whole reply is the job's last message.
                let rest = if reply.streamed { reply.flush() } else { Some(text.clone()) };
                reply = ReplyStream::default();
                update_job(job_id, |state| {
                    if let Some(rest) = rest.clone() {
                        record_and_emit(state, "agent_job_message", Some(rest));
                    }
                    state.record.last_message = Some(text.clone());
                });
            }
            EventBody::ToolStarted { title, .. } => progress(format!("Tool: {}", strip_prefixes(&title, prefixes))),
            EventBody::ToolFinished { status, title, id } if status == "failed" => {
                progress(format!("Failed: {}", strip_prefixes(title.as_deref().unwrap_or(&id), prefixes)));
            }
            EventBody::PermissionRequested { request, title, options } => {
                let question = crate::api_types::AgentJobQuestion {
                    request,
                    title,
                    options: options
                        .into_iter()
                        .map(|option| crate::api_types::AgentJobQuestionOption { id: option.id, name: option.name, kind: option.kind })
                        .collect(),
                };
                let names = question.options.iter().map(|option| option.name.as_str()).collect::<Vec<_>>().join(", ");
                let message = format!("{} ({names})", question.title);
                update_job(job_id, |state| {
                    state.record.question = Some(question.clone());
                    record_and_emit(state, "agent_job_question", Some(message.clone()));
                });
            }
            EventBody::PermissionAnswered { request, option, by } => update_job(job_id, |state| {
                let title = state.record.question.as_ref().filter(|question| question.request == request).map(|question| question.title.clone());
                if title.is_some() {
                    state.record.question = None;
                }
                let answer = option.clone().unwrap_or_else(|| "cancelled".to_string());
                let message = format!("{}: {answer} ({by})", title.unwrap_or_else(|| format!("Permission request {request}")));
                record_and_emit(state, "agent_job_progress", Some(message));
            }),
            EventBody::Usage(usage) => {
                if let Some(total) = usage.cost_usd {
                    reported_cost = true;
                    meter.report(total);
                }
                if let Some(budget) = budget.filter(|budget| meter.job > *budget) {
                    session.cancel();
                    let usage = crate::api_types::AgentJobUsage { cost_usd: meter.turn.take(), ..Default::default() };
                    update_job(job_id, |state| state.record.usage.get_or_insert_with(Default::default).add(&usage));
                    return RunEnd::Failed(format!(
                        "The job cost ${:.2}, more than the profile's max_budget_usd of ${budget:.2}; LoTaR cancelled the agent's turn.",
                        meter.job
                    ));
                }
            }
            EventBody::Notice { text } => progress(text),
            EventBody::TurnFinished { stop_reason, usage } => {
                let usage = crate::api_types::AgentJobUsage {
                    input_tokens: match (usage.input_tokens, usage.cached_input_tokens) {
                        (None, None) => None,
                        (input, cached) => Some(input.unwrap_or(0) + cached.unwrap_or(0)),
                    },
                    cached_input_tokens: usage.cached_input_tokens,
                    output_tokens: usage.output_tokens,
                    cost_usd: usage.cost_usd.or(meter.turn.take()),
                    turns: Some(1),
                    ..Default::default()
                };
                update_job(job_id, |state| state.record.usage.get_or_insert_with(Default::default).add(&usage));
                waiting = stop_reason == harnessmux::StopReason::WaitingForAnswer;
                exit_code = match stop_reason {
                    harnessmux::StopReason::EndTurn => 0,
                    harnessmux::StopReason::WaitingForAnswer => WAITS_FOR_ANSWER,
                    harnessmux::StopReason::Cancelled => {
                        progress("The turn was cancelled outside LoTaR.".to_string());
                        1
                    }
                    other => {
                        progress(format!(
                            "The turn stopped: {}",
                            serde_json::to_value(other).ok().and_then(|v| v.as_str().map(str::to_string)).unwrap_or_default()
                        ));
                        1
                    }
                };
            }
            EventBody::TurnFailed { error } => {
                progress(error);
                exit_code = 1;
            }
            EventBody::SessionEnded { reason } => {
                return RunEnd::Failed(format!("The session {} ended: {reason}", session.name()));
            }
            _ => {}
        }
        if !ended {
            continue;
        }
        if let Some(part) = reply.flush() {
            reply_part(part);
        }
        reply = ReplyStream::default();
        turns += 1;
        update_job(job_id, |state| state.record.question = None);
        if waiting {
            let at = link.as_deref().map(|link| format!(": {link}")).unwrap_or_default();
            return RunEnd::WaitingForAnswer(format!("The {agent_name} session waits for an answer{at}"));
        }
        if session.idle() {
            return finish(exit_code, reported_cost);
        }
        if turns >= MAX_TURNS {
            session.cancel();
            progress(format!("The job ran {MAX_TURNS} turns; the messages still queued stay on the ticket."));
            return finish(exit_code, reported_cost);
        }
    }
}

/// A session's events in order. When the broadcast channel lags (a chatty turn
/// sends more than it holds), the missed ones come from the session's event log
/// (DEV-520), so the turn's end is never lost. harnessmux numbers events one
/// after another; a gap the log can't fill (missing, unreadable or malformed)
/// fails the job. An owner that took the session over counts on from the count
/// its predecessor last saved, so its first event can repeat a seq in the log.
struct SessionEvents {
    live: tokio::sync::broadcast::Receiver<harnessmux::Event>,
    /// The log, or why lotar doesn't know where it is.
    log: Result<harnessmux::follow::Follower, String>,
    /// Events read but not handed out yet, by seq.
    pending: std::collections::BTreeMap<u64, harnessmux::Event>,
    /// The last event handed out, or the log's last when it opened; none while the log is unknown.
    last_seq: Option<u64>,
    /// Before the first event, nothing read is a duplicate: everything came after the log's end.
    handed_out: bool,
}

impl SessionEvents {
    /// Reads the log to its end for the last seq, then subscribes: every later
    /// event is in the log, the channel or both.
    /// Also returns the log's events so far.
    fn open(
        log: Result<std::path::PathBuf, String>,
        subscribe: impl FnOnce() -> tokio::sync::broadcast::Receiver<harnessmux::Event>,
    ) -> (Self, Vec<harnessmux::Event>) {
        let mut earlier = Vec::new();
        let log = log.and_then(|path| {
            let mut log = harnessmux::follow::Follower::from_start(&path);
            earlier = log.read_new().map_err(|err| err.to_string())?;
            Ok(log)
        });
        let last_seq = log.is_ok().then(|| earlier.last().map_or(0, |event| event.seq));
        (Self { live: subscribe(), log, pending: Default::default(), last_seq, handed_out: false }, earlier)
    }

    /// The next event, none when nothing came within `wait`. The error ends
    /// "The session <name> …": it ended, or lotar missed events it can't read back.
    fn next(&mut self, wait: Duration) -> Result<Option<harnessmux::Event>, String> {
        use tokio::sync::broadcast::error::RecvError;
        let mut read_log = false;
        loop {
            if let Some(entry) = self.pending.first_entry() {
                let seq = *entry.key();
                match self.last_seq {
                    Some(last) if seq <= last && self.handed_out => {
                        entry.remove();
                        continue;
                    }
                    Some(last) if seq > last + 1 => {
                        if read_log {
                            return Err(format!("sent events {} to {} lotar missed and can't read back: its event log doesn't have them", last + 1, seq - 1));
                        }
                        self.read_log(seq - last - 1)?;
                        read_log = true;
                        continue;
                    }
                    _ => {
                        self.last_seq = Some(seq);
                        self.handed_out = true;
                        return Ok(Some(entry.remove()));
                    }
                }
            }
            match block_on(async { tokio::time::timeout(wait, self.live.recv()).await }) {
                Err(_) => return Ok(None),
                Ok(Err(RecvError::Closed)) => return Err("ended before the job's turn did.".to_string()),
                // The owner appends each event to the log before it sends it.
                Ok(Err(RecvError::Lagged(missed))) => self.read_log(missed)?,
                Ok(Ok(event)) => {
                    self.pending.insert(event.seq, event);
                }
            }
        }
    }

    /// Adds the log's new events to the pending ones.
    fn read_log(&mut self, missed: u64) -> Result<(), String> {
        let read = match &mut self.log {
            Ok(log) => log.read_new().map_err(|err| err.to_string()),
            Err(why) => Err(format!("its event log is unknown: {why}")),
        };
        let read = read.map_err(|why| format!("sent {missed} events lotar missed and can't read back: {why}"))?;
        self.pending.extend(read.into_iter().map(|event| (event.seq, event)));
        Ok(())
    }
}

trait JobOutputPipe: Read {
    fn output_ready(&self) -> std::io::Result<bool>;
    /// Bytes waiting to be read.
    fn output_available(&self) -> std::io::Result<usize>;
}

#[cfg(unix)]
impl<T: Read + std::os::fd::AsRawFd> JobOutputPipe for T {
    fn output_ready(&self) -> std::io::Result<bool> {
        let mut poll = libc::pollfd { fd: self.as_raw_fd(), events: libc::POLLIN, revents: 0 };
        // One reader owns this pipe; readiness permits a nonblocking-sized read.
        let result = unsafe { libc::poll(&mut poll, 1, 0) };
        if result < 0 { Err(std::io::Error::last_os_error()) } else { Ok(result > 0) }
    }

    fn output_available(&self) -> std::io::Result<usize> {
        let mut available: libc::c_int = 0;
        let result = unsafe { libc::ioctl(self.as_raw_fd(), libc::FIONREAD, &mut available) };
        if result < 0 { Err(std::io::Error::last_os_error()) } else { Ok(usize::try_from(available).unwrap_or(0)) }
    }
}

/// Bytes waiting in a pipe; `None` once the writer closed it (broken pipe).
#[cfg(windows)]
fn peek_named_pipe_available(handle: std::os::windows::io::RawHandle) -> std::io::Result<Option<u32>> {
    #[link(name = "kernel32")]
    unsafe extern "system" {
        #[link_name = "PeekNamedPipe"]
        fn peek_named_pipe(
            handle: *mut std::ffi::c_void,
            buffer: *mut std::ffi::c_void,
            size: u32,
            read: *mut u32,
            available: *mut u32,
            remaining: *mut u32,
        ) -> i32;
    }
    let mut available = 0;
    let result = unsafe { peek_named_pipe(handle, std::ptr::null_mut(), 0, std::ptr::null_mut(), &mut available, std::ptr::null_mut()) };
    if result != 0 {
        return Ok(Some(available));
    }
    let error = std::io::Error::last_os_error();
    if error.raw_os_error() == Some(109) { Ok(None) } else { Err(error) }
}

#[cfg(windows)]
impl<T: Read + std::os::windows::io::AsRawHandle> JobOutputPipe for T {
    fn output_ready(&self) -> std::io::Result<bool> {
        // A closed pipe is ready: the read returns its end.
        Ok(peek_named_pipe_available(self.as_raw_handle())?.is_none_or(|available| available > 0))
    }

    fn output_available(&self) -> std::io::Result<usize> {
        Ok(peek_named_pipe_available(self.as_raw_handle())?.map_or(0, |available| usize::try_from(available).unwrap_or(0)))
    }
}

fn read_job_output(mut pipe: impl JobOutputPipe, exited: &AtomicBool, drain_limit: Duration, mut on_line: impl FnMut(&str)) -> std::io::Result<()> {
    let mut pending = Vec::new();
    let mut buffer = [0; 8192];
    let mut drain_started = None;
    let mut unread_at_exit = None;
    loop {
        let process_exited = exited.load(Ordering::Acquire);
        if process_exited {
            // What the runner wrote before it exited is read however long its
            // lines take; only output after that (from descendants holding the
            // streams) is limited (DEV-358).
            let unread = *unread_at_exit.get_or_insert_with(|| pipe.output_available().unwrap_or(0));
            if unread == 0 && drain_started.get_or_insert_with(Instant::now).elapsed() > drain_limit {
                return Err(std::io::Error::new(std::io::ErrorKind::TimedOut, format!("Runner output drain exceeded {drain_limit:?} after exit")));
            }
        }
        match pipe.output_ready() {
            Ok(true) => {}
            Ok(false) => {
                if process_exited {
                    break;
                }
                thread::sleep(Duration::from_millis(10));
                continue;
            }
            Err(err) if err.kind() == std::io::ErrorKind::Interrupted => continue,
            Err(err) => return Err(err),
        }
        match pipe.read(&mut buffer) {
            Ok(0) => break,
            Ok(count) => {
                pending.extend_from_slice(&buffer[..count]);
                if let Some(unread) = unread_at_exit.as_mut() {
                    *unread = unread.saturating_sub(count);
                }
            }
            Err(err) if err.kind() == std::io::ErrorKind::Interrupted => continue,
            Err(err) => return Err(err),
        }
        let mut consumed = 0;
        for (index, byte) in pending.iter().enumerate() {
            if *byte == b'\n' {
                on_line(&String::from_utf8_lossy(&pending[consumed..=index]));
                consumed = index + 1;
            }
        }
        pending.drain(..consumed);
    }
    if !pending.is_empty() {
        on_line(&String::from_utf8_lossy(&pending));
    }
    Ok(())
}

struct RunningJobSlot<'a> {
    job_id: &'a str,
    ticket_id: &'a str,
    tasks_dir: &'a std::path::Path,
}

impl Drop for RunningJobSlot<'_> {
    fn drop(&mut self) {
        // All process/output teardown and context writes precede ownership transfer.
        // Keep this slot counted while cancellation automation schedules a successor.
        finalize_cancelled_job(self.job_id);
        crate::services::agent_active_jobs::unregister(self.tasks_dir, self.job_id);
        if let Ok(mut registry) = JOB_REGISTRY.lock() {
            if registry.active_by_ticket.get(self.ticket_id).is_some_and(|id| id == self.job_id) {
                registry.active_by_ticket.remove(self.ticket_id);
            }
            if let Some(state) = registry.jobs.get_mut(self.job_id) {
                state.runtime = None;
                state.worker_running = false;
            }
            registry.running_count = registry.running_count.saturating_sub(1);
        }
        process_pending_queue();
    }
}

fn finalize_cancelled_job(job_id: &str) {
    let record = {
        let Ok(mut registry) = JOB_REGISTRY.lock() else {
            return;
        };
        let Some(state) = registry.jobs.get_mut(job_id) else {
            return;
        };
        if state.record.status != AgentJobStatus::Cancelled || state.cancellation_finalized {
            return;
        }
        state.cancellation_finalized = true;
        state.record.finished_at = Some(time::now_rfc3339());
        state.record.summary = state.record.last_message.clone();
        state.runtime = None;
        let record = state.record.clone();
        if registry.active_by_ticket.get(&record.ticket_id).is_some_and(|id| id == job_id) {
            registry.active_by_ticket.remove(&record.ticket_id);
        }
        record
    };
    let _ = AgentLogService::write_status(
        &record.workspace_root,
        record.agent_logs_dir.as_deref(),
        job_id,
        "cancelled",
        record.finished_at.as_deref().unwrap_or_default(),
        record.exit_code,
        record.summary.clone(),
    );
    apply_job_end_rules(job_id, &record.ticket_id, &record.tasks_dir, AutomationEvent::JobCancelled, job_context_for(job_id));
    let prefix = crate::storage::TaskId::parse(&record.ticket_id).ok().map(|parsed| parsed.project);
    if let Ok(config) = crate::config::resolution::config_for_project(&record.tasks_dir, prefix.as_deref()) {
        maybe_cleanup_worktree(job_id, &record.ticket_id, &record.tasks_dir, &config, JobOutcome::Cancelled);
        remove_build_output(job_id, &record.ticket_id, &record.tasks_dir, &config);
        cleanup_done_worktrees(&record.tasks_dir, &config);
    }
}

/// Commit whatever a successful worktree job left uncommitted on its
/// branch. `Ok(Some(note))` when LoTaR made a commit, `Ok(None)` when there
/// was nothing to commit or the job has no worktree.
fn commit_leftover_work(job_id: &str) -> Result<Option<String>, String> {
    let Some((worktree, ticket_id, profile)) = JOB_REGISTRY.lock().ok().and_then(|registry| {
        let record = &registry.jobs.get(job_id)?.record;
        Some((record.worktree_path.clone()?, record.ticket_id.clone(), record.agent_profile.clone()))
    }) else {
        return Ok(None);
    };
    let worktree = std::path::PathBuf::from(worktree);
    if !worktree.is_dir() {
        return Ok(None);
    }
    let git = |args: &[&str]| git_command(&worktree).args(args).output().map_err(|err| format!("git {}: {err}", args.join(" ")));
    let status = git(&["status", "--porcelain"])?;
    if !status.status.success() || status.stdout.is_empty() {
        return Ok(None);
    }
    let failed = |output: std::process::Output| {
        format!("The agent left uncommitted changes and LoTaR could not commit them: {}", String::from_utf8_lossy(&output.stderr).trim())
    };
    let added = git(&["add", "--all"])?;
    if !added.status.success() {
        return Err(failed(added));
    }
    let message = match profile.as_deref() {
        Some(profile) => format!("{ticket_id}: changes from the {profile} agent"),
        None => format!("{ticket_id}: changes from the agent"),
    };
    let committed = git(&["commit", "--quiet", "-m", &message])?;
    if !committed.status.success() {
        return Err(failed(committed));
    }
    Ok(Some(format!("Committed changes the agent left uncommitted: {message}")))
}

/// For a merge job on a worktree branch: make sure the branch is in the
/// main checkout's current branch, fast-forwarding it when the agent could
/// not (a sandbox may keep git metadata read-only). `Ok(Some(note))` when
/// LoTaR fast-forwarded or the agent landed the branch reworked; `Err`
/// explains why the branch is still unmerged. A branch whose work is on
/// main already (`branch_landed`) is merged. So is one the agent landed in
/// a form that conflicts with it: commits on main since the job started,
/// named after the ticket, that change every file the branch changes. A
/// merged branch's commit is recorded as landed, so cleanup at Done can
/// remove it (DEV-411). Jobs that are not merge jobs, and states git can't
/// judge, pass. A job whose agent handed the ticket back (it reassigned the
/// ticket) is not fast-forwarded: it passes with the branch unmerged. A
/// branch that had nothing beyond main when the job started fails while
/// another of the ticket's branches (`prefix` names them) holds work main
/// lacks (DEV-602): the merge missed the fix.
fn ensure_branch_merged(job_id: &str, ticket_id: &str, tasks_dir: &std::path::Path, prefix: &str) -> Result<Option<String>, String> {
    let Some((branch, start, assignee_at_start)) =
        JOB_REGISTRY.lock().ok().and_then(|registry| {
            registry.jobs.get(job_id).filter(|state| state.record.is_merge_job).and_then(|state| {
                state.record.worktree_branch.clone().map(|branch| (branch, state.main_head_at_start.clone(), state.assignee_at_start.clone()))
            })
        })
    else {
        return Ok(None);
    };
    let Some(repo_root) = resolve_repo_root(tasks_dir) else {
        return Ok(None);
    };
    match merged_landed(&repo_root, tasks_dir, &branch) {
        Ok(true) => {
            let brought_nothing = start.as_deref().is_some_and(|start| {
                git_stdout(&repo_root, &["cherry", start, &format!("refs/heads/{branch}")]).is_ok_and(|out| !out.lines().any(|line| line.starts_with('+')))
            });
            if brought_nothing && let Some(holder) = unlanded_ticket_branch(tasks_dir, ticket_id, prefix, Some(&branch)) {
                return Err(format!(
                    "Merge job finished, but {branch} has nothing beyond the main checkout's branch, while {holder} holds {ticket_id}'s work that isn't in it: the merge missed the fix"
                ));
            }
            record_landing(&repo_root, &branch);
            return Ok(None);
        }
        // A branch git can't judge passes, like a merged one.
        Err(_) => return Ok(None),
        Ok(false) => {}
    }
    // An agent that handed the ticket back (for example because a check failed
    // on the work) left the branch unmerged on purpose (DEV-516). A handback
    // reassigns the ticket; finishing never does, with or without end rules,
    // though it may set a status. A T3 Code thread's change waits as the
    // job's outcome; an agent with the job's environment changes it directly.
    // A job taken over from an older lotar may have no starting assignee;
    // its recorded outcome still counts.
    let recorded = crate::services::agent_active_jobs::peek_outcome(tasks_dir, job_id).and_then(|outcome| outcome.assignee);
    let changed = || {
        let at_start = assignee_at_start?;
        ticket_state(tasks_dir, ticket_id).map(|(_, assignee)| assignee).filter(|now| *now != at_start).map(Option::unwrap_or_default)
    };
    if let Some(assignee) = recorded.or_else(changed) {
        let to = if assignee.is_empty() { "unassigned".to_string() } else { format!("to {assignee}") };
        return Ok(Some(format!("The agent handed {ticket_id} back ({to}): LoTaR leaves {branch} unmerged")));
    }
    let output = git_command(&repo_root).args(["merge", "--ff-only", "--quiet", &branch]).output().map_err(|err| format!("git merge: {err}"))?;
    if output.status.success() {
        record_landing(&repo_root, &branch);
        return Ok(Some(format!("Fast-forwarded the main checkout to {branch}")));
    }
    let reworked = start.map(|start| reworked_landing(&repo_root, &start, &branch, ticket_id)).unwrap_or_default();
    if !reworked.is_empty()
        && let Some(head) = record_landing(&repo_root, &branch)
    {
        let short = |commit: &str| commit.get(..12).unwrap_or(commit).to_string();
        return Ok(Some(format!(
            "{branch} conflicts with the main checkout, but this job landed its work reworked as {}; recorded {branch} at {} as landed in {}",
            reworked.iter().map(|commit| short(commit)).collect::<Vec<_>>().join(", "),
            rev_parse(&repo_root, &format!("refs/heads/{branch}")).map_or_else(String::new, |tip| short(&tip)),
            short(&head)
        )));
    }
    Err(format!(
        "Merge job finished, but {branch} is not merged into the main checkout's current branch and can't be fast-forwarded: {}",
        String::from_utf8_lossy(&output.stderr).trim()
    ))
}

/// Whether a merge job's `branch` is in the checkout's current branch:
/// `branch_landed`, or landed apart from the tasks directory. The merge
/// amends ticket changes into the landed commit (QIS-180), so its copy on
/// main may differ from the branch, and conflict with it, there alone.
fn merged_landed(repo_root: &std::path::Path, tasks_dir: &std::path::Path, branch: &str) -> LoTaRResult<bool> {
    if branch_landed(repo_root, branch)? {
        return Ok(true);
    }
    let tasks_dir = std::path::absolute(tasks_dir).map_err(LoTaRError::IoError)?;
    match tasks_dir.strip_prefix(repo_root) {
        Ok(tasks) => branch_landed_outside(repo_root, branch, tasks),
        Err(_) => Ok(false),
    }
}

/// For a merge job whose runner failed: the note that explains why it
/// completes anyway, when its branch is on main already (`merged_landed`)
/// and its worktree holds nothing uncommitted. `None` for other jobs, and
/// for states git can't judge.
fn merged_despite_failure(job_id: &str, tasks_dir: &std::path::Path, exit_code: Option<i32>, runner_failed: bool) -> Option<String> {
    let (runner, branch, worktree) = JOB_REGISTRY.lock().ok().and_then(|registry| {
        registry
            .jobs
            .get(job_id)
            .filter(|state| state.record.is_merge_job)
            .and_then(|state| state.record.worktree_branch.clone().map(|branch| (state.record.runner.clone(), branch, state.record.worktree_path.clone())))
    })?;
    let repo_root = resolve_repo_root(tasks_dir)?;
    if let Some(worktree) = worktree.map(std::path::PathBuf::from).filter(|path| path.exists()) {
        let status = git_command(&worktree).args(["status", "--porcelain"]).output().ok()?;
        if !status.status.success() || !status.stdout.is_empty() {
            return None;
        }
    }
    if !merged_landed(&repo_root, tasks_dir, &branch).ok()? {
        return None;
    }
    let failure = if runner_failed { "reported a failure".to_string() } else { exit_description(exit_code) };
    Some(format!("The {runner} runner {failure}, but {branch} is in the main checkout's current branch already: the merge job completed"))
}

/// The commit a job's worktree `branch` ended on, when the ticket's branch
/// points at it too (for [`continued_thread_worktree`]).
fn ended_on_ticket_branch(tasks_dir: &std::path::Path, ticket_id: &str, config: &ResolvedConfig, branch: Option<&str>) -> Option<String> {
    let repo_root = resolve_repo_root(tasks_dir)?;
    let tip = rev_parse(&repo_root, &format!("refs/heads/{}", branch?))?;
    let ticket_branch = build_worktree_branch(&config.agent_worktree.branch_prefix, &sanitize_worktree_token(ticket_id));
    (rev_parse(&repo_root, &format!("refs/heads/{ticket_branch}")).as_deref() == Some(tip.as_str())).then_some(tip)
}

fn finalize_job(
    job_id: &str,
    ticket_id: &str,
    exit_code: Option<i32>,
    context_messages: &[crate::services::agent_context_service::AgentContextMessage],
    tasks_dir: &std::path::Path,
    config: &ResolvedConfig,
) {
    let now = time::now_rfc3339();
    // A runner that reported a failure fails the job even when it exits 0.
    let runner_failed = JOB_REGISTRY.lock().ok().and_then(|registry| registry.jobs.get(job_id).map(|state| state.runner_failed)).unwrap_or(false);
    let mut success = exit_code.unwrap_or(1) == 0 && !runner_failed;
    // A merge job whose runner failed after its branch reached main did its
    // work (DEV-465): it completes, so automation doesn't retry a merged fix.
    if !success && let Some(message) = merged_despite_failure(job_id, tasks_dir, exit_code, runner_failed) {
        update_job(job_id, |state| {
            record_and_emit(state, "agent_job_progress", Some(message.clone()));
        });
        success = true;
    }
    // Work an agent leaves uncommitted in its worktree (Codex's sandbox keeps
    // git metadata read-only, signing may be unreachable) is committed here,
    // so the next phase and the merge see it. Failing to commit fails the job.
    let mut commit_failure = None;
    if success {
        match commit_leftover_work(job_id) {
            Ok(Some(message)) => update_job(job_id, |state| {
                record_and_emit(state, "agent_job_progress", Some(message.clone()));
            }),
            Ok(None) => {}
            Err(message) => commit_failure = Some(message),
        }
    }
    if commit_failure.is_some() {
        success = false;
    }
    // After that commit, so the ticket's branch gets LoTaR's commit too. A
    // branch that can't follow fails the job: automation must not hand the
    // ticket on to a job that would read the stale branch.
    if let Err(note) = follow_thread_branch(job_id, tasks_dir, ticket_id, config) {
        if success {
            commit_failure = Some(note);
            success = false;
        } else {
            update_job(job_id, |state| {
                record_and_emit(state, "agent_job_progress", Some(note.clone()));
            });
        }
    }
    // A merge job must end with its branch in the main checkout; LoTaR
    // fast-forwards when the agent couldn't, and fails the job otherwise so
    // automation doesn't treat the ticket as merged.
    let failure_note = if success {
        match ensure_branch_merged(job_id, ticket_id, tasks_dir, &config.agent_worktree.branch_prefix) {
            Ok(Some(message)) => {
                update_job(job_id, |state| {
                    record_and_emit(state, "agent_job_progress", Some(message.clone()));
                });
                None
            }
            Ok(None) => None,
            Err(message) => Some(message),
        }
    } else {
        commit_failure
    };
    if failure_note.is_some() {
        success = false;
    }
    // Every failure says why (DEV-401); the agent's own last words stay
    // the summary unless LoTaR's note explains the failure.
    let failure_reason = failure_note.clone().or_else(|| {
        let runner = JOB_REGISTRY.lock().ok().and_then(|registry| registry.jobs.get(job_id).map(|state| state.record.runner.clone())).unwrap_or_default();
        (!success).then(|| {
            if runner_failed { format!("The {runner} runner reported a failure.") } else { format!("The {runner} runner {}.", exit_description(exit_code)) }
        })
    });
    // Before the job ends: whoever waits for its end reads the ticket then.
    let worktree_branch = JOB_REGISTRY.lock().ok().and_then(|registry| registry.jobs.get(job_id).and_then(|state| state.record.worktree_branch.clone()));
    let ticket_commit = ended_on_ticket_branch(tasks_dir, ticket_id, config, worktree_branch.as_deref());
    let session_name = JOB_REGISTRY.lock().ok().and_then(|registry| registry.jobs.get(job_id).and_then(|state| state.record.session_id.clone()));
    let agent_session = session_name.and_then(|name| crate::services::agent_runner::registry().load(&name).ok()).and_then(|record| record.agent_session);
    let mut finalized = false;
    update_job(job_id, |state| {
        if state.record.status == AgentJobStatus::Cancelled {
            return;
        }
        state.record.exit_code = exit_code;
        state.record.finished_at = Some(now.clone());
        state.record.status = if success { AgentJobStatus::Completed } else { AgentJobStatus::Failed };
        if let Some(message) = failure_note.as_ref().or(failure_reason.as_ref().filter(|_| state.record.last_message.is_none())) {
            state.record.last_message = Some(message.clone());
        }
        state.record.summary = state.record.last_message.clone();
        finalized = true;
        let event_kind = if success { "agent_job_completed" } else { "agent_job_failed" };
        record_and_emit(state, event_kind, failure_reason.clone());
    });
    if !finalized {
        let _ = AgentContextService::append_messages(tasks_dir, config, ticket_id, context_messages.to_vec(), None);
        return;
    }

    // Write final status to persistent log (if logging is enabled)
    let status_str = if success { "completed" } else { "failed" };
    let (summary, workspace_root, logs_dir, usage, session_id, worktree_path, worktree_branch) = if let Ok(registry) = JOB_REGISTRY.lock() {
        if let Some(state) = registry.jobs.get(job_id) {
            (
                state.record.summary.clone(),
                state.record.workspace_root.clone(),
                state.record.agent_logs_dir.clone(),
                state.record.usage.clone(),
                state.record.session_id.clone(),
                state.record.worktree_path.clone(),
                state.record.worktree_branch.clone(),
            )
        } else {
            (None, tasks_dir.to_path_buf(), None, None, None, None, None)
        }
    } else {
        (None, tasks_dir.to_path_buf(), None, None, None, None, None)
    };
    let _ = AgentLogService::write_status_with_usage(
        &workspace_root,
        logs_dir.as_deref(),
        job_id,
        crate::services::agent_log_service::JobStatusLine {
            status: status_str.to_string(),
            at: time::now_rfc3339(),
            exit_code,
            summary,
            usage,
            session_id,
            worktree_path,
            worktree_branch,
            ticket_commit,
            agent_session,
        },
    );

    let job_context = job_context_for(job_id);
    let mut registry = JOB_REGISTRY.lock().ok();
    if let Some(ref mut registry) = registry {
        if registry.active_by_ticket.get(ticket_id).is_some_and(|id| id == job_id) {
            registry.active_by_ticket.remove(ticket_id);
        }
        if let Some(state) = registry.jobs.get_mut(job_id) {
            state.runtime = None;
        }
    }
    drop(registry);

    // The context goes first: the end rules may start the next phase's agent, whose prompt carries it.
    let _ = AgentContextService::append_messages(tasks_dir, config, ticket_id, context_messages.to_vec(), None);

    if !should_stop_job(job_id) {
        let event = if success { AutomationEvent::JobCompleted } else { AutomationEvent::JobFailed };
        apply_job_end_rules(job_id, ticket_id, tasks_dir, event, job_context);
    }

    maybe_cleanup_worktree(job_id, ticket_id, tasks_dir, config, if success { JobOutcome::Success } else { JobOutcome::Failure });
    remove_build_output(job_id, ticket_id, tasks_dir, config);
    cleanup_done_worktrees(tasks_dir, config);
}

/// How a runner process ended, for a failure's reason.
fn exit_description(exit_code: Option<i32>) -> String {
    match exit_code {
        Some(code) => format!("exited with code {code}"),
        None => "was ended by a signal".to_string(),
    }
}

fn update_job_failure(job_id: &str, ticket_id: &str, message: String, tasks_dir: &std::path::Path) {
    let now = time::now_rfc3339();
    let mut failed = false;
    update_job(job_id, |state| {
        if !matches!(state.record.status, AgentJobStatus::Queued | AgentJobStatus::Running) {
            return;
        }
        failed = true;
        state.record.status = AgentJobStatus::Failed;
        state.record.finished_at = Some(now.clone());
        state.record.last_message = Some(message.clone());
        state.record.summary = Some(message.clone());
        record_and_emit(state, "agent_job_failed", Some(message.clone()));
    });
    if !failed {
        return;
    }

    // Write final status to persistent log (if logging is enabled)
    let (workspace_root, logs_dir) = if let Ok(registry) = JOB_REGISTRY.lock() {
        if let Some(state) = registry.jobs.get(job_id) {
            (state.record.workspace_root.clone(), state.record.agent_logs_dir.clone())
        } else {
            (tasks_dir.to_path_buf(), None)
        }
    } else {
        (tasks_dir.to_path_buf(), None)
    };
    let _ = AgentLogService::write_status(&workspace_root, logs_dir.as_deref(), job_id, "failed", &now, None, Some(message));

    let mut registry = JOB_REGISTRY.lock().ok();
    if let Some(ref mut registry) = registry
        && registry.active_by_ticket.get(ticket_id).is_some_and(|id| id == job_id)
    {
        registry.active_by_ticket.remove(ticket_id);
    }
    drop(registry);
    apply_job_end_rules(job_id, ticket_id, tasks_dir, AutomationEvent::JobFailed, job_context_for(job_id));
}

/// Runs the rules for the job's end and logs where they moved the ticket,
/// so the agent's summary needn't say it (DEV-469).
fn apply_job_end_rules(job_id: &str, ticket_id: &str, tasks_dir: &std::path::Path, event: AutomationEvent, job_context: Option<AutomationJobContext>) {
    let before = ticket_state(tasks_dir, ticket_id);
    if let (Some(job), Some((_, assignee))) = (job_context.as_ref(), before.as_ref())
        && crate::services::automation_service::reassigned_during_job(job, assignee.as_deref())
    {
        let to = assignee.as_deref().map_or_else(|| "unassigned".to_string(), |assignee| format!("to {assignee}"));
        update_job(job_id, |state| {
            push_event(state, "agent_job_progress", Some(format!("{ticket_id} was reassigned ({to}) while the job ran: its end rules don't move it again")));
        });
    }
    if let Err(err) = AutomationService::apply_job_event(tasks_dir, ticket_id, event, job_context) {
        update_job(job_id, |state| {
            record_and_emit(state, "agent_job_progress", Some(format!("The end rules for {ticket_id} failed: {err}")));
        });
    }
    if let (Some(before), Some(after)) = (before, ticket_state(tasks_dir, ticket_id))
        && let Some(message) = ticket_move_note(ticket_id, &before, &after)
    {
        update_job(job_id, |state| {
            push_event(state, "agent_job_progress", Some(message.clone()));
        });
    }
}

/// A job's starting assignee as its active-job record keeps it: empty for
/// none, so it survives a takeover (DEV-516).
fn recorded_assignee(assignee_at_start: Option<Option<String>>) -> Option<String> {
    assignee_at_start.map(Option::unwrap_or_default)
}

/// The job runs from now on. Its record is saved before the start is
/// logged, so a process killed from then on leaves the job to the next
/// worker or server, which ends it (DEV-593). The record names no session
/// yet: a takeover must not resume a continued session this job never reached.
fn mark_started(state: &mut AgentJobState, tasks_dir: &std::path::Path, ticket_id: &str, started_at: &str) {
    state.record.status = AgentJobStatus::Running;
    state.record.started_at = Some(started_at.to_string());
    crate::services::agent_active_jobs::register(tasks_dir, &active_record_of(state, ticket_id, started_at, None));
    record_and_emit(state, "agent_job_started", None);
}

/// The record other processes see of a job this process runs (DEV-279).
fn active_record_of(state: &AgentJobState, ticket_id: &str, started_at: &str, session_id: Option<String>) -> crate::services::agent_active_jobs::ActiveJob {
    crate::services::agent_active_jobs::ActiveJob {
        job_id: state.record.id.clone(),
        ticket_id: ticket_id.to_string(),
        runner: state.record.runner.clone(),
        agent: state.record.agent_profile.clone(),
        owner_pid: std::process::id(),
        child_pid: 0,
        started_at: started_at.to_string(),
        session_id,
        merge: state.record.is_merge_job,
        assignee_at_start: recorded_assignee(state.assignee_at_start.clone()),
        in_setup: false,
    }
}

/// The starting assignee a taken-over job's record kept; see [`recorded_assignee`].
fn restored_assignee(recorded: Option<String>) -> Option<Option<String>> {
    recorded.map(|assignee| Some(assignee).filter(|assignee| !assignee.is_empty()))
}

/// The ticket's status and assignee.
fn ticket_state(tasks_dir: &std::path::Path, ticket_id: &str) -> Option<(String, Option<String>)> {
    TaskService::get(&Storage::new(tasks_dir), ticket_id, None).ok().map(|task| (task.status.to_string(), task.assignee))
}

/// "DEV-1 is now Review, assigned to mallox" when the job's end changed the
/// ticket's status or assignee.
fn ticket_move_note(ticket_id: &str, before: &(String, Option<String>), after: &(String, Option<String>)) -> Option<String> {
    if before == after {
        return None;
    }
    let assignee = after.1.as_deref().map_or_else(|| "unassigned".to_string(), |assignee| format!("assigned to {assignee}"));
    Some(format!("{ticket_id} is now {}, {assignee}", after.0))
}

fn update_job<F>(job_id: &str, mut updater: F)
where
    F: FnMut(&mut AgentJobState),
{
    if let Ok(mut registry) = JOB_REGISTRY.lock()
        && let Some(state) = registry.jobs.get_mut(job_id)
    {
        updater(state);
    }
    flush_event_log();
}

/// Start the waiting jobs that have a free slot, in queue order.
fn process_pending_queue() {
    let started: Vec<PendingJob> = {
        let Ok(mut registry) = JOB_REGISTRY.lock() else {
            return;
        };
        let mut started = Vec::new();
        loop {
            let merge_busy = has_dispatched_merge_job(&registry);
            // A config that can't be read keeps the job waiting.
            let next = registry.pending_queue.iter().position(|pending| {
                !(pending.is_merge_job && merge_busy) && matches!(slot_wait_reason(&registry, &pending.tasks_dir, &pending.ticket_id), Ok(None))
            });
            let Some(pending) = next.and_then(|index| registry.pending_queue.remove(index)) else {
                break;
            };
            registry.running_count += 1;
            started.push(pending);
        }
        started
    };

    // Start the jobs outside the lock
    for pending in started {
        thread::spawn(move || {
            run_job(
                pending.job_id,
                pending.runner_kind,
                pending.profile,
                pending.tasks_dir,
                pending.ticket_id,
                pending.prompt,
                pending.user_prompt,
                pending.config,
            );
        });
    }
}

/// Why a job for the ticket waits under `agent.worktree.max_parallel_jobs`:
/// the queue worker's rule, with the jobs that hold this process's slots.
fn slot_wait_reason(registry: &JobRegistry, tasks_dir: &std::path::Path, ticket_id: &str) -> LoTaRResult<Option<String>> {
    crate::services::agent_queue_service::AgentQueueService::slot_wait_reason(tasks_dir, ticket_id, slot_jobs(registry))
}

/// The jobs (ticket, job id) that hold one of this process's slots: those
/// dispatched or still running, not those waiting in its queue.
fn slot_jobs(registry: &JobRegistry) -> Vec<(String, String)> {
    let waiting: HashSet<&str> = registry.pending_queue.iter().map(|pending| pending.job_id.as_str()).collect();
    let mut jobs: HashSet<(String, String)> =
        registry.active_by_ticket.iter().filter(|(_, job)| !waiting.contains(job.as_str())).map(|(ticket, job)| (ticket.clone(), job.clone())).collect();
    jobs.extend(registry.jobs.iter().filter(|(_, state)| state.worker_running).map(|(job, state)| (state.record.ticket_id.clone(), job.clone())));
    jobs.into_iter().collect()
}

static QUEUE_WATCHED: AtomicBool = AtomicBool::new(false);
const QUEUE_WATCH_INTERVAL: Duration = Duration::from_millis(500);

/// A job of another process frees a slot without telling this one: while
/// jobs wait in this process's queue, look for a free slot (DEV-646).
fn watch_pending_queue() {
    if QUEUE_WATCHED.swap(true, Ordering::SeqCst) {
        return;
    }
    let waiting = || JOB_REGISTRY.lock().is_ok_and(|registry| !registry.pending_queue.is_empty());
    thread::spawn(move || {
        loop {
            thread::sleep(QUEUE_WATCH_INTERVAL);
            process_pending_queue();
            if waiting() {
                continue;
            }
            QUEUE_WATCHED.store(false, Ordering::SeqCst);
            // A job queued meanwhile found the watch still running.
            if !waiting() || QUEUE_WATCHED.swap(true, Ordering::SeqCst) {
                return;
            }
        }
    });
}

/// A merge job holds the merge slot from its dispatch on, also while it
/// still sets up its worktree (status queued).
fn has_dispatched_merge_job(registry: &JobRegistry) -> bool {
    registry.jobs.iter().any(|(id, state)| {
        state.record.is_merge_job
            && matches!(state.record.status, AgentJobStatus::Queued | AgentJobStatus::Running)
            && !registry.pending_queue.iter().any(|pending| &pending.job_id == id)
    })
}

fn has_active_merge_job(registry: &JobRegistry) -> bool {
    registry.jobs.values().any(|state| state.record.is_merge_job && matches!(state.record.status, AgentJobStatus::Queued | AgentJobStatus::Running))
}

fn is_merge_job_candidate(task: &crate::api_types::TaskDTO, agent_profile: Option<&str>) -> bool {
    task.status.as_str().eq_ignore_ascii_case("Merging") || is_merge_profile(agent_profile)
}

/// The tag a blocking review sets; a rework removes it.
const REVIEW_BLOCKED_TAG: &str = "review-blocked";

/// Why a merge job (profile `merge`, or a ticket in `Merging`) can't start
/// (DEV-566): its ticket carries a blocking review that no rework has
/// answered yet.
pub(crate) fn review_blocks_merge(task: &crate::api_types::TaskDTO, agent_profile: Option<&str>) -> Option<String> {
    let blocked = is_merge_job_candidate(task, agent_profile) && task.tags.iter().any(|tag| tag.trim().eq_ignore_ascii_case(REVIEW_BLOCKED_TAG));
    let job = agent_profile.map(|profile| format!("@{}", profile.trim().trim_start_matches('@'))).unwrap_or_else(|| "merge".to_string());
    blocked.then(|| format!("The {job} job didn't start: {} carries tag {REVIEW_BLOCKED_TAG}, and no rework has removed it.", task.id))
}

fn is_merge_profile(agent_profile: Option<&str>) -> bool {
    agent_profile.is_some_and(|profile| profile.trim().eq_ignore_ascii_case("merge"))
}

/// A taken-over job is a merge job when it started as one (DEV-544): the
/// ticket's status now may be the next phase's. A record of an older build
/// has only its profile to tell.
fn takes_over_as_merge_job(job: &crate::services::agent_active_jobs::ActiveJob) -> bool {
    job.merge || is_merge_profile(job.agent.as_deref())
}

fn should_stop_job(job_id: &str) -> bool {
    if let Ok(registry) = JOB_REGISTRY.lock()
        && let Some(state) = registry.jobs.get(job_id)
    {
        return state.record.status == AgentJobStatus::Cancelled;
    }
    false
}

fn push_event(state: &mut AgentJobState, kind: &str, message: Option<String>) -> AgentJobEvent {
    let at = time::now_rfc3339();
    state.next_sequence = state.next_sequence.saturating_add(1);
    let event = AgentJobEvent { kind: kind.to_string(), sequence: state.next_sequence, at: at.clone(), message };
    state.events.push(event.clone());
    if state.events.len() > EVENT_LOG_LIMIT {
        let start = state.events.len().saturating_sub(EVENT_LOG_LIMIT);
        state.events = state.events[start..].to_vec();
    }

    // Also append to persistent log file (if logging is enabled), once the
    // caller has released the registry: see `flush_event_log`.
    if let Ok(mut queue) = EVENT_LOG_QUEUE.lock() {
        queue.push_back(QueuedLogEvent {
            workspace_root: state.record.workspace_root.clone(),
            logs_dir: state.record.agent_logs_dir.clone(),
            job_id: state.record.id.clone(),
            entry: crate::services::agent_log_service::JobLogEntry {
                kind: event.kind.clone(),
                at: event.at.clone(),
                message: event.message.clone(),
                sequence: Some(event.sequence),
            },
        });
    }
    event
}

/// Write the event log lines `push_event` queued. Call it after releasing
/// `JOB_REGISTRY`; the lines are written by the time it returns.
fn flush_event_log() {
    let Ok(_writer) = EVENT_LOG_WRITER.lock() else {
        return;
    };
    loop {
        let Some(entry) = EVENT_LOG_QUEUE.lock().ok().and_then(|mut queue| queue.pop_front()) else {
            return;
        };
        let _ = AgentLogService::append_event(&entry.workspace_root, entry.logs_dir.as_deref(), &entry.job_id, entry.entry);
    }
}

fn emit_job_event(event: &AgentJobEvent, record: &AgentJobRecord) {
    let payload = json!({
        "id": record.id,
        "ticket_id": record.ticket_id,
        "runner": record.runner,
        "agent": record.agent_profile,
        "status": record.status.as_str(),
        "created_at": record.created_at,
        "started_at": record.started_at,
        "finished_at": record.finished_at,
        "exit_code": record.exit_code,
        "sequence": event.sequence,
        "at": event.at,
        "message": event.message,
        "worktree_path": record.worktree_path,
        "worktree_branch": record.worktree_branch,
        "model": record.model,
        "effort": record.effort,
        "usage": record.usage,
        "question": record.question,
    });
    crate::api_events::emit(&crate::api_events::ApiEvent { kind: event.kind.clone(), data: payload });
}

/// Finished jobs stay in memory for a day (status, events for the UI) and
/// at most `MAX_FINISHED_JOBS` of them, so a long-running server doesn't
/// grow without bound (DEV-279). Their logs stay on disk.
const MAX_FINISHED_JOBS: usize = 500;

fn prune_finished_jobs(registry: &mut JobRegistry) {
    let cutoff = Timestamp::now() - SignedDuration::from_hours(24);
    let finished_at = |state: &AgentJobState| state.record.finished_at.as_deref().and_then(|at| time::parse_rfc3339(at).ok());
    let mut finished: Vec<(Timestamp, String)> = registry
        .jobs
        .iter()
        .filter(|(_, state)| {
            matches!(state.record.status, AgentJobStatus::Completed | AgentJobStatus::Failed | AgentJobStatus::Cancelled) && state.runtime.is_none()
        })
        .filter_map(|(id, state)| finished_at(state).map(|at| (at, id.clone())))
        .collect();
    finished.sort();
    let over = finished.len().saturating_sub(MAX_FINISHED_JOBS);
    for (index, (at, id)) in finished.into_iter().enumerate() {
        if index < over || at < cutoff {
            registry.jobs.remove(&id);
        }
    }
}

/// Record an event and stream it to the UI (SSE). Notes only the job log
/// needs use bare `push_event`.
fn record_and_emit(state: &mut AgentJobState, kind: &str, message: Option<String>) -> AgentJobEvent {
    let event = push_event(state, kind, message);
    emit_job_event(&event, &state.record);
    event
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::types::{AgentInstructionsConfig, AgentProfileConfig, GlobalConfig};
    use crate::services::agent_context_service::AgentContextService;
    use std::fs;
    use std::io::Write;
    use tempfile::tempdir;

    fn sample_task() -> crate::api_types::TaskDTO {
        crate::api_types::TaskDTO {
            url: None,
            task_state: None,
            deleted_at: None,
            archived: false,
            moved_from: Vec::new(),
            id: "TEST-1".to_string(),
            title: "Test task".to_string(),
            status: crate::types::TaskStatus::from("Todo"),
            priority: crate::types::Priority::from("High"),
            task_type: crate::types::TaskType::from("Feature"),
            reporter: None,
            assignee: None,
            created: "now".to_string(),
            modified: "now".to_string(),
            due_date: None,
            effort: None,
            subtitle: None,
            description: Some("Do the thing".to_string()),
            tags: vec!["infra".to_string()],
            relationships: Default::default(),
            comments: vec![],
            references: vec![],
            acceptance_criteria: vec![],
            sprints: vec![],
            sprint_order: Default::default(),
            releases: Vec::new(),
            history: vec![],
            custom_fields: Default::default(),
        }
    }

    const BRIEF: PromptHistory = PromptHistory { full: false, entries: DEFAULT_CONTEXT_ENTRIES };

    fn comment(date: &str, author: &str, text: &str) -> crate::types::TaskComment {
        crate::types::TaskComment::new(date.to_string(), Some(author.to_string()), text.to_string())
    }

    fn assigned(at: &str, old: Option<&str>, new: &str) -> crate::types::TaskChangeLogEntry {
        crate::types::TaskChangeLogEntry {
            at: at.to_string(),
            actor: Some("claude".to_string()),
            changes: vec![crate::types::TaskChange { field: "assignee".to_string(), old: old.map(str::to_string), new: Some(new.to_string()) }],
            ..Default::default()
        }
    }

    /// DEV-620: a ticket that went fix, question, answer, fix, review, rework and review again. The prompt carries
    /// the question and its answer and the newest fix and review comments, cut, and says how to read the rest.
    fn churned_task() -> crate::api_types::TaskDTO {
        let mut task = sample_task();
        task.comments = vec![
            comment("2026-11-16T01:00:00Z", "claude", "Plan: old fix progress"),
            comment("2026-11-16T02:00:00Z", "claude", "Question: A or B?"),
            comment("2026-11-16T03:00:00Z", "mallox", "Answer: A"),
            comment("2026-11-16T04:00:00Z", "claude", "Done as decided: first fix"),
            comment("2026-11-16T05:00:00Z", "codex", "Review: blocked, first review"),
            comment("2026-11-16T06:00:00Z", "claude", &format!("Reworked: {}", "x".repeat(2000))),
            comment("2026-11-16T07:00:00Z", "codex", "Review: passed"),
        ];
        task.history = vec![
            assigned("2026-11-16T00:30:00Z", None, "@fix"),
            assigned("2026-11-16T02:00:10Z", Some("@fix"), "mallox"),
            assigned("2026-11-16T03:00:05Z", Some("mallox"), "@fix"),
            assigned("2026-11-16T04:00:10Z", Some("@fix"), "@review-codex"),
            assigned("2026-11-16T05:00:10Z", Some("@review-codex"), "@fix"),
            assigned("2026-11-16T06:00:10Z", Some("@fix"), "@review-codex"),
            assigned("2026-11-16T07:00:10Z", Some("@review-codex"), "@merge"),
        ];
        task
    }

    #[test]
    fn the_prompt_carries_the_question_and_the_newest_fix_and_review_comments() {
        let task = churned_task();
        let picked: Vec<(&str, &str)> = prompt_comments(&task).into_iter().map(|(label, comment)| (label, comment.date.as_str())).collect();
        assert_eq!(
            picked,
            vec![
                ("Question", "2026-11-16T02:00:00Z"),
                ("Answer", "2026-11-16T03:00:00Z"),
                ("Newest fix comment", "2026-11-16T06:00:00Z"),
                ("Newest review comment", "2026-11-16T07:00:00Z"),
            ]
        );

        let prompt = build_prompt(&task, "merge it", None, BRIEF, None, None, None);
        assert!(prompt.contains("Comments (7 on the ticket;"), "{prompt}");
        assert!(prompt.contains("`lotar show TEST-1`") && prompt.contains("`lotar task timeline TEST-1`"), "{prompt}");
        assert!(prompt.contains("- Question, by claude, 2026-11-16T02:00:00Z:\nQuestion: A or B?"), "{prompt}");
        assert!(!prompt.contains("old fix progress") && !prompt.contains("first review") && !prompt.contains("first fix"), "{prompt}");
        assert!(prompt.contains(&format!("Reworked: {} …", "x".repeat(PROMPT_EXCERPT_CHARS - "Reworked: ".len()))), "cut at the set length");
    }

    /// DEV-620, as on DEV-563: the fix agent works on while the owner holds the ticket after a hand-back, with
    /// automation comments in between, and the hand-back is logged just before its question.
    #[test]
    fn work_after_a_hand_back_counts_for_the_phase_before_it() {
        let mut task = sample_task();
        let mut question = comment("2026-11-16T02:00:00.5Z", "automation", "Retry with which agent?");
        question.question = serde_json::from_value(serde_json::json!({"options": []})).ok();
        task.comments = vec![
            comment("2026-11-16T01:00:00Z", "claude", "first fix report"),
            question,
            comment("2026-11-16T02:30:00Z", "mallox", "Answer: retry"),
            comment("2026-11-16T03:00:00Z", "claude", "Rebased and checked"),
            comment("2026-11-16T03:30:00Z", "automation", "The fix agent's job failed"),
        ];
        task.history = vec![assigned("2026-11-16T00:30:00Z", None, "@fix"), assigned("2026-11-16T02:00:00.1Z", Some("@fix"), "mallox")];
        let picked: Vec<(&str, &str)> = prompt_comments(&task).into_iter().map(|(label, comment)| (label, comment.text.as_str())).collect();
        assert_eq!(picked, vec![("Question", "Retry with which agent?"), ("Answer", "Answer: retry"), ("Newest fix comment", "Rebased and checked")]);
    }

    #[test]
    fn a_question_without_an_answer_is_the_open_question() {
        let mut task = churned_task();
        task.comments.truncate(2);
        task.history.truncate(2);
        let picked: Vec<&str> = prompt_comments(&task).into_iter().map(|(label, _)| label).collect();
        assert_eq!(picked, vec!["Newest fix comment", "Open question"]);
    }

    #[test]
    fn recent_context_keeps_the_newest_entries_cut_and_full_history_brings_back_the_old_prompt() {
        use crate::services::agent_context_service::{AgentContextFile, build_assistant_message, build_user_message};
        let task = churned_task();
        let context = AgentContextFile {
            ticket_id: task.id.clone(),
            updated_at: "now".to_string(),
            messages: vec![
                build_user_message("oldest prompt"),
                build_assistant_message("old reply"),
                build_user_message("newest prompt"),
                build_assistant_message(&"r".repeat(3000)),
            ],
        };

        let brief = build_prompt(&task, "go", Some(&context), BRIEF, None, None, None);
        assert!(brief.contains("Recent context (the newest 2 of 4 prompts and replies"), "{brief}");
        assert!(!brief.contains("oldest prompt") && brief.contains("- user: newest prompt"), "{brief}");
        assert!(brief.contains(&format!("- assistant: {} …\n", "r".repeat(PROMPT_EXCERPT_CHARS))), "{brief}");

        let none = build_prompt(&task, "go", Some(&context), PromptHistory { full: false, entries: 0 }, None, None, None);
        assert!(!none.contains("Recent context"), "{none}");

        let full = build_prompt(&task, "go", Some(&context), PromptHistory { full: true, entries: 0 }, None, None, None);
        assert!(full.contains("\nRecent context:\n- user: oldest prompt\n"), "{full}");
        assert!(full.contains(&format!("- assistant: {}\n", "r".repeat(3000))), "uncut");
        assert!(!full.contains("Comments ("), "no comment excerpts: {full}");
    }

    #[test]
    fn build_prompt_includes_ticket_and_request() {
        let task = sample_task();
        let prompt = build_prompt(&task, "ship it", None, BRIEF, None, None, None);
        assert!(prompt.contains("TEST-1"));
        assert!(prompt.contains("ship it"));
    }

    #[test]
    fn build_prompt_includes_instructions() {
        let task = sample_task();
        let prompt = build_prompt(&task, "ship it", None, BRIEF, None, Some("Use the tools"), Some("- Commit to the branch."));
        assert!(prompt.contains("Agent instructions:"));
        assert!(prompt.contains("Use the tools"));
        let notes_at = prompt.find("This job:\n- Commit to the branch.").unwrap();
        assert!(notes_at < prompt.find("Ticket context:").unwrap());
    }

    #[test]
    fn resolve_agent_instructions_reads_file() {
        let dir = tempdir().unwrap();
        let file_path = dir.path().join("agent-instructions.txt");
        fs::write(&file_path, "Use the tools").unwrap();

        let mut config = ResolvedConfig::from_global(GlobalConfig::default());
        config.agent_instructions = Some(AgentInstructionsConfig::File { file: "agent-instructions.txt".to_string() });

        let profile = AgentProfileConfig::Runner("claude".to_string()).to_detail();
        let resolved = resolve_agent_instructions(dir.path(), &config, &profile).unwrap();
        assert_eq!(resolved.as_deref(), Some("Use the tools"));
    }

    #[test]
    fn profile_instructions_follow_the_base_instructions() {
        let dir = tempdir().unwrap();
        let mut profile = AgentProfileConfig::Runner("claude".to_string()).to_detail();
        profile.instructions = Some(AgentInstructionsConfig::Inline("You are in the TESTING phase.".to_string()));

        let config = ResolvedConfig::from_global(GlobalConfig::default());
        let resolved = resolve_agent_instructions(dir.path(), &config, &profile).unwrap().unwrap();
        assert!(resolved.contains("## LoTaR CLI\n"), "built-in text kept");
        assert!(resolved.ends_with("You are in the TESTING phase."));

        let mut config = ResolvedConfig::from_global(GlobalConfig::default());
        config.agent_instructions = Some(AgentInstructionsConfig::Inline("Follow AGENTS.md.".to_string()));
        let resolved = resolve_agent_instructions(dir.path(), &config, &profile).unwrap().unwrap();
        assert!(resolved.starts_with("Follow AGENTS.md."));
        assert!(!resolved.contains("## LoTaR CLI\n"));
        assert!(resolved.ends_with("You are in the TESTING phase."));

        config.agent_instructions = crate::config::normalization::parse_global_from_yaml_str("agent:\n  instructions: \"\"\n").unwrap().agent_instructions;
        assert!(config.agent_instructions.is_some(), "empty text is kept");
        let resolved = resolve_agent_instructions(dir.path(), &config, &profile).unwrap().unwrap();
        assert_eq!(resolved, "You are in the TESTING phase.");
    }

    #[test]
    fn instructions_expand_to_literal_ticket_values() {
        let dir = tempdir().unwrap();
        let tasks_dir = dir.path().join(".tasks");
        fs::create_dir_all(&tasks_dir).unwrap();
        fs::write(tasks_dir.join("config.yml"), "default:\n  project: TPL\nagent:\n  worktree:\n    enabled: true\nagents:\n  merge: claude\n").unwrap();
        let mut storage = Storage::new(&tasks_dir);
        let task = crate::services::task_service::TaskService::create(
            &mut storage,
            crate::api_types::TaskCreate {
                title: "Templates".to_string(),
                project: Some("TPL".to_string()),
                reporter: Some("Sam Example".to_string()),
                ..Default::default()
            },
        )
        .unwrap();
        let config = crate::config::resolution::config_for_project(&tasks_dir, Some("TPL")).unwrap();
        let template = instruction_template(&tasks_dir, &config, &task, "job-1", AgentRunnerKind::Claude, Some("merge"));

        let builtin = template.expand(DEFAULT_AGENT_INSTRUCTIONS);
        assert!(!builtin.contains("${{"), "{builtin}");
        assert!(builtin.contains("--format json show TPL-1"), "{builtin}");
        assert!(builtin.contains("--project TPL"), "{builtin}");
        assert!(builtin.contains("to its reporter (Sam Example)"), "{builtin}");

        let merge = template.expand("git -C \"${{workspace.root}}\" merge --ff-only ${{agent.worktree_branch}} (${{agent.profile}})");
        assert!(merge.ends_with("merge --ff-only agent/TPL-1 (merge)"), "{merge}");
        assert!(!merge.contains("\"\""), "workspace root is filled in: {merge}");
    }

    #[test]
    fn job_notes_describe_the_branch_and_who_moves_the_ticket() {
        let dir = tempdir().unwrap();
        let tasks_dir = dir.path().join(".tasks");
        fs::create_dir_all(&tasks_dir).unwrap();
        fs::write(tasks_dir.join("config.yml"), "default:\n  project: NOTE\nagents:\n  implement: claude\n").unwrap();
        // SAFETY: test-local override; creating the ticket queues an agent
        // job, which must not land in the user's cache (DEV-380).
        unsafe { std::env::set_var("LOTAR_AGENT_QUEUE_DIR", dir.path().join("queue")) };
        let mut storage = Storage::new(&tasks_dir);
        let task = crate::services::task_service::TaskService::create(
            &mut storage,
            crate::api_types::TaskCreate {
                title: "Notes".to_string(),
                project: Some("NOTE".to_string()),
                assignee: Some("@implement".to_string()),
                ..Default::default()
            },
        )
        .unwrap();
        let mut config = crate::config::resolution::config_for_project(&tasks_dir, Some("NOTE")).unwrap();

        let notes = job_notes(&tasks_dir, &config, &task.id, false, false, false, false);
        assert!(notes.contains("main checkout"), "{notes}");
        assert!(notes.contains("No automation rule"), "{notes}");

        config.agent_worktree.enabled = true;
        fs::write(
            tasks_dir.join("automation.yml"),
            "automation:\n  rules:\n    - when:\n        assignee: \"@implement\"\n      on:\n        complete:\n          set:\n            status: Done\n",
        )
        .unwrap();
        let notes = job_notes(&tasks_dir, &config, &task.id, false, false, false, false);
        assert!(notes.contains("branch `agent/NOTE-1`"), "{notes}");
        assert!(notes.contains("Commit your work"), "{notes}");
        assert!(notes.contains("move this ticket to the next step"), "{notes}");
        assert!(!notes.contains("final summary"), "LoTaR logs the next step itself: {notes}");
        assert!(notes.contains("hand the ticket back"), "{notes}");

        let notes = job_notes(&tasks_dir, &config, &task.id, false, true, false, false);
        assert!(notes.contains("This job is read-only"), "{notes}");
        assert!(notes.contains("final answer"), "{notes}");
        assert!(!notes.contains("Commit your work"), "{notes}");
    }

    #[test]
    fn resolve_agent_instructions_defaults_when_unset() {
        let dir = tempdir().unwrap();
        let config = ResolvedConfig::from_global(GlobalConfig::default());
        let profile = AgentProfileConfig::Runner("claude".to_string()).to_detail();
        let resolved = resolve_agent_instructions(dir.path(), &config, &profile).unwrap();
        let content = resolved.expect("default instructions");
        assert!(!content.trim().is_empty());
    }

    #[test]
    fn worktree_helpers_sanitize_branch_names() {
        let suffix = sanitize_worktree_token("TEST-1 / scratch");
        assert_eq!(suffix, "TEST-1---scratch");
        let branch = build_worktree_branch("agent", &suffix);
        assert_eq!(branch, "agent/TEST-1---scratch");
    }

    #[test]
    fn resolve_worktree_root_defaults_to_parent_dir() {
        let dir = tempdir().unwrap();
        let repo_root = dir.path().join("repo");
        fs::create_dir_all(&repo_root).unwrap();
        let resolved = resolve_worktree_root(&repo_root, None).unwrap();
        let display = resolved.to_string_lossy();
        assert!(display.contains(".lotar-worktrees"));
    }

    #[test]
    fn a_worktree_job_may_write_the_git_dir_of_a_checkout_whose_git_is_a_file() {
        let tmp = tempdir().unwrap();
        let main = tmp.path().join("main");
        let git_dir = tmp.path().join("git-data");
        let init = crate::utils::git::git_command(tmp.path())
            .args(["init", "--quiet", "--separate-git-dir", &git_dir.to_string_lossy(), &main.to_string_lossy()])
            .output()
            .unwrap();
        assert!(init.status.success(), "{}", String::from_utf8_lossy(&init.stderr));
        let tasks_dir = main.join(".tasks");
        std::fs::create_dir_all(&tasks_dir).unwrap();
        let worktree = tmp.path().join("worktree");
        std::fs::create_dir_all(&worktree).unwrap();

        let dirs = extra_writable_dirs(&worktree, &tasks_dir, false);
        assert!(dirs.contains(&git_dir.canonicalize().unwrap()), "{dirs:?}");
    }

    #[test]
    fn context_append_respects_limit() {
        let dir = tempdir().unwrap();
        let mut config = ResolvedConfig::from_global(GlobalConfig::default());
        config.agent_context_enabled = true;

        let mut messages = Vec::new();
        for idx in 0..30 {
            messages.push(build_user_message(&format!("msg-{idx}")));
        }
        AgentContextService::append_messages(dir.path(), &config, "TEST-1", messages, Some(5)).unwrap();

        let loaded = AgentContextService::load(dir.path(), &config, "TEST-1").unwrap().unwrap();
        assert_eq!(loaded.messages.len(), 5);
        assert!(loaded.messages.last().unwrap().content.contains("msg-29"));
    }

    fn dev56_workspace() -> (tempfile::TempDir, std::path::PathBuf) {
        let tmp = tempfile::tempdir().unwrap();
        let tasks_dir = tmp.path().join("main").join(".tasks");
        std::fs::create_dir_all(&tasks_dir).unwrap();
        std::fs::write(
            crate::utils::paths::global_config_path(&tasks_dir),
            "default:\n  project: AJB\nissue:\n  states: [Todo, InProgress, Done]\n  priorities: [Low, Medium, High]\n  types: [Feature]\nagent:\n  worktree:\n    enabled: false\n    max_parallel_jobs: 0\n",
        )
        .unwrap();
        let mut storage = Storage::new(&tasks_dir);
        storage
            .add(&crate::storage::task::Task::new(tasks_dir.clone(), "Padded job target".to_string(), crate::types::Priority::from("Medium")), "AJB", None)
            .unwrap();
        (tmp, tasks_dir)
    }

    fn dev56_job_request(ticket: &str) -> AgentJobCreateRequest {
        AgentJobCreateRequest {
            ticket_id: ticket.to_string(),
            prompt: "do the thing".to_string(),
            runner: Some("command".to_string()),
            agent: None,
            ..Default::default()
        }
    }

    fn unit_job_state(id: &str) -> AgentJobState {
        let tasks_dir = std::env::temp_dir().join("dev108-no-logs").join(".tasks");
        AgentJobState {
            record: AgentJobRecord {
                id: id.to_string(),
                ticket_id: "TEST-1".to_string(),
                tasks_dir,
                workspace_root: std::env::temp_dir().join("dev108-no-logs"),
                // None keeps AgentLogService::append_event a no-op.
                agent_logs_dir: None,
                runner: "command".to_string(),
                agent_profile: None,
                status: AgentJobStatus::Running,
                created_at: time::now_rfc3339(),
                started_at: None,
                finished_at: None,
                exit_code: None,
                last_message: None,
                summary: None,
                session_id: None,
                worktree_path: None,
                worktree_branch: None,
                model: None,
                effort: None,
                usage: None,
                question: None,
                is_merge_job: false,
            },
            events: Vec::new(),
            next_sequence: 0,
            runtime: None,
            cancellation_finalized: false,
            runner_failed: false,
            worker_running: false,
            main_head_at_start: None,
            assignee_at_start: None,
            ticket_commit_at_start: None,
        }
    }

    /// A taken-over merge job whose ticket the agent handed back (DEV-516):
    /// a repository with branch `agent/TKO-1` one commit ahead of main, the
    /// ticket assigned to sam, and the job in the registry as `adopt_orphaned_job`
    /// leaves it. Returns the repository and the tasks directory.
    fn taken_over_merge_job(job_id: &str, assignee_at_start: Option<Option<String>>) -> (tempfile::TempDir, std::path::PathBuf) {
        let dir = tempdir().unwrap();
        let repo = dir.path();
        // SAFETY: test-local override; outcomes and job records stay out of the user's cache.
        unsafe { std::env::set_var("LOTAR_AGENT_QUEUE_DIR", repo.join("queue")) };
        let git = |args: &[&str]| assert!(git_command(repo).args(args).output().unwrap().status.success(), "git {args:?}");
        git(&["init", "-q", "-b", "main"]);
        git(&["config", "user.name", "Agent Tests"]);
        git(&["config", "user.email", "agent-tests@example.com"]);
        git(&["config", "commit.gpgsign", "false"]);
        fs::write(repo.join(".gitignore"), ".tasks/\nqueue/\n").unwrap();
        git(&["add", "."]);
        git(&["commit", "-qm", "init"]);
        git(&["checkout", "-q", "-b", "agent/TKO-1"]);
        fs::write(repo.join("feature.txt"), "feature\n").unwrap();
        git(&["add", "feature.txt"]);
        git(&["commit", "-qm", "TKO-1 feature"]);
        git(&["checkout", "-q", "main"]);
        let tasks_dir = repo.join(".tasks");
        fs::create_dir_all(&tasks_dir).unwrap();
        fs::write(tasks_dir.join("config.yml"), "default:\n  project: TKO\n").unwrap();
        let mut storage = Storage::new(&tasks_dir);
        crate::services::task_service::TaskService::create(
            &mut storage,
            crate::api_types::TaskCreate {
                title: "Taken over".to_string(),
                project: Some("TKO".to_string()),
                assignee: Some("sam".to_string()),
                ..Default::default()
            },
        )
        .unwrap();
        let mut state = unit_job_state(job_id);
        state.record.ticket_id = "TKO-1".to_string();
        state.record.tasks_dir = tasks_dir.clone();
        state.record.is_merge_job = true;
        state.record.worktree_branch = Some("agent/TKO-1".to_string());
        state.assignee_at_start = assignee_at_start;
        JOB_REGISTRY.lock().unwrap().jobs.insert(job_id.to_string(), state);
        (dir, tasks_dir)
    }

    #[test]
    fn a_taken_over_merge_job_keeps_its_starting_assignee() {
        let started_unassigned = recorded_assignee(Some(None));
        assert_eq!(started_unassigned.as_deref(), Some(""));
        assert_eq!(restored_assignee(started_unassigned), Some(None));
        assert_eq!(restored_assignee(recorded_assignee(Some(Some("@merge".to_string())))), Some(Some("@merge".to_string())));
        assert_eq!(restored_assignee(recorded_assignee(None)), None, "a job that is no merge job, or a record from an older lotar");
    }

    #[test]
    fn a_taken_over_merge_job_handed_back_directly_stays_unmerged() {
        let (dir, tasks_dir) = taken_over_merge_job("job-tko-direct", restored_assignee(Some("sam".to_string())));
        let mut storage = Storage::new(&tasks_dir);
        crate::services::task_service::TaskService::update(
            &mut storage,
            "TKO-1",
            crate::api_types::TaskUpdate { assignee: Some("reviewer".to_string()), ..Default::default() },
        )
        .unwrap();
        let note = ensure_branch_merged("job-tko-direct", "TKO-1", &tasks_dir, "agent/").unwrap();
        assert_eq!(note.as_deref(), Some("The agent handed TKO-1 back (to reviewer): LoTaR leaves agent/TKO-1 unmerged"));
        assert!(!dir.path().join("feature.txt").exists(), "main stays where it was");
    }

    #[test]
    fn a_taken_over_merge_job_without_a_starting_assignee_still_sees_a_recorded_handback() {
        let (dir, tasks_dir) = taken_over_merge_job("job-tko-recorded", None);
        let outcome = crate::api_types::JobOutcome { job_id: "job-tko-recorded".to_string(), assignee: Some("reviewer".to_string()), ..Default::default() };
        crate::services::agent_active_jobs::record_outcome(&tasks_dir, &outcome).unwrap();
        let note = ensure_branch_merged("job-tko-recorded", "TKO-1", &tasks_dir, "agent/").unwrap();
        assert_eq!(note.as_deref(), Some("The agent handed TKO-1 back (to reviewer): LoTaR leaves agent/TKO-1 unmerged"));
        assert!(!dir.path().join("feature.txt").exists(), "main stays where it was");
    }

    #[test]
    fn a_taken_over_merge_job_that_finished_is_fast_forwarded() {
        let (dir, tasks_dir) = taken_over_merge_job("job-tko-done", restored_assignee(Some("sam".to_string())));
        let note = ensure_branch_merged("job-tko-done", "TKO-1", &tasks_dir, "agent/").unwrap();
        assert_eq!(note.as_deref(), Some("Fast-forwarded the main checkout to agent/TKO-1"));
        assert!(dir.path().join("feature.txt").exists());
    }

    #[test]
    fn the_job_end_logs_where_the_rules_moved_the_ticket() {
        let todo = ("Todo".to_string(), Some("claude".to_string()));
        assert_eq!(ticket_move_note("DEV-1", &todo, &todo), None, "nothing moved, nothing logged");
        let review = ("Review".to_string(), Some("mallox".to_string()));
        assert_eq!(ticket_move_note("DEV-1", &todo, &review).as_deref(), Some("DEV-1 is now Review, assigned to mallox"));
        let unassigned = ("Done".to_string(), None);
        assert_eq!(ticket_move_note("DEV-1", &todo, &unassigned).as_deref(), Some("DEV-1 is now Done, unassigned"));
    }

    #[test]
    fn built_in_instructions_let_the_request_name_who_gets_a_question() {
        assert!(DEFAULT_AGENT_INSTRUCTIONS.contains("the person your request below names for questions; if it names no one, to its reporter"));
        assert!(!DEFAULT_AGENT_INSTRUCTIONS.contains("Summarize changes and tests"));
    }

    /// DEV-516: a merge job handed back as soon as lint hit a sandbox-blocked sccache, since "a command you need is
    /// refused" read as a blocker. The instructions now retry such a command first, and only a check that fails on
    /// the work stops a merge.
    #[test]
    fn built_in_instructions_retry_a_sandbox_blocked_command_before_a_handback() {
        let text = DEFAULT_AGENT_INSTRUCTIONS;
        let sandbox = text.find("## Commands the sandbox blocks").expect("sandbox section");
        let blockers = text.find("## Clarifications, blockers and feedback").expect("blockers section");
        assert!(sandbox < blockers, "the retries come before the handback");
        for phrase in [
            "`Operation not permitted`",
            "Rerun the same command with your harness's approval to run it outside the sandbox.",
            "use the fallback the repository's guidance documents",
            "Hand the ticket back only when the retries are refused or fail too",
            "A merge job doesn't land a branch whose checks fail.",
            "keeps failing, also after the retries under \"Commands the sandbox blocks\"",
        ] {
            assert!(text.contains(phrase), "missing: {phrase}");
        }
    }

    /// DEV-620: the built-in instructions were 3.9k characters in every job; they say the same in two thirds.
    #[test]
    fn built_in_instructions_stay_short() {
        assert!(DEFAULT_AGENT_INSTRUCTIONS.chars().count() <= 2600, "{} characters", DEFAULT_AGENT_INSTRUCTIONS.chars().count());
    }

    #[test]
    fn built_in_instructions_ask_for_short_markdown_comments_with_a_report() {
        assert!(DEFAULT_AGENT_INSTRUCTIONS.contains("short markdown"));
        assert!(DEFAULT_AGENT_INSTRUCTIONS.contains("-F - <<'EOF'"), "shows how to pass a multi-line comment");
        for field in ["--commit", "--check", "--next"] {
            assert!(DEFAULT_AGENT_INSTRUCTIONS.contains(field), "names the report field {field}");
        }
    }

    #[test]
    fn push_event_sequences_start_positive_and_stay_strictly_monotonic() {
        let mut state = unit_job_state("job-dev108-mono");
        let started = push_event(&mut state, "agent_job_started", None);
        let first = push_event(&mut state, "agent_job_progress", Some("identical".into()));
        let second = push_event(&mut state, "agent_job_progress", Some("identical".into()));

        assert_eq!(started.sequence, 1, "sequences are positive and start at 1");
        assert!(first.sequence > started.sequence);
        assert!(second.sequence > first.sequence);
        // Identical payloads must still carry distinct identities.
        assert_eq!(first.kind, second.kind);
        assert_eq!(first.message, second.message);
        assert_ne!(first.sequence, second.sequence);
        assert_eq!(state.events.iter().map(|event| event.sequence).collect::<Vec<_>>(), vec![1, 2, 3]);
    }

    #[test]
    fn push_event_writes_the_log_only_when_flushed_after_the_lock() {
        let workspace = tempdir().expect("temp dir");
        let mut state = unit_job_state("job-dev521-log");
        state.record.workspace_root = workspace.path().to_path_buf();
        state.record.agent_logs_dir = Some("logs".to_string());
        AgentLogService::init_log(
            workspace.path(),
            Some("logs"),
            crate::services::agent_log_service::JobLogHeader {
                job_id: state.record.id.clone(),
                ticket_id: "TEST-1".to_string(),
                runner: "command".to_string(),
                created_at: time::now_rfc3339(),
                worktree_path: None,
                worktree_branch: None,
                model: None,
                effort: None,
                agent_profile: None,
            },
        )
        .expect("init log");
        push_event(&mut state, "agent_job_started", None);
        push_event(&mut state, "agent_job_progress", Some("second".into()));
        let logged = || AgentLogService::load_events(workspace.path(), "logs", "job-dev521-log").expect("load events");
        assert!(logged().is_empty(), "no file I/O while the registry may be locked");

        flush_event_log();
        let kinds: Vec<String> = logged().into_iter().map(|entry| entry.kind).collect();
        assert_eq!(kinds, vec!["agent_job_started", "agent_job_progress"], "written in sequence order");
    }

    fn orphan_record(job_id: &str, agent: Option<&str>, merge: bool) -> crate::services::agent_active_jobs::ActiveJob {
        crate::services::agent_active_jobs::ActiveJob {
            job_id: job_id.to_string(),
            ticket_id: "TEST-1".to_string(),
            runner: "t3".to_string(),
            agent: agent.map(str::to_string),
            owner_pid: 0,
            child_pid: 0,
            started_at: time::now_rfc3339(),
            session_id: Some("thread-1".to_string()),
            assignee_at_start: None,
            in_setup: false,
            merge,
        }
    }

    #[test]
    fn a_job_whose_log_ended_is_not_taken_over() {
        let queue = tempdir().expect("queue dir");
        // SAFETY: test-local override of the queue directory.
        unsafe { std::env::set_var("LOTAR_AGENT_QUEUE_DIR", queue.path()) };
        let workspace = tempdir().expect("temp dir");
        let tasks_dir = workspace.path().join(".tasks");
        fs::create_dir_all(&tasks_dir).expect("tasks dir");
        let logs_dir = tasks_dir.join(crate::services::agent_log_service::DEFAULT_LOGS_DIR).to_string_lossy().to_string();
        let job_id = "job-dev544-ended";
        AgentLogService::init_log(
            workspace.path(),
            Some(&logs_dir),
            crate::services::agent_log_service::JobLogHeader {
                job_id: job_id.to_string(),
                ticket_id: "TEST-1".to_string(),
                runner: "t3".to_string(),
                created_at: time::now_rfc3339(),
                worktree_path: None,
                worktree_branch: Some("agent/TEST-1".to_string()),
                model: None,
                effort: None,
                agent_profile: Some("review".to_string()),
            },
        )
        .expect("init log");
        let status = crate::services::agent_log_service::JobStatusLine {
            status: "completed".to_string(),
            at: time::now_rfc3339(),
            exit_code: Some(0),
            summary: Some("Review passed".to_string()),
            usage: None,
            session_id: None,
            worktree_path: None,
            worktree_branch: None,
            ticket_commit: None,
            agent_session: None,
        };
        AgentLogService::write_status_with_usage(workspace.path(), Some(&logs_dir), job_id, status).expect("status line");
        crate::services::agent_active_jobs::register(&tasks_dir, &orphan_record(job_id, Some("review"), false));

        AgentJobService::recover_orphaned_jobs(&tasks_dir);

        assert!(crate::services::agent_active_jobs::orphaned(&tasks_dir).is_empty(), "the leftover record is gone");
        assert!(!JOB_REGISTRY.lock().expect("registry").jobs.contains_key(job_id), "the job doesn't run again");
        let events = AgentLogService::load_events(workspace.path(), &logs_dir, job_id).expect("events");
        assert!(events.is_empty(), "nothing is added to the ended job's log: {events:?}");
    }

    #[test]
    fn a_job_saves_its_record_before_its_start_is_logged() {
        let queue = tempdir().expect("queue dir");
        // SAFETY: test-local override of the queue directory.
        unsafe { std::env::set_var("LOTAR_AGENT_QUEUE_DIR", queue.path()) };
        let tasks = tempdir().expect("tasks dir");
        let mut state = unit_job_state("job-dev593-start");
        state.record.status = AgentJobStatus::Queued;

        mark_started(&mut state, tasks.path(), "TEST-1", "2026-11-15T22:00:00Z");

        assert_eq!(state.events.last().map(|event| event.kind.as_str()), Some("agent_job_started"));
        let record = crate::services::agent_active_jobs::find(tasks.path(), "job-dev593-start").expect("the record goes with the start");
        assert_eq!((record.ticket_id.as_str(), record.session_id), ("TEST-1", None), "no session before the runner starts one");
    }

    /// Killed after it saved its record, before the start reached the log
    /// (or during its `job_started` rules): the takeover fails the job and
    /// starts no session (DEV-593).
    #[test]
    fn a_job_killed_before_its_session_started_fails_when_taken_over() {
        let queue = tempdir().expect("queue dir");
        // SAFETY: test-local override of the queue directory.
        unsafe { std::env::set_var("LOTAR_AGENT_QUEUE_DIR", queue.path()) };
        let workspace = tempdir().expect("temp dir");
        let tasks_dir = workspace.path().join(".tasks");
        fs::create_dir_all(&tasks_dir).expect("tasks dir");
        let logs_dir = tasks_dir.join(crate::services::agent_log_service::DEFAULT_LOGS_DIR).to_string_lossy().to_string();
        let job_id = "job-dev593-unstarted";
        AgentLogService::init_log(
            workspace.path(),
            Some(&logs_dir),
            crate::services::agent_log_service::JobLogHeader {
                job_id: job_id.to_string(),
                ticket_id: "TEST-1".to_string(),
                runner: "t3".to_string(),
                created_at: time::now_rfc3339(),
                worktree_path: None,
                worktree_branch: None,
                model: None,
                effort: None,
                agent_profile: Some("fix".to_string()),
            },
        )
        .expect("init log");
        let record = crate::services::agent_active_jobs::ActiveJob { session_id: None, ..orphan_record(job_id, Some("fix"), false) };
        crate::services::agent_active_jobs::register(&tasks_dir, &record);

        AgentJobService::recover_orphaned_jobs(&tasks_dir);

        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(30);
        while AgentJobService::get_job(job_id).is_none_or(|job| job.status == "running") {
            assert!(std::time::Instant::now() < deadline, "the taken-over job ends");
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
        let job = AgentJobService::get_job(job_id).expect("job");
        assert_eq!(job.status, "failed", "{job:?}");
        let reason = job.summary.or(job.last_message).unwrap_or_default();
        assert!(reason.contains("exited before the job ended, before it started the t3 session"), "{reason}");
    }

    #[test]
    fn a_taken_over_job_keeps_the_kind_it_started_as() {
        assert!(!takes_over_as_merge_job(&orphan_record("job-1", Some("review"), false)), "a review job stays one on a Merging ticket");
        assert!(takes_over_as_merge_job(&orphan_record("job-2", Some("fix"), true)), "a job that started on a Merging ticket");
        assert!(takes_over_as_merge_job(&orphan_record("job-3", Some("merge"), false)), "an older build's record of the merge profile");
        assert!(!takes_over_as_merge_job(&orphan_record("job-4", None, false)));
        let json = serde_json::to_string(&orphan_record("job-5", None, false)).expect("json");
        assert!(!json.contains("merge"), "a record of a job that isn't a merge job stays as older builds wrote it: {json}");
    }

    #[test]
    fn push_event_sequence_survives_event_log_truncation() {
        let mut state = unit_job_state("job-dev108-trunc");
        let total = EVENT_LOG_LIMIT + 25;
        for idx in 0..total {
            push_event(&mut state, "agent_job_progress", Some(format!("m{idx}")));
        }

        assert_eq!(state.events.len(), EVENT_LOG_LIMIT, "snapshot is capped");
        let sequences: Vec<u64> = state.events.iter().map(|event| event.sequence).collect();
        assert!(sequences.windows(2).all(|pair| pair[0] < pair[1]), "capped snapshot stays strictly monotonic");
        assert_eq!(sequences.first(), Some(&((total - EVENT_LOG_LIMIT + 1) as u64)), "oldest entries are trimmed, not renumbered");
        assert_eq!(sequences.last(), Some(&(total as u64)));

        // Sequences are never reused after the cap: the next push continues
        // the lifetime counter instead of deriving from the vec length.
        let next = push_event(&mut state, "agent_job_progress", Some("after-cap".into()));
        assert_eq!(next.sequence, total as u64 + 1);
        assert_eq!(state.events.len(), EVENT_LOG_LIMIT);
        assert_eq!(state.events.last().map(|event| event.sequence), Some(next.sequence));
    }

    #[test]
    fn a_held_job_past_the_event_cap_returns_its_whole_thread_with_live_sequences() {
        // DEV-518 review: a job this process holds gave only its last EVENT_LOG_LIMIT events.
        let tmp = tempfile::tempdir().unwrap();
        let tasks_dir = tmp.path().join(".tasks");
        fs::create_dir_all(&tasks_dir).unwrap();
        let logs_dir = crate::services::agent_log_service::effective_logs_dir(&tasks_dir, None);
        let id = "job-dev518-held";
        let mut state = unit_job_state(id);
        state.record.tasks_dir = tasks_dir.clone();
        state.record.workspace_root = tmp.path().to_path_buf();
        state.record.agent_logs_dir = logs_dir.clone();
        // An event from before the log was opened lives only in memory, and the cap drops it.
        push_event(&mut state, "agent_job_progress", Some("before the log".into()));
        flush_event_log();
        AgentLogService::init_log(
            tmp.path(),
            logs_dir.as_deref(),
            crate::services::agent_log_service::JobLogHeader {
                job_id: id.to_string(),
                ticket_id: "TEST-1".to_string(),
                runner: "command".to_string(),
                created_at: time::now_rfc3339(),
                worktree_path: None,
                worktree_branch: None,
                model: None,
                effort: None,
                agent_profile: None,
            },
        )
        .unwrap();
        let total = EVENT_LOG_LIMIT + 50;
        for index in 1..total {
            push_event(&mut state, "agent_job_message", Some(format!("m{index}")));
        }
        flush_event_log();
        assert_eq!(state.events.len(), EVENT_LOG_LIMIT);
        JOB_REGISTRY.lock().unwrap().jobs.insert(id.to_string(), state);

        let thread = AgentJobService::job_with_events_in(&tasks_dir, id).map(|(_, events)| events);
        let full = AgentJobService::full_thread_in(&tasks_dir, id);
        JOB_REGISTRY.lock().unwrap().jobs.remove(id);

        let thread = thread.unwrap();
        assert_eq!(thread.iter().map(|event| event.sequence).collect::<Vec<_>>(), (2..=total as u64).collect::<Vec<_>>());
        assert_eq!(thread[0].message.as_deref(), Some("m1"), "the oldest logged events are back");
        assert_eq!(full.len(), total - 1);
    }

    #[cfg(unix)]
    #[test]
    fn output_left_in_the_pipe_at_exit_is_read_however_slow_its_lines_are() {
        let (reader, mut writer) = std::io::pipe().unwrap();
        for index in 0..20 {
            writeln!(writer, "line-{index}").unwrap();
        }
        drop(writer);
        let exited = AtomicBool::new(true);
        let mut lines = Vec::new();
        // Each line takes longer than the whole drain limit (DEV-358).
        let result = read_job_output(reader, &exited, Duration::from_millis(1), |line| {
            thread::sleep(Duration::from_millis(5));
            lines.push(line.trim_end().to_string());
        });
        assert!(result.is_ok(), "{result:?}");
        assert_eq!(lines.len(), 20);
        assert_eq!(lines.last().map(String::as_str), Some("line-19"));
    }

    /// A pipe a descendant never lets run dry: a real writer thread can be
    /// descheduled for longer than the drain limit under load (DEV-559).
    struct NeverDryPipe;

    impl Read for NeverDryPipe {
        fn read(&mut self, buffer: &mut [u8]) -> std::io::Result<usize> {
            let line = b"still-running\n";
            let count = line.len().min(buffer.len());
            buffer[..count].copy_from_slice(&line[..count]);
            Ok(count)
        }
    }

    impl JobOutputPipe for NeverDryPipe {
        fn output_ready(&self) -> std::io::Result<bool> {
            Ok(true)
        }

        fn output_available(&self) -> std::io::Result<usize> {
            Ok(0)
        }
    }

    #[test]
    fn output_after_exit_is_limited() {
        let exited = AtomicBool::new(true);
        let started = Instant::now();
        let result = read_job_output(NeverDryPipe, &exited, Duration::from_millis(50), |_| thread::sleep(Duration::from_millis(1)));
        assert_eq!(result.map_err(|error| error.kind()), Err(std::io::ErrorKind::TimedOut));
        assert!(started.elapsed() >= Duration::from_millis(50));
    }

    #[test]
    fn a_streamed_reply_splits_at_paragraphs_outside_code_blocks() {
        let mut reply = ReplyStream::default();
        assert_eq!(reply.push("First "), None);
        assert_eq!(reply.push("part.\n\n```sh\nls\n\n"), Some("First part.".to_string()));
        assert_eq!(reply.push("pwd\n```\n\nLast"), Some("```sh\nls\n\npwd\n```".to_string()));
        assert_eq!(reply.flush(), Some("Last".to_string()));
        assert!(reply.streamed);
    }

    #[test]
    fn a_job_costs_what_the_session_total_grew_by() {
        let usage = |cost: f64| harnessmux::Event {
            v: 1,
            ts: String::new(),
            session: "s".to_string(),
            project: None,
            seq: 1,
            turn: None,
            body: harnessmux::EventBody::Usage(harnessmux::Usage { cost_usd: Some(cost), ..Default::default() }),
            raw: None,
        };
        let mut meter = CostMeter::after(&[usage(1.0)]);
        meter.report(1.5);
        assert_eq!((meter.job, meter.turn), (0.5, Some(0.5)));
        // The agent process started again: its total starts over.
        meter.report(0.25);
        assert_eq!(meter.job, 0.75);
    }

    #[test]
    fn record_and_emit_carries_exact_pushed_sequence_and_at() {
        static SSE_TEST_COUNTER: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let job_id = format!("job-dev108-sse-{}-{}", std::process::id(), SSE_TEST_COUNTER.fetch_add(1, Ordering::SeqCst));
        let mut state = unit_job_state(&job_id);
        let rx = crate::api_events::subscribe();

        let first = record_and_emit(&mut state, "agent_job_progress", Some("same".into()));
        let second = record_and_emit(&mut state, "agent_job_progress", Some("same".into()));
        // Log-only kinds consume sequence numbers but are never emitted.
        push_event(&mut state, "agent_job_raw", Some("raw line".into()));
        push_event(&mut state, "agent_job_init", None);
        push_event(&mut state, "agent_job_result", None);

        let mut sse = Vec::new();
        while let Ok(event) = rx.recv_timeout(Duration::from_millis(150)) {
            if event.data.get("id").and_then(|id| id.as_str()) == Some(job_id.as_str()) {
                sse.push(event);
            }
        }

        assert_eq!(sse.len(), 2, "only the emitted pair reaches SSE");
        for (received, pushed) in sse.iter().zip([&first, &second]) {
            assert_eq!(received.kind, "agent_job_progress");
            assert_eq!(received.data.get("sequence"), Some(&serde_json::json!(pushed.sequence)));
            assert_eq!(received.data.get("at"), Some(&serde_json::json!(pushed.at)));
            assert_eq!(received.data.get("message"), Some(&serde_json::json!("same")));
        }
        assert_eq!(sse[0].data.get("sequence"), Some(&serde_json::json!(1)));

        // REST snapshot parity: the emitted entries are byte-identical members
        // of the stored log, and push-only kinds hold their own sequences.
        assert_eq!(
            state.events.iter().map(|event| (event.kind.as_str(), event.sequence)).collect::<Vec<_>>(),
            vec![("agent_job_progress", 1), ("agent_job_progress", 2), ("agent_job_raw", 3), ("agent_job_init", 4), ("agent_job_result", 5),]
        );
    }

    #[test]
    fn padded_job_start_registers_canonical_and_holds_back_canonical_updates() {
        let (_tmp, tasks_dir) = dev56_workspace();
        // SAFETY: test-local override of the queue directory (nextest runs
        // each test in its own process).
        unsafe { std::env::set_var("LOTAR_AGENT_QUEUE_DIR", _tmp.path().join("queue")) };

        // Padded alias start registers the CANONICAL ticket (queue mode via
        // max_parallel_jobs: 0 keeps the job pending, so no runner spawns).
        let job = AgentJobService::start_job_with_tasks_dir(dev56_job_request("AJB-001"), &tasks_dir).unwrap();
        assert_eq!(job.ticket_id, "AJB-1", "record carries canonical id");
        assert!(AgentJobService::has_active_job("AJB-1"), "canonical lookup must see the padded-start registration");

        // Duplicate alias jobs are refused.
        let err = AgentJobService::start_job_with_tasks_dir(dev56_job_request("AJB-1"), &tasks_dir).unwrap_err();
        assert!(err.to_string().contains("already has an active job"), "{err}");

        // Canonical and padded status/assignee updates from unrelated callers
        // don't touch the ticket; they become the job's outcome (DEV-320).
        let mut storage = Storage::new(&tasks_dir);
        for (id, status) in [("AJB-1", "InProgress"), ("AJB-001", "Done")] {
            let receipt = crate::services::task_service::TaskService::update_with_receipt(
                &mut storage,
                id,
                crate::api_types::TaskUpdate { status: Some(status.to_string()), ..Default::default() },
                crate::services::task_service::TaskUpdateContext::default(),
            )
            .unwrap();
            assert!(!receipt.changed, "update via {id} must wait for the job");
            assert_eq!(receipt.dto.status.as_str(), "Todo");
            let deferred = receipt.deferred.expect("recorded as the outcome");
            assert_eq!(deferred.job_id, job.id);
            assert_eq!(deferred.status.as_deref(), Some(status));
        }
        let recorded = crate::services::agent_active_jobs::peek_outcome(&tasks_dir, &job.id).unwrap();
        assert_eq!(recorded.status.as_deref(), Some("Done"), "the last change wins");
    }

    #[test]
    fn job_start_refuses_cross_root_ticket_before_registration() {
        let (_tmp, tasks_dir) = dev56_workspace();
        // Sibling workspace holding the target ticket.
        let workspace = tasks_dir.parent().unwrap().to_path_buf();
        let sibling = workspace.join("sibling").join(".tasks");
        std::fs::create_dir_all(sibling.join("AJB2")).unwrap();
        std::fs::write(sibling.join("AJB2").join("1.yml"), "title: elsewhere\nstatus: Todo\npriority: Medium\ntype: Feature\ncreated: 2026-01-01T00:00:00Z\n")
            .unwrap();

        let err = AgentJobService::start_job_with_tasks_dir(dev56_job_request("AJB2-1"), &tasks_dir).unwrap_err();
        assert!(err.to_string().contains("cannot be managed from"), "{err}");
        assert!(!AgentJobService::has_active_job("AJB2-1"), "no registration side effect on refusal");
    }

    #[test]
    fn resolve_profile_uses_agent_map() {
        let mut config = ResolvedConfig::from_global(GlobalConfig::default());
        config.agent_profiles.insert("codex-default".to_string(), AgentProfileConfig::Runner("codex".to_string()).to_detail());
        let req = AgentJobCreateRequest {
            ticket_id: "TEST-1".to_string(),
            prompt: "hello".to_string(),
            runner: None,
            agent: Some("codex-default".to_string()),
            ..Default::default()
        };
        let profile = resolve_profile(&config, &req).unwrap();
        assert_eq!(profile.runner, "codex");
    }

    /// DEV-512: a profile's command and args reach its harnessmux agent
    /// instead of being refused (DEV-503).
    #[test]
    fn agent_profiles_with_a_command_are_accepted() {
        let mut config = ResolvedConfig::from_global(GlobalConfig::default());
        let mut detail = AgentProfileConfig::Runner("claude".to_string()).to_detail();
        detail.args = vec!["--verbose".to_string()];
        config.agent_profiles.insert("pinned".to_string(), detail);
        let req =
            AgentJobCreateRequest { ticket_id: "TEST-1".to_string(), prompt: "hello".to_string(), agent: Some("pinned".to_string()), ..Default::default() };
        let profile = resolve_profile(&config, &req).unwrap();
        assert_eq!(profile.args, ["--verbose"]);
    }
}

#[cfg(test)]
mod dev21_ticket_done_state_tests {
    use super::ticket_done_state;

    fn write_task(root: &std::path::Path, prefix: &str, num: u32, status: &str) {
        let dir = root.join(".tasks").join(prefix);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            dir.join(format!("{num}.yml")),
            format!("title: t{num}\nstatus: {status}\npriority: medium\ntype: task\ncreated: 2026-01-01T00:00:00Z\nmodified: 2026-01-01T00:00:00Z\ntags: []\n"),
        )
        .unwrap();
    }

    fn write_config(root: &std::path::Path, done: &str) {
        let tasks = root.join(".tasks");
        std::fs::create_dir_all(&tasks).unwrap();
        std::fs::write(tasks.join("config.yml"), format!("issue:\n  states: [Todo, Done, Shipped]\n  done_states: [{done}]\n")).unwrap();
    }

    /// DEV-21 review F1: cleanup eligibility classifies against the ticket's
    /// ACTUAL root policy, not the issuing workspace's policy. Destructive
    // guards are elsewhere and unchanged; this covers classification only.
    #[test]
    fn ticket_done_state_uses_actual_ticket_root() {
        let tmp = tempfile::tempdir().unwrap();
        // Nested-workspace layout (the locator's monorepo discovery): B is a
        // separate tasks root discovered from A.
        let root_a = tmp.path().join("wsa");
        let root_b = tmp.path().join("wsa").join("wsb");
        write_config(&root_a, "Shipped");
        write_config(&root_b, "Done");
        write_task(&root_a, "PX", 1, "Done");
        write_task(&root_b, "PX", 2, "Done");

        let tasks_a = root_a.join(".tasks");
        let tasks_b = root_b.join(".tasks");

        // Issued from A: PX-1 (in A, Done non-terminal) is open; PX-2 (in
        // sibling B, Done terminal under B's policy) is done.
        assert_eq!(ticket_done_state(&tasks_a, "PX-1"), Some(false));
        assert_eq!(ticket_done_state(&tasks_a, "PX-2"), Some(true));

        // Issued from B: the local ticket still classifies under B's
        // policy. The PARENT workspace A is not discoverable from the
        // nested root (locator discovers downward only), so PX-1 stays
        // indeterminate from B — conservatively no cleanup.
        assert_eq!(ticket_done_state(&tasks_b, "PX-1"), None);
        assert_eq!(ticket_done_state(&tasks_b, "PX-2"), Some(true));

        // Unknown identity stays indeterminate (conservatively no cleanup).
        assert_eq!(ticket_done_state(&tasks_a, "PX-99"), None);
    }
}

#[cfg(test)]
mod dev520_session_events_tests {
    use super::{CostMeter, RunEnd, SessionEvents, TurnControl, TurnSettings, drive_turns};
    use harnessmux::{Event, EventBody, StopReason, Usage};
    use std::io::Write as _;
    use std::path::{Path, PathBuf};
    use std::time::Duration;

    struct Idle;

    impl TurnControl for Idle {
        fn name(&self) -> &str {
            "s"
        }

        fn cancel(&self) {}

        fn idle(&self) -> bool {
            true
        }
    }

    fn delta(seq: u64, turn: u64) -> Event {
        event(seq, turn, EventBody::MessageDelta { text: format!("chunk {seq}") })
    }

    fn event(seq: u64, turn: u64, body: EventBody) -> Event {
        Event { v: harnessmux::event::EVENT_VERSION, ts: String::new(), session: "s".into(), project: None, seq, turn: Some(turn), body, raw: None }
    }

    /// Opens `log`, then turn 1 ends with `end` at seq 100 and turn 2 sends 1400 more events (each
    /// written to `written` first) into harnessmux's 1024-event channel before the job reads one, and the session
    /// ends: the channel has dropped turn 1's end, the log has it.
    fn lagged(end: EventBody, log: Result<PathBuf, String>, written: &Path) -> SessionEvents {
        lagged_with(end, log, written, |_| true)
    }

    /// [`lagged`], with only the events `logged` accepts written whole to the log; the others as a broken line.
    fn lagged_with(end: EventBody, log: Result<PathBuf, String>, written: &Path, logged: impl Fn(u64) -> bool) -> SessionEvents {
        let (tx, rx) = tokio::sync::broadcast::channel(1024);
        let (events, _) = SessionEvents::open(log, || rx);
        let mut file = std::fs::OpenOptions::new().create(true).append(true).open(written).unwrap();
        let mut end = Some(end);
        for seq in 1..=1500 {
            let event = match seq {
                ..100 => delta(seq, 1),
                100 => event(seq, 1, end.take().unwrap()),
                _ => delta(seq, 2),
            };
            if logged(seq) {
                writeln!(file, "{}", serde_json::to_string(&event).unwrap()).unwrap();
            } else {
                writeln!(file, "{{\"seq\":{seq},").unwrap();
            }
            tx.send(event).unwrap();
        }
        events
    }

    fn finished() -> EventBody {
        EventBody::TurnFinished { stop_reason: StopReason::EndTurn, usage: Usage::default() }
    }

    fn drive(events: &mut SessionEvents, tasks_dir: &Path) -> RunEnd {
        let settings = TurnSettings { first: 1, prefixes: &[], budget: None, agent: "fake", link: None };
        drive_turns("dev520-test", tasks_dir, &Idle, events, settings, CostMeter::default(), &mut Vec::new())
    }

    #[test]
    fn a_lagging_stream_replays_the_missed_events_from_the_log() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("events.jsonl");
        let mut events = lagged(finished(), Ok(path.clone()), &path);
        let mut seen = Vec::new();
        while let Ok(Some(event)) = events.next(Duration::from_millis(50)) {
            assert_eq!(event.body.ends_turn(), event.seq == 100);
            seen.push(event.seq);
        }
        assert_eq!(seen, (1..=1500).collect::<Vec<_>>(), "every event once, in order, the turn's end included");
    }

    #[test]
    fn a_job_whose_turn_end_the_stream_dropped_ends_with_the_turns_exit_code() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("finished.jsonl");
        let mut events = lagged(finished(), Ok(path.clone()), &path);
        assert!(matches!(drive(&mut events, dir.path()), RunEnd::Exited(Some(0))));

        let path = dir.path().join("failed.jsonl");
        let mut events = lagged(EventBody::TurnFailed { error: "boom".into() }, Ok(path.clone()), &path);
        assert!(matches!(drive(&mut events, dir.path()), RunEnd::Exited(Some(1))));
    }

    #[test]
    fn missed_events_lotar_cannot_read_back_fail_the_job() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("events.jsonl");
        let mut events = lagged(finished(), Err("no status".into()), &path);
        let RunEnd::Failed(why) = drive(&mut events, dir.path()) else { panic!("an unknown log fails the job") };
        assert!(why.contains("can't read back") && why.contains("no status"), "{why}");

        // A directory where the log should be: reading it fails.
        let unreadable = dir.path().join("unreadable");
        std::fs::create_dir(&unreadable).unwrap();
        let mut events = lagged(finished(), Ok(unreadable), &path);
        let RunEnd::Failed(why) = drive(&mut events, dir.path()) else { panic!("an unreadable log fails the job") };
        assert!(why.contains("can't read back"), "{why}");

        // harnessmux hasn't created the log, or another one is written.
        let mut events = lagged(finished(), Ok(dir.path().join("missing.jsonl")), &path);
        let RunEnd::Failed(why) = drive(&mut events, dir.path()) else { panic!("a missing log fails the job") };
        assert!(why.contains("doesn't have them"), "{why}");

        // A broken line where turn 1's end should be: the gap shows.
        let broken = dir.path().join("broken.jsonl");
        let mut events = lagged_with(finished(), Ok(broken.clone()), &broken, |seq| seq != 100);
        let RunEnd::Failed(why) = drive(&mut events, dir.path()) else { panic!("a log with a gap fails the job") };
        assert!(why.contains("events 100 to 100"), "{why}");

        // Broken lines from the first event through turn 1's end: the gap before the first event lotar sees shows too.
        let broken = dir.path().join("broken-start.jsonl");
        let mut events = lagged_with(finished(), Ok(broken.clone()), &broken, |seq| seq > 100);
        let RunEnd::Failed(why) = drive(&mut events, dir.path()) else { panic!("a log missing its first events fails the job") };
        assert!(why.contains("events 1 to 100"), "{why}");
    }

    /// The log holds earlier turns when the job opens it, and events come while it subscribes (in the log only).
    #[test]
    fn events_start_after_the_logs_last_one() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("events.jsonl");
        let write = |seqs: std::ops::RangeInclusive<u64>| {
            let mut file = std::fs::OpenOptions::new().create(true).append(true).open(&path).unwrap();
            for seq in seqs {
                writeln!(file, "{}", serde_json::to_string(&delta(seq, 1)).unwrap()).unwrap();
            }
        };
        write(1..=10);
        let (tx, rx) = tokio::sync::broadcast::channel(1024);
        let (mut events, earlier) = SessionEvents::open(Ok(path.clone()), || {
            write(11..=15);
            rx
        });
        assert_eq!(earlier.iter().map(|event| event.seq).collect::<Vec<_>>(), (1..=10).collect::<Vec<_>>(), "what the session logged before");
        let mut file = std::fs::OpenOptions::new().append(true).open(&path).unwrap();
        for seq in 16..=1515 {
            let event = delta(seq, 2);
            writeln!(file, "{}", serde_json::to_string(&event).unwrap()).unwrap();
            tx.send(event).unwrap();
        }
        drop(tx);
        let mut seen = Vec::new();
        while let Ok(Some(event)) = events.next(Duration::from_millis(50)) {
            seen.push(event.seq);
        }
        assert_eq!(seen, (11..=1515).collect::<Vec<_>>());
    }

    /// A lotar that took the job over owns the session now; harnessmux's new owner counts on from the count the old one
    /// last saved, below the log's last seq (DEV-510's takeover). Its events are new all the same.
    #[test]
    fn a_new_owner_that_numbers_events_again_is_followed() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("events.jsonl");
        let mut file = std::fs::OpenOptions::new().create(true).append(true).open(&path).unwrap();
        for seq in 1..=3 {
            writeln!(file, "{}", serde_json::to_string(&delta(seq, 1)).unwrap()).unwrap();
        }
        let (tx, rx) = tokio::sync::broadcast::channel(1024);
        let (mut events, _) = SessionEvents::open(Ok(path.clone()), || rx);
        for event in [delta(2, 2), event(3, 2, EventBody::TurnFailed { error: "boom".into() })] {
            writeln!(file, "{}", serde_json::to_string(&event).unwrap()).unwrap();
            tx.send(event).unwrap();
        }
        let mut seen = Vec::new();
        while let Ok(Some(event)) = events.next(Duration::from_millis(50)) {
            seen.push((event.seq, event.turn));
        }
        assert_eq!(seen, [(2, Some(2)), (3, Some(2))]);
    }
}

#[cfg(test)]
mod dev596_t3_worktree_tests {
    use super::move_aside_detached_t3_worktree;
    use std::fs;

    /// DEV-596: T3 Code makes a new ticket branch's worktree at a fixed path, where an earlier thread's worktree
    /// stayed detached after the branch was deleted. It moves aside with its staged change; another ticket's stays.
    #[test]
    fn a_detached_worktree_at_the_t3_path_of_a_new_ticket_branch_moves_aside() {
        let tmp = tempfile::tempdir().unwrap();
        let repo = tmp.path().join("repo");
        let git = |dir: &std::path::Path, args: &[&str]| {
            let output = crate::utils::git::git_command(dir).args(args).current_dir(dir).output().unwrap();
            assert!(output.status.success(), "git {args:?}: {}", String::from_utf8_lossy(&output.stderr));
        };
        fs::create_dir_all(&repo).unwrap();
        git(&repo, &["init", "-q", "-b", "main"]);
        git(&repo, &["-c", "user.name=T", "-c", "user.email=t@example.invalid", "commit", "-q", "--allow-empty", "-m", "init"]);
        let trees = tmp.path().join("t3").join("repo");
        let stale = trees.join("agent-TEST-1-0123456789abcdef");
        let other = trees.join("agent-TEST-2-0123456789abcdef");
        git(&repo, &["worktree", "add", "-q", "-b", "agent/TEST-1", &stale.to_string_lossy()]);
        git(&repo, &["worktree", "add", "-q", "-b", "agent/TEST-2", &other.to_string_lossy()]);
        git(&other, &["checkout", "-q", "--detach"]);
        fs::write(stale.join("draft.txt"), "draft").unwrap();
        git(&stale, &["add", "draft.txt"]);
        git(&stale, &["checkout", "-q", "--detach"]);
        git(&repo, &["branch", "-D", "agent/TEST-1"]);

        let note = move_aside_detached_t3_worktree(&repo, "agent", "agent/TEST-1").unwrap();
        let moved = trees.join("agent-TEST-1-0123456789abcdef-detached-1");
        assert!(note.is_some_and(|note| note.contains(&moved.display().to_string())));
        assert!(!stale.exists() && fs::read_to_string(moved.join("draft.txt")).unwrap() == "draft", "the draft moved along");
        assert!(other.is_dir(), "another ticket's worktree stays");
        assert!(move_aside_detached_t3_worktree(&repo, "agent", "agent/TEST-1").unwrap().is_none(), "nothing left at the path");
    }
}

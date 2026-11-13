use crate::api_types::{AgentJob, AgentJobCreateRequest};
use crate::config::types::{AgentInstructionsConfig, AgentProfileDetail, AgentRunnerKind, AgentSession, ResolvedConfig};
use crate::errors::{LoTaRError, LoTaRResult};
use crate::services::agent_context_service::{AgentContextService, build_assistant_message, build_user_message};
use crate::services::agent_log_service::AgentLogService;
use crate::services::agent_runner::{
    RunnerEventKind, T3Launch, T3Worktree, build_runner_command, build_t3_command, build_t3_send_command, format_stdin_message, is_t3_noise, parse_runner_line,
    supports_stdin, t3_thread_id, validate_runner_command,
};
use crate::services::automation_service::{AutomationEvent, AutomationJobContext, AutomationService, build_lotar_env};
use crate::services::sprint_metrics::determine_done_statuses_from_config;
use crate::services::task_service::TaskService;
use crate::storage::manager::Storage;
use crate::utils::git::{clear_repository_env, git_command, verified_common_dir};
use crate::workspace::TasksDirectoryResolver;
use chrono::Utc;
use serde::Serialize;
use serde_json::json;
use std::collections::{HashMap, VecDeque};
use std::fs;
use std::io::{Read, Write};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, LazyLock, Mutex};
use std::thread;
use std::time::{Duration, Instant};

const EVENT_LOG_LIMIT: usize = 200;
/// How long a runner's descendants may keep its output streams busy after it exits.
const OUTPUT_DRAIN_LIMIT: Duration = Duration::from_secs(5);
const DEFAULT_AGENT_INSTRUCTIONS: &str = include_str!("../../docs/help/agent-instructions.md");

static JOB_COUNTER: AtomicU64 = AtomicU64::new(1);
static ORCHESTRATOR_MODE: LazyLock<Mutex<AgentOrchestratorMode>> = LazyLock::new(|| Mutex::new(AgentOrchestratorMode::Standalone));
static JOB_REGISTRY: LazyLock<Mutex<JobRegistry>> = LazyLock::new(|| {
    Mutex::new(JobRegistry {
        jobs: HashMap::new(),
        active_by_ticket: HashMap::new(),
        pending_queue: VecDeque::new(),
        running_count: 0,
        max_parallel_jobs: None,
    })
});

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
    runner_kind: AgentRunnerKind,
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
    is_merge_job: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct AgentJobEvent {
    pub kind: String,
    pub sequence: u64,
    pub at: String,
    pub message: Option<String>,
}

struct AgentJobRuntime {
    child: Arc<Mutex<Child>>,
    stdin: Option<Arc<Mutex<ChildStdin>>>,
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
    /// The T3 Code thread of a t3 job, once it exists (DEV-318).
    t3_link: Option<String>,
    /// A worker thread runs the job; it clears this only after the job's
    /// end automation, context and worktree cleanup (DEV-322).
    worker_running: bool,
    /// The main checkout's commit when a merge job started, so its end
    /// knows which of main's commits the job landed (DEV-411).
    main_head_at_start: Option<String>,
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
    pending_queue: VecDeque<PendingJob>,
    running_count: usize,
    max_parallel_jobs: Option<usize>,
}

pub struct AgentJobService;

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

        if is_merge_job && !config.agent_worktree.enabled {
            return Err(LoTaRError::ValidationError("Merge agent jobs require agent.worktree.enabled=true so merges run on isolated branches.".to_string()));
        }

        let job_id = make_job_id();
        let context = AgentContextService::load(tasks_dir, &config, &ticket_id)?;
        let template = instruction_template(tasks_dir, &config, &task, &job_id, runner_kind, req.agent.as_deref());
        let instructions = resolve_agent_instructions(tasks_dir, &config, &profile)?.map(|text| template.expand(&text));
        let user_prompt = req.prompt.trim().to_string();
        if user_prompt.is_empty() {
            return Err(LoTaRError::ValidationError("Prompt cannot be empty".to_string()));
        }
        // A T3 Code thread runs in its project's runtime mode, whatever the
        // profile's permissions say.
        let read_only = runner_kind != AgentRunnerKind::T3 && profile.permissions == Some(crate::config::types::AgentPermissions::ReadOnly);
        let lotar_mcp = profile.mcp.as_ref().is_some_and(|mcp| mcp.lotar) && crate::services::agent_runner::supports_lotar_mcp(runner_kind);
        let mut notes = job_notes(tasks_dir, &config, &ticket_id, is_merge_job, read_only, lotar_mcp);
        if runner_kind == AgentRunnerKind::T3 {
            notes.push_str(&t3_thread_notes(tasks_dir));
        }
        let prompt = build_prompt(&task, &user_prompt, context.as_ref(), instructions.as_deref(), Some(&notes));

        let created_at = Utc::now().to_rfc3339();
        let tasks_dir_buf = tasks_dir.to_path_buf();
        let (job_model, job_effort) = (profile.model.clone(), profile.effort.clone());

        let should_queue: bool;
        {
            let mut registry = JOB_REGISTRY.lock().map_err(|_| LoTaRError::ValidationError("Job registry unavailable".to_string()))?;

            if registry.active_by_ticket.contains_key(&ticket_id) || crate::services::agent_active_jobs::for_ticket_elsewhere(tasks_dir, &ticket_id).is_some() {
                return Err(LoTaRError::ValidationError(format!("Ticket '{}' already has an active job", ticket_id)));
            }

            // Update max_parallel_jobs from config (allows runtime changes)
            registry.max_parallel_jobs = config.agent_worktree.max_parallel_jobs;

            let parallel_limit_reached = registry.max_parallel_jobs.is_some_and(|max| registry.running_count >= max);
            let merge_slot_busy = is_merge_job && has_active_merge_job(&registry);

            should_queue = matches!(mode, JobStartMode::RespectLimits) && (parallel_limit_reached || merge_slot_busy);

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
                        runner_kind,
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
                        is_merge_job,
                    },
                    events: Vec::new(),
                    next_sequence: 0,
                    runtime: None,
                    cancellation_finalized: false,
                    runner_failed: false,
                    t3_link: None,
                    worker_running: false,
                    main_head_at_start: None,
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

        if !should_queue {
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
        }))
    }

    /// Finish the jobs whose process exited before they ended, as a worker
    /// or server starts (DEV-378). Each one runs here from now on; see
    /// [`adopt_orphaned_job`].
    pub fn recover_orphaned_jobs(tasks_dir: &std::path::Path) {
        for orphan in crate::services::agent_active_jobs::orphaned(tasks_dir) {
            adopt_orphaned_job(tasks_dir, &orphan);
        }
    }

    pub fn list_jobs() -> Vec<AgentJob> {
        let registry = JOB_REGISTRY.lock();
        if let Ok(registry) = registry {
            return registry.jobs.values().map(|state| state.record.to_dto()).collect();
        }
        Vec::new()
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
        let (job, cancelled, was_pending) = {
            let was_pending = registry.pending_queue.iter().any(|p| p.job_id == id);
            let Some(state) = registry.jobs.get_mut(id) else {
                return Ok(None);
            };
            if !matches!(state.record.status, AgentJobStatus::Queued | AgentJobStatus::Running) {
                return Ok(Some(state.record.to_dto()));
            }
            if let Some(runtime) = state.runtime.as_ref()
                && let Ok(mut child) = runtime.child.lock()
            {
                terminate_child(&mut child);
            }
            let mut cancelled = false;
            if state.record.status != AgentJobStatus::Cancelled {
                state.record.status = AgentJobStatus::Cancelled;
                state.record.finished_at = Some(Utc::now().to_rfc3339());
                record_and_emit(state, "agent_job_cancelled", None);
                cancelled = true;
            }
            let job = state.record.to_dto_with_cancelled(cancelled);
            (job, cancelled, was_pending)
        };
        if cancelled {
            // Remove from pending queue if it was queued
            registry.pending_queue.retain(|p| p.job_id != id);
            // A dispatched job owns its slot until its thread (and child) has exited.
        }
        drop(registry);

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

    pub fn send_message(id: &str, message: &str) -> LoTaRResult<AgentJob> {
        let trimmed = message.trim();
        if trimmed.is_empty() {
            return Err(LoTaRError::ValidationError("Message cannot be empty".to_string()));
        }

        let mut registry = JOB_REGISTRY.lock().map_err(|_| LoTaRError::ValidationError("Job registry unavailable".to_string()))?;
        let (stdin, runner_kind) = {
            let Some(state) = registry.jobs.get_mut(id) else {
                return Err(LoTaRError::ValidationError("Job not found".to_string()));
            };

            if state.record.status != AgentJobStatus::Running {
                return Err(LoTaRError::ValidationError("Job is not running".to_string()));
            }

            let Some(runtime) = state.runtime.as_ref() else {
                return Err(LoTaRError::ValidationError("Job is not accepting messages".to_string()));
            };
            let Some(stdin) = runtime.stdin.as_ref() else {
                return Err(LoTaRError::ValidationError("Runner does not support messages".to_string()));
            };

            (Arc::clone(stdin), state.record.runner_kind)
        };

        let payload = format_stdin_message(runner_kind, trimmed)
            .ok_or_else(|| LoTaRError::ValidationError(format!("Runner '{}' does not support stdin messages", runner_kind.as_str())))?;

        {
            let mut writer = stdin.lock().map_err(|_| LoTaRError::ValidationError("Job stdin unavailable".to_string()))?;
            writer.write_all(&payload).map_err(LoTaRError::IoError)?;
            writer.flush().map_err(LoTaRError::IoError)?;
        }

        let state = registry.jobs.get_mut(id).ok_or_else(|| LoTaRError::ValidationError("Job not found".to_string()))?;
        record_and_emit(state, "agent_job_input", Some(trimmed.to_string()));

        Ok(state.record.to_dto())
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
    })
}

fn make_job_id() -> String {
    let stamp = Utc::now().format("%Y%m%dT%H%M%S%.3fZ").to_string();
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

    crate::services::agent_runner::RunnerCommand { program: wrapper_program, args, env: command.env, initial_input: command.initial_input }
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

    // A run's own model and effort win over the profile's, including model
    // or effort flags in the profile's own args (DEV-213).
    let run_model = non_blank(req.model.as_deref());
    let run_effort = non_blank(req.effort.as_deref());
    if (run_model.is_some() || run_effort.is_some())
        && let Ok(kind) = profile.runner.parse::<AgentRunnerKind>()
    {
        profile.args = crate::services::agent_runner::drop_overridden_args(kind, &profile.args, run_model.is_some(), run_effort.is_some());
    }
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
/// ticket's status.
fn job_notes(tasks_dir: &std::path::Path, config: &ResolvedConfig, ticket_id: &str, is_merge_job: bool, read_only: bool, lotar_mcp: bool) -> String {
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
        notes.push_str(&format!(
            "- You are working in a git worktree on branch `{branch}`, separate from the main checkout. \
Commit your work to this branch before you finish. If git refuses to commit (for example inside a sandbox), \
leave the changes in place: LoTaR commits them when the job ends. Don't switch branches.\n"
        ));
        if is_merge_job {
            notes.push_str(&format!(
                "- This is a merge job: it succeeds only if the branch ends up in the main checkout's current branch. \
If git refuses to fast-forward the main checkout (for example inside a sandbox), leave it: LoTaR fast-forwards it when the job ends, \
as long as the branch is rebased onto it. If you land the work in another form instead (for example you resolve conflicts \
in the main checkout), start each of those commits' messages with `{ticket_id}`, so LoTaR counts the branch as landed.\n"
            ));
        }
    } else {
        notes.push_str("- You are working in the main checkout. Leave your changes uncommitted unless the ticket or your instructions ask for a commit.\n");
    }
    if AutomationService::handles_job_end(tasks_dir, ticket_id) {
        notes.push_str(
            "- When this job ends, LoTaR's automation rules move this ticket to the next step as if you finished. \
If you finished, don't change its status or assignee; add a comment that says what you did. \
If you could not finish (you are blocked, something fails that you can't fix, or you need a decision), \
hand the ticket back as described under \"Clarifications, blockers and feedback\" so it doesn't move on.\n",
        );
    } else {
        notes.push_str(
            "- No automation rule updates this ticket when the job ends. When you finish, \
set its status to a configured status that fits the result (for example a review status) and add a comment that says what you did.\n",
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

fn build_prompt(
    task: &crate::api_types::TaskDTO,
    user_prompt: &str,
    context: Option<&crate::services::agent_context_service::AgentContextFile>,
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

    if let Some(ctx) = context
        && !ctx.messages.is_empty()
    {
        out.push_str("\nRecent context:\n");
        for msg in &ctx.messages {
            out.push_str(&format!("- {}: {}\n", msg.role, msg.content));
        }
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
/// job may write: the tasks directory and the repository's git data. A merge
/// job also gets the main checkout, since merging updates it.
fn extra_writable_dirs(working_dir: &std::path::Path, tasks_dir: &std::path::Path, is_merge_job: bool) -> Vec<std::path::PathBuf> {
    let canonical = |path: &std::path::Path| path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
    let work = canonical(working_dir);
    let mut dirs = vec![canonical(tasks_dir)];
    if let Some(repo_root) = resolve_repo_root(tasks_dir) {
        let git_dir = repo_root.join(".git");
        if is_merge_job {
            dirs.push(canonical(&repo_root));
        } else if git_dir.is_dir() {
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
}

fn prepare_worktree(_job_id: &str, tasks_dir: &std::path::Path, ticket_id: &str, config: &ResolvedConfig) -> LoTaRResult<WorktreeContext> {
    let repo_root = resolve_repo_root(tasks_dir).ok_or_else(|| LoTaRError::ValidationError("Git repository not found".to_string()))?;

    let mut context = WorktreeContext { working_dir: repo_root.clone(), worktree_path: None, worktree_branch: None };

    if !config.agent_worktree.enabled {
        return Ok(context);
    }
    let repo_common = verified_common_dir(&repo_root).map_err(LoTaRError::IoError)?;

    let worktree_root = resolve_worktree_root(&repo_root, config.agent_worktree.dir.as_deref())?;
    let ticket_token = sanitize_worktree_token(ticket_id);
    let branch = build_worktree_branch(&config.agent_worktree.branch_prefix, &ticket_token);
    // Worktree is per-ticket, not per-job. All phases for a ticket share the same worktree.
    let worktree_path = worktree_root.join(&ticket_token);

    std::fs::create_dir_all(&worktree_root).map_err(LoTaRError::IoError)?;

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

    let worktree_arg = worktree_path.to_string_lossy().to_string();
    run_git_command(&repo_root, &["worktree", "add", &worktree_arg, &branch])?;
    if verified_common_dir(&worktree_path).map_err(LoTaRError::IoError)? != repo_common {
        return Err(LoTaRError::ValidationError("Worktree belongs to a different repository".into()));
    }

    context.working_dir = worktree_path.clone();
    context.worktree_path = Some(worktree_path);
    context.worktree_branch = Some(branch);
    Ok(context)
}

/// `t3 thread start --wait` exits 2 when the thread waits for an answer.
const T3_WAITS_FOR_ANSWER: i32 = 2;

/// How to run `t3` again for a job (`t3 thread show`).
#[derive(Clone)]
struct T3Cli {
    program: String,
    env: HashMap<String, String>,
    cwd: std::path::PathBuf,
}

/// The part of `t3 thread show --json` a job uses.
#[derive(Debug, Default, PartialEq, Eq)]
struct T3ThreadInfo {
    link: Option<String>,
    worktree_path: Option<String>,
    branch: Option<String>,
    /// The provider instance and model the thread runs on.
    provider: Option<String>,
    model: Option<String>,
}

/// Where a thread's run stands, for a job whose process died (DEV-378).
#[derive(Debug, Default, PartialEq, Eq)]
struct T3ThreadState {
    info: T3ThreadInfo,
    status: String,
    running: bool,
    waits_for_answer: bool,
    reply: Option<String>,
}

fn parse_t3_thread_state(output: &str) -> Option<T3ThreadState> {
    let value: serde_json::Value = serde_json::from_str(output.trim()).ok()?;
    let thread = value.get("thread")?;
    let text = |key: &str| thread.get(key).and_then(|v| v.as_str()).map(str::trim).filter(|v| !v.is_empty()).map(str::to_string);
    let status = text("status").unwrap_or_default();
    let reply = value
        .get("messages")
        .and_then(|messages| messages.as_array())
        .and_then(|messages| messages.iter().rev().find(|message| message.get("role").and_then(|role| role.as_str()) == Some("assistant")))
        .and_then(|message| message.get("text").and_then(|text| text.as_str()))
        .map(str::trim)
        .filter(|text| !text.is_empty())
        .map(str::to_string);
    Some(T3ThreadState {
        info: T3ThreadInfo {
            link: text("link"),
            worktree_path: text("worktreePath"),
            branch: text("branch"),
            provider: text("providerInstanceId"),
            model: text("model"),
        },
        running: status == "running" || thread.get("activeRunId").is_some_and(|id| !id.is_null()),
        waits_for_answer: thread.get("pendingRequestCount").and_then(|count| count.as_u64()).is_some_and(|count| count > 0),
        status,
        reply,
    })
}

/// How long after its job started the thread `t3 thread start` made for
/// it can have been created (DEV-478). `thread start` makes it within
/// seconds; a short window leaves out threads the agent starts later.
const T3_THREAD_START_WINDOW: chrono::Duration = chrono::Duration::minutes(2);

/// The threads, from `t3 thread list --json`, that may be the one a job
/// started, for a job that never recorded it (DEV-478): created within
/// the window after the job started, with a title that starts with the
/// ticket id. The agent may rename its thread but keeps the id in front.
/// Only a single match identifies the job's thread.
fn job_thread_candidates(output: &str, ticket_id: &str, started_at: &str) -> Vec<String> {
    let Ok(started) = chrono::DateTime::parse_from_rfc3339(started_at) else {
        return Vec::new();
    };
    let threads: Vec<serde_json::Value> = serde_json::from_str(output.trim()).unwrap_or_default();
    let prefix = format!("{ticket_id} ");
    threads
        .iter()
        .filter(|thread| thread.get("title").and_then(|title| title.as_str()).is_some_and(|title| title.starts_with(&prefix)))
        .filter_map(|thread| {
            let created = chrono::DateTime::parse_from_rfc3339(thread.get("createdAt")?.as_str()?).ok()?;
            let id = thread.get("threadId")?.as_str()?;
            (created >= started && created - started <= T3_THREAD_START_WINDOW).then(|| id.to_string())
        })
        .collect()
}

impl T3Cli {
    fn job_thread_candidates(&self, ticket_id: &str, started_at: &str) -> Vec<String> {
        let mut cmd = Command::new(&self.program);
        cmd.args(["thread", "list", "--json"]).envs(&self.env).current_dir(&self.cwd).stdin(Stdio::null()).stderr(Stdio::null());
        clear_repository_env(&mut cmd);
        match cmd.output() {
            Ok(output) if output.status.success() => job_thread_candidates(&String::from_utf8_lossy(&output.stdout), ticket_id, started_at),
            _ => Vec::new(),
        }
    }

    fn thread_info(&self, thread_id: &str) -> Option<T3ThreadInfo> {
        self.thread_state(thread_id).map(|state| state.info)
    }

    fn thread_state(&self, thread_id: &str) -> Option<T3ThreadState> {
        let mut cmd = Command::new(&self.program);
        cmd.args(["thread", "show", thread_id, "--json", "--messages", "1"]).envs(&self.env).current_dir(&self.cwd).stdin(Stdio::null()).stderr(Stdio::null());
        clear_repository_env(&mut cmd);
        let output = cmd.output().ok()?;
        if !output.status.success() {
            return None;
        }
        parse_t3_thread_state(&String::from_utf8_lossy(&output.stdout))
    }
}

/// A T3 Code thread runs on the T3 server and doesn't get the job's
/// environment (`LOTAR_TASKS_DIR`), so its lotar commands name the tasks
/// folder; in a worktree they would otherwise change the worktree's copy.
/// `t3 thread start --wait` returns when the turn ends, even while
/// `t3_background_wait` keeps the thread working (T3 Code 0.1.9, DEV-335).
fn t3_thread_notes(tasks_dir: &std::path::Path) -> String {
    let dir = fs::canonicalize(tasks_dir).unwrap_or_else(|_| tasks_dir.to_path_buf());
    format!(
        "- This job runs as a T3 Code thread, which doesn't get LoTaR's environment. Add `--tasks-dir \"{}\"` \
to every lotar command, so it changes this ticket in the main checkout and not a copy in a worktree.\n\
- The job ends when your turn ends, even while `t3_background_wait` keeps the thread working. Run tests and other \
checks in the foreground and finish all work before your final message; don't end your turn to wait for background work.\n",
        dir.display()
    )
}

/// Settings a t3 profile can't pass to T3 Code, as job notes.
fn t3_profile_notes(profile: &AgentProfileDetail) -> Vec<&'static str> {
    let mut notes = Vec::new();
    if profile.effort.is_some() {
        notes.push("T3 Code's t3 CLI has no effort option; the effort setting is not passed.");
    }
    if profile.permissions.is_some() {
        notes.push("T3 Code runs the thread in the project's own runtime mode; permissions don't apply.");
    }
    notes
}

/// The worktree a t3 job asks T3 Code for: a new ticket branch from the main
/// checkout's current branch, or the existing ticket branch of an earlier
/// phase. The job runs `t3` in the main checkout (the T3 project).
fn plan_t3_worktree(tasks_dir: &std::path::Path, ticket_id: &str, config: &ResolvedConfig) -> LoTaRResult<(WorktreeContext, T3Worktree)> {
    let repo_root = resolve_repo_root(tasks_dir).ok_or_else(|| LoTaRError::ValidationError("Git repository not found".to_string()))?;
    let branch = build_worktree_branch(&config.agent_worktree.branch_prefix, &sanitize_worktree_token(ticket_id));
    let plan = if branch_exists(&repo_root, &branch) {
        T3Worktree { base: branch.clone(), branch: None }
    } else {
        let head =
            git_command(&repo_root).args(["symbolic-ref", "--quiet", "--short", "HEAD"]).current_dir(&repo_root).output().map_err(LoTaRError::IoError)?;
        let base = String::from_utf8_lossy(&head.stdout).trim().to_string();
        T3Worktree { base: if head.status.success() && !base.is_empty() { base } else { "HEAD".to_string() }, branch: Some(branch.clone()) }
    };
    let context = WorktreeContext { working_dir: repo_root, worktree_path: None, worktree_branch: Some(branch) };
    Ok((context, plan))
}

/// The thread exists: link it on the ticket so the owner and other agents
/// find it, and record T3 Code's worktree as the job's.
fn on_t3_thread_started(cli: &T3Cli, job_id: &str, tasks_dir: &std::path::Path, ticket_id: &str, thread_id: &str) {
    let info = cli.thread_info(thread_id).unwrap_or_default();
    let reference = info.link.clone().unwrap_or_else(|| thread_id.to_string());
    update_job(job_id, |state| {
        state.record.session_id = Some(thread_id.to_string());
        state.t3_link = Some(reference.clone());
        record_t3_worktree(state, &info);
        record_and_emit(state, "agent_job_progress", Some(format!("T3 Code thread: {reference}")));
    });
    crate::services::agent_active_jobs::set_session(tasks_dir, job_id, thread_id);
    let mut storage = Storage::new(tasks_dir);
    if let Err(err) = crate::services::reference_service::ReferenceService::attach_link_reference(&mut storage, ticket_id, &reference) {
        update_job(job_id, |state| {
            record_and_emit(state, "agent_job_progress", Some(format!("Could not link the T3 Code thread on {ticket_id}: {err}")));
        });
    }
}

fn record_t3_worktree(state: &mut AgentJobState, info: &T3ThreadInfo) {
    if let Some(path) = info.worktree_path.as_ref() {
        state.record.worktree_path = Some(path.clone());
        state.record.worktree_branch = info.branch.clone().or(state.record.worktree_branch.take());
    }
}

/// The thread ended: T3 Code may have renamed its worktree branch while it
/// worked (`agent/DEV-1` becomes `t3/<topic>`), so the merge check and the
/// next phase see the branch the thread actually worked on.
fn on_t3_thread_ended(cli: &T3Cli, job_id: &str) {
    let thread_id = JOB_REGISTRY.lock().ok().and_then(|registry| registry.jobs.get(job_id).and_then(|state| state.record.session_id.clone()));
    let Some(info) = thread_id.and_then(|thread_id| cli.thread_info(&thread_id)) else {
        return;
    };
    update_job(job_id, |state| record_t3_worktree(state, &info));
}

/// A `session: continue` job's T3 Code thread: the ticket's last one, still
/// there (DEV-368).
struct ContinuedThread {
    thread_id: String,
    info: T3ThreadInfo,
    command: crate::services::agent_runner::RunnerCommand,
}

/// The thread a `session: continue` job sends its prompt to, or why it
/// starts a new session as usual (for the job log). Unless the job asks for
/// a fresh thread, that is the newest thread of its own agent profile when
/// there is one (DEV-472).
fn continue_session(
    job_id: &str,
    runner_kind: AgentRunnerKind,
    profile: &AgentProfileDetail,
    prompt: &str,
    tasks_dir: &std::path::Path,
    ticket_id: &str,
    config: &ResolvedConfig,
) -> Result<ContinuedThread, String> {
    if runner_kind != AgentRunnerKind::T3 {
        return Err(format!("The {} runner can't continue an earlier session; this job starts a new one.", runner_kind.as_str()));
    }
    let own_profile = profile.session != Some(AgentSession::Fresh);
    let Some(thread_id) = previous_session(job_id, runner_kind, tasks_dir, ticket_id, config, own_profile) else {
        return Err(format!("No earlier T3 Code thread on {ticket_id}; starting a new thread."));
    };
    let command = build_t3_send_command(profile, prompt, &thread_id);
    let cli =
        T3Cli { program: command.program.clone(), env: command.env.clone(), cwd: resolve_repo_root(tasks_dir).unwrap_or_else(|| tasks_dir.to_path_buf()) };
    let Some(info) = cli.thread_info(&thread_id) else {
        return Err(format!("The T3 Code thread {thread_id} is gone; starting a new thread."));
    };
    if let Some(path) = info.worktree_path.as_deref()
        && !std::path::Path::new(path).is_dir()
    {
        return Err(format!("The worktree {path} of the T3 Code thread {thread_id} is gone; starting a new thread."));
    }
    Ok(ContinuedThread { thread_id, info, command })
}

/// What a new t3 job does with the ticket's earlier thread that holds
/// unmerged work.
enum OpenWork {
    /// Continue the thread (DEV-377).
    Continue(Box<ContinuedThread>),
    /// The profile runs another provider or model, which a continued thread
    /// can't take (DEV-408), or asks for `session: fresh` (DEV-414): start a
    /// new thread on the earlier thread's branch, its prompt naming that
    /// thread.
    NewThread { earlier: String, branch: String },
    /// A `session: fresh` job whose branch git can't free: it fails, as
    /// continuing would have it check its own work (DEV-486).
    Fail,
}

/// The ticket's last T3 Code thread when its worktree still holds work the
/// main checkout lacks: commits that haven't landed, or uncommitted
/// changes. A new job builds on it instead of starting a second branch for
/// the same ticket: it continues the thread, or, when its profile runs
/// another provider or model or asks for `session: fresh`, starts a new
/// thread on the thread's branch. When git can't free that branch, the job
/// continues the thread, or fails for `session: fresh`.
fn thread_with_open_work(
    job_id: &str,
    profile: &AgentProfileDetail,
    prompt: &str,
    tasks_dir: &std::path::Path,
    ticket_id: &str,
    config: &ResolvedConfig,
) -> Option<(OpenWork, String)> {
    let thread = continue_session(job_id, AgentRunnerKind::T3, profile, prompt, tasks_dir, ticket_id, config).ok()?;
    let worktree = std::path::Path::new(thread.info.worktree_path.as_deref()?);
    let branch = thread.info.branch.clone()?;
    let repo_root = resolve_repo_root(tasks_dir)?;
    let uncommitted = git_command(worktree)
        .args(["status", "--porcelain"])
        .current_dir(worktree)
        .output()
        .is_ok_and(|output| output.status.success() && !output.stdout.is_empty());
    if !uncommitted && branch_landed(&repo_root, &branch).unwrap_or(true) {
        return None;
    }
    let reference = thread.info.link.clone().unwrap_or_else(|| thread.thread_id.clone());
    let reason =
        format!("The ticket's earlier T3 Code thread {reference} has unmerged work on {branch}; this job continues it instead of starting a new thread.");
    let why = if profile.session == Some(AgentSession::Fresh) {
        "the job asks for a fresh thread (session: fresh)".to_string()
    } else if let Some(runs_on) = runs_other_model(&thread.info, profile) {
        format!("runs {runs_on}, not this job's provider and model")
    } else {
        return Some((OpenWork::Continue(Box::new(thread)), reason));
    };
    match release_branch(worktree, &branch, ticket_id) {
        Ok(committed) => {
            let mut reason =
                format!("The ticket's earlier T3 Code thread {reference} has unmerged work on {branch} and {why}; this job starts a new thread on {branch}.");
            if let Some(note) = committed {
                reason = format!("{reason} {note}");
            }
            Some((OpenWork::NewThread { earlier: reference, branch }, reason))
        }
        Err(err) if profile.session == Some(AgentSession::Fresh) => Some((
            OpenWork::Fail,
            format!("{err} The job asks for a fresh thread (session: fresh), so it doesn't continue the earlier thread {reference} and fails."),
        )),
        Err(err) => Some((OpenWork::Continue(Box::new(thread)), format!("{reason} {err}"))),
    }
}

/// The thread's `provider/model` when the profile sets another provider or
/// model. What T3 Code doesn't report counts as the same.
fn runs_other_model(info: &T3ThreadInfo, profile: &AgentProfileDetail) -> Option<String> {
    let differs = |wanted: Option<&str>, runs: Option<&str>| wanted.zip(runs).is_some_and(|(wanted, runs)| wanted != runs);
    (differs(profile.provider.as_deref(), info.provider.as_deref()) || differs(profile.model.as_deref(), info.model.as_deref()))
        .then(|| format!("{}/{}", info.provider.as_deref().unwrap_or("?"), info.model.as_deref().unwrap_or("?")))
}

/// Free `branch` for a new thread's worktree, as git checks a branch out in
/// one worktree only: commit what the earlier thread left uncommitted, then
/// detach its worktree, which keeps its files. `Ok(Some(note))` when LoTaR
/// made a commit.
fn release_branch(worktree: &std::path::Path, branch: &str, ticket_id: &str) -> Result<Option<String>, String> {
    let git = |args: &[&str]| -> Result<(), String> {
        let output = git_command(worktree).args(args).output().map_err(|err| err.to_string())?;
        if output.status.success() {
            Ok(())
        } else {
            Err(format!("LoTaR could not free {branch} for a new thread (git {}: {}).", args.join(" "), String::from_utf8_lossy(&output.stderr).trim()))
        }
    };
    let uncommitted = git_command(worktree).args(["status", "--porcelain"]).output().is_ok_and(|output| output.status.success() && !output.stdout.is_empty());
    let mut note = None;
    if uncommitted {
        let message = format!("{ticket_id}: uncommitted changes from the earlier thread");
        git(&["add", "--all"])?;
        git(&["commit", "--quiet", "-m", &message])?;
        note = Some(format!("Committed what the earlier thread left uncommitted: {message}"));
    }
    git(&["switch", "--quiet", "--detach"])?;
    Ok(note)
}

/// The session of the ticket's newest earlier job on this runner that has
/// one: from this process's jobs or the job logs, whichever is newer. With
/// `own_profile`, the newest one of the job's own agent profile when there
/// is one (DEV-472).
fn previous_session(
    job_id: &str,
    runner_kind: AgentRunnerKind,
    tasks_dir: &std::path::Path,
    ticket_id: &str,
    config: &ResolvedConfig,
    own_profile: bool,
) -> Option<String> {
    // (created at, session, agent profile) of each earlier job.
    let mut earlier: Vec<(String, String, Option<String>)> = Vec::new();
    let mut profile = None;
    if let Ok(registry) = JOB_REGISTRY.lock() {
        profile = registry.jobs.get(job_id).and_then(|state| state.record.agent_profile.clone());
        earlier.extend(
            registry
                .jobs
                .values()
                .filter(|state| {
                    let record = &state.record;
                    record.id != job_id && record.tasks_dir == tasks_dir && record.ticket_id == ticket_id && record.runner_kind == runner_kind
                })
                .filter_map(|state| {
                    state.record.session_id.clone().map(|session| (state.record.created_at.clone(), session, state.record.agent_profile.clone()))
                }),
        );
    }
    let profile = profile.filter(|_| own_profile);
    let workspace_root = tasks_dir.parent().unwrap_or(tasks_dir);
    if let Some(logs_dir) = crate::services::agent_log_service::effective_logs_dir(tasks_dir, config.agent_logs_dir.as_deref()) {
        // Logs come newest first: the first one, and with a profile the
        // first of that profile, are all a later pick can use.
        let mut found_newest = false;
        for id in AgentLogService::list_logs(workspace_root, &logs_dir).unwrap_or_default().into_iter().filter(|id| id != job_id) {
            let Some(header) = AgentLogService::load_header(workspace_root, &logs_dir, &id).ok().flatten() else {
                continue;
            };
            if header.ticket_id != ticket_id || header.runner != runner_kind.as_str() {
                continue;
            }
            let Some(session) = AgentLogService::load_status(workspace_root, &logs_dir, &id).ok().flatten().and_then(|status| status.session_id) else {
                continue;
            };
            let own = profile.is_some() && header.agent_profile == profile;
            if found_newest && !own {
                continue;
            }
            found_newest = true;
            earlier.push((header.created_at, session, header.agent_profile));
            if profile.is_none() || own {
                break;
            }
        }
    }
    let newest_own = earlier.iter().filter(|(_, _, of)| profile.is_some() && *of == profile).max();
    newest_own.or_else(|| earlier.iter().max()).map(|(_, session, _)| session.clone())
}

/// Check the continued thread's branch out again in its worktree when a
/// later thread's worktree took it (a fresh review, DEV-472): free it there
/// as for a new thread, then switch the thread's worktree back to it.
/// `Ok(Some(note))` when LoTaR moved the branch.
fn take_back_branch(thread: &ContinuedThread, tasks_dir: &std::path::Path, ticket_id: &str) -> Result<Option<String>, String> {
    let (Some(worktree), Some(branch)) = (thread.info.worktree_path.as_deref().map(std::path::Path::new), thread.info.branch.as_deref()) else {
        return Ok(None);
    };
    let head = git_command(worktree).args(["symbolic-ref", "--quiet", "--short", "HEAD"]).output().map_err(|err| err.to_string())?;
    if head.status.success() && String::from_utf8_lossy(&head.stdout).trim() == branch {
        return Ok(None);
    }
    let holder = resolve_repo_root(tasks_dir).and_then(|repo_root| worktree_on_branch(&repo_root, branch));
    let mut note = format!("Checked {branch} out again in the thread's worktree {}", worktree.display());
    if let Some(holder) = holder {
        let committed = release_branch(&holder, branch, ticket_id)?;
        note = format!("{note}, freed from {}.", holder.display());
        if let Some(committed) = committed {
            note = format!("{note} {committed}");
        }
    } else {
        note.push('.');
    }
    let output = git_command(worktree).args(["switch", "--quiet", branch]).output().map_err(|err| err.to_string())?;
    if !output.status.success() {
        return Err(format!(
            "LoTaR could not check {branch} out again in the worktree {} of the T3 Code thread {} (git switch: {}).",
            worktree.display(),
            thread.info.link.as_deref().unwrap_or(&thread.thread_id),
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }
    Ok(Some(note))
}

/// The job continues `thread`: it is the job's session from the start, and
/// its worktree the job's. The job's active record, written after this,
/// takes the thread from the job (DEV-480).
fn on_t3_thread_continued(job_id: &str, thread: &ContinuedThread) {
    let reference = thread.info.link.clone().unwrap_or_else(|| thread.thread_id.clone());
    update_job(job_id, |state| {
        state.record.session_id = Some(thread.thread_id.clone());
        state.t3_link = Some(reference.clone());
        record_t3_worktree(state, &thread.info);
        record_and_emit(state, "agent_job_progress", Some(format!("Continuing the T3 Code thread: {reference}")));
    });
}

/// The thread waits for an answer: hand the ticket back to its reporter
/// (status HelpNeeded where the project has it) with the thread's link,
/// and let the job fail, so `job_completed` rules don't treat it as done.
fn hand_back_waiting_t3_job(job_id: &str, ticket_id: &str, tasks_dir: &std::path::Path, config: &ResolvedConfig) {
    let link = JOB_REGISTRY.lock().ok().and_then(|registry| registry.jobs.get(job_id).and_then(|state| state.t3_link.clone()));
    let message = match link.as_deref() {
        Some(link) => format!("The T3 Code thread waits for an answer: {link}"),
        None => "The T3 Code thread waits for an answer.".to_string(),
    };
    let mut storage = Storage::new(tasks_dir);
    if let Ok(task) = TaskService::get(&storage, ticket_id, None) {
        let mut patch = crate::api_types::TaskUpdate::default();
        if let Ok(status) = crate::types::TaskStatus::parse_with_config("HelpNeeded", config) {
            patch.status = Some(status.to_string());
        }
        patch.assignee = task.reporter.clone();
        let _ = TaskService::update_with_context(&mut storage, ticket_id, patch, crate::services::task_service::TaskUpdateContext::automation_disabled());
        let _ = TaskService::add_comment_with_context(&mut storage, ticket_id, &message, crate::services::task_service::CommentContext::automation());
    }
    update_job(job_id, |state| {
        state.record.last_message = Some(message.clone());
        record_and_emit(state, "agent_job_progress", Some(message.clone()));
    });
}

/// How often a job reads its T3 Code thread while it waits for its run.
const ORPHAN_POLL: Duration = Duration::from_secs(5);
/// Failed `t3 thread show` reads in a row before a job stops waiting.
const ORPHAN_READ_ATTEMPTS: u32 = 3;

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
    let header = agent_logs_dir.as_deref().and_then(|dir| AgentLogService::load_header(&workspace_root, dir, &job.job_id).ok().flatten());
    let task = Storage::try_open(tasks_dir).and_then(|storage| TaskService::get(&storage, &ticket_id, None).ok());
    let is_merge_job = task.as_ref().is_some_and(|task| is_merge_job_candidate(task, job.agent.as_deref()));
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
                    runner_kind,
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
                    is_merge_job,
                },
                events: Vec::new(),
                next_sequence: 0,
                runtime: None,
                cancellation_finalized: false,
                runner_failed: false,
                t3_link: None,
                worker_running: true,
                main_head_at_start: None,
            },
        );
        registry.running_count += 1;
    }
    update_job(&job.job_id, |state| {
        let note =
            format!("The lotar process that ran this job (pid {}) exited before it ended; process {} took it over.", orphan.owner_pid, std::process::id());
        record_and_emit(state, "agent_job_progress", Some(note));
    });
    let profile = job
        .agent
        .as_deref()
        .and_then(|name| config.agent_profiles.get(name).cloned())
        .unwrap_or_else(|| AgentProfileDetail { runner: job.runner.clone(), ..Default::default() });
    let tasks_dir = tasks_dir.to_path_buf();
    thread::spawn(move || finish_orphaned_job(&job, runner_kind, &profile, &tasks_dir, &ticket_id, &config));
}

/// Wait until a job's T3 Code thread ends its run. `None` when the job is
/// stopped meanwhile (the thread keeps running); `Err` when the thread
/// can't be read.
fn wait_for_t3_run(cli: &T3Cli, job_id: &str, tasks_dir: &std::path::Path, thread_id: &str) -> Option<Result<T3ThreadState, String>> {
    let mut failed_reads = 0;
    let mut noted_running = false;
    loop {
        if crate::services::agent_active_jobs::cancel_requested(tasks_dir, job_id) {
            let _ = AgentJobService::cancel_job(job_id);
        }
        if should_stop_job(job_id) {
            update_job(job_id, |state| {
                let note = format!("Stopped waiting for the T3 Code thread; it keeps running until it is stopped in T3 Code: {thread_id}");
                record_and_emit(state, "agent_job_progress", Some(note));
            });
            return None;
        }
        match cli.thread_state(thread_id) {
            Some(thread) if !thread.running => return Some(Ok(thread)),
            Some(_) => {
                failed_reads = 0;
                if !noted_running {
                    noted_running = true;
                    update_job(job_id, |state| {
                        record_and_emit(state, "agent_job_progress", Some("The T3 Code thread still runs; the job ends when its run ends.".to_string()));
                    });
                }
            }
            None => {
                failed_reads += 1;
                if failed_reads >= ORPHAN_READ_ATTEMPTS {
                    return Some(Err(format!("Could not read the job's T3 Code thread {thread_id} with `t3 thread show`.")));
                }
            }
        }
        thread::sleep(ORPHAN_POLL);
    }
}

/// Record how a thread's run ended on its job and give the exit code
/// `t3 thread start --wait` would have: 0 when it completed, 2 when it
/// waits for an answer, 1 otherwise.
fn t3_run_exit_code(job_id: &str, thread_id: &str, thread: &T3ThreadState) -> i32 {
    let link = thread.info.link.clone().unwrap_or_else(|| thread_id.to_string());
    update_job(job_id, |state| {
        state.t3_link = Some(link.clone());
        record_t3_worktree(state, &thread.info);
        if let Some(reply) = thread.reply.as_ref() {
            state.record.last_message = Some(reply.clone());
            record_and_emit(state, "agent_job_message", Some(reply.clone()));
        }
    });
    if thread.waits_for_answer {
        T3_WAITS_FOR_ANSWER
    } else if thread.status == "completed" {
        0
    } else {
        update_job(job_id, |state| {
            let message = format!("The T3 Code thread's run ended with status '{}': {link}", thread.status);
            state.record.last_message = Some(message.clone());
            record_and_emit(state, "agent_job_progress", Some(message));
        });
        1
    }
}

/// End a taken-over job: a t3 job from its thread's state, once the thread's
/// run ends; any other runner fails, as its result went with its process.
/// A t3 job whose process died before it recorded its thread looks for it
/// in T3 Code (DEV-478).
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
    if runner_kind != AgentRunnerKind::T3 {
        let message = format!("The lotar process running the job exited before the {} runner finished, so its result is unknown.", job.runner);
        update_job_failure(job_id, ticket_id, message, tasks_dir);
        return;
    }
    let repo_root = resolve_repo_root(tasks_dir).unwrap_or_else(|| tasks_dir.to_path_buf());
    let command = crate::services::agent_runner::build_t3_send_command(profile, "", "");
    let cli = T3Cli { program: command.program, env: command.env, cwd: repo_root };
    let thread_id = match job.session_id.clone() {
        Some(thread_id) => thread_id,
        None => match cli.job_thread_candidates(ticket_id, &job.started_at).as_slice() {
            [thread_id] => {
                on_t3_thread_started(&cli, job_id, tasks_dir, ticket_id, thread_id);
                thread_id.clone()
            }
            [] => {
                let message = format!(
                    "The lotar process running the job exited before the t3 runner recorded its T3 Code thread, and T3 Code has no thread for {ticket_id} started right after the job, so its result is unknown."
                );
                update_job_failure(job_id, ticket_id, message, tasks_dir);
                return;
            }
            candidates => {
                let message = format!(
                    "The lotar process running the job exited before the t3 runner recorded its T3 Code thread, and T3 Code has several threads for {ticket_id} started right after the job ({}), so its result is unknown.",
                    candidates.join(", ")
                );
                update_job_failure(job_id, ticket_id, message, tasks_dir);
                return;
            }
        },
    };
    let thread_id = thread_id.as_str();
    let thread = match wait_for_t3_run(&cli, job_id, tasks_dir, thread_id) {
        Some(Ok(thread)) => thread,
        Some(Err(message)) => {
            update_job_failure(job_id, ticket_id, message, tasks_dir);
            return;
        }
        None => return,
    };
    let exit_code = t3_run_exit_code(job_id, thread_id, &thread);
    let mut context_messages: Vec<_> = thread.reply.as_deref().map(build_assistant_message).into_iter().collect();
    let exit_code = run_queued_turns(&cli, job_id, tasks_dir, Some(exit_code), &mut context_messages);
    if should_stop_job(job_id) {
        return;
    }
    if exit_code == Some(T3_WAITS_FOR_ANSWER) {
        hand_back_waiting_t3_job(job_id, ticket_id, tasks_dir, config);
    }
    finalize_job(job_id, ticket_id, exit_code, &context_messages, tasks_dir, config);
}

/// Hand a running job a message (DEV-384): a t3 job runs it as its
/// thread's next turn before it ends, from whichever process sent it; a
/// runner of this process that reads messages gets it on stdin. `false`
/// when the job can't take it (another runner, or the job is ending).
pub fn tell_running_job(tasks_dir: &std::path::Path, job: &crate::services::agent_active_jobs::ActiveJob, message: &str) -> bool {
    if job.runner == AgentRunnerKind::T3.as_str() {
        return crate::services::agent_active_jobs::push_message(tasks_dir, &job.job_id, message);
    }
    job.owner_pid == std::process::id() && AgentJobService::send_message(&job.job_id, message).is_ok()
}

/// Messages queued while a t3 job ran (DEV-384) run as its thread's next
/// turns, in the same job, so its end rules see what the agent did with
/// them. Returns the exit code of the job's last turn. A thread that waits
/// for an answer or a stopped job takes none.
fn run_queued_turns(
    cli: &T3Cli,
    job_id: &str,
    tasks_dir: &std::path::Path,
    mut exit_code: Option<i32>,
    context_messages: &mut Vec<crate::services::agent_context_service::AgentContextMessage>,
) -> Option<i32> {
    let thread_id = JOB_REGISTRY.lock().ok().and_then(|registry| registry.jobs.get(job_id).and_then(|state| state.record.session_id.clone()));
    for turn in 0.. {
        let Some(thread_id) = thread_id.as_deref().filter(|_| turn < MAX_QUEUED_TURNS && exit_code != Some(T3_WAITS_FOR_ANSWER) && !should_stop_job(job_id))
        else {
            let left = crate::services::agent_active_jobs::close_inbox(tasks_dir, job_id);
            if !left.is_empty() {
                update_job(job_id, |state| {
                    let note = format!("{} message(s) for the agent arrived after it stopped taking turns; they stay on the ticket.", left.len());
                    record_and_emit(state, "agent_job_progress", Some(note));
                });
            }
            return exit_code;
        };
        let messages = crate::services::agent_active_jobs::take_messages_or_close(tasks_dir, job_id);
        if messages.is_empty() {
            return exit_code;
        }
        let prompt = messages.join("\n\n");
        update_job(job_id, |state| {
            record_and_emit(state, "agent_job_input", Some(prompt.clone()));
        });
        context_messages.push(build_user_message(&prompt));
        let Some((code, reply)) = run_t3_turn(cli, job_id, tasks_dir, thread_id, &prompt) else {
            return exit_code;
        };
        exit_code = code;
        if !reply.is_empty() {
            context_messages.push(build_assistant_message(&reply));
            update_job(job_id, |state| {
                state.record.last_message = Some(reply.clone());
                record_and_emit(state, "agent_job_message", Some(reply.clone()));
            });
        }
    }
    exit_code
}

/// Queued turns one job runs at most: a rule that tells the agent about
/// its own comments would otherwise keep it going.
const MAX_QUEUED_TURNS: usize = 10;

/// `t3 thread send <thread> - --wait` with `prompt`: the exit code and the
/// reply, or `None` when it couldn't run or the job was stopped.
fn run_t3_turn(cli: &T3Cli, job_id: &str, tasks_dir: &std::path::Path, thread_id: &str, prompt: &str) -> Option<(Option<i32>, String)> {
    let mut cmd = Command::new(&cli.program);
    cmd.args(["thread", "send", thread_id, "-", "--wait"])
        .envs(&cli.env)
        .current_dir(&cli.cwd)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    clear_repository_env(&mut cmd);
    let mut child = match cmd.spawn() {
        Ok(child) => child,
        Err(err) => {
            update_job(job_id, |state| {
                record_and_emit(state, "agent_job_progress", Some(format!("Could not send the message to the T3 Code thread: {err}")));
            });
            return None;
        }
    };
    if let Some(mut stdin) = child.stdin.take() {
        let _ = stdin.write_all(prompt.as_bytes());
    }
    let read = |pipe: Option<Box<dyn Read + Send>>| {
        thread::spawn(move || {
            let mut text = String::new();
            if let Some(mut pipe) = pipe {
                let _ = pipe.read_to_string(&mut text);
            }
            text
        })
    };
    let stdout = read(child.stdout.take().map(|pipe| Box::new(pipe) as Box<dyn Read + Send>));
    let stderr = read(child.stderr.take().map(|pipe| Box::new(pipe) as Box<dyn Read + Send>));
    let status = loop {
        if crate::services::agent_active_jobs::cancel_requested(tasks_dir, job_id) {
            let _ = AgentJobService::cancel_job(job_id);
        }
        if should_stop_job(job_id) {
            terminate_child(&mut child);
            let _ = child.wait();
            return None;
        }
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) => thread::sleep(Duration::from_millis(200)),
            Err(_) => {
                terminate_child(&mut child);
                let _ = child.wait();
                return None;
            }
        }
    };
    let reply = stdout.join().unwrap_or_default().trim().to_string();
    for line in stderr.join().unwrap_or_default().lines().map(str::trim).filter(|line| !line.is_empty() && !is_t3_noise(line)) {
        update_job(job_id, |state| {
            record_and_emit(state, "agent_job_progress", Some(line.to_string()));
        });
    }
    Some((status.code(), reply))
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
    let output = git_command(repo_root).args(args).current_dir(repo_root).output().map_err(LoTaRError::IoError)?;

    if output.status.success() {
        return Ok(());
    }

    let stderr = String::from_utf8_lossy(&output.stderr);
    Err(LoTaRError::ValidationError(format!("git {} failed: {}", args.join(" "), stderr.trim())))
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

pub(crate) fn ticket_done_state(tasks_dir: &std::path::Path, ticket_id: &str) -> Option<bool> {
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
pub(crate) fn check_agent_worktree_cleanup(repo_root: &std::path::Path, worktree: &AgentWorktree, allow_ignored: bool) -> LoTaRResult<()> {
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
    let git_output = |args: &[&str]| -> LoTaRResult<String> {
        let output = git_command(repo_root).args(args).current_dir(repo_root).output().map_err(LoTaRError::IoError)?;
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(LoTaRError::ValidationError(format!("git {} failed: {}", args.join(" "), stderr.trim())));
        }
        Ok(String::from_utf8_lossy(&output.stdout).into_owned())
    };
    let branch_ref = rev;
    if !git_output(&["cherry", "HEAD", branch_ref])?.lines().any(|line| line.starts_with('+')) {
        return Ok(true);
    }
    if landing_recorded(repo_root, branch, rev) {
        return Ok(true);
    }
    let base = git_output(&["merge-base", "HEAD", branch_ref])?;
    let base = base.trim();
    let changed = git_output(&["diff", "--name-only", "--no-renames", "-z", base, branch_ref])?;
    let excluded = format!("^{base}");
    let mut args = vec!["rev-list", "HEAD", excluded.as_str(), "--"];
    args.extend(changed.split('\0').filter(|path| !path.is_empty()));
    let candidates = git_output(&args)?;
    Ok(std::iter::once("HEAD").chain(candidates.lines()).any(|commit| merge_adds_nothing(repo_root, commit, branch_ref)))
}

/// Whether the changes `branch` makes outside `excluded` (a path relative to
/// the repository root) are in the checkout's current branch: merging the
/// branch into HEAD, or into a commit since the fork that touched those
/// files, changes nothing outside `excluded`. Conflicts inside it don't
/// count; a branch that changes nothing outside it hasn't landed.
fn branch_landed_outside(repo_root: &std::path::Path, branch: &str, excluded: &std::path::Path) -> LoTaRResult<bool> {
    let git_output = |args: &[&str]| -> LoTaRResult<String> {
        let output = git_command(repo_root).args(args).current_dir(repo_root).output().map_err(LoTaRError::IoError)?;
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(LoTaRError::ValidationError(format!("git {} failed: {}", args.join(" "), stderr.trim())));
        }
        Ok(String::from_utf8_lossy(&output.stdout).into_owned())
    };
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
    let git_output = |args: &[&str]| {
        git_command(repo_root)
            .args(args)
            .current_dir(repo_root)
            .output()
            .ok()
            .filter(|output| output.status.success())
            .map(|output| String::from_utf8_lossy(&output.stdout).into_owned())
    };
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

/// The worktree that has `branch` checked out, if any.
fn worktree_on_branch(repo_root: &std::path::Path, branch: &str) -> Option<std::path::PathBuf> {
    let output = git_command(repo_root).args(["worktree", "list", "--porcelain"]).current_dir(repo_root).output().ok()?;
    if !output.status.success() {
        return None;
    }
    let wanted = format!("branch refs/heads/{branch}");
    let mut path = None;
    for line in String::from_utf8_lossy(&output.stdout).lines() {
        if let Some(worktree) = line.strip_prefix("worktree ") {
            path = Some(std::path::PathBuf::from(worktree));
        } else if line == wanted {
            return path;
        }
    }
    None
}

/// A worktree of an agent ticket branch (DEV-476): it has the ticket
/// branch or a T3 Code thread branch created from it checked out, or was
/// detached from one when a later thread took over the branch.
#[derive(Debug, Clone)]
pub(crate) struct AgentWorktree {
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
pub(crate) fn agent_worktrees(repo_root: &std::path::Path, prefix: &str) -> LoTaRResult<Vec<AgentWorktree>> {
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

/// The agent branches no worktree has checked out whose work is in the
/// main checkout, as (ticket branch, branch): ticket branches and the T3
/// Code thread branches created from them (DEV-476). With `ticket_branch`,
/// only that ticket's.
pub(crate) fn landed_agent_branches(repo_root: &std::path::Path, prefix: &str, ticket_branch: Option<&str>) -> Vec<(String, String)> {
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
        .filter(|(_, branch)| branch_landed(repo_root, branch).unwrap_or(false))
        .collect()
}

/// Delete a branch whose work is in the main checkout, possibly rebased,
/// which `git branch -d` would refuse, and its landing records. The
/// branches and detached worktrees traced to their ticket through it get
/// their ticket branch recorded first, so a later cleanup still finds them
/// (`BranchOwners`).
pub(crate) fn delete_landed_branch(repo_root: &std::path::Path, prefix: &str, branch: &str) -> LoTaRResult<()> {
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
/// (DEV-476); any with work missing from main stay. Waits while a job runs
/// for the ticket; its end calls this again.
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
    // Holding the registry keeps a job from starting in the worktree meanwhile.
    let Ok(registry) = JOB_REGISTRY.lock() else {
        return;
    };
    if registry.active_by_ticket.contains_key(ticket_id) || crate::services::agent_active_jobs::for_ticket_elsewhere(tasks_dir, ticket_id).is_some() {
        return;
    }
    let delete_branches = config.agent_worktree.cleanup_delete_branches;
    for worktree in worktrees.iter().filter(|worktree| worktree.ticket_branch == branch) {
        if check_agent_worktree_cleanup(&repo_root, worktree, config.agent_worktree.cleanup_ignored_files).is_ok() {
            remove_worktree(&repo_root, &worktree.path, (!worktree.detached).then_some(worktree.branch.as_str()), delete_branches, branch_prefix);
        }
    }
    if delete_branches {
        for (_, landed) in landed_agent_branches(&repo_root, branch_prefix, Some(&branch)) {
            let _ = delete_landed_branch(&repo_root, branch_prefix, &landed);
        }
    }
    drop(registry);
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
    let Ok(mut registry) = JOB_REGISTRY.lock() else {
        return;
    };
    if registry.active_by_ticket.get(ticket_id).is_some_and(|id| id != job_id) {
        return;
    }
    if let Err(err) = check_worktree_cleanup(&repo_root, &target.path, target.branch.as_deref(), config.agent_worktree.cleanup_ignored_files) {
        let reason = match err {
            LoTaRError::ValidationError(reason) => reason,
            other => other.to_string(),
        };
        let note = format!("Kept the worktree {}: {reason}. Remove it with `lotar agent worktree cleanup` once it is safe.", target.path.display());
        if let Some(state) = registry.jobs.get_mut(job_id) {
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
    drop(registry);
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
    // Holding the registry keeps a job from starting in the worktree while
    // its build output moves out; the slow delete runs after the release.
    let Ok(mut registry) = JOB_REGISTRY.lock() else {
        return;
    };
    if registry.active_by_ticket.get(ticket_id).is_some_and(|id| id != job_id)
        || crate::services::agent_active_jobs::for_ticket_elsewhere(tasks_dir, ticket_id).is_some()
    {
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
    if !removed.is_empty()
        && let Some(state) = registry.jobs.get_mut(job_id)
    {
        let list = removed.iter().map(|path| format!("`{path}`")).collect::<Vec<_>>().join(", ");
        record_and_emit(state, "agent_job_progress", Some(format!("Deleted build output {list} from the worktree {}.", target.path.display())));
    }
    drop(registry);
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
    profile: AgentProfileDetail,
    tasks_dir: std::path::PathBuf,
    ticket_id: String,
    prompt: String,
    user_prompt: String,
    config: ResolvedConfig,
) {
    update_job(&job_id, |state| state.worker_running = true);
    let _slot = RunningJobSlot { job_id: &job_id, ticket_id: &ticket_id, tasks_dir: &tasks_dir };
    if should_stop_job(&job_id) {
        return;
    }
    // Why the job runs in the session it does, for the job log.
    let mut open_branch = None;
    let (mut continued, mut session_reason) = if profile.session == Some(AgentSession::Continue) {
        match continue_session(&job_id, runner_kind, &profile, &prompt, &tasks_dir, &ticket_id, &config) {
            Ok(thread) => (Some(thread), None),
            Err(reason) => (None, Some(reason)),
        }
    } else if runner_kind == AgentRunnerKind::T3
        && let Some((open, reason)) = thread_with_open_work(&job_id, &profile, &prompt, &tasks_dir, &ticket_id, &config)
    {
        match open {
            OpenWork::Continue(thread) => (Some(*thread), Some(reason)),
            OpenWork::NewThread { earlier, branch } => {
                open_branch = Some((earlier, branch));
                (None, Some(reason))
            }
            OpenWork::Fail => {
                // The log holds git's error (DEV-486).
                init_job_log(&job_id, runner_kind, &profile, &tasks_dir, &ticket_id, &config, &Utc::now().to_rfc3339(), None, None);
                update_job_failure(&job_id, &ticket_id, reason, &tasks_dir);
                return;
            }
        }
    } else {
        (None, None)
    };
    if let Some(thread) = continued.as_ref() {
        let note = match take_back_branch(thread, &tasks_dir, &ticket_id) {
            Ok(note) => note,
            Err(err) => {
                continued = None;
                Some(format!("{err} This job starts a new thread."))
            }
        };
        if let Some(note) = note {
            session_reason = Some(session_reason.map_or(note.clone(), |reason| format!("{reason} {note}")));
        }
    }
    let prompt = match open_branch.as_ref() {
        Some((earlier, branch)) => format!(
            "{prompt}\n\nEarlier thread:\nThe ticket's earlier T3 Code thread {earlier} worked on branch `{branch}`, and its work hasn't landed. This thread runs on that branch: build on its commits.\n"
        ),
        None => prompt,
    };
    // A t3 job asks T3 Code for the worktree: a thread can only run inside a
    // T3 project, which LoTaR's own worktree folder is not (DEV-318). A
    // continued thread stays in its own.
    let mut t3_worktree = None;
    let prepared = if let Some((_, branch)) = open_branch {
        t3_worktree = Some(T3Worktree { base: branch.clone(), branch: None });
        Ok(WorktreeContext {
            working_dir: resolve_repo_root(&tasks_dir).unwrap_or_else(|| tasks_dir.clone()),
            worktree_path: None,
            worktree_branch: Some(branch),
        })
    } else if let Some(thread) = continued.as_ref() {
        let repo_root = resolve_repo_root(&tasks_dir).unwrap_or_else(|| tasks_dir.clone());
        let worktree_path = thread.info.worktree_path.as_ref().map(std::path::PathBuf::from);
        Ok(WorktreeContext {
            working_dir: worktree_path.clone().unwrap_or(repo_root),
            worktree_branch: worktree_path.as_ref().and(thread.info.branch.clone()),
            worktree_path,
        })
    } else if runner_kind == AgentRunnerKind::T3 && config.agent_worktree.enabled {
        plan_t3_worktree(&tasks_dir, &ticket_id, &config).map(|(context, plan)| {
            t3_worktree = Some(plan);
            context
        })
    } else {
        prepare_worktree(&job_id, &tasks_dir, &ticket_id, &config)
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
                update_job_failure(&job_id, &ticket_id, format!("Worktree setup failed: {err}"), &tasks_dir);
                return;
            }
            let msg = format!("Worktree setup skipped: {}", err);
            update_job(&job_id, |state| {
                record_and_emit(state, "agent_job_progress", Some(msg.clone()));
            });
            WorktreeContext { working_dir: resolve_repo_root(&tasks_dir).unwrap_or_else(|| tasks_dir.clone()), worktree_path: None, worktree_branch: None }
        }
    };

    let started_at = Utc::now().to_rfc3339();

    let worktree_path_str = worktree_context.worktree_path.as_ref().map(|p| p.to_string_lossy().to_string());
    init_job_log(
        &job_id,
        runner_kind,
        &profile,
        &tasks_dir,
        &ticket_id,
        &config,
        &started_at,
        worktree_path_str.clone(),
        worktree_context.worktree_branch.clone(),
    );

    let mut cancelled = false;
    update_job(&job_id, |state| {
        if state.record.status == AgentJobStatus::Cancelled {
            cancelled = true;
            return;
        }
        state.record.status = AgentJobStatus::Running;
        state.record.started_at = Some(started_at.clone());
        record_and_emit(state, "agent_job_started", None);
    });

    if cancelled {
        return;
    }

    let _ = AutomationService::apply_job_event(tasks_dir.as_path(), &ticket_id, AutomationEvent::JobStarted, job_context_for(&job_id));

    let is_merge_job = JOB_REGISTRY.lock().ok().is_some_and(|registry| registry.jobs.get(&job_id).is_some_and(|state| state.record.is_merge_job));
    let writable_dirs = extra_writable_dirs(&worktree_context.working_dir, &tasks_dir, is_merge_job);
    if is_merge_job && let Some(head) = resolve_repo_root(&tasks_dir).and_then(|root| rev_parse(&root, "HEAD")) {
        update_job(&job_id, |state| state.main_head_at_start = Some(head.clone()));
    }
    if let Some(reason) = session_reason {
        update_job(&job_id, |state| {
            record_and_emit(state, "agent_job_progress", Some(reason.clone()));
        });
    }
    let built = if runner_kind == AgentRunnerKind::T3 {
        for note in t3_profile_notes(&profile) {
            update_job(&job_id, |state| {
                record_and_emit(state, "agent_job_progress", Some(note.to_string()));
            });
        }
        if let Some(thread) = continued.take() {
            if profile.model.is_some() || profile.provider.is_some() || !profile.args.is_empty() {
                update_job(&job_id, |state| {
                    let note = "A continued T3 Code thread keeps its model; the provider, model and args settings are not passed.";
                    record_and_emit(state, "agent_job_progress", Some(note.to_string()));
                });
            }
            on_t3_thread_continued(&job_id, &thread);
            Ok(thread.command)
        } else {
            let title =
                Storage::try_open(&tasks_dir).and_then(|storage| TaskService::get(&storage, &ticket_id, None).ok()).map(|task| task.title).unwrap_or_default();
            let launch = T3Launch { title: format!("{ticket_id} {title}").trim().to_string(), worktree: t3_worktree.take() };
            build_t3_command(&profile, &prompt, &launch)
        }
    } else {
        build_runner_command(runner_kind, &profile, &prompt, &writable_dirs)
    };
    let mut command = match built {
        Ok(cmd) => cmd,
        Err(err) => {
            update_job_failure(&job_id, &ticket_id, err.to_string(), &tasks_dir);
            return;
        }
    };
    if profile.mcp.as_ref().is_some_and(|mcp| mcp.lotar) {
        let bin = current_lotar_binary().map(|bin| bin.to_string_lossy().to_string()).unwrap_or_else(|| "lotar".to_string());
        if !crate::services::agent_runner::attach_lotar_mcp(&mut command, runner_kind, &bin, &tasks_dir) {
            let note = format!("The {} runner can't be given LoTaR's MCP server; the agent uses the lotar CLI instead.", runner_kind.as_str());
            update_job(&job_id, |state| {
                record_and_emit(state, "agent_job_progress", Some(note.clone()));
            });
        }
    }

    // `t3 thread show` runs with the same program and environment.
    let t3_cli = (runner_kind == AgentRunnerKind::T3).then(|| T3Cli {
        program: command.program.clone(),
        env: command.env.clone(),
        cwd: worktree_context.working_dir.clone(),
    });
    let command = wrap_runner_command(command, &job_id, &ticket_id, runner_kind.as_str(), &tasks_dir);

    if let Err(err) = validate_runner_command(&command) {
        update_job_failure(&job_id, &ticket_id, err.to_string(), &tasks_dir);
        return;
    }

    // Inject LOTAR_* environment variables so the agent runner process has
    // full context about the task it is working on.
    let mut command = command;
    if let Some(storage) = Storage::try_open(&tasks_dir.clone())
        && let Ok(task) = TaskService::get(&storage, &ticket_id, None)
    {
        let job_ctx = job_context_for(&job_id);
        let lotar_env = build_lotar_env(&task, &config, &tasks_dir, None, job_ctx.as_ref());
        command.env.extend(lotar_env);
    }
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
                command.env.insert("PATH".to_string(), joined.to_string_lossy().to_string());
            }
        }
        command.env.insert("LOTAR_BIN".to_string(), bin.to_string_lossy().to_string());
    }

    let initial_input = command.initial_input.take();
    let mut cmd = Command::new(&command.program);
    cmd.args(&command.args)
        .envs(command.env)
        // Claude reads the prompt and later messages from stdin, t3 only the
        // prompt; the other CLIs get null stdin so none of them waits for input.
        .stdin(if supports_stdin(runner_kind) || initial_input.is_some() { Stdio::piped() } else { Stdio::null() })
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());

    cmd.current_dir(&worktree_context.working_dir);
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

    // Other processes see the job, and can ask to cancel it (DEV-279). The
    // record comes before the runner: they see its wrapper process from the
    // start, and a wrapper without a record refuses ticket changes (DEV-360).
    let mut active_record = JOB_REGISTRY.lock().ok().and_then(|registry| {
        registry.jobs.get(&job_id).map(|state| crate::services::agent_active_jobs::ActiveJob {
            job_id: job_id.clone(),
            ticket_id: ticket_id.clone(),
            runner: state.record.runner.clone(),
            agent: state.record.agent_profile.clone(),
            owner_pid: std::process::id(),
            child_pid: 0,
            started_at: Utc::now().to_rfc3339(),
            session_id: state.record.session_id.clone(),
        })
    });
    if let Some(record) = active_record.as_ref() {
        crate::services::agent_active_jobs::register(&tasks_dir, record);
    }

    // Serialize cancellation with spawn and runtime publication.
    let Ok(mut registry) = JOB_REGISTRY.lock() else {
        return;
    };
    if registry.jobs.get(&job_id).is_none_or(|state| state.record.status == AgentJobStatus::Cancelled) {
        return;
    }
    let child = match cmd.spawn() {
        Ok(child) => child,
        Err(err) => {
            drop(registry);
            update_job_failure(&job_id, &ticket_id, format!("Failed to spawn runner: {}", err), &tasks_dir);
            return;
        }
    };

    let child = Arc::new(Mutex::new(child));
    let stdout = child.lock().ok().and_then(|mut c| c.stdout.take());
    let stderr = child.lock().ok().and_then(|mut c| c.stderr.take());
    let stdin = child.lock().ok().and_then(|mut c| c.stdin.take());
    let stdin = stdin.map(|handle| Arc::new(Mutex::new(handle)));

    if let Some(state) = registry.jobs.get_mut(&job_id) {
        state.runtime = Some(AgentJobRuntime {
            child: Arc::clone(&child),
            // Only runners that take messages keep the pipe; for the others
            // dropping it below ends their input after the prompt.
            stdin: stdin.clone().filter(|_| supports_stdin(runner_kind)),
        });
    }
    drop(registry);
    if let Some(record) = active_record.as_mut() {
        record.child_pid = child.lock().map(|c| c.id()).unwrap_or_default();
        crate::services::agent_active_jobs::register(&tasks_dir, record);
    }

    // DEV-199: Claude reads its first prompt from stdin. Only the runtime
    // keeps the pipe afterwards, so closing it there ends the session.
    if let (Some(bytes), Some(pipe)) = (initial_input.as_ref(), stdin.as_ref()) {
        let written = pipe.lock().map_err(|_| std::io::Error::other("stdin lock poisoned")).and_then(|mut writer| {
            writer.write_all(bytes)?;
            writer.flush()
        });
        if let Err(err) = written {
            // A broken pipe means the agent closed its input or already
            // exited without reading the prompt; its exit status decides
            // how the job ends. Any other write error fails the job.
            let broken_pipe = err.kind() == std::io::ErrorKind::BrokenPipe;
            update_job(&job_id, |state| {
                if !broken_pipe {
                    state.runner_failed = true;
                }
                record_and_emit(state, "agent_job_progress", Some(format!("Could not send the prompt to the agent: {err}")));
            });
        }
    }
    drop(stdin);

    let exited = Arc::new(AtomicBool::new(false));
    let stderr_reader = stderr.map(|stderr| {
        let job_id_clone = job_id.clone();
        let exited = Arc::clone(&exited);
        let t3_cli = t3_cli.clone();
        let tasks_dir = tasks_dir.clone();
        let ticket_id = ticket_id.clone();
        thread::spawn(move || {
            read_job_output(stderr, &exited, OUTPUT_DRAIN_LIMIT, |line| {
                let msg = line.trim().to_string();
                if let Some(cli) = t3_cli.as_ref() {
                    if is_t3_noise(&msg) {
                        return;
                    }
                    if let Some(thread_id) = t3_thread_id(&msg) {
                        on_t3_thread_started(cli, &job_id_clone, &tasks_dir, &ticket_id, &thread_id);
                        return;
                    }
                }
                if !msg.is_empty() {
                    update_job(&job_id_clone, |state| {
                        state.record.last_message = Some(msg.clone());
                        record_and_emit(state, "agent_job_progress", Some(msg.clone()));
                    });
                }
            })
        })
    });
    // Tool lines name files relative to the job's working directory. Runners
    // may report either spelling of it (macOS /var vs /private/var).
    let mut working_dir_prefixes = vec![format!("{}/", worktree_context.working_dir.display())];
    if let Ok(canonical) = worktree_context.working_dir.canonicalize() {
        working_dir_prefixes.push(format!("{}/", canonical.display()));
    }
    working_dir_prefixes.sort_by_key(|prefix| std::cmp::Reverse(prefix.len()));
    working_dir_prefixes.dedup();
    let working_dir_prefix = working_dir_prefixes;
    let stdout_reader = stdout.map(|stdout| {
        let job_id_clone = job_id.clone();
        let working_dir_prefix = working_dir_prefix.clone();
        let exited = Arc::clone(&exited);
        thread::spawn(move || {
            let mut messages = Vec::new();
            let mut reply = String::new();
            let result = read_job_output(stdout, &exited, OUTPUT_DRAIN_LIMIT, |line| {
                if runner_kind == AgentRunnerKind::T3 {
                    reply.push_str(line.trim_end_matches(['\r', '\n']));
                    reply.push('\n');
                }
                handle_runner_line(&job_id_clone, runner_kind, line, &working_dir_prefix, &mut messages);
            });
            (messages, result, reply)
        })
    });

    let status = loop {
        // A cancel asked for by another process (`lotar agent cancel`).
        if crate::services::agent_active_jobs::cancel_requested(&tasks_dir, &job_id) {
            let _ = AgentJobService::cancel_job(&job_id);
        }
        if should_stop_job(&job_id)
            && let Ok(mut c) = child.lock()
        {
            terminate_child(&mut c);
            if runner_kind == AgentRunnerKind::T3 {
                update_job(&job_id, |state| {
                    let note = format!(
                        "Stopped waiting for the T3 Code thread; it keeps running until it is stopped in T3 Code{}",
                        state.t3_link.as_deref().map(|link| format!(": {link}")).unwrap_or_default()
                    );
                    record_and_emit(state, "agent_job_progress", Some(note));
                });
            }
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
    if let Some(reader) = stderr_reader {
        output_error = reader.join().unwrap_or_else(|_| Err(std::io::Error::other("Stderr reader panicked"))).err();
    }
    let mut context_messages = vec![build_user_message(&user_prompt)];
    if let Some(reader) = stdout_reader {
        match reader.join() {
            Ok((messages, result, reply)) => {
                context_messages.extend(messages);
                output_error = output_error.or(result.err());
                // t3 prints the agent's whole reply; it is the job's summary.
                let reply = reply.trim().to_string();
                if !reply.is_empty() {
                    context_messages.push(build_assistant_message(&reply));
                    update_job(&job_id, |state| {
                        state.record.last_message = Some(reply.clone());
                        record_and_emit(state, "agent_job_message", Some(reply.clone()));
                    });
                }
            }
            Err(_) => {
                output_error = Some(std::io::Error::other("Stdout reader panicked"));
            }
        }
    }
    if should_stop_job(&job_id) || output_error.is_some() || status.is_err() {
        let _ = AgentContextService::append_messages(&tasks_dir, &config, &ticket_id, context_messages, None);
        if !should_stop_job(&job_id) {
            let error = output_error.or_else(|| status.err()).expect("job failed");
            update_job_failure(&job_id, &ticket_id, format!("Runner output/process failed: {error}"), &tasks_dir);
        }
        return;
    }
    let mut exit_code = status.ok().and_then(|status| status.code());
    if let Some(cli) = t3_cli.as_ref() {
        // `t3 thread start --wait` can exit (a signal, a lost connection)
        // while its thread still runs (DEV-401): the job waits for the run.
        let thread_id = JOB_REGISTRY.lock().ok().and_then(|registry| registry.jobs.get(&job_id).and_then(|state| state.record.session_id.clone()));
        if let Some(thread_id) = thread_id.filter(|_| !matches!(exit_code, Some(0 | T3_WAITS_FOR_ANSWER)))
            && cli.thread_state(&thread_id).is_some_and(|thread| thread.running)
        {
            update_job(&job_id, |state| {
                let note = format!("t3 exited ({}) before its thread's run ended.", exit_description(exit_code));
                record_and_emit(state, "agent_job_progress", Some(note));
            });
            match wait_for_t3_run(cli, &job_id, &tasks_dir, &thread_id) {
                Some(Ok(thread)) => {
                    exit_code = Some(t3_run_exit_code(&job_id, &thread_id, &thread));
                    context_messages.extend(thread.reply.as_deref().map(build_assistant_message));
                }
                Some(Err(message)) => update_job(&job_id, |state| {
                    record_and_emit(state, "agent_job_progress", Some(message.clone()));
                }),
                None => {
                    let _ = AgentContextService::append_messages(&tasks_dir, &config, &ticket_id, context_messages, None);
                    return;
                }
            }
        }
        on_t3_thread_ended(cli, &job_id);
        exit_code = run_queued_turns(cli, &job_id, &tasks_dir, exit_code, &mut context_messages);
        if should_stop_job(&job_id) {
            let _ = AgentContextService::append_messages(&tasks_dir, &config, &ticket_id, context_messages, None);
            return;
        }
    }
    if runner_kind == AgentRunnerKind::T3 && exit_code == Some(T3_WAITS_FOR_ANSWER) {
        hand_back_waiting_t3_job(&job_id, &ticket_id, &tasks_dir, &config);
    }
    finalize_job(&job_id, &ticket_id, exit_code, &context_messages, &tasks_dir, &config);
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
        state.record.finished_at = Some(Utc::now().to_rfc3339());
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
    }
}

fn handle_runner_line(
    job_id: &str,
    runner_kind: AgentRunnerKind,
    line: &str,
    working_dir_prefix: &[String],
    context_messages: &mut Vec<crate::services::agent_context_service::AgentContextMessage>,
) {
    let Some(events) = parse_runner_line(runner_kind, line) else {
        // Preserve a small amount of raw output for debugging when parsing fails.
        let raw = line.trim();
        if !raw.is_empty() {
            let truncated: String = raw.chars().take(500).collect();
            update_job(job_id, |state| {
                push_event(state, "agent_job_raw", Some(truncated.clone()));
            });
        }
        return;
    };

    for event in events {
        if let Some(session_id) = event.session_id.clone() {
            update_job(job_id, |state| {
                state.record.session_id = Some(session_id.clone());
            });
        }
        if event.failed {
            update_job(job_id, |state| state.runner_failed = true);
        }

        match event.kind {
            RunnerEventKind::Init => {
                update_job(job_id, |state| {
                    push_event(state, "agent_job_init", None);
                });
            }
            RunnerEventKind::Progress => {
                let message = event.text.clone().map(|text| working_dir_prefix.iter().fold(text, |text, prefix| text.replace(prefix.as_str(), "")));
                update_job(job_id, |state| {
                    if let Some(ref msg) = message {
                        state.record.last_message = Some(msg.clone());
                    }
                    record_and_emit(state, "agent_job_progress", message.clone());
                });
            }
            RunnerEventKind::Message => {
                let message = event.text.clone();
                if let Some(ref msg) = message {
                    context_messages.push(build_assistant_message(msg));
                }
                update_job(job_id, |state| {
                    if let Some(ref msg) = message {
                        state.record.last_message = Some(msg.clone());
                    }
                    record_and_emit(state, "agent_job_message", message.clone());
                });
            }
            RunnerEventKind::Result => {
                let failure_text = event.failed.then(|| event.text.clone()).flatten();
                // The runner's final answer is the job's summary, ahead of any
                // note (such as denied tools) logged just before it.
                let final_text = event.text.clone().filter(|text| !text.trim().is_empty());
                let usage = event.payload.as_ref().and_then(|payload| crate::services::agent_runner::usage_from_result(runner_kind, payload));
                update_job(job_id, |state| {
                    if let Some(ref usage) = usage {
                        state.record.usage.get_or_insert_with(Default::default).add(usage);
                    }
                    if let Some(ref text) = final_text {
                        state.record.last_message = Some(text.clone());
                    }
                    push_event(state, "agent_job_result", failure_text.clone());
                    // DEV-199: Claude keeps running until stdin closes. Messages
                    // sent before this result are already in the pipe and are
                    // still answered; the session ends after them.
                    if runner_kind == AgentRunnerKind::Claude
                        && let Some(runtime) = state.runtime.as_mut()
                    {
                        runtime.stdin = None;
                    }
                });
            }
        }
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
/// judge, pass.
fn ensure_branch_merged(job_id: &str, ticket_id: &str, tasks_dir: &std::path::Path) -> Result<Option<String>, String> {
    let Some((branch, start)) = JOB_REGISTRY.lock().ok().and_then(|registry| {
        registry
            .jobs
            .get(job_id)
            .filter(|state| state.record.is_merge_job)
            .and_then(|state| state.record.worktree_branch.clone().map(|branch| (branch, state.main_head_at_start.clone())))
    }) else {
        return Ok(None);
    };
    let Some(repo_root) = resolve_repo_root(tasks_dir) else {
        return Ok(None);
    };
    match merged_landed(&repo_root, tasks_dir, &branch) {
        Ok(true) => {
            record_landing(&repo_root, &branch);
            return Ok(None);
        }
        // A branch git can't judge passes, like a merged one.
        Err(_) => return Ok(None),
        Ok(false) => {}
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

fn finalize_job(
    job_id: &str,
    ticket_id: &str,
    exit_code: Option<i32>,
    context_messages: &[crate::services::agent_context_service::AgentContextMessage],
    tasks_dir: &std::path::Path,
    config: &ResolvedConfig,
) {
    let now = Utc::now().to_rfc3339();
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
    // A merge job must end with its branch in the main checkout; LoTaR
    // fast-forwards when the agent couldn't, and fails the job otherwise so
    // automation doesn't treat the ticket as merged.
    let failure_note = if success {
        match ensure_branch_merged(job_id, ticket_id, tasks_dir) {
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
    let (summary, workspace_root, logs_dir, usage, session_id) = if let Ok(registry) = JOB_REGISTRY.lock() {
        if let Some(state) = registry.jobs.get(job_id) {
            (
                state.record.summary.clone(),
                state.record.workspace_root.clone(),
                state.record.agent_logs_dir.clone(),
                state.record.usage.clone(),
                state.record.session_id.clone(),
            )
        } else {
            (None, tasks_dir.to_path_buf(), None, None, None)
        }
    } else {
        (None, tasks_dir.to_path_buf(), None, None, None)
    };
    let _ = AgentLogService::write_status_with_usage(
        &workspace_root,
        logs_dir.as_deref(),
        job_id,
        crate::services::agent_log_service::JobStatusLine {
            status: status_str.to_string(),
            at: Utc::now().to_rfc3339(),
            exit_code,
            summary,
            usage,
            session_id,
        },
    );

    let mut job_context = None;
    let mut registry = JOB_REGISTRY.lock().ok();
    if let Some(ref mut registry) = registry {
        if let Some(state) = registry.jobs.get(job_id) {
            job_context = Some(AutomationJobContext {
                job_id: state.record.id.clone(),
                runner: state.record.runner.clone(),
                agent: state.record.agent_profile.clone(),
                worktree_path: state.record.worktree_path.clone(),
                worktree_branch: state.record.worktree_branch.clone(),
                usage: state.record.usage.clone(),
            });
        }
        if registry.active_by_ticket.get(ticket_id).is_some_and(|id| id == job_id) {
            registry.active_by_ticket.remove(ticket_id);
        }
        if let Some(state) = registry.jobs.get_mut(job_id) {
            state.runtime = None;
        }
    }
    drop(registry);

    if !should_stop_job(job_id) {
        let event = if success { AutomationEvent::JobCompleted } else { AutomationEvent::JobFailed };
        apply_job_end_rules(job_id, ticket_id, tasks_dir, event, job_context);
    }

    let _ = AgentContextService::append_messages(tasks_dir, config, ticket_id, context_messages.to_vec(), None);

    maybe_cleanup_worktree(job_id, ticket_id, tasks_dir, config, if success { JobOutcome::Success } else { JobOutcome::Failure });
    remove_build_output(job_id, ticket_id, tasks_dir, config);
}

/// How a runner process ended, for a failure's reason.
fn exit_description(exit_code: Option<i32>) -> String {
    match exit_code {
        Some(code) => format!("exited with code {code}"),
        None => "was ended by a signal".to_string(),
    }
}

fn update_job_failure(job_id: &str, ticket_id: &str, message: String, tasks_dir: &std::path::Path) {
    let now = Utc::now().to_rfc3339();
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
    let ticket_state = || TaskService::get(&Storage::new(tasks_dir), ticket_id, None).ok().map(|task| (task.status.to_string(), task.assignee));
    let before = ticket_state();
    let _ = AutomationService::apply_job_event(tasks_dir, ticket_id, event, job_context);
    if let (Some(before), Some(after)) = (before, ticket_state())
        && let Some(message) = ticket_move_note(ticket_id, &before, &after)
    {
        update_job(job_id, |state| {
            push_event(state, "agent_job_progress", Some(message.clone()));
        });
    }
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
}

/// Process the pending job queue - starts the next job if a slot is available
fn process_pending_queue() {
    let pending_job: Option<PendingJob> = {
        let mut registry = match JOB_REGISTRY.lock() {
            Ok(r) => r,
            Err(_) => return,
        };

        // Check if we can start another job
        let can_start = registry.max_parallel_jobs.is_none_or(|max| registry.running_count < max);

        if !can_start {
            return;
        }

        let running_merge_job = has_running_merge_job(&registry);
        let next_index = registry.pending_queue.iter().position(|pending| !(pending.is_merge_job && running_merge_job));

        if let Some(index) = next_index {
            let Some(pending) = registry.pending_queue.remove(index) else {
                return;
            };
            registry.running_count += 1;
            Some(pending)
        } else {
            None
        }
    };

    // Start the job outside the lock
    if let Some(pending) = pending_job {
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

fn has_running_merge_job(registry: &JobRegistry) -> bool {
    registry.jobs.values().any(|state| state.record.is_merge_job && state.record.status == AgentJobStatus::Running)
}

fn has_active_merge_job(registry: &JobRegistry) -> bool {
    registry.jobs.values().any(|state| state.record.is_merge_job && matches!(state.record.status, AgentJobStatus::Queued | AgentJobStatus::Running))
}

fn is_merge_job_candidate(task: &crate::api_types::TaskDTO, agent_profile: Option<&str>) -> bool {
    task.status.as_str().eq_ignore_ascii_case("Merging")
        || agent_profile.is_some_and(|profile| {
            let normalized = profile.trim().to_ascii_lowercase();
            normalized == "merge" || normalized == "merge-retry"
        })
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
    let at = Utc::now().to_rfc3339();
    state.next_sequence = state.next_sequence.saturating_add(1);
    let event = AgentJobEvent { kind: kind.to_string(), sequence: state.next_sequence, at: at.clone(), message };
    state.events.push(event.clone());
    if state.events.len() > EVENT_LOG_LIMIT {
        let start = state.events.len().saturating_sub(EVENT_LOG_LIMIT);
        state.events = state.events[start..].to_vec();
    }

    // Also append to persistent log file (if logging is enabled)
    let _ = AgentLogService::append_event(
        &state.record.workspace_root,
        state.record.agent_logs_dir.as_deref(),
        &state.record.id,
        &event.kind,
        &event.at,
        event.message.clone(),
    );
    event
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
    });
    crate::api_events::emit(&crate::api_events::ApiEvent { kind: event.kind.clone(), data: payload });
}

/// Record-then-stream pair for SSE-visible job events. Log-only kinds
/// (agent_job_raw/init/result) stay on bare `push_event`.
/// Finished jobs stay in memory for a day (status, events for the UI) and
/// at most `MAX_FINISHED_JOBS` of them, so a long-running server doesn't
/// grow without bound (DEV-279). Their logs stay on disk.
const MAX_FINISHED_JOBS: usize = 500;

fn prune_finished_jobs(registry: &mut JobRegistry) {
    let cutoff = Utc::now() - chrono::Duration::hours(24);
    let finished_at = |state: &AgentJobState| {
        state.record.finished_at.as_deref().and_then(|at| chrono::DateTime::parse_from_rfc3339(at).ok()).map(|at| at.with_timezone(&Utc))
    };
    let mut finished: Vec<(chrono::DateTime<Utc>, String)> = registry
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

    #[test]
    fn build_prompt_includes_ticket_and_request() {
        let task = sample_task();
        let prompt = build_prompt(&task, "ship it", None, None, None);
        assert!(prompt.contains("TEST-1"));
        assert!(prompt.contains("ship it"));
    }

    #[test]
    fn build_prompt_includes_instructions() {
        let task = sample_task();
        let prompt = build_prompt(&task, "ship it", None, Some("Use the tools"), Some("- Commit to the branch."));
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
        assert!(resolved.contains("LoTaR CLI quick reference"), "built-in text kept");
        assert!(resolved.ends_with("You are in the TESTING phase."));

        let mut config = ResolvedConfig::from_global(GlobalConfig::default());
        config.agent_instructions = Some(AgentInstructionsConfig::Inline("Follow AGENTS.md.".to_string()));
        let resolved = resolve_agent_instructions(dir.path(), &config, &profile).unwrap().unwrap();
        assert!(resolved.starts_with("Follow AGENTS.md."));
        assert!(!resolved.contains("LoTaR CLI quick reference"));
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
        assert!(builtin.contains("list TPL-1"), "{builtin}");
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

        let notes = job_notes(&tasks_dir, &config, &task.id, false, false, false);
        assert!(notes.contains("main checkout"), "{notes}");
        assert!(notes.contains("No automation rule"), "{notes}");

        config.agent_worktree.enabled = true;
        fs::write(
            tasks_dir.join("automation.yml"),
            "automation:\n  rules:\n    - when:\n        assignee: \"@implement\"\n      on:\n        complete:\n          set:\n            status: Done\n",
        )
        .unwrap();
        let notes = job_notes(&tasks_dir, &config, &task.id, false, false, false);
        assert!(notes.contains("branch `agent/NOTE-1`"), "{notes}");
        assert!(notes.contains("Commit your work"), "{notes}");
        assert!(notes.contains("move this ticket to the next step"), "{notes}");
        assert!(!notes.contains("final summary"), "LoTaR logs the next step itself: {notes}");
        assert!(notes.contains("hand the ticket back"), "{notes}");

        let notes = job_notes(&tasks_dir, &config, &task.id, false, true, false);
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
    fn t3_thread_show_gives_the_link_and_worktree() {
        let info = parse_t3_thread_state(
            r#"{"thread":{"threadId":"t1","link":"https://app.t3.codes/threads/e/t1","worktreePath":"/w/DEV-1","branch":"agent/DEV-1","providerInstanceId":"codex","model":"gpt-5.5"},"messages":[]}"#,
        )
        .unwrap()
        .info;
        assert_eq!(info.link.as_deref(), Some("https://app.t3.codes/threads/e/t1"));
        assert_eq!(info.worktree_path.as_deref(), Some("/w/DEV-1"));
        assert_eq!(info.branch.as_deref(), Some("agent/DEV-1"));
        assert_eq!((info.provider.as_deref(), info.model.as_deref()), (Some("codex"), Some("gpt-5.5")));
        let root = parse_t3_thread_state(r#"{"thread":{"link":"x","worktreePath":null}}"#).unwrap();
        assert_eq!(root.info.worktree_path, None);
        assert!(parse_t3_thread_state("not json").is_none());
    }

    #[test]
    fn t3_thread_show_tells_whether_the_run_ended_and_how() {
        let running = parse_t3_thread_state(r#"{"thread":{"status":"running","activeRunId":"run:1","pendingRequestCount":0},"messages":[]}"#).unwrap();
        assert!(running.running && !running.waits_for_answer);
        let done = parse_t3_thread_state(
            r#"{"thread":{"status":"completed","activeRunId":null,"pendingRequestCount":0},"messages":[{"role":"user","text":"q"},{"role":"assistant","text":" Done. "}]}"#,
        )
        .unwrap();
        assert_eq!((done.running, done.status.as_str(), done.reply.as_deref()), (false, "completed", Some("Done.")));
        let waiting = parse_t3_thread_state(r#"{"thread":{"status":"idle","activeRunId":null,"pendingRequestCount":1}}"#).unwrap();
        assert!(!waiting.running && waiting.waits_for_answer);
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
                runner_kind: AgentRunnerKind::Command,
                agent_profile: None,
                status: AgentJobStatus::Running,
                created_at: Utc::now().to_rfc3339(),
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
                is_merge_job: false,
            },
            events: Vec::new(),
            next_sequence: 0,
            runtime: None,
            cancellation_finalized: false,
            runner_failed: false,
            t3_link: None,
            worker_running: false,
            main_head_at_start: None,
        }
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

    #[cfg(unix)]
    #[test]
    fn output_after_exit_is_limited() {
        let (reader, mut writer) = std::io::pipe().unwrap();
        writeln!(writer, "last-runner-line").unwrap();
        // A descendant that keeps the pipe full; it stops once the reader is gone.
        let descendant = thread::spawn(move || while writeln!(writer, "still-running").is_ok() {});
        let exited = AtomicBool::new(true);
        let result = read_job_output(reader, &exited, Duration::from_millis(50), |_| thread::sleep(Duration::from_millis(1)));
        assert_eq!(result.map_err(|error| error.kind()), Err(std::io::ErrorKind::TimedOut));
        descendant.join().unwrap();
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
    fn running_job_matches_accepts_padded_spellings() {
        assert!(crate::cli::handlers::agent::running_job_matches("TP-1", "TP-001"));
        assert!(crate::cli::handlers::agent::running_job_matches("TP-001", "TP-1"));
        assert!(!crate::cli::handlers::agent::running_job_matches("TP-1", "TP-2"));
        assert!(!crate::cli::handlers::agent::running_job_matches("abc", "TP-1"));
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

    /// DEV-213: a run's own model/effort replace the profile's flags, even
    /// when the profile passes them in its `args`.
    #[test]
    fn run_overrides_replace_model_and_effort_in_profile_args() {
        let mut config = ResolvedConfig::from_global(GlobalConfig::default());
        let mut detail = AgentProfileConfig::Runner("claude".to_string()).to_detail();
        detail.args = vec!["--model".to_string(), "opus".to_string(), "--effort".to_string(), "low".to_string(), "--verbose".to_string()];
        config.agent_profiles.insert("pinned".to_string(), detail);
        let mut req = AgentJobCreateRequest {
            ticket_id: "TEST-1".to_string(),
            prompt: "hello".to_string(),
            agent: Some("pinned".to_string()),
            model: Some("sonnet".to_string()),
            ..Default::default()
        };
        let profile = resolve_profile(&config, &req).unwrap();
        assert_eq!(profile.model.as_deref(), Some("sonnet"));
        // Only the overridden flag goes; the profile's own effort stays.
        assert_eq!(profile.args, vec!["--effort", "low", "--verbose"]);

        // Without overrides the profile's args are untouched.
        req.model = None;
        let profile = resolve_profile(&config, &req).unwrap();
        assert_eq!(profile.args.len(), 5);
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

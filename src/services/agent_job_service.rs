use crate::api_types::{AgentJob, AgentJobCreateRequest};
use crate::config::manager::ConfigManager;
use crate::config::resolution::load_and_merge_configs;
use crate::config::types::{AgentInstructionsConfig, AgentProfileDetail, ResolvedConfig};
use crate::errors::{LoTaRError, LoTaRResult};
use crate::services::agent_context_service::{
    AgentContextService, build_assistant_message, build_user_message,
};
use crate::services::agent_log_service::AgentLogService;
use crate::services::agent_runner::{
    AgentRunnerKind, RunnerEventKind, build_runner_command, format_stdin_message,
    parse_runner_line, supports_stdin, validate_runner_command,
};
use crate::services::automation_service::{
    AutomationEvent, AutomationJobContext, AutomationService, build_lotar_env,
};
use crate::services::sprint_metrics::determine_done_statuses_from_config;
use crate::services::task_service::TaskService;
use crate::storage::manager::Storage;
use crate::workspace::TasksDirectoryResolver;
use chrono::Utc;
use serde::Serialize;
use serde_json::json;
use std::collections::{HashMap, VecDeque};
use std::fs;
use std::io::{BufRead, BufReader, Write};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, LazyLock, Mutex};
use std::thread;
use std::time::Duration;

const EVENT_LOG_LIMIT: usize = 200;
const DEFAULT_AGENT_INSTRUCTIONS: &str = include_str!("../../docs/help/agent-instructions.md");

static JOB_COUNTER: AtomicU64 = AtomicU64::new(1);
static ORCHESTRATOR_MODE: LazyLock<Mutex<AgentOrchestratorMode>> =
    LazyLock::new(|| Mutex::new(AgentOrchestratorMode::Standalone));
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
    is_merge_job: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct AgentJobEvent {
    pub kind: String,
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
    runtime: Option<AgentJobRuntime>,
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
        ORCHESTRATOR_MODE
            .lock()
            .map(|guard| *guard)
            .unwrap_or(AgentOrchestratorMode::Standalone)
    }

    pub fn start_job(
        req: AgentJobCreateRequest,
        resolver: &TasksDirectoryResolver,
    ) -> LoTaRResult<AgentJob> {
        Self::start_job_with_tasks_dir(req, &resolver.path)
    }

    pub fn start_job_with_tasks_dir(
        req: AgentJobCreateRequest,
        tasks_dir: &std::path::Path,
    ) -> LoTaRResult<AgentJob> {
        Self::start_job_with_tasks_dir_mode(req, tasks_dir, JobStartMode::RespectLimits)
    }

    pub fn start_job_with_tasks_dir_mode(
        req: AgentJobCreateRequest,
        tasks_dir: &std::path::Path,
        mode: JobStartMode,
    ) -> LoTaRResult<AgentJob> {
        let ticket_id = req.ticket_id.trim().to_string();
        if ticket_id.is_empty() {
            return Err(LoTaRError::ValidationError(
                "Ticket id is required".to_string(),
            ));
        }

        let cfg_mgr = ConfigManager::new_manager_with_tasks_dir_readonly(tasks_dir)
            .map_err(|e| LoTaRError::ValidationError(e.to_string()))?;

        let project_prefix = ticket_id.split('-').next().unwrap_or("");
        let config = if project_prefix.is_empty() {
            cfg_mgr.get_resolved_config().clone()
        } else {
            cfg_mgr
                .get_project_config(project_prefix)
                .unwrap_or_else(|_| cfg_mgr.get_resolved_config().clone())
        };

        let profile = resolve_profile(&config, &req)?;
        let runner_kind = profile.runner.parse::<AgentRunnerKind>().map_err(|_| {
            LoTaRError::ValidationError(format!(
                "Unsupported runner '{}'. Expected copilot, claude, codex, gemini, or command",
                profile.runner
            ))
        })?;

        let storage = Storage::new(tasks_dir.to_path_buf());
        let task = TaskService::get(&storage, &ticket_id, None)?;
        let is_merge_job = is_merge_job_candidate(&task, req.agent.as_deref());

        if is_merge_job && !config.agent_worktree.enabled {
            return Err(LoTaRError::ValidationError(
                "Merge agent jobs require agent.worktree.enabled=true so merges run on isolated branches."
                    .to_string(),
            ));
        }

        let context = AgentContextService::load(tasks_dir, &config, &ticket_id)?;
        let instructions = resolve_agent_instructions(tasks_dir, &config, &profile)?;
        let user_prompt = req.prompt.trim().to_string();
        if user_prompt.is_empty() {
            return Err(LoTaRError::ValidationError(
                "Prompt cannot be empty".to_string(),
            ));
        }
        let prompt = build_prompt(
            &task,
            &user_prompt,
            context.as_ref(),
            instructions.as_deref(),
        );

        let job_id = make_job_id();
        let created_at = Utc::now().to_rfc3339();
        let tasks_dir_buf = tasks_dir.to_path_buf();

        let should_queue: bool;
        {
            let mut registry = JOB_REGISTRY
                .lock()
                .map_err(|_| LoTaRError::ValidationError("Job registry unavailable".to_string()))?;

            if registry.active_by_ticket.contains_key(&ticket_id) {
                return Err(LoTaRError::ValidationError(format!(
                    "Ticket '{}' already has an active job",
                    ticket_id
                )));
            }

            // Update max_parallel_jobs from config (allows runtime changes)
            registry.max_parallel_jobs = config.agent_worktree.max_parallel_jobs;

            let parallel_limit_reached = registry
                .max_parallel_jobs
                .is_some_and(|max| registry.running_count >= max);
            let merge_slot_busy = is_merge_job && has_active_merge_job(&registry);

            should_queue = matches!(mode, JobStartMode::RespectLimits)
                && (parallel_limit_reached || merge_slot_busy);

            // Compute workspace root (parent of tasks_dir)
            let workspace_root = tasks_dir_buf
                .parent()
                .map(|p| p.to_path_buf())
                .unwrap_or_else(|| tasks_dir_buf.clone());

            registry
                .active_by_ticket
                .insert(ticket_id.clone(), job_id.clone());
            registry.jobs.insert(
                job_id.clone(),
                AgentJobState {
                    record: AgentJobRecord {
                        id: job_id.clone(),
                        ticket_id: ticket_id.clone(),
                        tasks_dir: tasks_dir_buf.clone(),
                        workspace_root,
                        agent_logs_dir: config.agent_logs_dir.clone(),
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
                        is_merge_job,
                    },
                    events: Vec::new(),
                    runtime: None,
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
                run_job(
                    job_id_clone,
                    runner_kind,
                    profile,
                    tasks_dir_buf,
                    ticket_id,
                    prompt,
                    user_prompt_clone,
                    config,
                );
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
        }))
    }

    pub fn list_jobs() -> Vec<AgentJob> {
        let registry = JOB_REGISTRY.lock();
        if let Ok(registry) = registry {
            return registry
                .jobs
                .values()
                .map(|state| state.record.to_dto())
                .collect();
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
            crate::api_types::AgentQueueStats {
                running: 0,
                queued: 0,
                max_parallel: None,
            }
        }
    }

    pub fn has_active_job(ticket_id: &str) -> bool {
        if let Ok(registry) = JOB_REGISTRY.lock() {
            return registry.active_by_ticket.contains_key(ticket_id);
        }
        false
    }

    pub fn get_job(id: &str) -> Option<AgentJob> {
        get_job_dto(id)
    }

    pub fn cancel_job(id: &str) -> LoTaRResult<Option<AgentJob>> {
        let mut registry = JOB_REGISTRY
            .lock()
            .map_err(|_| LoTaRError::ValidationError("Job registry unavailable".to_string()))?;
        let (job, ticket_id, cancelled, tasks_dir, was_pending) = {
            let was_pending = registry.pending_queue.iter().any(|p| p.job_id == id);
            let Some(state) = registry.jobs.get_mut(id) else {
                return Ok(None);
            };
            let ticket_id = state.record.ticket_id.clone();
            let tasks_dir = state.record.tasks_dir.clone();
            if let Some(runtime) = state.runtime.as_ref()
                && let Ok(mut child) = runtime.child.lock()
            {
                terminate_child(&mut child);
            }
            let mut cancelled = false;
            if state.record.status != AgentJobStatus::Cancelled {
                state.record.status = AgentJobStatus::Cancelled;
                state.record.finished_at = Some(Utc::now().to_rfc3339());
                push_event(state, "agent_job_cancelled", None);
                emit_job_event("agent_job_cancelled", &state.record, None);
                cancelled = true;
            }
            let job = state.record.to_dto_with_cancelled(cancelled);
            (job, ticket_id, cancelled, tasks_dir, was_pending)
        };
        if cancelled {
            registry.active_by_ticket.remove(&ticket_id);
            // Remove from pending queue if it was queued
            registry.pending_queue.retain(|p| p.job_id != id);
            // Decrement running count if the job was occupying a running slot
            if !was_pending && registry.running_count > 0 {
                registry.running_count -= 1;
            }
        }
        drop(registry);

        if cancelled {
            let job_context = job_context_for(id);
            let _ = AutomationService::apply_job_event(
                tasks_dir.as_path(),
                &ticket_id,
                AutomationEvent::JobCancelled,
                job_context,
            );
            if let Ok(config) = load_and_merge_configs(Some(tasks_dir.as_path())) {
                maybe_cleanup_worktree(
                    id,
                    &ticket_id,
                    tasks_dir.as_path(),
                    &config,
                    JobOutcome::Cancelled,
                );
            }
            process_pending_queue();
        }
        Ok(Some(job))
    }

    pub fn cancel_all_jobs() -> LoTaRResult<Vec<AgentJob>> {
        let ids = {
            let registry = JOB_REGISTRY
                .lock()
                .map_err(|_| LoTaRError::ValidationError("Job registry unavailable".to_string()))?;
            registry
                .jobs
                .iter()
                .filter_map(|(id, state)| {
                    matches!(
                        state.record.status,
                        AgentJobStatus::Queued | AgentJobStatus::Running
                    )
                    .then_some(id.clone())
                })
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
            return Err(LoTaRError::ValidationError(
                "Message cannot be empty".to_string(),
            ));
        }

        let mut registry = JOB_REGISTRY
            .lock()
            .map_err(|_| LoTaRError::ValidationError("Job registry unavailable".to_string()))?;
        let (stdin, runner_kind) = {
            let Some(state) = registry.jobs.get_mut(id) else {
                return Err(LoTaRError::ValidationError("Job not found".to_string()));
            };

            if state.record.status != AgentJobStatus::Running {
                return Err(LoTaRError::ValidationError(
                    "Job is not running".to_string(),
                ));
            }

            let Some(runtime) = state.runtime.as_ref() else {
                return Err(LoTaRError::ValidationError(
                    "Job is not accepting messages".to_string(),
                ));
            };
            let Some(stdin) = runtime.stdin.as_ref() else {
                return Err(LoTaRError::ValidationError(
                    "Runner does not support messages".to_string(),
                ));
            };

            (Arc::clone(stdin), state.record.runner_kind)
        };

        let payload = format_stdin_message(runner_kind, trimmed).ok_or_else(|| {
            LoTaRError::ValidationError(format!(
                "Runner '{}' does not support stdin messages",
                runner_kind.as_str()
            ))
        })?;

        {
            let mut writer = stdin
                .lock()
                .map_err(|_| LoTaRError::ValidationError("Job stdin unavailable".to_string()))?;
            writer.write_all(&payload).map_err(LoTaRError::IoError)?;
            writer.flush().map_err(LoTaRError::IoError)?;
        }

        let state = registry
            .jobs
            .get_mut(id)
            .ok_or_else(|| LoTaRError::ValidationError("Job not found".to_string()))?;
        push_event(state, "agent_job_input", Some(trimmed.to_string()));
        emit_job_event("agent_job_input", &state.record, Some(trimmed.to_string()));

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
        "--".to_string(),
        command.program.clone(),
    ];
    args.extend(command.args.iter().cloned());

    crate::services::agent_runner::RunnerCommand {
        program: wrapper_program,
        args,
        env: command.env,
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
        if candidate.is_file() {
            return Some(candidate.to_string_lossy().to_string());
        }
    }

    let path = std::env::var_os("PATH")?;
    for entry in std::env::split_paths(&path) {
        let candidate = entry.join(wrapper_executable_name());
        if candidate.is_file() {
            return Some(candidate.to_string_lossy().to_string());
        }
    }

    None
}

fn wrapper_executable_name() -> &'static str {
    if cfg!(windows) {
        "lotar-agent-wrapper.exe"
    } else {
        "lotar-agent-wrapper"
    }
}

fn terminate_child(child: &mut Child) {
    #[cfg(unix)]
    unsafe {
        let pid = child.id() as i32;
        let _ = libc::kill(-pid, libc::SIGTERM);
    }
    let _ = child.kill();
}

fn resolve_profile(
    config: &ResolvedConfig,
    req: &AgentJobCreateRequest,
) -> LoTaRResult<AgentProfileDetail> {
    if let Some(name) = req.agent.as_deref() {
        let profile = config.agent_profiles.get(name).ok_or_else(|| {
            LoTaRError::ValidationError(format!("Unknown agent profile '{}'.", name))
        })?;
        return Ok(profile.clone());
    }

    if let Some(runner) = req.runner.as_deref() {
        return Ok(AgentProfileDetail {
            runner: runner.to_string(),
            command: None,
            args: Vec::new(),
            env: HashMap::new(),
            tools: None,
            mcp: None,
            instructions: None,
        });
    }

    Err(LoTaRError::ValidationError(
        "Either agent or runner must be provided".to_string(),
    ))
}

fn resolve_agent_instructions(
    tasks_dir: &std::path::Path,
    config: &ResolvedConfig,
    profile: &AgentProfileDetail,
) -> LoTaRResult<Option<String>> {
    // Profile-level instructions take precedence over global config
    let instructions_config = profile
        .instructions
        .as_ref()
        .or(config.agent_instructions.as_ref());

    match instructions_config {
        Some(AgentInstructionsConfig::Inline(raw)) => {
            let trimmed = raw.trim();
            if trimmed.is_empty() {
                Ok(None)
            } else {
                Ok(Some(trimmed.to_string()))
            }
        }
        Some(AgentInstructionsConfig::File { file }) => {
            let path = resolve_agent_instructions_path(tasks_dir, file)?;
            let payload = fs::read_to_string(&path).map_err(|err| {
                LoTaRError::ValidationError(format!(
                    "Failed to read agent.instructions file '{}': {}",
                    path.display(),
                    err
                ))
            })?;
            let trimmed = payload.trim();
            if trimmed.is_empty() {
                Ok(None)
            } else {
                Ok(Some(trimmed.to_string()))
            }
        }
        None => Ok(Some(DEFAULT_AGENT_INSTRUCTIONS.to_string())),
    }
}

fn resolve_agent_instructions_path(
    tasks_dir: &std::path::Path,
    raw: &str,
) -> LoTaRResult<std::path::PathBuf> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Err(LoTaRError::ValidationError(
            "agent.instructions file path cannot be empty".to_string(),
        ));
    }

    let configured = std::path::Path::new(trimmed);
    if configured.is_absolute() {
        return Ok(configured.to_path_buf());
    }

    if configured
        .components()
        .any(|component| matches!(component, std::path::Component::ParentDir))
    {
        return Err(LoTaRError::ValidationError(
            "agent.instructions file path cannot contain '..'".to_string(),
        ));
    }

    Ok(tasks_dir.join(configured))
}

fn build_prompt(
    task: &crate::api_types::TaskDTO,
    user_prompt: &str,
    context: Option<&crate::services::agent_context_service::AgentContextFile>,
    instructions: Option<&str>,
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
            out.push_str(&format!(
                "- Depends on: {}\n",
                task.relationships.depends_on.join(", ")
            ));
        }
        if !task.relationships.blocks.is_empty() {
            out.push_str(&format!(
                "- Blocks: {}\n",
                task.relationships.blocks.join(", ")
            ));
        }
        if !task.relationships.related.is_empty() {
            out.push_str(&format!(
                "- Related: {}\n",
                task.relationships.related.join(", ")
            ));
        }
        if let Some(parent) = task.relationships.parent.as_ref() {
            out.push_str(&format!("- Parent: {}\n", parent));
        }
        if !task.relationships.children.is_empty() {
            out.push_str(&format!(
                "- Children: {}\n",
                task.relationships.children.join(", ")
            ));
        }
        if !task.relationships.fixes.is_empty() {
            out.push_str(&format!(
                "- Fixes: {}\n",
                task.relationships.fixes.join(", ")
            ));
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

struct WorktreeContext {
    working_dir: std::path::PathBuf,
    worktree_path: Option<std::path::PathBuf>,
    worktree_branch: Option<String>,
}

fn prepare_worktree(
    _job_id: &str,
    tasks_dir: &std::path::Path,
    ticket_id: &str,
    config: &ResolvedConfig,
) -> LoTaRResult<WorktreeContext> {
    let repo_root = resolve_repo_root(tasks_dir)
        .ok_or_else(|| LoTaRError::ValidationError("Git repository not found".to_string()))?;

    let mut context = WorktreeContext {
        working_dir: repo_root.clone(),
        worktree_path: None,
        worktree_branch: None,
    };

    if !config.agent_worktree.enabled {
        return Ok(context);
    }

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
        // Worktree already set up for this ticket, reuse it
        context.working_dir = worktree_path.clone();
        context.worktree_path = Some(worktree_path);
        context.worktree_branch = Some(branch);
        return Ok(context);
    }

    let worktree_arg = worktree_path.to_string_lossy().to_string();
    run_git_command(&repo_root, &["worktree", "add", &worktree_arg, &branch])?;

    context.working_dir = worktree_path.clone();
    context.worktree_path = Some(worktree_path);
    context.worktree_branch = Some(branch);
    Ok(context)
}

fn resolve_repo_root(tasks_dir: &std::path::Path) -> Option<std::path::PathBuf> {
    crate::utils::git::find_repo_root(tasks_dir)
}

fn resolve_worktree_root(
    repo_root: &std::path::Path,
    configured: Option<&str>,
) -> LoTaRResult<std::path::PathBuf> {
    let base = repo_root.parent().unwrap_or(repo_root);
    if let Some(raw) = configured {
        let trimmed = raw.trim();
        if trimmed.is_empty() {
            return Err(LoTaRError::ValidationError(
                "agent.worktree.dir cannot be empty".to_string(),
            ));
        }
        let path = std::path::Path::new(trimmed);
        if path.is_absolute() {
            return Ok(path.to_path_buf());
        }
        if path
            .components()
            .any(|component| matches!(component, std::path::Component::ParentDir))
        {
            return Err(LoTaRError::ValidationError(
                "agent.worktree.dir cannot contain '..'".to_string(),
            ));
        }
        return Ok(base.join(path));
    }

    let repo_name = repo_root
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("repo");
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
    if out.is_empty() {
        "ticket".to_string()
    } else {
        out
    }
}

fn build_worktree_branch(prefix: &str, suffix: &str) -> String {
    let trimmed = prefix.trim();
    let mut normalized = if trimmed.is_empty() {
        "agent/".to_string()
    } else {
        trimmed.to_string()
    };
    if !normalized.ends_with('/') {
        normalized.push('/');
    }
    normalized.push_str(suffix);
    normalized
}

fn branch_exists(repo_root: &std::path::Path, branch: &str) -> bool {
    let reference = format!("refs/heads/{}", branch);
    Command::new("git")
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
    let output = Command::new("git")
        .args(args)
        .current_dir(repo_root)
        .output()
        .map_err(LoTaRError::IoError)?;

    if output.status.success() {
        return Ok(());
    }

    let stderr = String::from_utf8_lossy(&output.stderr);
    Err(LoTaRError::ValidationError(format!(
        "git {} failed: {}",
        args.join(" "),
        stderr.trim()
    )))
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
    Some(WorktreeCleanupTarget {
        path: std::path::PathBuf::from(path),
        branch: state.record.worktree_branch.clone(),
    })
}

fn ticket_is_done(tasks_dir: &std::path::Path, ticket_id: &str, config: &ResolvedConfig) -> bool {
    let storage = Storage::new(tasks_dir.to_path_buf());
    let done_statuses = determine_done_statuses_from_config(config);
    match TaskService::get(&storage, ticket_id, None) {
        Ok(task) => done_statuses.contains(&task.status.as_str().to_ascii_lowercase()),
        Err(_) => true,
    }
}

enum JobOutcome {
    Success,
    Failure,
    Cancelled,
}

fn maybe_cleanup_worktree(
    job_id: &str,
    ticket_id: &str,
    tasks_dir: &std::path::Path,
    config: &ResolvedConfig,
    outcome: JobOutcome,
) {
    let should_cleanup = match outcome {
        JobOutcome::Success => {
            config.agent_worktree.cleanup_on_done && ticket_is_done(tasks_dir, ticket_id, config)
        }
        JobOutcome::Failure => config.agent_worktree.cleanup_on_failure,
        JobOutcome::Cancelled => config.agent_worktree.cleanup_on_cancel,
    };
    if !should_cleanup {
        return;
    }
    let Some(target) = resolve_job_worktree(job_id) else {
        return;
    };
    let repo_root = match resolve_repo_root(tasks_dir) {
        Some(root) => root,
        None => return,
    };
    if target.path == repo_root {
        return;
    }

    let worktree_arg = target.path.to_string_lossy().to_string();
    let _ = run_git_command(
        &repo_root,
        &["worktree", "remove", "--force", &worktree_arg],
    );

    if config.agent_worktree.cleanup_delete_branches
        && let Some(branch) = target.branch.as_deref()
    {
        let _ = run_git_command(&repo_root, &["branch", "-D", branch]);
    }
}

#[allow(clippy::too_many_arguments)]
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
    let worktree_context = match prepare_worktree(&job_id, &tasks_dir, &ticket_id, &config) {
        Ok(context) => {
            update_job(&job_id, |state| {
                state.record.worktree_path = context
                    .worktree_path
                    .as_ref()
                    .map(|path| path.to_string_lossy().to_string());
                state.record.worktree_branch = context.worktree_branch.clone();
            });
            if let Some(path) = context.worktree_path.as_ref() {
                let msg = format!("Worktree: {}", path.to_string_lossy());
                update_job(&job_id, |state| {
                    push_event(state, "agent_job_progress", Some(msg.clone()));
                    emit_job_event("agent_job_progress", &state.record, Some(msg.clone()));
                });
            }
            context
        }
        Err(err) => {
            let msg = format!("Worktree setup skipped: {}", err);
            update_job(&job_id, |state| {
                push_event(state, "agent_job_progress", Some(msg.clone()));
                emit_job_event("agent_job_progress", &state.record, Some(msg.clone()));
            });
            WorktreeContext {
                working_dir: resolve_repo_root(&tasks_dir).unwrap_or_else(|| tasks_dir.clone()),
                worktree_path: None,
                worktree_branch: None,
            }
        }
    };

    let started_at = Utc::now().to_rfc3339();

    // Compute workspace root (parent of tasks_dir)
    let workspace_root = tasks_dir
        .parent()
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|| tasks_dir.clone());

    // Initialize persistent log file (if logging is enabled)
    let worktree_path_str = worktree_context
        .worktree_path
        .as_ref()
        .map(|p| p.to_string_lossy().to_string());
    let _ = AgentLogService::init_log(
        &workspace_root,
        config.agent_logs_dir.as_deref(),
        &job_id,
        &ticket_id,
        runner_kind.as_str(),
        &started_at,
        worktree_path_str.as_deref(),
        worktree_context.worktree_branch.as_deref(),
    );

    let mut cancelled = false;
    update_job(&job_id, |state| {
        if state.record.status == AgentJobStatus::Cancelled {
            cancelled = true;
            return;
        }
        state.record.status = AgentJobStatus::Running;
        state.record.started_at = Some(started_at.clone());
        push_event(state, "agent_job_started", None);
        emit_job_event("agent_job_started", &state.record, None);
    });

    if cancelled {
        return;
    }

    let _ = AutomationService::apply_job_event(
        tasks_dir.as_path(),
        &ticket_id,
        AutomationEvent::JobStarted,
        job_context_for(&job_id),
    );

    let command = match build_runner_command(runner_kind, &profile, &prompt) {
        Ok(cmd) => cmd,
        Err(err) => {
            update_job_failure(&job_id, &ticket_id, err.to_string(), &tasks_dir);
            let _ = AutomationService::apply_job_event(
                tasks_dir.as_path(),
                &ticket_id,
                AutomationEvent::JobFailed,
                job_context_for(&job_id),
            );
            return;
        }
    };

    let command = wrap_runner_command(command, &job_id, &ticket_id, runner_kind.as_str());

    if let Err(err) = validate_runner_command(&command) {
        update_job_failure(&job_id, &ticket_id, err.to_string(), &tasks_dir);
        let _ = AutomationService::apply_job_event(
            tasks_dir.as_path(),
            &ticket_id,
            AutomationEvent::JobFailed,
            job_context_for(&job_id),
        );
        return;
    }

    // Inject LOTAR_* environment variables so the agent runner process has
    // full context about the task it is working on.
    let mut command = command;
    if let Some(storage) = Storage::try_open(tasks_dir.clone())
        && let Ok(task) = TaskService::get(&storage, &ticket_id, None)
    {
        let job_ctx = job_context_for(&job_id);
        let lotar_env = build_lotar_env(&task, &config, &tasks_dir, None, job_ctx.as_ref());
        command.env.extend(lotar_env);
    }

    let mut cmd = Command::new(&command.program);
    cmd.args(&command.args)
        .envs(command.env)
        // Use null stdin - Claude CLI waits indefinitely when stdin is a pipe
        // unless --input-format stream-json is used.
        .stdin(if supports_stdin(runner_kind) {
            Stdio::piped()
        } else {
            Stdio::null()
        })
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());

    cmd.current_dir(&worktree_context.working_dir);

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

    let child = match cmd.spawn() {
        Ok(child) => child,
        Err(err) => {
            update_job_failure(
                &job_id,
                &ticket_id,
                format!("Failed to spawn runner: {}", err),
                &tasks_dir,
            );
            let _ = AutomationService::apply_job_event(
                tasks_dir.as_path(),
                &ticket_id,
                AutomationEvent::JobFailed,
                job_context_for(&job_id),
            );
            return;
        }
    };

    let child = Arc::new(Mutex::new(child));
    let stdout = child.lock().ok().and_then(|mut c| c.stdout.take());
    let stderr = child.lock().ok().and_then(|mut c| c.stderr.take());
    let stdin = child.lock().ok().and_then(|mut c| c.stdin.take());
    let stdin = stdin.map(|handle| Arc::new(Mutex::new(handle)));

    update_job(&job_id, |state| {
        state.runtime = Some(AgentJobRuntime {
            child: Arc::clone(&child),
            stdin: stdin.clone(),
        });
    });

    let mut context_messages = vec![build_user_message(&user_prompt)];

    if let Some(stderr) = stderr {
        let job_id_clone = job_id.clone();
        thread::spawn(move || {
            let mut reader = BufReader::new(stderr);
            let mut line = String::new();
            loop {
                line.clear();
                match reader.read_line(&mut line) {
                    Ok(0) => break,
                    Ok(_) => {
                        let msg = line.trim().to_string();
                        if !msg.is_empty() {
                            update_job(&job_id_clone, |state| {
                                state.record.last_message = Some(msg.clone());
                                push_event(state, "agent_job_progress", Some(msg.clone()));
                                emit_job_event(
                                    "agent_job_progress",
                                    &state.record,
                                    Some(msg.clone()),
                                );
                            });
                        }
                    }
                    Err(_) => break,
                }
            }
        });
    }

    if let Some(stdout) = stdout {
        let job_id_clone = job_id.clone();
        let child_clone = Arc::clone(&child);
        let mut reader = BufReader::new(stdout);
        let mut line = String::new();
        loop {
            line.clear();
            match reader.read_line(&mut line) {
                Ok(0) => break,
                Ok(_) => {
                    handle_runner_line(&job_id_clone, runner_kind, &line, &mut context_messages);
                }
                Err(_) => break,
            }
            if should_stop_job(&job_id_clone) {
                if let Ok(mut c) = child_clone.lock() {
                    terminate_child(&mut c);
                }
                break;
            }
        }
    }

    loop {
        let status = child
            .lock()
            .ok()
            .and_then(|mut c| c.try_wait().ok().flatten());
        if let Some(status) = status {
            let code = status.code();
            finalize_job(
                &job_id,
                &ticket_id,
                code,
                &context_messages,
                &tasks_dir,
                &config,
            );
            break;
        }
        if should_stop_job(&job_id) {
            if let Ok(mut c) = child.lock() {
                terminate_child(&mut c);
            }
            break;
        }
        thread::sleep(Duration::from_millis(200));
    }
}

#[allow(clippy::ptr_arg)]
fn handle_runner_line(
    job_id: &str,
    runner_kind: AgentRunnerKind,
    line: &str,
    context_messages: &mut Vec<crate::services::agent_context_service::AgentContextMessage>,
) {
    let Some(event) = parse_runner_line(runner_kind, line) else {
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

    if let Some(session_id) = event.session_id.clone() {
        update_job(job_id, |state| {
            state.record.session_id = Some(session_id.clone());
        });
    }

    match event.kind {
        RunnerEventKind::Init => {
            update_job(job_id, |state| {
                push_event(state, "agent_job_init", None);
            });
        }
        RunnerEventKind::Progress => {
            let message = event.text.clone();
            update_job(job_id, |state| {
                if let Some(ref msg) = message {
                    state.record.last_message = Some(msg.clone());
                }
                push_event(state, "agent_job_progress", message.clone());
                emit_job_event("agent_job_progress", &state.record, message.clone());
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
                push_event(state, "agent_job_message", message.clone());
                emit_job_event("agent_job_message", &state.record, message.clone());
            });
        }
        RunnerEventKind::Result => {
            update_job(job_id, |state| {
                push_event(state, "agent_job_result", None);
            });
        }
    }
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
    let success = exit_code.unwrap_or(1) == 0;
    update_job(job_id, |state| {
        if state.record.status == AgentJobStatus::Cancelled {
            return;
        }
        state.record.exit_code = exit_code;
        state.record.finished_at = Some(now.clone());
        state.record.status = if success {
            AgentJobStatus::Completed
        } else {
            AgentJobStatus::Failed
        };
        state.record.summary = state.record.last_message.clone();
        let event_kind = if success {
            "agent_job_completed"
        } else {
            "agent_job_failed"
        };
        push_event(state, event_kind, None);
        emit_job_event(event_kind, &state.record, None);
    });

    // Write final status to persistent log (if logging is enabled)
    let status_str = if success { "completed" } else { "failed" };
    let (summary, workspace_root, logs_dir) = if let Ok(registry) = JOB_REGISTRY.lock() {
        if let Some(state) = registry.jobs.get(job_id) {
            (
                state.record.summary.clone(),
                state.record.workspace_root.clone(),
                state.record.agent_logs_dir.clone(),
            )
        } else {
            (None, tasks_dir.to_path_buf(), None)
        }
    } else {
        (None, tasks_dir.to_path_buf(), None)
    };
    let _ = AgentLogService::write_status(
        &workspace_root,
        logs_dir.as_deref(),
        job_id,
        status_str,
        &Utc::now().to_rfc3339(),
        exit_code,
        summary,
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
            });
        }
        registry.active_by_ticket.remove(ticket_id);
        if let Some(state) = registry.jobs.get_mut(job_id) {
            state.runtime = None;
        }
        if registry.running_count > 0 {
            registry.running_count -= 1;
        }
    }
    drop(registry);

    if !should_stop_job(job_id) {
        let event = if success {
            AutomationEvent::JobCompleted
        } else {
            AutomationEvent::JobFailed
        };
        let _ = AutomationService::apply_job_event(tasks_dir, ticket_id, event, job_context);
    }

    let _ = AgentContextService::append_messages(
        tasks_dir,
        config,
        ticket_id,
        context_messages.to_vec(),
        None,
    );

    maybe_cleanup_worktree(
        job_id,
        ticket_id,
        tasks_dir,
        config,
        if success {
            JobOutcome::Success
        } else {
            JobOutcome::Failure
        },
    );

    // Try to start the next queued job
    process_pending_queue();
}

fn update_job_failure(job_id: &str, ticket_id: &str, message: String, tasks_dir: &std::path::Path) {
    let now = Utc::now().to_rfc3339();
    update_job(job_id, |state| {
        state.record.status = AgentJobStatus::Failed;
        state.record.finished_at = Some(now.clone());
        state.record.last_message = Some(message.clone());
        state.record.summary = Some(message.clone());
        push_event(state, "agent_job_failed", Some(message.clone()));
        emit_job_event("agent_job_failed", &state.record, Some(message.clone()));
    });

    // Write final status to persistent log (if logging is enabled)
    let (workspace_root, logs_dir) = if let Ok(registry) = JOB_REGISTRY.lock() {
        if let Some(state) = registry.jobs.get(job_id) {
            (
                state.record.workspace_root.clone(),
                state.record.agent_logs_dir.clone(),
            )
        } else {
            (tasks_dir.to_path_buf(), None)
        }
    } else {
        (tasks_dir.to_path_buf(), None)
    };
    let _ = AgentLogService::write_status(
        &workspace_root,
        logs_dir.as_deref(),
        job_id,
        "failed",
        &now,
        None,
        Some(message),
    );

    let mut registry = JOB_REGISTRY.lock().ok();
    if let Some(ref mut registry) = registry {
        registry.active_by_ticket.remove(ticket_id);
        // Decrement running count
        if registry.running_count > 0 {
            registry.running_count -= 1;
        }
    }
    drop(registry);

    // Try to start the next queued job
    process_pending_queue();
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
        let can_start = registry
            .max_parallel_jobs
            .is_none_or(|max| registry.running_count < max);

        if !can_start {
            return;
        }

        let running_merge_job = has_running_merge_job(&registry);
        let next_index = registry
            .pending_queue
            .iter()
            .position(|pending| !(pending.is_merge_job && running_merge_job));

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
    registry
        .jobs
        .values()
        .any(|state| state.record.is_merge_job && state.record.status == AgentJobStatus::Running)
}

fn has_active_merge_job(registry: &JobRegistry) -> bool {
    registry.jobs.values().any(|state| {
        state.record.is_merge_job
            && matches!(
                state.record.status,
                AgentJobStatus::Queued | AgentJobStatus::Running
            )
    })
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

fn push_event(state: &mut AgentJobState, kind: &str, message: Option<String>) {
    let at = Utc::now().to_rfc3339();
    state.events.push(AgentJobEvent {
        kind: kind.to_string(),
        at: at.clone(),
        message: message.clone(),
    });
    if state.events.len() > EVENT_LOG_LIMIT {
        let start = state.events.len().saturating_sub(EVENT_LOG_LIMIT);
        state.events = state.events[start..].to_vec();
    }

    // Also append to persistent log file (if logging is enabled)
    let _ = AgentLogService::append_event(
        &state.record.workspace_root,
        state.record.agent_logs_dir.as_deref(),
        &state.record.id,
        kind,
        &at,
        message,
    );
}

fn emit_job_event(kind: &str, record: &AgentJobRecord, message: Option<String>) {
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
        "message": message,
        "worktree_path": record.worktree_path,
        "worktree_branch": record.worktree_branch,
    });
    crate::api_events::emit(crate::api_events::ApiEvent {
        kind: kind.to_string(),
        data: payload,
    });
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
            sprints: vec![],
            sprint_order: Default::default(),
            history: vec![],
            custom_fields: Default::default(),
        }
    }

    #[test]
    fn build_prompt_includes_ticket_and_request() {
        let task = sample_task();
        let prompt = build_prompt(&task, "ship it", None, None);
        assert!(prompt.contains("TEST-1"));
        assert!(prompt.contains("ship it"));
    }

    #[test]
    fn build_prompt_includes_instructions() {
        let task = sample_task();
        let prompt = build_prompt(&task, "ship it", None, Some("Use the tools"));
        assert!(prompt.contains("Agent instructions:"));
        assert!(prompt.contains("Use the tools"));
    }

    #[test]
    fn resolve_agent_instructions_reads_file() {
        let dir = tempdir().unwrap();
        let file_path = dir.path().join("agent-instructions.txt");
        fs::write(&file_path, "Use the tools").unwrap();

        let mut config = ResolvedConfig::from_global(GlobalConfig::default());
        config.agent_instructions = Some(AgentInstructionsConfig::File {
            file: "agent-instructions.txt".to_string(),
        });

        let profile = AgentProfileConfig::Runner("claude".to_string()).to_detail();
        let resolved = resolve_agent_instructions(dir.path(), &config, &profile).unwrap();
        assert_eq!(resolved.as_deref(), Some("Use the tools"));
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
    fn resolve_agent_instructions_profile_overrides_global() {
        let dir = tempdir().unwrap();
        let mut config = ResolvedConfig::from_global(GlobalConfig::default());
        config.agent_instructions = Some(AgentInstructionsConfig::Inline(
            "global instructions".to_string(),
        ));

        let mut profile = AgentProfileConfig::Runner("claude".to_string()).to_detail();
        profile.instructions = Some(AgentInstructionsConfig::Inline(
            "profile instructions".to_string(),
        ));

        let resolved = resolve_agent_instructions(dir.path(), &config, &profile).unwrap();
        assert_eq!(resolved.as_deref(), Some("profile instructions"));
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
        AgentContextService::append_messages(dir.path(), &config, "TEST-1", messages, Some(5))
            .unwrap();

        let loaded = AgentContextService::load(dir.path(), &config, "TEST-1")
            .unwrap()
            .unwrap();
        assert_eq!(loaded.messages.len(), 5);
        assert!(loaded.messages.last().unwrap().content.contains("msg-29"));
    }

    #[test]
    fn resolve_profile_uses_agent_map() {
        let mut config = ResolvedConfig::from_global(GlobalConfig::default());
        config.agent_profiles.insert(
            "codex-default".to_string(),
            AgentProfileConfig::Runner("codex".to_string()).to_detail(),
        );
        let req = AgentJobCreateRequest {
            ticket_id: "TEST-1".to_string(),
            prompt: "hello".to_string(),
            runner: None,
            agent: Some("codex-default".to_string()),
        };
        let profile = resolve_profile(&config, &req).unwrap();
        assert_eq!(profile.runner, "codex");
    }
}

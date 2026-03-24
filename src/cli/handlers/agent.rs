use crate::api_types::AgentJobCreateRequest;
use crate::automation::types::AutomationFile;
use crate::cli::args::{
    AgentAction, AgentCheckArgs, AgentListJobsArgs, AgentQueueAction, AgentQueueArgs, AgentRunArgs,
    AgentWorkerArgs, WorktreeAction, WorktreeCleanupArgs,
};
use crate::config::manager::ConfigManager;
use crate::output::{OutputFormat, OutputRenderer};
use crate::services::agent_job_service::AgentJobService;
use crate::services::agent_log_service::AgentLogService;
use crate::services::agent_queue_service::AgentQueueService;
use crate::services::automation_service::AutomationService;
use crate::services::task_service::TaskService;
use crate::workspace::TasksDirectoryResolver;
use serde::Serialize;
use std::fmt::Write as _;
use std::thread;
use std::time::{Duration, Instant};
use sysinfo::System;

/// Get the logs directory and workspace root from config.
/// Returns (workspace_root, logs_dir) where logs_dir may be None if not configured.
fn get_logs_config(resolver: &TasksDirectoryResolver) -> (std::path::PathBuf, Option<String>) {
    let workspace_root = resolver
        .path
        .parent()
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|| resolver.path.clone());

    let logs_dir = ConfigManager::new_manager_with_tasks_dir_readonly(&resolver.path)
        .ok()
        .and_then(|cfg| cfg.get_resolved_config().agent_logs_dir.clone());

    (workspace_root, logs_dir)
}

pub struct AgentHandler;

impl AgentHandler {
    pub fn execute(
        action: AgentAction,
        project: Option<&str>,
        resolver: &TasksDirectoryResolver,
        renderer: &OutputRenderer,
    ) -> Result<(), String> {
        match action {
            AgentAction::Run(args) => Self::run(args, resolver, renderer),
            AgentAction::Status { id } => Self::status(&id, resolver, renderer),
            AgentAction::Logs { id } => Self::logs(&id, resolver, renderer),
            AgentAction::Cancel { id } => Self::cancel(&id, renderer),
            AgentAction::Check(args) => Self::check(args, project, resolver, renderer),
            AgentAction::ListRunning => Self::list_running(renderer),
            AgentAction::ListJobs(args) => Self::list_jobs(args, resolver, renderer),
            AgentAction::Queue(args) => Self::queue(args, resolver, renderer),
            AgentAction::Worktree(args) => match args.action {
                WorktreeAction::List => Self::worktree_list(resolver, renderer),
                WorktreeAction::Cleanup(cleanup_args) => {
                    Self::worktree_cleanup(cleanup_args, resolver, renderer)
                }
            },
            AgentAction::Worker(args) => Self::worker(args, resolver),
        }
    }

    fn run(
        args: AgentRunArgs,
        resolver: &TasksDirectoryResolver,
        renderer: &OutputRenderer,
    ) -> Result<(), String> {
        let req = AgentJobCreateRequest {
            ticket_id: args.ticket,
            prompt: args.prompt,
            runner: args.runner,
            agent: args.agent,
        };
        let job = AgentJobService::start_job(req, resolver).map_err(|e| e.to_string())?;

        if matches!(renderer.format, OutputFormat::Json) {
            renderer.emit_json(&serde_json::json!({ "job": job }));
            return Ok(());
        }

        renderer.emit_success(format!("Job {} started", job.id));
        renderer.emit_info(format!("Ticket: {}", job.ticket_id));
        renderer.emit_info(format!("Runner: {}", job.runner));

        let should_wait = args.wait || args.follow;
        if !should_wait {
            return Ok(());
        }

        wait_for_job_completion(&job.id, args.follow, args.timeout_seconds, renderer)
    }

    fn status(
        id: &str,
        resolver: &TasksDirectoryResolver,
        renderer: &OutputRenderer,
    ) -> Result<(), String> {
        // Try in-memory registry first
        if let Some(job) = AgentJobService::get_job(id) {
            if matches!(renderer.format, OutputFormat::Json) {
                renderer.emit_json(&serde_json::json!({ "job": job }));
                return Ok(());
            }
            renderer.emit_success(format!("Job {}: {}", job.id, job.status));
            if let Some(msg) = job.last_message.as_ref() {
                renderer.emit_info(msg);
            }
            return Ok(());
        }

        // Fall back to persisted log file (if logging is enabled)
        let (workspace_root, logs_dir) = get_logs_config(resolver);
        let Some(logs_dir) = logs_dir else {
            return Err(format!(
                "Job '{}' not found (agent logging not enabled)",
                id
            ));
        };

        let header = AgentLogService::load_header(&workspace_root, &logs_dir, id)
            .map_err(|e| e.to_string())?
            .ok_or_else(|| format!("Job '{}' not found", id))?;

        let status = AgentLogService::load_status(&workspace_root, &logs_dir, id)
            .map_err(|e| e.to_string())?;

        if matches!(renderer.format, OutputFormat::Json) {
            let job = serde_json::json!({
                "id": header.job_id,
                "ticket_id": header.ticket_id,
                "runner": header.runner,
                "created_at": header.created_at,
                "status": status.as_ref().map(|s| s.status.as_str()).unwrap_or("unknown"),
                "exit_code": status.as_ref().and_then(|s| s.exit_code),
                "summary": status.as_ref().and_then(|s| s.summary.clone()),
                "worktree_path": header.worktree_path,
                "worktree_branch": header.worktree_branch,
            });
            renderer.emit_json(&serde_json::json!({ "job": job }));
            return Ok(());
        }

        let status_str = status
            .as_ref()
            .map(|s| s.status.as_str())
            .unwrap_or("unknown");
        renderer.emit_success(format!("Job {}: {}", header.job_id, status_str));
        renderer.emit_info(format!("Ticket: {}", header.ticket_id));
        renderer.emit_info(format!("Runner: {}", header.runner));
        if let Some(ref s) = status
            && let Some(ref summary) = s.summary
        {
            renderer.emit_info(format!("Summary: {}", summary));
        }
        Ok(())
    }

    fn logs(
        id: &str,
        resolver: &TasksDirectoryResolver,
        renderer: &OutputRenderer,
    ) -> Result<(), String> {
        // Try in-memory registry first
        let events = AgentJobService::events_for(id);
        if !events.is_empty() {
            if matches!(renderer.format, OutputFormat::Json) {
                renderer.emit_json(&serde_json::json!({ "events": events }));
                return Ok(());
            }
            for entry in events {
                if let Some(msg) = entry.message.as_ref() {
                    renderer.emit_raw_stdout(format_args!("{} [{}] {}", entry.at, entry.kind, msg));
                } else {
                    renderer.emit_raw_stdout(format_args!("{} [{}]", entry.at, entry.kind));
                }
            }
            return Ok(());
        }

        // Fall back to persisted log file (if logging is enabled)
        let (workspace_root, logs_dir) = get_logs_config(resolver);
        let Some(logs_dir) = logs_dir else {
            renderer.emit_notice("No job events recorded (agent logging not enabled).");
            return Ok(());
        };

        let log_events = AgentLogService::load_events(&workspace_root, &logs_dir, id)
            .map_err(|e| e.to_string())?;

        if matches!(renderer.format, OutputFormat::Json) {
            renderer.emit_json(&serde_json::json!({ "events": log_events }));
            return Ok(());
        }

        if log_events.is_empty() {
            renderer.emit_notice("No job events recorded.");
            return Ok(());
        }

        for entry in log_events {
            if let Some(msg) = entry.message.as_ref() {
                renderer.emit_raw_stdout(format_args!("{} [{}] {}", entry.at, entry.kind, msg));
            } else {
                renderer.emit_raw_stdout(format_args!("{} [{}]", entry.at, entry.kind));
            }
        }
        Ok(())
    }

    fn cancel(id: &str, renderer: &OutputRenderer) -> Result<(), String> {
        let job = AgentJobService::cancel_job(id).map_err(|e| e.to_string())?;
        if matches!(renderer.format, OutputFormat::Json) {
            renderer.emit_json(&serde_json::json!({ "job": job }));
            return Ok(());
        }
        if let Some(job) = job {
            renderer.emit_success(format!("Job {} cancelled", job.id));
        } else {
            renderer.emit_warning("Job not found");
        }
        Ok(())
    }

    fn list_running(renderer: &OutputRenderer) -> Result<(), String> {
        let jobs = list_running_jobs();
        if matches!(renderer.format, OutputFormat::Json) {
            renderer.emit_json(&serde_json::json!({ "jobs": jobs }));
            return Ok(());
        }

        if jobs.is_empty() {
            renderer.emit_notice("No running agent jobs found.");
            return Ok(());
        }

        renderer.emit_raw_stdout(format_args!("pid\tjob_id\tticket\trunner"));
        for job in jobs {
            renderer.emit_raw_stdout(format_args!(
                "{}\t{}\t{}\t{}",
                job.pid,
                job.job_id.as_deref().unwrap_or("-"),
                job.ticket_id.as_deref().unwrap_or("-"),
                job.runner.as_deref().unwrap_or("-")
            ));
        }
        Ok(())
    }

    fn list_jobs(
        args: AgentListJobsArgs,
        resolver: &TasksDirectoryResolver,
        renderer: &OutputRenderer,
    ) -> Result<(), String> {
        let (workspace_root, logs_dir) = get_logs_config(resolver);
        let Some(logs_dir) = logs_dir else {
            if matches!(renderer.format, OutputFormat::Json) {
                renderer.emit_json(&serde_json::json!({ "jobs": [] }));
                return Ok(());
            }
            renderer.emit_notice("No job logs found (agent logging not enabled).");
            return Ok(());
        };

        let job_ids =
            AgentLogService::list_logs(&workspace_root, &logs_dir).map_err(|e| e.to_string())?;

        if job_ids.is_empty() {
            if matches!(renderer.format, OutputFormat::Json) {
                renderer.emit_json(&serde_json::json!({ "jobs": [] }));
                return Ok(());
            }
            renderer.emit_notice("No job logs found.");
            return Ok(());
        }

        let mut jobs = Vec::new();
        for job_id in job_ids.iter().take(args.limit) {
            let header = match AgentLogService::load_header(&workspace_root, &logs_dir, job_id) {
                Ok(Some(h)) => h,
                _ => continue,
            };
            let status = AgentLogService::load_status(&workspace_root, &logs_dir, job_id)
                .ok()
                .flatten();

            jobs.push(serde_json::json!({
                "id": header.job_id,
                "ticket_id": header.ticket_id,
                "runner": header.runner,
                "created_at": header.created_at,
                "status": status.as_ref().map(|s| s.status.as_str()).unwrap_or("unknown"),
                "exit_code": status.as_ref().and_then(|s| s.exit_code),
                "summary": status.as_ref().and_then(|s| s.summary.clone()),
            }));
        }

        if matches!(renderer.format, OutputFormat::Json) {
            renderer.emit_json(&serde_json::json!({ "jobs": jobs }));
            return Ok(());
        }

        renderer.emit_raw_stdout(format_args!("job_id\tticket\tstatus\trunner"));
        for job in &jobs {
            renderer.emit_raw_stdout(format_args!(
                "{}\t{}\t{}\t{}",
                job["id"].as_str().unwrap_or("-"),
                job["ticket_id"].as_str().unwrap_or("-"),
                job["status"].as_str().unwrap_or("-"),
                job["runner"].as_str().unwrap_or("-")
            ));
        }
        Ok(())
    }

    fn queue(
        args: AgentQueueArgs,
        resolver: &TasksDirectoryResolver,
        renderer: &OutputRenderer,
    ) -> Result<(), String> {
        let tasks_dir = &resolver.path;

        match args.action {
            Some(AgentQueueAction::Flush) => {
                let count = AgentQueueService::flush(tasks_dir).map_err(|e| e.to_string())?;
                if matches!(renderer.format, OutputFormat::Json) {
                    renderer.emit_json(&serde_json::json!({ "flushed": count }));
                } else if count > 0 {
                    renderer.emit_success(format!("Flushed {} pending entries.", count));
                } else {
                    renderer.emit_notice("Queue is already empty.");
                }
                Ok(())
            }
            Some(AgentQueueAction::Remove { ticket }) => {
                let removed =
                    AgentQueueService::remove(tasks_dir, &ticket).map_err(|e| e.to_string())?;
                if matches!(renderer.format, OutputFormat::Json) {
                    renderer.emit_json(&serde_json::json!({
                        "ticket": ticket,
                        "removed": removed,
                    }));
                } else if removed {
                    renderer.emit_success(format!("Removed {} from queue.", ticket));
                } else {
                    renderer.emit_notice(format!("{} was not in the queue.", ticket));
                }
                Ok(())
            }
            None => {
                // Default: list pending entries + worker status
                let pending =
                    AgentQueueService::list_pending(tasks_dir).map_err(|e| e.to_string())?;
                let worker_running = AgentQueueService::is_worker_running(tasks_dir);
                let stats = AgentJobService::queue_stats();

                if matches!(renderer.format, OutputFormat::Json) {
                    let entries: Vec<_> = pending
                        .iter()
                        .map(|e| {
                            serde_json::json!({
                                "ticket_id": e.ticket_id,
                                "agent": e.agent,
                                "queued_at": e.queued_at,
                                "attempts": e.attempts,
                            })
                        })
                        .collect();
                    renderer.emit_json(&serde_json::json!({
                        "pending": entries,
                        "worker_running": worker_running,
                        "running_jobs": stats.running,
                        "queued_jobs": stats.queued,
                    }));
                    return Ok(());
                }

                renderer.emit_info(format_args!(
                    "Worker: {}  |  Running: {}  |  Queued: {}",
                    if worker_running { "active" } else { "idle" },
                    stats.running,
                    stats.queued,
                ));

                if pending.is_empty() {
                    renderer.emit_notice("No pending entries in queue.");
                } else {
                    renderer.emit_raw_stdout(format_args!("ticket\tagent\tqueued_at\tattempts"));
                    for entry in &pending {
                        renderer.emit_raw_stdout(format_args!(
                            "{}\t{}\t{}\t{}",
                            entry.ticket_id, entry.agent, entry.queued_at, entry.attempts
                        ));
                    }
                }
                Ok(())
            }
        }
    }

    fn worker(_args: AgentWorkerArgs, resolver: &TasksDirectoryResolver) -> Result<(), String> {
        AgentJobService::set_orchestrator_mode(
            crate::services::agent_job_service::AgentOrchestratorMode::Worker,
        );
        AgentQueueService::run_worker(resolver.path.as_path()).map_err(|e| e.to_string())
    }

    fn check(
        args: AgentCheckArgs,
        project: Option<&str>,
        resolver: &TasksDirectoryResolver,
        renderer: &OutputRenderer,
    ) -> Result<(), String> {
        let cfg_mgr = ConfigManager::new_manager_with_tasks_dir_readonly(&resolver.path)
            .map_err(|e| e.to_string())?;
        let config = if let Some(prefix) = project {
            cfg_mgr
                .get_project_config(prefix)
                .unwrap_or_else(|_| cfg_mgr.get_resolved_config().clone())
        } else {
            cfg_mgr.get_resolved_config().clone()
        };

        let statuses = if args.statuses.is_empty() {
            let derived = derive_default_statuses(resolver.path.as_path(), project, &config)
                .map_err(|e| e.to_string())?;
            if derived.is_empty() {
                return Err("No automation status configured; pass --status to check.".to_string());
            }
            derived
        } else {
            let mut parsed = Vec::new();
            for raw in &args.statuses {
                let status = crate::types::TaskStatus::parse_with_config(raw, &config)
                    .map_err(|e| e.to_string())?;
                parsed.push(status);
            }
            parsed
        };

        let filter = crate::api_types::TaskListFilter {
            status: statuses.clone(),
            project: project.map(|p| p.to_string()),
            ..Default::default()
        };
        let storage = crate::storage::manager::Storage::new(resolver.path.clone());
        let mut matches: Vec<crate::api_types::TaskDTO> = TaskService::list(&storage, &filter)
            .into_iter()
            .map(|(_, task)| task)
            .collect();

        if let Some(target) = args.assignee.as_deref() {
            let trimmed = target.trim();
            if !trimmed.is_empty() {
                matches.retain(|task| {
                    task.assignee
                        .as_deref()
                        .is_some_and(|val| val.eq_ignore_ascii_case(trimmed))
                });
            }
        }

        if matches.is_empty() {
            if matches!(renderer.format, OutputFormat::Json) {
                renderer.emit_json(&serde_json::json!({
                    "matches": [],
                    "count": 0,
                    "statuses": statuses,
                }));
            } else {
                renderer.emit_success("No matching tasks found.");
            }
            return Ok(());
        }

        if matches!(renderer.format, OutputFormat::Json) {
            renderer.emit_json(&serde_json::json!({
                "matches": matches,
                "count": matches.len(),
                "statuses": statuses,
            }));
        } else {
            renderer.emit_warning(format!(
                "Found {} task(s) matching the check criteria.",
                matches.len()
            ));
            renderer.emit_raw_stdout(format_args!("id\tstatus\tassignee\ttitle"));
            for task in &matches {
                let mut line = String::new();
                let _ = write!(
                    &mut line,
                    "{}\t{}\t{}\t{}",
                    task.id,
                    task.status,
                    task.assignee.as_deref().unwrap_or("-"),
                    task.title
                );
                renderer.emit_raw_stdout(format_args!("{}", line));
            }
        }

        Err("Agent check failed: matching tasks found.".to_string())
    }
}

fn derive_default_statuses(
    tasks_dir: &std::path::Path,
    project: Option<&str>,
    config: &crate::config::types::ResolvedConfig,
) -> Result<Vec<crate::types::TaskStatus>, String> {
    let inspect = AutomationService::inspect(tasks_dir, project).map_err(|e| e.to_string())?;
    let file: AutomationFile = serde_yaml::from_str(&inspect.effective_yaml)
        .map_err(|e| format!("Invalid automation YAML: {}", e))?;
    let mut statuses = Vec::new();
    for rule in file.automation.rules() {
        let action = rule.on.job_started.as_ref().or(rule.on.start.as_ref());
        if let Some(action) = action
            && let Some(set) = action.set.as_ref()
            && let Some(value) = set.status.as_ref()
            && let Ok(status) = crate::types::TaskStatus::parse_with_config(value, config)
            && !statuses
                .iter()
                .any(|entry: &crate::types::TaskStatus| entry.eq_ignore_case(status.as_str()))
        {
            statuses.push(status);
        }
    }
    Ok(statuses)
}

/// Information about an agent worktree
#[derive(Debug, Clone, Serialize)]
struct WorktreeInfo {
    path: String,
    branch: String,
    ticket_id: String,
    /// Whether the ticket is in a "done" state
    is_done: bool,
    /// Whether there's an active job for this ticket
    has_active_job: bool,
}

impl AgentHandler {
    fn worktree_list(
        resolver: &TasksDirectoryResolver,
        renderer: &OutputRenderer,
    ) -> Result<(), String> {
        let config_manager = ConfigManager::new_manager_with_tasks_dir_readonly(&resolver.path)
            .map_err(|e| e.to_string())?;
        let config = config_manager.get_resolved_config();

        let worktrees = list_agent_worktrees(resolver, config)?;

        if matches!(renderer.format, OutputFormat::Json) {
            renderer.emit_json(&serde_json::json!({ "worktrees": worktrees }));
            return Ok(());
        }

        if worktrees.is_empty() {
            renderer.emit_info("No agent worktrees found");
            return Ok(());
        }

        renderer.emit_info(format!("Found {} agent worktree(s):", worktrees.len()));
        for wt in &worktrees {
            let status = if wt.has_active_job {
                " (active job)"
            } else if wt.is_done {
                " (done)"
            } else {
                ""
            };
            renderer.emit_raw_stdout(format_args!(
                "  {} -> {} [{}]{}",
                wt.ticket_id, wt.branch, wt.path, status
            ));
        }
        Ok(())
    }

    fn worktree_cleanup(
        args: WorktreeCleanupArgs,
        resolver: &TasksDirectoryResolver,
        renderer: &OutputRenderer,
    ) -> Result<(), String> {
        let config_manager = ConfigManager::new_manager_with_tasks_dir_readonly(&resolver.path)
            .map_err(|e| e.to_string())?;
        let config = config_manager.get_resolved_config();

        let worktrees = list_agent_worktrees(resolver, config)?;

        // Filter to worktrees that should be removed
        let to_remove: Vec<_> = worktrees
            .iter()
            .filter(|wt| {
                if wt.has_active_job {
                    return false; // Never remove worktrees with active jobs
                }
                args.all || wt.is_done
            })
            .collect();

        if to_remove.is_empty() {
            if matches!(renderer.format, OutputFormat::Json) {
                renderer.emit_json(&serde_json::json!({
                    "removed": [],
                    "dry_run": args.dry_run
                }));
            } else {
                renderer.emit_info("No worktrees to remove");
            }
            return Ok(());
        }

        let repo_root = crate::utils::git::find_repo_root(&resolver.path)
            .ok_or_else(|| "Git repository not found".to_string())?;

        let mut removed = Vec::new();
        let mut errors = Vec::new();

        for wt in &to_remove {
            if args.dry_run {
                renderer.emit_info(format!("Would remove: {} ({})", wt.ticket_id, wt.path));
                removed.push(wt.ticket_id.clone());
                continue;
            }

            // Remove worktree
            let worktree_result = std::process::Command::new("git")
                .args(["worktree", "remove", "--force", &wt.path])
                .current_dir(&repo_root)
                .output();

            match worktree_result {
                Ok(output) if output.status.success() => {
                    renderer
                        .emit_success(format!("Removed worktree: {} ({})", wt.ticket_id, wt.path));

                    // Optionally delete the branch
                    if args.delete_branches {
                        let branch_result = std::process::Command::new("git")
                            .args(["branch", "-D", &wt.branch])
                            .current_dir(&repo_root)
                            .output();

                        match branch_result {
                            Ok(out) if out.status.success() => {
                                renderer.emit_success(format!("Deleted branch: {}", wt.branch));
                            }
                            Ok(out) => {
                                let stderr = String::from_utf8_lossy(&out.stderr);
                                renderer.emit_warning(format!(
                                    "Failed to delete branch {}: {}",
                                    wt.branch,
                                    stderr.trim()
                                ));
                            }
                            Err(e) => {
                                renderer.emit_warning(format!(
                                    "Failed to delete branch {}: {}",
                                    wt.branch, e
                                ));
                            }
                        }
                    }
                    removed.push(wt.ticket_id.clone());
                }
                Ok(output) => {
                    let stderr = String::from_utf8_lossy(&output.stderr);
                    errors.push(format!("{}: {}", wt.ticket_id, stderr.trim()));
                    renderer.emit_error(format!(
                        "Failed to remove worktree {}: {}",
                        wt.ticket_id,
                        stderr.trim()
                    ));
                }
                Err(e) => {
                    errors.push(format!("{}: {}", wt.ticket_id, e));
                    renderer
                        .emit_error(format!("Failed to remove worktree {}: {}", wt.ticket_id, e));
                }
            }
        }

        if matches!(renderer.format, OutputFormat::Json) {
            renderer.emit_json(&serde_json::json!({
                "removed": removed,
                "errors": errors,
                "dry_run": args.dry_run,
                "delete_branches": args.delete_branches
            }));
        } else if !args.dry_run {
            renderer.emit_success(format!(
                "Cleanup complete: {} worktree(s) removed",
                removed.len()
            ));
        }

        Ok(())
    }
}

/// List all agent worktrees
fn list_agent_worktrees(
    resolver: &TasksDirectoryResolver,
    config: &crate::config::types::ResolvedConfig,
) -> Result<Vec<WorktreeInfo>, String> {
    let repo_root = match crate::utils::git::find_repo_root(&resolver.path) {
        Some(root) => root,
        None => return Ok(Vec::new()),
    };

    let branch_prefix = &config.agent_worktree.branch_prefix;

    // Get list of worktrees from git
    let output = std::process::Command::new("git")
        .args(["worktree", "list", "--porcelain"])
        .current_dir(&repo_root)
        .output()
        .map_err(|e| format!("Failed to run git worktree list: {}", e))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!("git worktree list failed: {}", stderr.trim()));
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let worktrees = parse_git_worktree_list(&stdout, branch_prefix);

    // Enrich with task status information
    let storage = crate::storage::manager::Storage::new(resolver.path.clone());

    let mut results = Vec::new();
    for (path, branch) in worktrees {
        let ticket_id = extract_ticket_from_branch(&branch, branch_prefix);
        let is_done = check_ticket_done(&storage, &ticket_id);
        let has_active_job = AgentJobService::has_active_job(&ticket_id);

        results.push(WorktreeInfo {
            path,
            branch,
            ticket_id,
            is_done,
            has_active_job,
        });
    }

    Ok(results)
}

/// Parse git worktree list --porcelain output
fn parse_git_worktree_list(output: &str, branch_prefix: &str) -> Vec<(String, String)> {
    let mut results = Vec::new();
    let mut current_path: Option<String> = None;
    let mut current_branch: Option<String> = None;

    for line in output.lines() {
        if let Some(path) = line.strip_prefix("worktree ") {
            // Flush previous worktree if any
            if let (Some(path), Some(branch)) = (current_path.take(), current_branch.take())
                && branch.starts_with(branch_prefix)
            {
                results.push((path, branch));
            }
            current_path = Some(path.to_string());
        } else if let Some(branch) = line.strip_prefix("branch refs/heads/") {
            current_branch = Some(branch.to_string());
        } else if line.is_empty() {
            // End of worktree block - flush
            if let (Some(path), Some(branch)) = (current_path.take(), current_branch.take())
                && branch.starts_with(branch_prefix)
            {
                results.push((path, branch));
            }
        }
    }

    // Flush last entry
    if let (Some(path), Some(branch)) = (current_path, current_branch)
        && branch.starts_with(branch_prefix)
    {
        results.push((path, branch));
    }

    results
}

/// Extract ticket ID from branch name
fn extract_ticket_from_branch(branch: &str, prefix: &str) -> String {
    branch
        .strip_prefix(prefix)
        .unwrap_or(branch)
        .trim_start_matches('/')
        .to_string()
}

/// Check if a ticket is in a "done" state
fn check_ticket_done(storage: &crate::storage::manager::Storage, ticket_id: &str) -> bool {
    // Derive project prefix from ID (e.g., ABCD-1 -> ABCD)
    let derived = ticket_id.split('-').next().unwrap_or("");
    match storage.get(ticket_id, derived.to_string()) {
        Some(task) => {
            let status_lower = task.status.as_str().to_lowercase();
            status_lower == "done" || status_lower == "closed" || status_lower == "completed"
        }
        None => {
            // Task not found - consider it "done" for cleanup purposes
            true
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct RunningAgentJob {
    pub pid: u32,
    pub job_id: Option<String>,
    pub ticket_id: Option<String>,
    pub runner: Option<String>,
}

pub(crate) fn running_job_for_ticket(ticket_id: &str) -> Option<RunningAgentJob> {
    let needle = ticket_id.trim();
    if needle.is_empty() {
        return None;
    }
    list_running_jobs().into_iter().find(|job| {
        job.ticket_id
            .as_deref()
            .is_some_and(|value| value.eq_ignore_ascii_case(needle))
    })
}

fn list_running_jobs() -> Vec<RunningAgentJob> {
    let mut system = System::new_all();
    system.refresh_processes();

    system
        .processes()
        .iter()
        .filter_map(|(pid, process)| {
            if !is_wrapper_process(process) {
                return None;
            }
            if is_zombie_process(process) {
                return None;
            }
            let (job_id, ticket_id, runner) = parse_wrapper_metadata(process.cmd());
            Some(RunningAgentJob {
                pid: pid.as_u32(),
                job_id,
                ticket_id,
                runner,
            })
        })
        .collect()
}

fn wait_for_job_completion(
    job_id: &str,
    follow: bool,
    timeout_seconds: Option<u64>,
    renderer: &OutputRenderer,
) -> Result<(), String> {
    let start = Instant::now();
    let mut last_event_count = 0usize;

    loop {
        if follow {
            let events = AgentJobService::events_for(job_id);
            if events.len() > last_event_count {
                for event in events.iter().skip(last_event_count) {
                    if let Some(message) = event.message.as_ref() {
                        renderer.emit_info(message);
                    } else {
                        renderer.emit_info(format!("{} ({})", event.kind, event.at));
                    }
                }
                last_event_count = events.len();
            }
        }

        let Some(job) = AgentJobService::get_job(job_id) else {
            return Err(format!("Job '{}' not found", job_id));
        };

        if is_terminal_status(job.status.as_str()) {
            if job.status.eq_ignore_ascii_case("completed") {
                renderer.emit_success(format!("Job {} completed", job.id));
            } else {
                renderer.emit_error(format!("Job {} {}", job.id, job.status));
            }

            if let Some(summary) = job.summary.as_ref() {
                renderer.emit_info(summary);
            }

            return if job.status.eq_ignore_ascii_case("completed") {
                Ok(())
            } else {
                Err(format!("Job '{}' ended with status {}", job.id, job.status))
            };
        }

        if let Some(timeout) = timeout_seconds
            && start.elapsed() > Duration::from_secs(timeout)
        {
            return Err(format!("Timed out waiting for job '{}'", job_id));
        }

        thread::sleep(Duration::from_millis(500));
    }
}

fn is_terminal_status(status: &str) -> bool {
    status.eq_ignore_ascii_case("completed")
        || status.eq_ignore_ascii_case("failed")
        || status.eq_ignore_ascii_case("cancelled")
}

fn is_zombie_process(process: &sysinfo::Process) -> bool {
    // Avoid treating already-exited (zombie/defunct) processes as active jobs.
    // On macOS this often shows up as STAT=Z in ps output.
    matches!(process.status(), sysinfo::ProcessStatus::Zombie)
}

fn is_wrapper_process(process: &sysinfo::Process) -> bool {
    let name = process.name();
    if is_wrapper_name(name) {
        return true;
    }
    if let Some(exe_name) = process
        .exe()
        .and_then(|path| path.file_name())
        .and_then(|name| name.to_str())
        && is_wrapper_name(exe_name)
    {
        return true;
    }
    if let Some(cmd0) = process.cmd().first()
        && let Some(base) = std::path::Path::new(cmd0)
            .file_name()
            .and_then(|name| name.to_str())
        && is_wrapper_name(base)
    {
        return true;
    }
    false
}

fn is_wrapper_name(name: &str) -> bool {
    name == "lotar-agent-wrapper" || name == "lotar-agent-wrapper.exe"
}

fn parse_wrapper_metadata(cmd: &[String]) -> (Option<String>, Option<String>, Option<String>) {
    let mut job_id = None;
    let mut ticket_id = None;
    let mut runner = None;
    let mut idx = 0;
    while idx < cmd.len() {
        let arg = &cmd[idx];
        if arg == "--" {
            break;
        }
        match arg.as_str() {
            "--job-id" => {
                job_id = cmd.get(idx + 1).cloned();
                idx += 2;
            }
            "--ticket-id" => {
                ticket_id = cmd.get(idx + 1).cloned();
                idx += 2;
            }
            "--runner" => {
                runner = cmd.get(idx + 1).cloned();
                idx += 2;
            }
            _ => {
                idx += 1;
            }
        }
    }
    (job_id, ticket_id, runner)
}

#[cfg(test)]
mod tests {
    use super::{extract_ticket_from_branch, parse_git_worktree_list, parse_wrapper_metadata};

    #[test]
    fn parses_wrapper_metadata() {
        let cmd = vec![
            "lotar-agent-wrapper".to_string(),
            "--job-id".to_string(),
            "job-1".to_string(),
            "--ticket-id".to_string(),
            "TEST-1".to_string(),
            "--runner".to_string(),
            "codex".to_string(),
            "--".to_string(),
            "codex".to_string(),
            "exec".to_string(),
        ];
        let (job_id, ticket_id, runner) = parse_wrapper_metadata(&cmd);
        assert_eq!(job_id.as_deref(), Some("job-1"));
        assert_eq!(ticket_id.as_deref(), Some("TEST-1"));
        assert_eq!(runner.as_deref(), Some("codex"));
    }

    #[test]
    fn parses_git_worktree_list_output() {
        let output = r#"worktree /Users/test/repo
HEAD abc123
branch refs/heads/main

worktree /Users/test/.lotar-worktrees/repo/TEST-1
HEAD def456
branch refs/heads/agent/TEST-1

worktree /Users/test/.lotar-worktrees/repo/TEST-2
HEAD ghi789
branch refs/heads/agent/TEST-2

"#;
        let results = parse_git_worktree_list(output, "agent/");
        assert_eq!(results.len(), 2);
        assert_eq!(results[0].0, "/Users/test/.lotar-worktrees/repo/TEST-1");
        assert_eq!(results[0].1, "agent/TEST-1");
        assert_eq!(results[1].0, "/Users/test/.lotar-worktrees/repo/TEST-2");
        assert_eq!(results[1].1, "agent/TEST-2");
    }

    #[test]
    fn parses_git_worktree_list_with_custom_prefix() {
        let output = r#"worktree /Users/test/repo
HEAD abc123
branch refs/heads/main

worktree /Users/test/.lotar-worktrees/repo/TEST-1
HEAD def456
branch refs/heads/my-agent/TEST-1

"#;
        let results = parse_git_worktree_list(output, "my-agent/");
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].1, "my-agent/TEST-1");
    }

    #[test]
    fn extracts_ticket_from_branch() {
        assert_eq!(
            extract_ticket_from_branch("agent/TEST-1", "agent/"),
            "TEST-1"
        );
        assert_eq!(
            extract_ticket_from_branch("agent/PROJ-42", "agent/"),
            "PROJ-42"
        );
        assert_eq!(
            extract_ticket_from_branch("custom-prefix/ABC-1", "custom-prefix/"),
            "ABC-1"
        );
        // Edge case: no match falls back to full branch name
        assert_eq!(extract_ticket_from_branch("main", "agent/"), "main");
    }
}

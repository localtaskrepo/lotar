//! The task services' calls into automation and agent jobs
//! (`lotar_tasks::hooks`, DEV-634).

use crate::api_types::{JobOutcome, TaskDTO, TaskUpdate};
use crate::config::types::ResolvedConfig;
use crate::errors::LoTaRResult;
use crate::hooks::TaskHooks;
use crate::services::agent_active_jobs;
use crate::services::agent_job_service::{AgentJobService, cleanup_done_ticket_worktree};
use crate::services::agent_queue_service::AgentQueueService;
use crate::services::automation_service::{AutomationService, option_set_patch};
use crate::storage::manager::Storage;
use crate::types::{OptionSet, TaskComment};
use std::path::Path;

struct AgentHooks;

static AGENT_HOOKS: AgentHooks = AgentHooks;

/// Connect the task services to automation and agent jobs. Every program
/// that saves tasks calls this once before it does; later calls do nothing.
pub fn install_hooks() {
    crate::hooks::install(&AGENT_HOOKS);
}

impl TaskHooks for AgentHooks {
    fn task_saved(&self, storage: &mut Storage, previous: Option<&TaskDTO>, task: &TaskDTO, config: &ResolvedConfig) {
        let _ = AutomationService::apply_task_update(storage, previous, task, config);
    }

    fn comment_added(&self, storage: &mut Storage, task: &TaskDTO, comment: &TaskComment, config: &ResolvedConfig) {
        let _ = AutomationService::apply_comment_event(storage, task, comment, config);
    }

    fn dependents_unblocked(&self, storage: &mut Storage, previous: &TaskDTO, task: &TaskDTO) {
        AutomationService::fire_unblocked_dependents(storage, previous, task);
    }

    fn status_changed(&self, tasks_dir: &Path, ticket_id: &str) {
        cleanup_done_ticket_worktree(tasks_dir, ticket_id);
    }

    fn defer_job_outcome(&self, tasks_dir: &Path, outcome: &JobOutcome) -> std::io::Result<Option<JobOutcome>> {
        agent_active_jobs::record_outcome(tasks_dir, outcome)
    }

    fn running_job_id(&self, tasks_dir: &Path, ticket_id: &str) -> Option<String> {
        AgentJobService::active_job_id(ticket_id).or_else(|| agent_active_jobs::for_ticket_elsewhere(tasks_dir, ticket_id).map(|job| job.job_id))
    }

    fn agent_work_pending(&self, tasks_dir: &Path, ticket_id: &str) -> bool {
        AgentJobService::has_active_job(ticket_id)
            || AgentQueueService::list_pending(tasks_dir)
                .unwrap_or_default()
                .iter()
                .any(|entry| crate::storage::identity::aliases_match(&entry.ticket_id, ticket_id))
    }

    fn option_set_patch(&self, storage: &Storage, task: &TaskDTO, config: &ResolvedConfig, set: &OptionSet) -> LoTaRResult<TaskUpdate> {
        option_set_patch(storage, task, config, set)
    }
}

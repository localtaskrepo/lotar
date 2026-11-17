//! The task services' calls into automation and agent jobs (DEV-634).
//!
//! Those live in crates/lotar-agent, above this crate, which installs its
//! [`TaskHooks`] once at startup (`lotar_agent::install_hooks`, called by the
//! `lotar` binary and by the test setups). Until then every hook but one
//! does nothing: tasks save, but no automation runs and no job is
//! consulted. Answering a question whose option sets fields fails instead,
//! so the answer isn't recorded without its changes.

use crate::api_types::{JobOutcome, TaskDTO, TaskUpdate};
use crate::config::types::ResolvedConfig;
use crate::errors::{LoTaRError, LoTaRResult};
use crate::storage::manager::Storage;
use crate::types::{OptionSet, TaskComment};
use std::path::Path;
use std::sync::OnceLock;

pub trait TaskHooks: Sync {
    /// A task was created (`previous` is `None`) or updated: run automation.
    fn task_saved(&self, storage: &mut Storage, previous: Option<&TaskDTO>, task: &TaskDTO, config: &ResolvedConfig);
    /// A comment was added: fire `on.commented`.
    fn comment_added(&self, storage: &mut Storage, task: &TaskDTO, comment: &TaskComment, config: &ResolvedConfig);
    /// A task changed; when it closed, fire `on.unblocked` for its dependents.
    fn dependents_unblocked(&self, storage: &mut Storage, previous: &TaskDTO, task: &TaskDTO);
    /// The ticket's status changed: remove its job worktree once it is done.
    fn status_changed(&self, tasks_dir: &Path, ticket_id: &str);
    /// Hold a status or assignee change back for the running job; `None`
    /// when no job runs any more, so the caller applies it directly.
    fn defer_job_outcome(&self, tasks_dir: &Path, outcome: &JobOutcome) -> std::io::Result<Option<JobOutcome>>;
    /// The agent job running or queued for the ticket, in any process.
    fn running_job_id(&self, tasks_dir: &Path, ticket_id: &str) -> Option<String>;
    /// Whether a job of this process runs on the ticket or one waits for it
    /// in the queue (moving it would split the work).
    fn agent_work_pending(&self, tasks_dir: &Path, ticket_id: &str) -> bool;
    /// The update an answered option's `set:` makes, as an automation
    /// `set:` would (DEV-488).
    fn option_set_patch(&self, storage: &Storage, task: &TaskDTO, config: &ResolvedConfig, set: &OptionSet) -> LoTaRResult<TaskUpdate>;
}

struct NoHooks;

impl TaskHooks for NoHooks {
    fn task_saved(&self, _: &mut Storage, _: Option<&TaskDTO>, _: &TaskDTO, _: &ResolvedConfig) {}
    fn comment_added(&self, _: &mut Storage, _: &TaskDTO, _: &TaskComment, _: &ResolvedConfig) {}
    fn dependents_unblocked(&self, _: &mut Storage, _: &TaskDTO, _: &TaskDTO) {}
    fn status_changed(&self, _: &Path, _: &str) {}
    fn defer_job_outcome(&self, _: &Path, _: &JobOutcome) -> std::io::Result<Option<JobOutcome>> {
        Ok(None)
    }
    fn running_job_id(&self, _: &Path, _: &str) -> Option<String> {
        None
    }
    fn agent_work_pending(&self, _: &Path, _: &str) -> bool {
        false
    }
    fn option_set_patch(&self, _: &Storage, _: &TaskDTO, _: &ResolvedConfig, _: &OptionSet) -> LoTaRResult<TaskUpdate> {
        Err(LoTaRError::ValidationError("an answer that sets fields needs automation (lotar_agent::install_hooks)".to_string()))
    }
}

static HOOKS: OnceLock<&'static dyn TaskHooks> = OnceLock::new();

/// Install the hooks; later calls keep the first ones.
pub fn install(hooks: &'static dyn TaskHooks) {
    let _ = HOOKS.set(hooks);
}

pub fn installed() -> bool {
    HOOKS.get().is_some()
}

pub(crate) fn get() -> &'static dyn TaskHooks {
    HOOKS.get().copied().unwrap_or(&NoHooks)
}

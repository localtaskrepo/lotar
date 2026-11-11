//! Report a problem once (DEV-317).
//!
//! A ticket created with a finding key stores it in the `finding` custom
//! field. Reporting the same key again is decided by the newest ticket of
//! the project with that key: an open ticket gets a comment, a won't-do
//! ticket suppresses the report, a done ticket gets a new ticket related to
//! it. The check and the write run under the project's finding lock, so
//! two reporters of one key at the same moment end up with one ticket.

use std::path::Path;
use std::time::Duration;

pub use crate::api_types::{FindingReport, FindingState};
use crate::api_types::{TaskCreate, TaskDTO};
use crate::config::types::ResolvedConfig;
use crate::errors::{LoTaRError, LoTaRResult};
use crate::services::automation_service::AutomationService;
use crate::services::task_service::{CommentContext, CreateOptions, TaskService};
use crate::storage::layout::ArchiveFilter;
use crate::storage::manager::Storage;
use crate::storage::{TaskFilter, TaskId};

/// The custom field that holds a ticket's finding key.
pub const FINDING_FIELD: &str = "finding";

/// The custom field with the date a finding was last reported without news.
pub const FINDING_SEEN_FIELD: &str = "finding_seen";

/// How a repeated report's comment starts.
const REPORT_PREFIX: &str = "Reported again by ";

/// Status names that mean "decided not to do" (compared without case,
/// spaces, `-`, `_` and `'`).
const WONT_DO_NAMES: &[&str] = &["wontdo", "wontfix", "cancelled", "canceled", "rejected", "declined"];

/// A busy finding lock is waited for this long: the holder only checks the
/// project and writes one ticket or comment.
const LOCK_WAIT: Duration = Duration::from_secs(30);

impl FindingReport {
    /// One line naming the ticket, for text output.
    pub fn describe(&self, key: &str) -> String {
        match self.state {
            FindingState::New => format!("Created {} for finding {key}", self.id),
            FindingState::Recurred => format!("Created {} for finding {key}; it was fixed before and came back", self.id),
            FindingState::Open if self.commented => format!("{} already reports finding {key}; added this report as a comment", self.id),
            FindingState::Open => format!("{} already reports finding {key}; seen again with nothing new, no comment added", self.id),
            FindingState::Suppressed => format!("{} reported finding {key} and was closed as won't do; nothing created", self.id),
        }
    }
}

/// The newest ticket with a finding key, and what its status means.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Prior {
    None,
    Open(String),
    Suppressed(String),
    Done(String),
}

pub struct FindingService;

impl FindingService {
    /// A finding key is one line of at most 200 characters.
    pub fn validate_key(raw: &str) -> LoTaRResult<String> {
        let key = raw.trim();
        if key.is_empty() {
            return Err(LoTaRError::ValidationError("The finding key is empty".to_string()));
        }
        if key.chars().count() > 200 || key.chars().any(char::is_control) {
            return Err(LoTaRError::ValidationError(format!("Invalid finding key '{key}': use one line of at most 200 characters")));
        }
        Ok(key.to_string())
    }

    /// Hold the project's finding lock while checking and writing.
    pub fn lock(tasks_root: &Path, project: &str) -> LoTaRResult<std::fs::File> {
        crate::storage::safety::validate_project_prefix(project).map_err(LoTaRError::ValidationError)?;
        let dir = tasks_root.join(project);
        std::fs::create_dir_all(&dir)?;
        Ok(crate::storage::safety::acquire_storage_lock_within(&dir, "finding", LOCK_WAIT)?)
    }

    /// The newest ticket of `project` with this key (archived ones count,
    /// deleted ones don't).
    pub fn prior(storage: &Storage, project: &str, key: &str, config: &ResolvedConfig) -> Prior {
        let filter = TaskFilter { project: Some(project.to_string()), archived: ArchiveFilter::All, ..Default::default() };
        let newest = storage
            .search(&filter)
            .into_iter()
            .filter(|(_, task)| {
                crate::utils::custom_fields::extract_value_strings(&task.custom_fields, FINDING_FIELD)
                    .is_some_and(|values| values.iter().any(|value| value.trim() == key))
            })
            .max_by_key(|(id, _)| TaskId::parse(id).map(|id| id.number).unwrap_or(0));
        let Some((id, task)) = newest else {
            return Prior::None;
        };
        if is_wont_do(task.status.as_str()) {
            return Prior::Suppressed(id);
        }
        let done = crate::services::completion::effective_done_statuses(config);
        if done.contains(&task.status.as_str().to_ascii_lowercase()) { Prior::Done(id) } else { Prior::Open(id) }
    }

    /// Record a repeated report on the open ticket. A report with news (a
    /// description that differs from the ticket's and from the last report
    /// comment's) becomes a comment; one without only updates the
    /// `finding_seen` date (DEV-321), so a daily check adds no daily comment.
    /// Returns the ticket and whether a comment was added.
    pub fn record_repeat(storage: &mut Storage, id: &str, reporter: Option<&str>, description: Option<&str>) -> LoTaRResult<(TaskDTO, bool)> {
        let description = description.map(str::trim).filter(|d| !d.is_empty());
        let task = TaskService::get(storage, id, None)?;
        let Some(description) = description.filter(|text| !already_reported(&task, text)) else {
            return Ok((Self::mark_seen(storage, &task.id)?, false));
        };
        let reporter = reporter
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_string)
            .or_else(|| crate::utils::identity::resolve_current_user(Some(storage.root_path.as_path())))
            .unwrap_or_else(|| "unknown".to_string());
        let date = chrono::Local::now().format("%Y-%m-%d");
        let text = format!("{REPORT_PREFIX}{reporter} on {date}.\n\n{description}");
        let task =
            TaskService::add_comment_with_context(storage, &task.id, &text, CommentContext { actor_override: Some(reporter), ..CommentContext::default() })?;
        Ok((task, true))
    }

    /// Set `finding_seen` to today. Bookkeeping, not an edit: no history
    /// entry, `modified` stays, no automation.
    fn mark_seen(storage: &mut Storage, id: &str) -> LoTaRResult<TaskDTO> {
        let today = chrono::Local::now().format("%Y-%m-%d").to_string();
        let value = crate::types::custom_value_string(today);
        storage.mutate_task(id, |task| {
            if task.custom_fields.get(FINDING_SEEN_FIELD) == Some(&value) {
                return Ok(false);
            }
            task.custom_fields.insert(FINDING_SEEN_FIELD.to_string(), value);
            Ok(true)
        })?;
        TaskService::get(storage, id, None)
    }

    /// Report a finding through the shared create path (REST and MCP).
    /// Returns the outcome and the ticket it names.
    pub fn report(storage: &mut Storage, mut req: TaskCreate, key: &str) -> LoTaRResult<(FindingReport, TaskDTO)> {
        let key = Self::validate_key(key)?;
        let project = TaskService::resolve_create_project(&storage.root_path, req.project.clone());
        req.project = Some(project.clone());
        let config = TaskService::resolve_config_for_mutation(&storage.root_path, &project)?;

        let lock = Self::lock(&storage.root_path, &project)?;
        let (outcome, task) = match Self::prior(storage, &project, &key, &config) {
            Prior::Open(id) => {
                let (task, commented) = Self::record_repeat(storage, &id, req.reporter.as_deref(), req.description.as_deref())?;
                let outcome = FindingReport { created: false, state: FindingState::Open, id, commented };
                return Ok((outcome, task));
            }
            Prior::Suppressed(id) => {
                drop(lock);
                let task = TaskService::get(storage, &id, None)?;
                let outcome = FindingReport { created: false, state: FindingState::Suppressed, id, commented: false };
                return Ok((outcome, task));
            }
            Prior::Done(old) => {
                let relationships = req.relationships.get_or_insert_with(Default::default);
                if !relationships.related.iter().any(|id| id == &old) {
                    relationships.related.push(old);
                }
                let task = Self::create_locked(storage, req, &key)?;
                (FindingReport { created: true, state: FindingState::Recurred, id: task.id.clone(), commented: false }, task)
            }
            Prior::None => {
                let task = Self::create_locked(storage, req, &key)?;
                (FindingReport { created: true, state: FindingState::New, id: task.id.clone(), commented: false }, task)
            }
        };
        drop(lock);

        // `created` automation runs once the ticket is visible to other
        // reporters, so a slow rule never holds the lock.
        let _ = AutomationService::apply_task_update(storage, None, &task, &config);
        let task = TaskService::get(storage, &task.id, None).unwrap_or(task);
        Ok((outcome, task))
    }

    fn create_locked(storage: &mut Storage, req: TaskCreate, key: &str) -> LoTaRResult<TaskDTO> {
        TaskService::create_with(storage, req, CreateOptions { finding: Some(key.to_string()), skip_automation: true })
    }
}

/// Whether `text` is the ticket's description or the last report comment's.
fn already_reported(task: &TaskDTO, text: &str) -> bool {
    if task.description.as_deref().map(str::trim) == Some(text) {
        return true;
    }
    let last_report = task
        .comments
        .iter()
        .rev()
        .find_map(|comment| comment.text.strip_prefix(REPORT_PREFIX).map(|rest| rest.split_once("\n\n").map_or("", |(_, body)| body).trim()));
    last_report == Some(text)
}

fn is_wont_do(status: &str) -> bool {
    let normalized: String = status.chars().filter(|ch| ch.is_alphanumeric()).flat_map(char::to_lowercase).collect();
    WONT_DO_NAMES.contains(&normalized.as_str())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wont_do_states_are_recognised_by_name() {
        for status in ["WontDo", "Won't do", "wont-fix", "Cancelled", "Canceled", "Rejected"] {
            assert!(is_wont_do(status), "{status}");
        }
        for status in ["Done", "Todo", "Blocked", "Won"] {
            assert!(!is_wont_do(status), "{status}");
        }
    }

    #[test]
    fn finding_keys_are_one_short_line() {
        assert_eq!(FindingService::validate_key("  worktrees:lotar  ").unwrap_or_default(), "worktrees:lotar");
        assert!(FindingService::validate_key(" ").is_err());
        assert!(FindingService::validate_key("a\nb").is_err());
        assert!(FindingService::validate_key(&"x".repeat(201)).is_err());
    }
}

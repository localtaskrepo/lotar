//! Report a problem once (DEV-317).
//!
//! A ticket created with a finding key stores it in the `finding` custom
//! field. Reporting the same key again is decided by the newest ticket of
//! the project with that key: an open ticket gets a comment, a won't-do
//! ticket suppresses the report, a done ticket gets a new ticket related to
//! it. The check and the write run under the project's finding lock, so
//! two reporters of one key at the same moment end up with one ticket.
//!
//! A report with a bundle name and no ticket of its own joins the project's
//! open bundle ticket of the day (DEV-800): its key goes into the bundle's
//! `finding` list and its title and description become a section of the
//! bundle's description. A bundle an agent job works on takes no new
//! findings, so the report starts the next bundle instead.

use std::path::Path;
use std::time::Duration;

pub use crate::api_types::{FindingReport, FindingState};
use crate::api_types::{TaskCreate, TaskDTO, TaskUpdate};
use crate::config::types::ResolvedConfig;
use crate::errors::{LoTaRError, LoTaRResult};
use crate::services::automation_service::AutomationService;
use crate::services::task_service::{CommentContext, CreateOptions, TaskService};
use crate::storage::layout::ArchiveFilter;
use crate::storage::manager::Storage;
use crate::storage::{TaskFilter, TaskId};
use crate::utils::custom_fields::{FINDING_BUNDLE_FIELD, FINDING_FIELD, FINDING_SEEN_FIELD};

/// How a repeated report's comment starts.
const REPORT_PREFIX: &str = "Reported again by ";

/// Status names that mean "decided not to do" (compared without case,
/// spaces, `-`, `_` and `'`).
const WONT_DO_NAMES: &[&str] = &["wontdo", "wontfix", "cancelled", "canceled", "rejected", "declined"];

/// Status names of a ticket an agent job works on, compared like
/// [`WONT_DO_NAMES`]: a bundle in one of them takes no new findings.
const IN_JOB_NAMES: &[&str] = &["inprogress", "reviewing", "merging"];

/// Titles that share at least this percentage of their words (Dice
/// coefficient) name the same problem (DEV-562).
const SIMILAR_TITLE_PERCENT: usize = 60;

/// Words that say nothing about the problem.
const STOP_WORDS: &[&str] =
    &["a", "an", "and", "are", "as", "at", "be", "by", "for", "from", "has", "have", "in", "is", "it", "its", "of", "on", "or", "the", "to", "when", "with"];

/// A busy finding lock is waited for this long: the holder only checks the
/// project and writes one ticket or comment.
const LOCK_WAIT: Duration = Duration::from_secs(30);

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

    /// A bundle name is one word of at most 50 letters, digits, `-` or `_`.
    pub fn validate_bundle(raw: &str) -> LoTaRResult<String> {
        let name = raw.trim();
        if name.is_empty() || name.chars().count() > 50 || !name.chars().all(|ch| ch.is_alphanumeric() || ch == '-' || ch == '_') {
            return Err(LoTaRError::ValidationError(format!("Invalid bundle name '{name}': use one word of letters, digits, '-' or '_', at most 50")));
        }
        Ok(name.to_string())
    }

    /// The `finding_bundle` value of today's bundle: `<name>:<YYYY-MM-DD>`.
    pub fn bundle_of_today(name: &str) -> String {
        format!("{name}:{}", jiff::ZonedDateTime::now().strftime("%Y-%m-%d"))
    }

    /// The newest ticket of `project` in bundle `bundle` (a
    /// [`bundle_of_today`](Self::bundle_of_today) value) that takes new
    /// findings: not done, not won't do, not archived and not in an agent
    /// job (InProgress, Reviewing, Merging).
    pub fn open_bundle(storage: &Storage, project: &str, bundle: &str, config: &ResolvedConfig) -> Option<String> {
        let done = crate::services::completion::effective_done_statuses(config);
        let filter = TaskFilter { project: Some(project.to_string()), ..Default::default() };
        storage
            .search(&filter)
            .into_iter()
            .filter(|(_, task)| {
                crate::utils::custom_fields::extract_value_strings(&task.custom_fields, FINDING_BUNDLE_FIELD)
                    .is_some_and(|values| values.iter().any(|value| value.trim() == bundle))
            })
            .filter(|(_, task)| {
                let status = task.status.as_str();
                !is_wont_do(status) && !is_in_job(status) && !done.contains(&status.to_ascii_lowercase())
            })
            .map(|(id, _)| id)
            .max_by_key(|id| TaskId::parse(id).map(|id| id.number).unwrap_or(0))
    }

    /// The title and description a new bundle ticket starts with.
    pub fn new_bundle(bundle: &str, key: &str, title: &str, description: Option<&str>) -> (String, String) {
        let (name, date) = bundle.split_once(':').unwrap_or((bundle, ""));
        let intro = format!("Findings filed with `--bundle {name}` on {date}, one section each. Fix each finding the way its section says.");
        (format!("{} {date}: {title}", capitalized(name)), format!("{intro}\n\n{}", bundle_section(key, title, description)))
    }

    /// Add a finding to bundle ticket `id`: its key to the `finding` list, a
    /// section to the description, `related` (a done ticket of the key) to
    /// the relationships, and the count to the title.
    pub fn join_bundle(storage: &mut Storage, id: &str, key: &str, title: &str, description: Option<&str>, related: Option<&str>) -> LoTaRResult<TaskDTO> {
        let task = TaskService::get(storage, id, None)?;
        let mut keys = crate::utils::custom_fields::extract_value_strings(&task.custom_fields, FINDING_FIELD).unwrap_or_default();
        keys.push(key.to_string());
        let mut fields = task.custom_fields.clone();
        let list = serde_json::Value::Array(keys.iter().map(|key| serde_json::Value::String(key.clone())).collect());
        fields.insert(FINDING_FIELD.to_string(), crate::types::custom_value_from_json(&list));
        let mut relationships = task.relationships.clone();
        if let Some(old) = related
            && !relationships.related.iter().any(|id| id == old)
        {
            relationships.related.push(old.to_string());
        }
        let bundle = crate::utils::custom_fields::extract_value_strings(&task.custom_fields, FINDING_BUNDLE_FIELD).and_then(|values| values.into_iter().next());
        let new_title = bundle.as_deref().and_then(|bundle| counted_title(&task.title, bundle, keys.len()));
        let body = task.description.as_deref().map(str::trim_end).unwrap_or_default();
        let section = bundle_section(key, title, description);
        let patch = TaskUpdate {
            title: new_title,
            description: Some(if body.is_empty() { section } else { format!("{body}\n\n{section}") }),
            custom_fields: Some(fields),
            relationships: Some(relationships),
            ..TaskUpdate::default()
        };
        TaskService::update(storage, id, patch)
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

    /// The open tickets of `project` whose title shares most of its words
    /// with `title` (DEV-562), as `(id, title)`, lowest ID first. Done and
    /// won't-do tickets don't count. With a finding key, a ticket whose
    /// finding has another check prefix doesn't count either.
    pub fn similar_open(storage: &Storage, project: &str, title: &str, finding: Option<&str>, config: &ResolvedConfig) -> Vec<(String, String)> {
        let words = title_words(title);
        if words.is_empty() {
            return Vec::new();
        }
        let same_check = |task: &crate::storage::task::Task| {
            let Some(check) = finding.map(check_prefix) else {
                return true;
            };
            crate::utils::custom_fields::extract_value_strings(&task.custom_fields, FINDING_FIELD)
                .is_none_or(|values| values.is_empty() || values.iter().any(|value| check_prefix(value.trim()) == check))
        };
        let done = crate::services::completion::effective_done_statuses(config);
        let filter = TaskFilter { project: Some(project.to_string()), ..Default::default() };
        let mut similar: Vec<(String, String)> = storage
            .search(&filter)
            .into_iter()
            .filter(|(_, task)| !is_wont_do(task.status.as_str()) && !done.contains(&task.status.as_str().to_ascii_lowercase()))
            .filter(|(_, task)| same_check(task) && similar_words(&words, &title_words(&task.title)))
            .map(|(id, task)| (id, task.title))
            .collect();
        similar.sort_by_key(|(id, _)| TaskId::parse(id).map(|id| id.number).unwrap_or(0));
        similar
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
        let date = jiff::ZonedDateTime::now().strftime("%Y-%m-%d");
        let text = format!("{REPORT_PREFIX}{reporter} on {date}.\n\n{description}");
        let task =
            TaskService::add_comment_with_context(storage, &task.id, &text, CommentContext { actor_override: Some(reporter), ..CommentContext::default() })?;
        Ok((task, true))
    }

    /// Set `finding_seen` to today. Bookkeeping, not an edit: no history
    /// entry, `modified` stays, no automation.
    fn mark_seen(storage: &mut Storage, id: &str) -> LoTaRResult<TaskDTO> {
        let today = jiff::ZonedDateTime::now().strftime("%Y-%m-%d").to_string();
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
        .filter(|comment| !comment.is_deleted())
        .find_map(|comment| comment.text.strip_prefix(REPORT_PREFIX).map(|rest| rest.split_once("\n\n").map_or("", |(_, body)| body).trim()));
    last_report == Some(text)
}

/// The distinct lowercase words of a title, without stop words.
fn title_words(title: &str) -> std::collections::BTreeSet<String> {
    title.split(|ch: char| !ch.is_alphanumeric()).map(str::to_lowercase).filter(|word| !word.is_empty() && !STOP_WORDS.contains(&word.as_str())).collect()
}

/// The check of a `<check>:<subject>` finding key (the whole key without a `:`).
fn check_prefix(key: &str) -> &str {
    key.split_once(':').map_or(key, |(check, _)| check)
}

fn similar_words(a: &std::collections::BTreeSet<String>, b: &std::collections::BTreeSet<String>) -> bool {
    !a.is_empty() && !b.is_empty() && 2 * a.intersection(b).count() * 100 >= SIMILAR_TITLE_PERCENT * (a.len() + b.len())
}

fn normalized_status(status: &str) -> String {
    status.chars().filter(|ch| ch.is_alphanumeric()).flat_map(char::to_lowercase).collect()
}

fn is_wont_do(status: &str) -> bool {
    WONT_DO_NAMES.contains(&normalized_status(status).as_str())
}

fn is_in_job(status: &str) -> bool {
    IN_JOB_NAMES.contains(&normalized_status(status).as_str())
}

/// One finding of a bundle: a heading, the description and the key.
fn bundle_section(key: &str, title: &str, description: Option<&str>) -> String {
    let description = description.map(str::trim).filter(|text| !text.is_empty());
    match description {
        Some(text) => format!("## {title}\n\n{text}\n\nFinding: `{key}`"),
        None => format!("## {title}\n\nFinding: `{key}`"),
    }
}

fn capitalized(name: &str) -> String {
    let mut chars = name.chars();
    chars.next().map(|first| first.to_uppercase().chain(chars).collect()).unwrap_or_default()
}

/// A bundle's title for `count` findings: `<Name> <date>: <first> and <n>
/// more`. `None` when the title no longer starts with `<Name> <date>: `.
fn counted_title(title: &str, bundle: &str, count: usize) -> Option<String> {
    let (name, date) = bundle.split_once(':')?;
    let prefix = format!("{} {date}: ", capitalized(name));
    let rest = title.strip_prefix(&prefix)?;
    let first = match rest.rsplit_once(" and ") {
        Some((first, more)) if more.strip_suffix(" more").is_some_and(|n| !n.is_empty() && n.chars().all(|ch| ch.is_ascii_digit())) => first,
        _ => rest,
    };
    Some(if count > 1 { format!("{prefix}{first} and {} more", count - 1) } else { format!("{prefix}{first}") })
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
    fn titles_with_mostly_the_same_words_are_similar() {
        let similar = |a: &str, b: &str| similar_words(&title_words(a), &title_words(b));
        let locked = "opgate restart while the Mac is locked drops the signing key";
        assert!(similar(locked, "opgate restart on a locked Mac drops the signing key"));
        assert!(similar(locked, "Opgate restart while the Mac is locked drops the signing key"));
        assert!(!similar(locked, "opgate fails to start after an update"));
        assert!(!similar("the a", "the a"));
    }

    #[test]
    fn in_job_states_are_recognised_by_name() {
        for status in ["InProgress", "In progress", "Reviewing", "Merging"] {
            assert!(is_in_job(status), "{status}");
        }
        for status in ["Todo", "HelpNeeded", "Done"] {
            assert!(!is_in_job(status), "{status}");
        }
    }

    #[test]
    fn bundle_titles_count_their_findings() {
        let bundle = "health:2026-11-24";
        let (title, description) = FindingService::new_bundle(bundle, "a:x", "Stale worktree", Some("2 left"));
        assert_eq!(title, "Health 2026-11-24: Stale worktree");
        assert!(description.ends_with("## Stale worktree\n\n2 left\n\nFinding: `a:x`"), "{description}");
        assert_eq!(counted_title(&title, bundle, 2).as_deref(), Some("Health 2026-11-24: Stale worktree and 1 more"));
        assert_eq!(counted_title("Health 2026-11-24: Stale worktree and 1 more", bundle, 3).as_deref(), Some("Health 2026-11-24: Stale worktree and 2 more"));
        assert_eq!(counted_title("Health 2026-11-24: Cats and dogs", bundle, 2).as_deref(), Some("Health 2026-11-24: Cats and dogs and 1 more"));
        assert_eq!(counted_title("Renamed by hand", bundle, 2), None);
    }

    #[test]
    fn bundle_names_are_one_word() {
        assert_eq!(FindingService::validate_bundle(" health ").unwrap_or_default(), "health");
        assert!(FindingService::validate_bundle("health:2026").is_err());
        assert!(FindingService::validate_bundle("two words").is_err());
        assert!(FindingService::validate_bundle("").is_err());
    }

    #[test]
    fn finding_keys_are_one_short_line() {
        assert_eq!(FindingService::validate_key("  worktrees:lotar  ").unwrap_or_default(), "worktrees:lotar");
        assert!(FindingService::validate_key(" ").is_err());
        assert!(FindingService::validate_key("a\nb").is_err());
        assert!(FindingService::validate_key(&"x".repeat(201)).is_err());
    }
}

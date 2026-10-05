//! Shared task completion policy (DEV-21).
//!
//! One project-aware source of truth for "is this task done": an explicit
//! `issue.done_states` list is authoritative, and only when every
//! configuration layer leaves it unset does the legacy inference run —
//! after project resolution — from the effective `issue_states`
//! (last state, conventionally-named terminal states, and the `done`
//! branch alias, with the conventional `done`/`completed`/`closed`
//! fallback). Timing helpers reconstruct the current unbroken terminal
//! interval and per-cut completion from the task's JSON history so
//! reopened and terminal-to-terminal transitions are handled correctly.

use std::collections::HashMap;
use std::collections::HashSet;

use chrono::{DateTime, Utc};

use crate::api_types::{TaskDueBucketDTO, TaskStateDTO};
use crate::config::types::ResolvedConfig;
use crate::storage::manager::Storage;
use crate::storage::task::Task as StoredTask;
use crate::types::TaskStatus;

/// Which policy produced the effective done set.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DoneStatesMode {
    Explicit,
    Inferred,
}

pub fn done_states_mode(config: &ResolvedConfig) -> DoneStatesMode {
    if config.issue_done_states.is_some() {
        DoneStatesMode::Explicit
    } else {
        DoneStatesMode::Inferred
    }
}

impl DoneStatesMode {
    pub fn as_str(self) -> &'static str {
        match self {
            DoneStatesMode::Explicit => "explicit",
            DoneStatesMode::Inferred => "inferred",
        }
    }
}

/// Effective terminal statuses (lowercase) for a resolved configuration:
/// the explicit `issue.done_states` when any layer set it, otherwise the
/// legacy inference over the resolved `issue_states`.
pub fn effective_done_statuses(config: &ResolvedConfig) -> HashSet<String> {
    if let Some(explicit) = &config.issue_done_states {
        return explicit
            .values
            .iter()
            .map(|status| status.as_str().to_ascii_lowercase())
            .collect();
    }
    infer_done_statuses(config)
}

/// Legacy done-state inference, deliberately preserved (canonical helper):
/// the last configured state, every conventionally-named terminal state,
/// the `done` branch alias target, and the conventional fallback when
/// nothing matched.
pub fn infer_done_statuses(config: &ResolvedConfig) -> HashSet<String> {
    let mut done = HashSet::new();

    if let Some(last) = config.issue_states.values.last() {
        done.insert(last.as_str().to_ascii_lowercase());
    }

    for status in &config.issue_states.values {
        if status.eq_ignore_case("done")
            || status.eq_ignore_case("completed")
            || status.eq_ignore_case("closed")
        {
            done.insert(status.as_str().to_ascii_lowercase());
        }
    }

    for (alias, status) in &config.branch_status_aliases {
        if alias.eq_ignore_ascii_case("done") {
            done.insert(status.as_str().to_ascii_lowercase());
        }
    }

    if done.is_empty() {
        done.insert("done".to_string());
        done.insert("completed".to_string());
        done.insert("closed".to_string());
    }

    done
}

/// Ordered effective done states for display surfaces (`config show` and
/// `config inspect`): explicit lists keep their configured order; inferred
/// lists follow the inference steps deterministically.
pub fn effective_done_status_values(config: &ResolvedConfig) -> Vec<String> {
    if let Some(explicit) = &config.issue_done_states {
        return explicit
            .values
            .iter()
            .map(|status| status.as_str().to_string())
            .collect();
    }

    let mut values: Vec<String> = Vec::new();
    let mut push = |value: String| {
        if !values
            .iter()
            .any(|existing| existing.eq_ignore_ascii_case(&value))
        {
            values.push(value);
        }
    };

    if let Some(last) = config.issue_states.values.last() {
        push(last.as_str().to_string());
    }
    for status in &config.issue_states.values {
        if status.eq_ignore_case("done")
            || status.eq_ignore_case("completed")
            || status.eq_ignore_case("closed")
        {
            push(status.as_str().to_string());
        }
    }
    for (alias, status) in &config.branch_status_aliases {
        if alias.eq_ignore_ascii_case("done") {
            push(status.as_str().to_string());
        }
    }
    if values.is_empty() {
        values.extend(
            ["done", "completed", "closed"]
                .iter()
                .map(|value| value.to_string()),
        );
    }
    values
}

/// Server-local calendar day (`YYYY-MM-DD`) for a point in time. Derived
/// from the injected clock, never from a cached resolved snapshot.
pub fn local_calendar_day(now: DateTime<Utc>) -> String {
    crate::services::task_query::today_local(now)
        .format("%Y-%m-%d")
        .to_string()
}

/// Compute the runtime task-state projection: `is_done` from the task's own
/// project policy, the due bucket from the shared due parser and injected
/// clock, and the server-local calendar day at build time. Terminal past-due
/// tasks have no bucket (only the Overdue bucket excludes terminal tasks);
/// tasks due today stay `today` even when done.
pub fn compute_task_state(
    status: &TaskStatus,
    due_raw: Option<&str>,
    config: &ResolvedConfig,
    now: DateTime<Utc>,
) -> TaskStateDTO {
    let done = effective_done_statuses(config);
    let is_done = done.contains(&status.as_str().to_ascii_lowercase());
    let today = crate::services::task_query::today_local(now);
    let due_bucket = due_raw
        .and_then(crate::services::task_query::parse_stored_due)
        .and_then(|due| {
            let due = due.local_date();
            if due < today {
                if is_done {
                    None
                } else {
                    Some(TaskDueBucketDTO::Overdue)
                }
            } else if due == today {
                Some(TaskDueBucketDTO::Today)
            } else if due <= today + chrono::Duration::days(7) {
                Some(TaskDueBucketDTO::Soon)
            } else {
                Some(TaskDueBucketDTO::Later)
            }
        });
    TaskStateDTO {
        is_done,
        done_states: effective_done_status_values(config),
        due_bucket,
        calendar_day: today.format("%Y-%m-%d").to_string(),
    }
}

/// Per-task done sets for mixed-project collections: each task resolves
/// against its own project's policy (actual root when the task lives in a
/// nested/sibling workspace root), cached per (root, prefix) per call.
#[derive(Debug, Default, Clone)]
pub struct TaskDoneSets {
    by_task: HashMap<String, HashSet<String>>,
}

impl TaskDoneSets {
    pub fn is_done(&self, task_id: &str, status: &TaskStatus) -> bool {
        self.by_task
            .get(task_id)
            .is_some_and(|done| done.contains(&status.as_str().to_ascii_lowercase()))
    }

    pub fn done_set(&self, task_id: &str) -> Option<&HashSet<String>> {
        self.by_task.get(task_id)
    }

    pub fn len(&self) -> usize {
        self.by_task.len()
    }

    pub fn is_empty(&self) -> bool {
        self.by_task.is_empty()
    }
}

/// Build [`TaskDoneSets`] for the given task ids using the storage's
/// actual-root resolution (DEV-56 nested/sibling workspaces included).
///
/// The actual root is resolved PER TASK ID first, and the config cache is
/// keyed by `(root, prefix)`: two different IDs sharing one prefix across
/// sibling roots are individually unambiguous but may carry different
/// completion policies, so a prefix-only cache would wrongly reuse the
/// first task's policy for the rest (review F1).
pub fn resolve_task_done_sets<I, S>(storage: &Storage, task_ids: I) -> TaskDoneSets
where
    I: IntoIterator<Item = S>,
    S: AsRef<str>,
{
    let mut by_root_prefix: HashMap<(std::path::PathBuf, String), HashSet<String>> = HashMap::new();
    let mut by_task: HashMap<String, HashSet<String>> = HashMap::new();
    for task_id in task_ids {
        let task_id = task_id.as_ref();
        let prefix = crate::storage::TaskId::parse(task_id)
            .map(|parsed| parsed.project)
            .unwrap_or_default();
        // Per-ID actual root FIRST; only the (root, prefix) pair is cached.
        let root = storage
            .resolve_task_location(task_id)
            .map(|location| location.root)
            .unwrap_or_else(|_| storage.root_path.clone());
        let done = if let Some(done) = by_root_prefix.get(&(root.clone(), prefix.clone())) {
            done.clone()
        } else {
            let done = crate::config::resolution::config_for_project(root.as_path(), Some(&prefix))
                .map(|config| effective_done_statuses(&config))
                .unwrap_or_default();
            by_root_prefix.insert((root, prefix), done.clone());
            done
        };
        by_task.insert(task_id.to_string(), done);
    }
    TaskDoneSets { by_task }
}

fn parse_history_timestamp(raw: &str) -> Option<DateTime<Utc>> {
    DateTime::parse_from_rfc3339(raw.trim())
        .ok()
        .map(|parsed| parsed.with_timezone(&Utc))
}

/// Legacy best-effort completion estimate for tasks without usable status
/// history: the last modification (creation as the floor). This mirrors the
/// pre-DEV-21 behavior and is a conscious estimate, not a recorded fact.
fn legacy_completion_estimate(task: &StoredTask) -> Option<DateTime<Utc>> {
    let modified = task.modified.trim();
    if !modified.is_empty()
        && let Some(at) = parse_history_timestamp(modified)
    {
        return Some(at);
    }
    parse_history_timestamp(task.created.trim())
}

/// Start of the task's current unbroken terminal interval: the latest
/// non-terminal -> terminal transition that has not been followed by a
/// move out of the terminal set. Terminal -> terminal transitions do not
/// restart the interval. Tasks without usable status history fall back to
/// the legacy modified/created estimate. `None` when the task is not
/// currently terminal.
pub fn current_completion_started_at(
    task: &StoredTask,
    done: &HashSet<String>,
) -> Option<DateTime<Utc>> {
    if !done.contains(&task.status.as_str().to_ascii_lowercase()) {
        return None;
    }
    let mut started: Option<DateTime<Utc>> = None;
    for entry in &task.history {
        let Some(at) = parse_history_timestamp(&entry.at) else {
            continue;
        };
        for change in &entry.changes {
            if change.field != "status" {
                continue;
            }
            let Some((old, new)) = change.old.as_deref().zip(change.new.as_deref()) else {
                continue;
            };
            let old_done = done.contains(&old.to_ascii_lowercase());
            let new_done = done.contains(&new.to_ascii_lowercase());
            if new_done && !old_done {
                started = Some(at);
            } else if !new_done {
                started = None;
            }
        }
    }
    started.or_else(|| legacy_completion_estimate(task))
}

/// Whether the task was terminal strictly before the given cut instant,
/// reconstructed by replaying the status history. Reopened tasks count as
/// done before the reopen and open afterwards until they re-complete;
/// terminal-to-terminal transitions keep the task done throughout. Tasks
/// without status history use the legacy estimate.
pub fn task_done_at_cut(task: &StoredTask, done: &HashSet<String>, cut: DateTime<Utc>) -> bool {
    let mut replayed: Option<String> = None;
    let mut initial: Option<String> = None;
    for entry in &task.history {
        let Some(at) = parse_history_timestamp(&entry.at) else {
            continue;
        };
        for change in &entry.changes {
            if change.field != "status" {
                continue;
            }
            if initial.is_none() {
                initial = change.old.clone();
            }
            if let Some(new) = change.new.as_deref()
                && at < cut
            {
                replayed = Some(new.to_string());
            }
        }
    }
    match replayed {
        // The status held at the cut is the last recorded transition
        // before it.
        Some(status) => done.contains(&status.to_ascii_lowercase()),
        None => match initial {
            // History exists but starts at/after the cut: the
            // pre-history status is the first transition's `old` value.
            Some(initial) => done.contains(&initial.to_ascii_lowercase()),
            // No status history at all: legacy estimate.
            None => {
                done.contains(&task.status.as_str().to_ascii_lowercase())
                    && legacy_completion_estimate(task).is_some_and(|at| at < cut)
            }
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::types::ConfigurableField;
    use crate::types::TaskStatus;

    #[test]
    fn explicit_done_states_are_authoritative() {
        let config = ResolvedConfig::from_global(crate::config::types::GlobalConfig {
            issue_states: ConfigurableField {
                values: vec![
                    TaskStatus::from("Todo"),
                    TaskStatus::from("InProgress"),
                    TaskStatus::from("Done"),
                    TaskStatus::from("Archived"),
                ],
            },
            issue_done_states: Some(ConfigurableField {
                values: vec![TaskStatus::from("Done")],
            }),
            ..Default::default()
        });
        let effective = effective_done_statuses(&config);
        // Inference would mark the last state (Archived) done as well.
        assert!(!effective.contains("archived"));
        assert!(effective.contains("done"));
        assert_eq!(done_states_mode(&config), DoneStatesMode::Explicit);
        assert_eq!(
            effective_done_status_values(&config),
            vec!["Done".to_string()]
        );
    }

    #[test]
    fn inferred_mode_uses_legacy_helper_order() {
        let config = ResolvedConfig::from_global(crate::config::types::GlobalConfig::default());
        assert_eq!(done_states_mode(&config), DoneStatesMode::Inferred);
        // Default states end in Done: the ordered view dedupes
        // case-insensitively and never appends the conventional fallback
        // when an inferred match already exists.
        assert_eq!(
            effective_done_status_values(&config),
            vec!["Done".to_string()]
        );
    }

    #[test]
    fn terminal_past_due_has_no_bucket() {
        let config = ResolvedConfig::from_global(crate::config::types::GlobalConfig::default());
        let now = DateTime::parse_from_rfc3339("2026-10-03T12:00:00Z")
            .unwrap()
            .with_timezone(&Utc);
        let open_overdue =
            compute_task_state(&TaskStatus::from("Todo"), Some("2026-10-01"), &config, now);
        assert!(!open_overdue.is_done);
        // The embedded policy is the ordered effective list actually used
        // (default states end in Done: inferred = ["Done"]).
        assert_eq!(open_overdue.done_states, vec!["Done".to_string()]);
        assert_eq!(open_overdue.due_bucket, Some(TaskDueBucketDTO::Overdue));

        let done_overdue =
            compute_task_state(&TaskStatus::from("Done"), Some("2026-10-01"), &config, now);
        assert!(done_overdue.is_done);
        assert_eq!(done_overdue.due_bucket, None);

        let done_today = compute_task_state(
            &TaskStatus::from("Done"),
            Some("2026-10-03T08:00:00+00:00"),
            &config,
            now,
        );
        // Due today stays `today` even when done; only Overdue excludes
        // terminal tasks.
        assert_eq!(done_today.due_bucket, Some(TaskDueBucketDTO::Today));
        assert_eq!(done_today.calendar_day, local_calendar_day(now));
    }
}

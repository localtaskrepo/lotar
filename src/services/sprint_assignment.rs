use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};

use chrono::Utc;
use serde::Serialize;

use crate::services::sprint_service::{SprintRecord, SprintService};
use crate::services::sprint_status::{self, SprintLifecycleState};
use crate::services::task_service::TaskService;
use crate::storage::filter::TaskFilter;
use crate::storage::manager::Storage;
use crate::storage::sprint::{Sprint, SprintTaskEntry};
use crate::types::TaskStatus;
use crate::utils::identity::resolve_me_alias;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SprintAssignmentAction {
    Add,
    Remove,
}

impl SprintAssignmentAction {
    pub fn as_str(self) -> &'static str {
        match self {
            SprintAssignmentAction::Add => "add",
            SprintAssignmentAction::Remove => "remove",
        }
    }
}

#[derive(Debug, Clone)]
pub struct SprintAssignmentOutcome {
    pub action: SprintAssignmentAction,
    pub sprint_id: u32,
    pub sprint_label: Option<String>,
    pub sprint_display_name: String,
    pub modified: Vec<String>,
    pub unchanged: Vec<String>,
    pub replaced: Vec<SprintReassignmentInfo>,
}

#[derive(Debug, Clone)]
pub struct SprintReassignmentInfo {
    pub task_id: String,
    pub previous: Vec<u32>,
}

impl SprintReassignmentInfo {
    pub fn describe(&self) -> Option<String> {
        if self.previous.is_empty() {
            return None;
        }

        let mut previous = self.previous.clone();
        previous.sort_unstable();
        previous.dedup();
        let formatted = previous
            .into_iter()
            .map(|id| format!("#{}", id))
            .collect::<Vec<_>>()
            .join(", ");

        Some(format!(
            "{} moved from sprint(s) {}",
            self.task_id, formatted
        ))
    }
}

pub fn assign_tasks(
    storage: &mut Storage,
    records: &[SprintRecord],
    tasks: &[String],
    sprint_reference: Option<&str>,
    allow_closed: bool,
    force_single: bool,
) -> Result<SprintAssignmentOutcome, String> {
    if records.is_empty() {
        return Err("No sprints found. Create a sprint before assigning tasks.".to_string());
    }

    apply_assignment(
        storage,
        records,
        tasks,
        sprint_reference,
        allow_closed,
        force_single,
        SprintAssignmentAction::Add,
    )
}

pub fn remove_tasks(
    storage: &mut Storage,
    records: &[SprintRecord],
    tasks: &[String],
    sprint_reference: Option<&str>,
) -> Result<SprintAssignmentOutcome, String> {
    if records.is_empty() {
        return Err("No sprints found. Create a sprint before removing memberships.".to_string());
    }

    apply_assignment(
        storage,
        records,
        tasks,
        sprint_reference,
        true,
        false,
        SprintAssignmentAction::Remove,
    )
}

fn apply_assignment(
    storage: &mut Storage,
    records: &[SprintRecord],
    tasks: &[String],
    sprint_reference: Option<&str>,
    allow_closed: bool,
    force_single: bool,
    action: SprintAssignmentAction,
) -> Result<SprintAssignmentOutcome, String> {
    if tasks.is_empty() {
        let message = match action {
            SprintAssignmentAction::Add => {
                "Provide at least one task identifier to assign.".to_string()
            }
            SprintAssignmentAction::Remove => {
                "Provide at least one task identifier to update.".to_string()
            }
        };
        return Err(message);
    }

    let sprint_id = resolve_sprint_id(records, sprint_reference)?;
    let target_record = records
        .iter()
        .find(|record| record.id == sprint_id)
        .ok_or_else(|| format!("Sprint #{} not found.", sprint_id))?;

    if action == SprintAssignmentAction::Add && !allow_closed {
        ensure_sprint_is_open(target_record)?;
    }

    let mut sprint_map: BTreeMap<u32, Sprint> = records
        .iter()
        .map(|record| (record.id, record.sprint.clone()))
        .collect();

    let mut changed_sprints: HashSet<u32> = HashSet::new();
    let mut membership = build_membership_index(&sprint_map);
    let mut modified: Vec<String> = Vec::new();
    let mut unchanged: Vec<String> = Vec::new();
    let mut replaced: Vec<SprintReassignmentInfo> = Vec::new();

    let mut seen_tasks: HashSet<String> = HashSet::new();

    for raw in tasks {
        let task_id = resolve_task_identifier(storage, raw)?;
        if !seen_tasks.insert(task_id.clone()) {
            continue;
        }

        match action {
            SprintAssignmentAction::Add => {
                let current_membership = membership
                    .get(&task_id)
                    .map(|set| set.iter().copied().collect::<Vec<_>>())
                    .unwrap_or_default();

                let mut removed_from: Vec<u32> = Vec::new();

                if force_single {
                    for other_id in current_membership.iter().copied() {
                        if other_id == sprint_id {
                            continue;
                        }
                        if let Some(sprint) = sprint_map.get_mut(&other_id) {
                            if remove_task_from_sprint(sprint, &task_id) {
                                changed_sprints.insert(other_id);
                            }
                        }
                        removed_from.push(other_id);
                    }
                }

                let already_in_target = current_membership.contains(&sprint_id);
                let mut added_to_target = false;

                if !already_in_target {
                    let sprint = sprint_map
                        .get_mut(&sprint_id)
                        .ok_or_else(|| format!("Sprint #{} not found.", sprint_id))?;
                    if add_task_to_sprint(sprint, &task_id) {
                        changed_sprints.insert(sprint_id);
                        added_to_target = true;
                    }
                }

                if removed_from.is_empty() && !added_to_target && already_in_target {
                    unchanged.push(task_id.clone());
                    continue;
                }

                if !removed_from.is_empty() {
                    let mut previous = removed_from.clone();
                    previous.sort_unstable();
                    previous.dedup();
                    replaced.push(SprintReassignmentInfo {
                        task_id: task_id.clone(),
                        previous,
                    });
                }

                let mut updated_membership: BTreeSet<u32> =
                    current_membership.into_iter().collect();
                for id in &removed_from {
                    updated_membership.remove(id);
                }
                if added_to_target || already_in_target {
                    updated_membership.insert(sprint_id);
                }
                if updated_membership.is_empty() {
                    membership.remove(&task_id);
                } else {
                    membership.insert(task_id.clone(), updated_membership);
                }

                if !modified.contains(&task_id) {
                    modified.push(task_id.clone());
                }
            }
            SprintAssignmentAction::Remove => {
                let current_membership = membership
                    .get(&task_id)
                    .map(|set| set.iter().copied().collect::<Vec<_>>())
                    .unwrap_or_default();

                if !current_membership.contains(&sprint_id) {
                    unchanged.push(task_id.clone());
                    continue;
                }

                let sprint = sprint_map
                    .get_mut(&sprint_id)
                    .ok_or_else(|| format!("Sprint #{} not found.", sprint_id))?;
                if remove_task_from_sprint(sprint, &task_id) {
                    changed_sprints.insert(sprint_id);
                }

                let mut updated_membership: BTreeSet<u32> =
                    current_membership.into_iter().collect();
                updated_membership.remove(&sprint_id);
                if updated_membership.is_empty() {
                    membership.remove(&task_id);
                } else {
                    membership.insert(task_id.clone(), updated_membership);
                }

                modified.push(task_id.clone());
            }
        }
    }

    for sprint_id in &changed_sprints {
        if let Some(sprint) = sprint_map.get(sprint_id) {
            SprintService::update(storage, *sprint_id, sprint.clone())
                .map_err(|err| err.to_string())?;
        }
    }

    Ok(SprintAssignmentOutcome {
        action,
        sprint_id,
        sprint_label: target_record
            .sprint
            .plan
            .as_ref()
            .and_then(|plan| plan.label.clone()),
        sprint_display_name: sprint_display_name(target_record),
        modified,
        unchanged,
        replaced,
    })
}

fn build_membership_index(sprint_map: &BTreeMap<u32, Sprint>) -> HashMap<String, BTreeSet<u32>> {
    let mut map: HashMap<String, BTreeSet<u32>> = HashMap::new();
    for (sprint_id, sprint) in sprint_map.iter() {
        for entry in &sprint.tasks {
            let task_id = entry.id.trim();
            if task_id.is_empty() {
                continue;
            }
            map.entry(task_id.to_string())
                .or_default()
                .insert(*sprint_id);
        }
    }
    map
}

fn add_task_to_sprint(sprint: &mut Sprint, task_id: &str) -> bool {
    let normalized = task_id.trim();
    if normalized.is_empty() {
        return false;
    }
    if sprint.tasks.iter().any(|entry| entry.id == normalized) {
        return false;
    }
    sprint.tasks.push(SprintTaskEntry {
        id: normalized.to_string(),
        order: None,
    });
    true
}

fn remove_task_from_sprint(sprint: &mut Sprint, task_id: &str) -> bool {
    let before = sprint.tasks.len();
    sprint.tasks.retain(|entry| entry.id != task_id);
    before != sprint.tasks.len()
}

#[derive(Debug, Clone)]
pub struct SprintBacklogOptions {
    pub project: Option<String>,
    pub tags: Vec<String>,
    pub statuses: Vec<TaskStatus>,
    pub assignee: Option<String>,
    pub limit: usize,
}

#[derive(Debug, Clone, Serialize)]
pub struct SprintBacklogEntry {
    pub id: String,
    pub title: String,
    pub status: String,
    pub priority: String,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub assignee: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub due_date: Option<String>,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub tags: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct SprintBacklogResult {
    pub entries: Vec<SprintBacklogEntry>,
    pub truncated: bool,
}

pub fn fetch_backlog(
    storage: &Storage,
    options: SprintBacklogOptions,
) -> Result<SprintBacklogResult, String> {
    let resolved_assignee = match options.assignee.as_ref() {
        Some(raw) => {
            let trimmed = raw.trim();
            if trimmed.is_empty() {
                None
            } else {
                resolve_me_alias(trimmed, Some(storage.root_path.as_path())).ok_or_else(|| {
                    format!(
                        "Unable to resolve assignee '{}'. Set LOTAR identity or provide the full value.",
                        raw
                    )
                })?
                .into()
            }
        }
        None => None,
    };

    let mut filter = TaskFilter {
        project: options.project.clone(),
        ..TaskFilter::default()
    };
    if !options.statuses.is_empty() {
        filter.status = options.statuses.clone();
    }
    if !options.tags.is_empty() {
        filter.tags = options.tags.clone();
    }

    let use_assignee_filter = resolved_assignee.as_deref();

    let sprint_lookup = TaskService::load_sprint_lookup(storage);
    let mut entries = Vec::new();
    for (id, task) in storage.search(&filter) {
        if sprint_lookup.contains_key(&id) {
            continue;
        }

        if let Some(assignee) = use_assignee_filter {
            match task.assignee.as_ref() {
                Some(value) if value.eq_ignore_ascii_case(assignee) => {}
                _ => continue,
            }
        }

        entries.push(SprintBacklogEntry {
            id: id.clone(),
            title: task.title.clone(),
            status: task.status.to_string(),
            priority: task.priority.to_string(),
            assignee: task.assignee.clone(),
            due_date: task.due_date.clone(),
            tags: task.tags.clone(),
        });
    }

    entries.sort_by(|a, b| a.id.cmp(&b.id));

    let mut truncated = false;
    if options.limit > 0 && entries.len() > options.limit {
        entries.truncate(options.limit);
        truncated = true;
    }

    Ok(SprintBacklogResult { entries, truncated })
}

pub fn resolve_sprint_id(records: &[SprintRecord], reference: Option<&str>) -> Result<u32, String> {
    if records.is_empty() {
        return Err("No sprints found.".to_string());
    }

    if let Some(token) = reference {
        let trimmed = token.trim();
        if trimmed.is_empty() {
            return resolve_default_sprint(records);
        }
        let lowered = trimmed.to_ascii_lowercase();
        return match lowered.as_str() {
            "next" => resolve_next_sprint(records),
            "previous" | "prev" => resolve_previous_sprint(records),
            "active" => resolve_default_sprint(records),
            _ => {
                let normalized = trimmed.strip_prefix('#').unwrap_or(trimmed);
                let parsed = normalized.parse::<u32>().map_err(|_| {
                    format!(
                        "Invalid sprint reference '{}'. Expected a numeric identifier or keyword (next/previous).",
                        token
                    )
                })?;
                records
                    .iter()
                    .find(|record| record.id == parsed)
                    .map(|record| record.id)
                    .ok_or_else(|| format!("Sprint #{} not found.", parsed))
            }
        };
    }

    resolve_default_sprint(records)
}

pub fn resolve_default_sprint(records: &[SprintRecord]) -> Result<u32, String> {
    let now = Utc::now();
    let mut active = Vec::new();
    for record in records {
        let lifecycle = sprint_status::derive_status(&record.sprint, now);
        if matches!(
            lifecycle.state,
            SprintLifecycleState::Active | SprintLifecycleState::Overdue
        ) {
            active.push(record.id);
        }
    }

    match active.len() {
        0 => Err(
            "No active sprint found. Specify a sprint identifier with --sprint or prefix the command with the sprint number.".to_string(),
        ),
        1 => Ok(active[0]),
        _ => Err(
            "Multiple active sprints detected. Specify a sprint identifier with --sprint or prefix the command with the sprint number.".to_string(),
        ),
    }
}

pub fn resolve_next_sprint(records: &[SprintRecord]) -> Result<u32, String> {
    let base = resolve_default_sprint(records)?;
    let mut ids: Vec<u32> = records.iter().map(|record| record.id).collect();
    ids.sort_unstable();
    ids.into_iter()
        .find(|id| *id > base)
        .ok_or_else(|| format!("Sprint #{} is already the latest sprint.", base))
}

pub fn resolve_previous_sprint(records: &[SprintRecord]) -> Result<u32, String> {
    let base = resolve_default_sprint(records)?;
    let mut ids: Vec<u32> = records.iter().map(|record| record.id).collect();
    ids.sort_unstable();
    ids.into_iter()
        .rev()
        .find(|id| *id < base)
        .ok_or_else(|| format!("Sprint #{} is the earliest sprint.", base))
}

pub fn ensure_sprint_is_open(record: &SprintRecord) -> Result<(), String> {
    let lifecycle = sprint_status::derive_status(&record.sprint, Utc::now());
    if matches!(lifecycle.state, SprintLifecycleState::Complete) {
        return Err(format!(
            "Sprint #{} ({}) is closed. Pass --allow-closed to override.",
            record.id,
            sprint_display_name(record)
        ));
    }
    Ok(())
}

pub fn sprint_display_name(record: &SprintRecord) -> String {
    record
        .sprint
        .plan
        .as_ref()
        .and_then(|plan| plan.label.clone())
        .unwrap_or_else(|| format!("Sprint {}", record.id))
}

pub fn resolve_task_identifier(storage: &Storage, raw: &str) -> Result<String, String> {
    let token = raw.trim();
    if token.is_empty() {
        return Err("Task identifier cannot be empty.".to_string());
    }

    if token.contains('-') {
        let mut splitter = token.splitn(2, '-');
        let project = splitter.next().unwrap_or("");
        let project = project.to_string();
        if storage.get(token, project.clone()).is_some() {
            return Ok(token.to_string());
        }
        return Err(format!("Task {} not found.", token));
    }

    if let Some((full_id, _)) = storage.find_task_by_numeric_id(token) {
        return Ok(full_id);
    }

    Err(format!(
        "Task {} not found. Use the fully-qualified identifier (e.g. TEST-123).",
        token
    ))
}

pub fn likely_sprint_reference(storage: &Storage, records: &[SprintRecord], token: &str) -> bool {
    let trimmed = token.trim();
    if trimmed.is_empty() {
        return false;
    }

    let lowered = trimmed.to_ascii_lowercase();
    if matches!(lowered.as_str(), "next" | "previous" | "prev" | "active") {
        return true;
    }

    let normalized = trimmed.strip_prefix('#').unwrap_or(trimmed);

    if normalized.chars().all(|c| c.is_ascii_digit()) {
        if storage.find_task_by_numeric_id(normalized).is_some() {
            return false;
        }
        if let Ok(id) = normalized.parse::<u32>() {
            return records.iter().any(|record| record.id == id);
        }
    }

    false
}

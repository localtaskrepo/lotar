use std::collections::{BTreeMap, BTreeSet, HashSet};

use chrono::Utc;
use serde::Serialize;

use crate::errors::LoTaRResult;
use crate::services::sprint_service::SprintRecord;
use crate::services::task_service::TaskService;
use crate::storage::filter::TaskFilter;
use crate::storage::manager::Storage;
use crate::storage::task::Task;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SprintMissingReference {
    pub sprint_id: u32,
    pub count: usize,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct MissingSprintReport {
    pub scanned_tasks: usize,
    pub tasks_with_missing: usize,
    pub missing_sprints: Vec<u32>,
    pub reference_counts: Vec<SprintMissingReference>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SprintCleanupOutcome {
    pub scanned_tasks: usize,
    pub updated_tasks: usize,
    pub removed_references: usize,
    pub removed_by_sprint: Vec<SprintMissingReference>,
    pub missing_sprints: Vec<u32>,
    pub targeted: Option<u32>,
    pub remaining_missing: Vec<u32>,
}

pub fn detect_missing_sprints(storage: &Storage, records: &[SprintRecord]) -> MissingSprintReport {
    let existing_ids: HashSet<u32> = records.iter().map(|record| record.id).collect();
    let tasks = storage.search(&TaskFilter::default());
    build_missing_report(&tasks, &existing_ids)
}

pub fn cleanup_missing_sprint_refs(
    storage: &mut Storage,
    records: &mut Vec<SprintRecord>,
    target: Option<u32>,
) -> LoTaRResult<SprintCleanupOutcome> {
    let existing_ids: HashSet<u32> = records.iter().map(|record| record.id).collect();
    let tasks_snapshot = storage.search(&TaskFilter::default());
    let report = build_missing_report(&tasks_snapshot, &existing_ids);

    let mut removed_by_sprint: BTreeMap<u32, usize> = BTreeMap::new();
    let mut updated_tasks = 0usize;
    let mut removed_references = 0usize;
    let mut touched_sprints: HashSet<u32> = HashSet::new();

    for (task_id, mut task) in tasks_snapshot {
        if task.sprints.is_empty() {
            continue;
        }

        let normalized = TaskService::normalize_sprint_ids(&task.sprints);
        if normalized.is_empty() {
            continue;
        }

        let mut desired: BTreeSet<u32> = BTreeSet::new();
        let mut removed_for_task: Vec<u32> = Vec::new();

        for sprint_id in normalized.iter().copied() {
            let exists = existing_ids.contains(&sprint_id);
            let targeted = target.map(|value| value == sprint_id).unwrap_or(false);

            if targeted {
                removed_for_task.push(sprint_id);
            } else if exists {
                desired.insert(sprint_id);
            } else {
                removed_for_task.push(sprint_id);
            }
        }

        let touched =
            TaskService::apply_memberships_to_records(records.as_mut_slice(), &task_id, &desired)?;
        touched_sprints.extend(touched);

        for sprint_id in removed_for_task {
            *removed_by_sprint.entry(sprint_id).or_insert(0) += 1;
            removed_references += 1;
        }

        task.sprints.clear();
        task.modified = Utc::now().to_rfc3339();
        storage.edit(&task_id, &task);
        updated_tasks += 1;
    }

    TaskService::persist_sprint_records(storage, records.as_slice(), &touched_sprints)?;

    let refreshed_tasks = storage.search(&TaskFilter::default());
    let remaining_report = build_missing_report(&refreshed_tasks, &existing_ids);

    let removed_by_sprint_vec = removed_by_sprint
        .into_iter()
        .map(|(sprint_id, count)| SprintMissingReference { sprint_id, count })
        .collect();

    Ok(SprintCleanupOutcome {
        scanned_tasks: report.scanned_tasks,
        updated_tasks,
        removed_references,
        removed_by_sprint: removed_by_sprint_vec,
        missing_sprints: report.missing_sprints,
        targeted: target,
        remaining_missing: remaining_report.missing_sprints,
    })
}

fn build_missing_report(
    tasks: &[(String, Task)],
    existing_ids: &HashSet<u32>,
) -> MissingSprintReport {
    let scanned_tasks = tasks.len();
    let mut reference_counts: BTreeMap<u32, usize> = BTreeMap::new();
    let mut tasks_with_missing = 0usize;

    for (_, task) in tasks {
        let mut has_missing = false;
        for sprint_id in &task.sprints {
            if !existing_ids.contains(sprint_id) {
                *reference_counts.entry(*sprint_id).or_insert(0) += 1;
                has_missing = true;
            }
        }
        if has_missing {
            tasks_with_missing += 1;
        }
    }

    let missing_sprints: Vec<u32> = reference_counts.keys().copied().collect();
    let reference_counts_vec = reference_counts
        .into_iter()
        .map(|(sprint_id, references)| SprintMissingReference {
            sprint_id,
            count: references,
        })
        .collect();

    MissingSprintReport {
        scanned_tasks,
        tasks_with_missing,
        missing_sprints,
        reference_counts: reference_counts_vec,
    }
}

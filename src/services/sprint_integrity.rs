use std::collections::{BTreeMap, BTreeSet, HashSet};

use chrono::Utc;
use serde::Serialize;

use crate::errors::{LoTaRError, LoTaRResult};
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
    // Coordinated transaction (DEV-55): all task edits and sprint membership
    // writes in one staged commit. The pre-scan only picks candidate project
    // directories for the initial lock set; the authoritative project set is
    // recomputed under the coordinated locks and the operation retries with a
    // wider lock set (or fails closed) when fresh state reveals a project the
    // current transaction does not cover (reviews L1/L4). The caller's records
    // are only written back after a successful commit.
    let mut locked_projects: BTreeSet<String> = {
        let mut projects: BTreeSet<String> = BTreeSet::new();
        for (task_id, task) in storage.search(&TaskFilter::default()) {
            if TaskService::normalize_sprint_ids(&task.sprints).is_empty() {
                continue;
            }
            if let Some(prefix) =
                crate::storage::operations::StorageOperations::get_project_for_task(&task_id)
            {
                projects.insert(prefix);
            }
        }
        projects
    };

    const MAX_LOCK_EXTENSION_ATTEMPTS: usize = 3;
    let mut last_coverage_err = None;
    for _attempt in 0..MAX_LOCK_EXTENSION_ATTEMPTS {
        let candidate_projects: Vec<String> = locked_projects.iter().cloned().collect();
        let txn = crate::storage::transaction::MultiFileTransaction::begin(
            &storage.root_path,
            &candidate_projects,
        )?;
        let fresh_records = crate::services::sprint_service::SprintService::list(storage)?;
        let tasks_snapshot = storage.search(&TaskFilter::default());

        // Authoritative coverage check under the locks: every project that
        // still holds sprint references must be locked, otherwise abort this
        // (still write-free) transaction and retry with the union lock set.
        let mut observed: BTreeSet<String> = BTreeSet::new();
        for (task_id, task) in &tasks_snapshot {
            if TaskService::normalize_sprint_ids(&task.sprints).is_empty() {
                continue;
            }
            if let Some(prefix) =
                crate::storage::operations::StorageOperations::get_project_for_task(task_id)
            {
                observed.insert(prefix);
            }
        }
        if let Err(err) = txn.ensure_covers(&observed.iter().cloned().collect::<Vec<_>>()) {
            last_coverage_err = Some(err);
            let before = locked_projects.len();
            locked_projects.extend(observed);
            if locked_projects.len() > before {
                // Drop the transaction (releasing its locks) and retry wider.
                drop(txn);
                continue;
            }
            return Err(last_coverage_err.expect("coverage error checked above"));
        }

        return cleanup_under_transaction(
            storage,
            records,
            target,
            txn,
            fresh_records,
            tasks_snapshot,
        );
    }
    Err(last_coverage_err.unwrap_or_else(|| {
        LoTaRError::ValidationError(
            "Sprint cleanup could not stabilize its project lock set; retry the operation".into(),
        )
    }))
}

#[allow(clippy::too_many_arguments)]
fn cleanup_under_transaction(
    storage: &mut Storage,
    records: &mut Vec<SprintRecord>,
    target: Option<u32>,
    mut txn: crate::storage::transaction::MultiFileTransaction,
    fresh_records: Vec<SprintRecord>,
    tasks_snapshot: Vec<(String, Task)>,
) -> LoTaRResult<SprintCleanupOutcome> {
    let existing_ids: HashSet<u32> = fresh_records.iter().map(|record| record.id).collect();
    let report = build_missing_report(&tasks_snapshot, &existing_ids);

    let mut removed_by_sprint: BTreeMap<u32, usize> = BTreeMap::new();
    let mut updated_tasks = 0usize;
    let mut removed_references = 0usize;
    let mut touched_sprints: HashSet<u32> = HashSet::new();

    let mut snapshot = tasks_snapshot;
    snapshot.sort_by(|a, b| a.0.cmp(&b.0));
    let mut working_records = fresh_records;
    for (task_id, mut task) in snapshot {
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

        let touched = TaskService::apply_memberships_to_records(
            working_records.as_mut_slice(),
            &task_id,
            &desired,
        )?;
        touched_sprints.extend(touched);

        if removed_for_task.is_empty() {
            continue;
        }

        for sprint_id in removed_for_task {
            *removed_by_sprint.entry(sprint_id).or_insert(0) += 1;
            removed_references += 1;
        }

        task.sprints = desired.into_iter().collect();
        task.modified = Utc::now().to_rfc3339();
        let project_prefix =
            crate::storage::operations::StorageOperations::get_project_for_task(&task_id)
                .unwrap_or_default();
        let project_path = storage.root_path.join(&project_prefix);
        let (file_path, file_string) =
            crate::storage::operations::StorageOperations::prepare_task_edit(
                &project_path,
                &task_id,
                &task,
            )
            .map_err(crate::storage::manager::map_storage_error)?;
        txn.stage(&file_path, file_string)?;
        updated_tasks += 1;
    }

    TaskService::stage_sprint_records(
        &mut txn,
        &storage.root_path,
        &working_records,
        &touched_sprints,
    )?;
    txn.commit()?;
    // Only a committed outcome updates the caller's snapshot (review L4).
    *records = working_records;

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

use std::collections::BTreeSet;
use std::fs;
use std::path::Path;

use crate::config::types::SprintDefaultsConfig;
use crate::errors::{LoTaRError, LoTaRResult};
use crate::storage::manager::Storage;
use crate::storage::sprint::{
    Sprint, SprintActual, SprintCanonicalizationWarning, SprintCapacity, SprintPlan,
};
use crate::storage::task::Task;
use chrono::Utc;

#[derive(Debug, Clone, PartialEq)]
pub struct SprintRecord {
    pub id: u32,
    pub sprint: Sprint,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SprintOperationOutcome {
    pub record: SprintRecord,
    pub warnings: Vec<SprintCanonicalizationWarning>,
    pub applied_defaults: Vec<String>,
    /// Whether the operation persisted new bytes. No-op updates leave the
    /// stored file untouched (DEV-102).
    pub changed: bool,
}

/// Tri-state metadata patch for sprint updates (DEV-102).
///
/// `None` leaves a field untouched, `Some(None)` clears it, and
/// `Some(Some(value))` sets it. Patches are applied to a record loaded
/// fresh under the sprints lock, so a metadata edit cannot overwrite a
/// concurrent membership change staged from a stale snapshot.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct SprintMetadataPatch {
    pub label: Option<Option<String>>,
    pub goal: Option<Option<String>>,
    pub plan_length: Option<Option<String>>,
    pub ends_at: Option<Option<String>>,
    pub starts_at: Option<Option<String>>,
    pub overdue_after: Option<Option<String>>,
    pub notes: Option<Option<String>>,
    pub capacity_points: Option<Option<u32>>,
    pub capacity_hours: Option<Option<u32>>,
    pub actual_started_at: Option<Option<String>>,
    pub actual_closed_at: Option<Option<String>>,
}

fn clean_patch_text(value: Option<String>) -> Option<String> {
    value.and_then(|text| {
        let trimmed = text.trim();
        if trimmed.is_empty() {
            None
        } else {
            Some(trimmed.to_string())
        }
    })
}

impl SprintMetadataPatch {
    pub fn is_empty(&self) -> bool {
        *self == Self::default()
    }

    /// Build the patch from a REST/MCP update request. Request text fields
    /// are single-tier options where an empty string clears the field.
    pub fn from_request(body: &crate::api_types::SprintUpdateRequest) -> Self {
        Self {
            label: body.label.clone().map(Some),
            goal: body.goal.clone().map(Some),
            plan_length: body.plan_length.clone().map(Some),
            ends_at: body.ends_at.clone().map(Some),
            starts_at: body.starts_at.clone().map(Some),
            overdue_after: body.overdue_after.clone().map(Some),
            notes: body.notes.clone().map(Some),
            capacity_points: body.capacity_points,
            capacity_hours: body.capacity_hours,
            actual_started_at: body.actual_started_at.clone(),
            actual_closed_at: body.actual_closed_at.clone(),
        }
    }

    /// Apply the patch to a sprint in place with the field semantics the
    /// REST, MCP, and CLI update entry points share: set, clear, and prune
    /// newly emptied `plan`/`actual` sections.
    pub fn apply_to(&self, target: &mut Sprint) {
        if let Some(value) = self.label.clone() {
            let plan = target.plan.get_or_insert_with(SprintPlan::default);
            plan.label = clean_patch_text(value);
        }
        if let Some(value) = self.goal.clone() {
            let plan = target.plan.get_or_insert_with(SprintPlan::default);
            plan.goal = clean_patch_text(value);
        }
        if let Some(value) = self.plan_length.clone() {
            let plan = target.plan.get_or_insert_with(SprintPlan::default);
            plan.length = clean_patch_text(value);
        }
        if let Some(value) = self.ends_at.clone() {
            let plan = target.plan.get_or_insert_with(SprintPlan::default);
            plan.ends_at = clean_patch_text(value);
        }
        if let Some(value) = self.starts_at.clone() {
            let plan = target.plan.get_or_insert_with(SprintPlan::default);
            plan.starts_at = clean_patch_text(value);
        }
        if let Some(value) = self.overdue_after.clone() {
            let plan = target.plan.get_or_insert_with(SprintPlan::default);
            plan.overdue_after = clean_patch_text(value);
        }
        if let Some(value) = self.notes.clone() {
            let plan = target.plan.get_or_insert_with(SprintPlan::default);
            plan.notes = clean_patch_text(value);
        }

        if let Some(value) = self.capacity_points {
            match value {
                Some(points) => {
                    let plan = target.plan.get_or_insert_with(SprintPlan::default);
                    plan.capacity
                        .get_or_insert_with(SprintCapacity::default)
                        .points = Some(points);
                }
                None => {
                    if let Some(plan) = target.plan.as_mut()
                        && let Some(capacity) = plan.capacity.as_mut()
                    {
                        capacity.points = None;
                        if capacity.points.is_none() && capacity.hours.is_none() {
                            plan.capacity = None;
                        }
                    }
                }
            }
        }
        if let Some(value) = self.capacity_hours {
            match value {
                Some(hours) => {
                    let plan = target.plan.get_or_insert_with(SprintPlan::default);
                    plan.capacity
                        .get_or_insert_with(SprintCapacity::default)
                        .hours = Some(hours);
                }
                None => {
                    if let Some(plan) = target.plan.as_mut()
                        && let Some(capacity) = plan.capacity.as_mut()
                    {
                        capacity.hours = None;
                        if capacity.points.is_none() && capacity.hours.is_none() {
                            plan.capacity = None;
                        }
                    }
                }
            }
        }

        if let Some(value) = self.actual_started_at.clone() {
            match value {
                Some(text) => {
                    let actual = target.actual.get_or_insert_with(SprintActual::default);
                    actual.started_at = clean_patch_text(Some(text));
                }
                None => {
                    if let Some(actual) = target.actual.as_mut() {
                        actual.started_at = None;
                    }
                }
            }
        }
        if let Some(value) = self.actual_closed_at.clone() {
            match value {
                Some(text) => {
                    let actual = target.actual.get_or_insert_with(SprintActual::default);
                    actual.closed_at = clean_patch_text(Some(text));
                }
                None => {
                    if let Some(actual) = target.actual.as_mut() {
                        actual.closed_at = None;
                    }
                }
            }
        }

        let should_clear_actual = matches!(
            target.actual.as_ref(),
            Some(actual) if actual.started_at.is_none() && actual.closed_at.is_none()
        );
        if should_clear_actual {
            target.actual = None;
        }

        if let Some(plan) = target.plan.as_ref()
            && !plan.has_values()
        {
            target.plan = None;
        }
    }
}

pub struct SprintService;

impl SprintService {
    pub fn list(storage: &Storage) -> LoTaRResult<Vec<SprintRecord>> {
        let dir = Sprint::dir(&storage.root_path);
        if !dir.exists() {
            return Ok(Vec::new());
        }

        let mut records = Vec::new();
        for entry in fs::read_dir(&dir)? {
            let entry = entry?;
            if !entry.path().is_file()
                || entry
                    .path()
                    .file_stem()
                    .and_then(|stem| stem.to_str())
                    .and_then(|stem| stem.parse::<u32>().ok())
                    .is_none()
                || !matches!(
                    entry.path().extension().and_then(|ext| ext.to_str()),
                    Some("yml" | "yaml")
                )
            {
                continue;
            }
            let loaded = match Self::load_entry(&entry.path()) {
                Ok(loaded) => loaded,
                Err(e) => {
                    crate::storage::safety::warn_corrupt_once(&entry.path(), &e.to_string());
                    continue;
                }
            };
            let (id, mut sprint) = loaded;
            let _ = sprint.canonicalize();
            records.push(SprintRecord { id, sprint });
        }
        records.sort_by_key(|record| record.id);
        Ok(records)
    }

    pub fn get(storage: &Storage, id: u32) -> LoTaRResult<SprintRecord> {
        let path = Sprint::path_for_id(&storage.root_path, id);
        if !path.exists() {
            return Err(LoTaRError::SprintNotFound(id));
        }
        let (loaded_id, mut sprint) = Self::load_entry(&path)?;
        let _ = sprint.canonicalize();
        Ok(SprintRecord {
            id: loaded_id,
            sprint,
        })
    }

    pub fn create(
        storage: &mut Storage,
        mut sprint: Sprint,
        defaults: Option<&SprintDefaultsConfig>,
    ) -> LoTaRResult<SprintOperationOutcome> {
        let applied_defaults = defaults
            .map(|defaults| apply_sprint_defaults(&mut sprint, defaults))
            .unwrap_or_default();
        let warnings = sprint.canonicalize();
        let now = Utc::now().to_rfc3339();
        if sprint.created.is_none() {
            sprint.created = Some(now.clone());
        }
        sprint.modified = Some(now);
        let dir = Sprint::dir(&storage.root_path);
        fs::create_dir_all(&dir)?;
        let next_id = crate::storage::safety::with_storage_lock(
            &dir,
            crate::storage::safety::sprint_lock_name(),
            || -> LoTaRResult<u32> {
                let id = Self::next_identifier(&dir)?;
                Self::write_sprint(&dir, id, &sprint)?;
                Ok(id)
            },
        )?;
        Ok(SprintOperationOutcome {
            record: SprintRecord {
                id: next_id,
                sprint,
            },
            warnings,
            applied_defaults,
            changed: true,
        })
    }

    pub fn update(
        storage: &mut Storage,
        id: u32,
        sprint: Sprint,
    ) -> LoTaRResult<SprintOperationOutcome> {
        let dir = Sprint::dir(&storage.root_path);
        if !dir.exists() {
            return Err(LoTaRError::SprintNotFound(id));
        }
        crate::storage::safety::with_storage_lock(
            &dir,
            crate::storage::safety::sprint_lock_name(),
            || {
                let path = dir.join(format!("{}.yml", id));
                if !path.exists() {
                    return Err(LoTaRError::SprintNotFound(id));
                }
                let (_, current) = Self::load_entry(&path)?;
                let mut baseline = current;
                let _ = baseline.canonicalize();
                let mut candidate = sprint;
                let warnings = candidate.canonicalize();
                if candidate == baseline {
                    return Ok(SprintOperationOutcome {
                        record: SprintRecord {
                            id,
                            sprint: candidate,
                        },
                        warnings,
                        applied_defaults: Vec::new(),
                        changed: false,
                    });
                }
                let (sprint, serialized) = Self::stamp_for_write(candidate)?;
                crate::storage::safety::atomic_write_file(&path, &serialized)
                    .map_err(LoTaRError::from)?;
                Ok(SprintOperationOutcome {
                    record: SprintRecord { id, sprint },
                    warnings,
                    applied_defaults: Vec::new(),
                    changed: true,
                })
            },
        )
    }

    /// Apply a metadata patch to the sprint loaded fresh under the sprints
    /// lock and persist it only when the effective record changes. A no-op
    /// patch leaves the stored bytes, mtime, and `modified` stamp exactly as
    /// they were and does not canonicalize legacy on-disk spellings.
    pub fn update_with_patch(
        storage: &mut Storage,
        id: u32,
        patch: &SprintMetadataPatch,
    ) -> LoTaRResult<SprintOperationOutcome> {
        let dir = Sprint::dir(&storage.root_path);
        if !dir.exists() {
            return Err(LoTaRError::SprintNotFound(id));
        }
        crate::storage::safety::with_storage_lock(
            &dir,
            crate::storage::safety::sprint_lock_name(),
            || {
                let path = dir.join(format!("{}.yml", id));
                if !path.exists() {
                    return Err(LoTaRError::SprintNotFound(id));
                }
                let (_, current) = Self::load_entry(&path)?;
                // Mirror read canonicalization first so only conflicts the
                // patch itself introduces surface as warnings, matching the
                // previous get-then-update flow.
                let mut baseline = current;
                let _ = baseline.canonicalize();
                let mut candidate = baseline.clone();
                patch.apply_to(&mut candidate);
                let warnings = candidate.canonicalize();
                if candidate == baseline {
                    return Ok(SprintOperationOutcome {
                        record: SprintRecord {
                            id,
                            sprint: baseline,
                        },
                        warnings,
                        applied_defaults: Vec::new(),
                        changed: false,
                    });
                }
                let (sprint, serialized) = Self::stamp_for_write(candidate)?;
                crate::storage::safety::atomic_write_file(&path, &serialized)
                    .map_err(LoTaRError::from)?;
                Ok(SprintOperationOutcome {
                    record: SprintRecord { id, sprint },
                    warnings,
                    applied_defaults: Vec::new(),
                    changed: true,
                })
            },
        )
    }

    /// Canonicalize and serialize a sprint for an update without writing it,
    /// so coordinated transactions can stage the exact bytes (DEV-55).
    fn prepare_update(
        mut sprint: Sprint,
    ) -> LoTaRResult<(Sprint, Vec<SprintCanonicalizationWarning>, String)> {
        let warnings = sprint.canonicalize();
        let now = Utc::now().to_rfc3339();
        sprint.modified = Some(now.clone());
        if sprint.created.is_none() {
            sprint.created = Some(now);
        }
        let serialized = sprint.to_yaml()?;
        Ok((sprint, warnings, serialized))
    }

    /// Stamp write timestamps onto a canonicalized candidate that is already
    /// known to differ from the stored record.
    fn stamp_for_write(mut sprint: Sprint) -> LoTaRResult<(Sprint, String)> {
        let now = Utc::now().to_rfc3339();
        sprint.modified = Some(now.clone());
        if sprint.created.is_none() {
            sprint.created = Some(now);
        }
        let serialized = sprint.to_yaml()?;
        Ok((sprint, serialized))
    }

    /// Stage a sprint update inside a coordinated transaction. The caller must
    /// hold the transaction (which owns the sprints lock) for the staging and
    /// commit; this never takes storage locks itself. Snapshots that match
    /// the stored record are skipped so only genuine changes are staged.
    pub(crate) fn stage_update(
        txn: &mut crate::storage::transaction::MultiFileTransaction,
        root: &std::path::Path,
        id: u32,
        sprint: Sprint,
    ) -> LoTaRResult<()> {
        let path = Sprint::path_for_id(root, id);
        if !path.exists() {
            return Err(LoTaRError::SprintNotFound(id));
        }
        if let Ok((_, current)) = Self::load_entry(&path) {
            let mut baseline = current;
            let _ = baseline.canonicalize();
            let mut candidate = sprint.clone();
            let _ = candidate.canonicalize();
            if candidate == baseline {
                return Ok(());
            }
        }
        let (_, _, serialized) = Self::prepare_update(sprint)?;
        txn.stage(&path, serialized)
    }

    pub fn delete(storage: &mut Storage, id: u32) -> LoTaRResult<bool> {
        let dir = Sprint::dir(&storage.root_path);
        crate::storage::safety::with_storage_lock(
            &dir,
            crate::storage::safety::sprint_lock_name(),
            || -> LoTaRResult<bool> {
                let path = dir.join(format!("{}.yml", id));
                if !path.exists() {
                    return Ok(false);
                }
                fs::remove_file(path)?;
                Ok(true)
            },
        )
    }

    pub fn load_tasks_for_record(storage: &Storage, record: &SprintRecord) -> Vec<(String, Task)> {
        let mut tasks = Vec::new();
        for entry in &record.sprint.tasks {
            let task_id = entry.id.trim();
            if task_id.is_empty() {
                continue;
            }
            let project = crate::storage::TaskId::parse(task_id)
                .map(|parsed| parsed.project)
                .unwrap_or_default();
            if project.is_empty() {
                continue;
            }
            if let Some(task) = storage.get(task_id, &project) {
                // Sprint analytics are active-only (DEV-92): tombstones keep
                // their sprint memberships on disk but do not count toward
                // workload, progress, or ordering.
                if task.deleted_at.is_some() {
                    continue;
                }
                tasks.push((task_id.to_string(), task));
            }
        }
        tasks
    }

    fn next_identifier(dir: &Path) -> LoTaRResult<u32> {
        let mut max_id = 0u32;
        if dir.exists() {
            for entry in fs::read_dir(dir)? {
                let entry = entry?;
                if let Some(stem) = entry.path().file_stem().and_then(|s| s.to_str())
                    && let Ok(parsed) = stem.parse::<u32>()
                {
                    max_id = max_id.max(parsed);
                }
            }
        }
        Ok(max_id.saturating_add(1))
    }

    fn load_entry(path: &Path) -> LoTaRResult<(u32, Sprint)> {
        let stem = path
            .file_stem()
            .and_then(|s| s.to_str())
            .ok_or_else(|| LoTaRError::SerializationError("Invalid sprint filename".into()))?;
        let id: u32 = stem
            .parse()
            .map_err(|_| LoTaRError::SerializationError("Invalid sprint identifier".into()))?;
        let contents = fs::read_to_string(path)?;
        let sprint: Sprint = serde_yaml_ng::from_str(&contents)?;
        Ok((id, sprint))
    }

    fn write_sprint(dir: &Path, id: u32, sprint: &Sprint) -> LoTaRResult<()> {
        fs::create_dir_all(dir)?;
        let serialized = sprint.to_yaml()?;
        let path = dir.join(format!("{}.yml", id));
        crate::storage::safety::atomic_write_file(&path, &serialized)?;
        Ok(())
    }
}

fn apply_sprint_defaults(sprint: &mut Sprint, defaults: &SprintDefaultsConfig) -> Vec<String> {
    let mut applied = BTreeSet::new();
    let plan = sprint.plan.get_or_insert_with(SprintPlan::default);

    if let Some(points) = defaults.capacity_points {
        let capacity = plan.capacity.get_or_insert_with(SprintCapacity::default);
        if capacity.points.is_none() {
            capacity.points = Some(points);
            applied.insert("capacity_points".to_string());
        }
    }

    if let Some(hours) = defaults.capacity_hours {
        let capacity = plan.capacity.get_or_insert_with(SprintCapacity::default);
        if capacity.hours.is_none() {
            capacity.hours = Some(hours);
            applied.insert("capacity_hours".to_string());
        }
    }

    if let Some(length) = defaults.length.as_ref()
        && plan.length.is_none()
    {
        plan.length = Some(length.clone());
        applied.insert("length".to_string());
    }

    if let Some(overdue_after) = defaults.overdue_after.as_ref()
        && plan.overdue_after.is_none()
    {
        plan.overdue_after = Some(overdue_after.clone());
        applied.insert("overdue_after".to_string());
    }

    applied.into_iter().collect()
}

// -- Request and response mapping shared by the REST API, MCP and CLI --------

/// Build a new sprint from a create request. Text fields are trimmed and
/// dropped when blank; notes keep their formatting. The plan is omitted
/// when nothing was provided.
pub fn sprint_from_create_request(payload: &crate::api_types::SprintCreateRequest) -> Sprint {
    let mut plan = SprintPlan::default();

    if let Some(label) = clean_opt_string(payload.label.clone()) {
        plan.label = Some(label);
    }
    if let Some(goal) = clean_opt_string(payload.goal.clone()) {
        plan.goal = Some(goal);
    }
    if let Some(length) = clean_opt_string(payload.plan_length.clone()) {
        plan.length = Some(length);
    }
    if let Some(ends_at) = clean_opt_string(payload.ends_at.clone()) {
        plan.ends_at = Some(ends_at);
    }
    if let Some(starts_at) = clean_opt_string(payload.starts_at.clone()) {
        plan.starts_at = Some(starts_at);
    }
    if let Some(points) = payload.capacity_points {
        plan.capacity
            .get_or_insert_with(SprintCapacity::default)
            .points = Some(points);
    }
    if let Some(hours) = payload.capacity_hours {
        plan.capacity
            .get_or_insert_with(SprintCapacity::default)
            .hours = Some(hours);
    }
    if let Some(overdue_after) = clean_opt_string(payload.overdue_after.clone()) {
        plan.overdue_after = Some(overdue_after);
    }
    if let Some(notes) = payload
        .notes
        .clone()
        .filter(|value| !value.trim().is_empty())
    {
        plan.notes = Some(notes);
    }

    let mut sprint = Sprint::default();
    if plan_has_values(&plan) {
        sprint.plan = Some(plan);
    }
    sprint
}

/// List view of a stored sprint, with its lifecycle derived at `reference`.
pub fn sprint_record_to_list_item(
    record: &SprintRecord,
    reference: chrono::DateTime<Utc>,
) -> crate::api_types::SprintListItem {
    let lifecycle = crate::services::sprint_status::derive_status(&record.sprint, reference);
    let plan = record.sprint.plan.as_ref();
    let capacity = plan.and_then(|plan| plan.capacity.as_ref());
    crate::api_types::SprintListItem {
        id: record.id,
        label: record
            .sprint
            .plan
            .as_ref()
            .and_then(|plan| plan.label.clone()),
        display_name: crate::services::sprint_assignment::sprint_display_name(record),
        created: record.sprint.created.clone(),
        modified: record.sprint.modified.clone(),
        state: lifecycle.state.as_str().to_string(),
        planned_start: lifecycle.planned_start.map(|dt| dt.to_rfc3339()),
        planned_end: lifecycle.planned_end.map(|dt| dt.to_rfc3339()),
        actual_start: lifecycle.actual_start.map(|dt| dt.to_rfc3339()),
        actual_end: lifecycle.actual_end.map(|dt| dt.to_rfc3339()),
        computed_end: lifecycle.computed_end.map(|dt| dt.to_rfc3339()),
        goal: plan.and_then(|plan| plan.goal.clone()),
        plan_length: plan.and_then(|plan| plan.length.clone()),
        overdue_after: plan.and_then(|plan| plan.overdue_after.clone()),
        notes: plan.and_then(|plan| plan.notes.clone()),
        capacity_points: capacity.and_then(|capacity| capacity.points),
        capacity_hours: capacity.and_then(|capacity| capacity.hours),
        warnings: lifecycle
            .warnings
            .iter()
            .map(|warning| warning.message())
            .collect(),
    }
}

fn plan_has_values(plan: &SprintPlan) -> bool {
    plan.label.is_some()
        || plan.goal.is_some()
        || plan.length.is_some()
        || plan.ends_at.is_some()
        || plan.starts_at.is_some()
        || plan.capacity.is_some()
        || plan.overdue_after.is_some()
        || plan.notes.is_some()
}

fn clean_opt_string(input: Option<String>) -> Option<String> {
    input.and_then(|value| {
        let trimmed = value.trim();
        if trimmed.is_empty() {
            None
        } else {
            Some(trimmed.to_string())
        }
    })
}

#[cfg(test)]
mod request_mapping_tests {
    use super::*;
    use crate::api_types::SprintCreateRequest;

    #[test]
    fn create_request_trims_fields_keeps_notes_and_drops_blanks() {
        let sprint = sprint_from_create_request(&SprintCreateRequest {
            label: Some("  Sprint 7 ".into()),
            goal: Some("   ".into()),
            capacity_hours: Some(30),
            notes: Some("  - indented note\n".into()),
            ..SprintCreateRequest::default()
        });
        let plan = sprint.plan.expect("plan");
        assert_eq!(plan.label.as_deref(), Some("Sprint 7"));
        assert_eq!(plan.goal, None);
        assert_eq!(plan.notes.as_deref(), Some("  - indented note\n"));
        let capacity = plan.capacity.expect("capacity");
        assert_eq!((capacity.points, capacity.hours), (None, Some(30)));

        let blank = sprint_from_create_request(&SprintCreateRequest {
            notes: Some(" \n".into()),
            ..SprintCreateRequest::default()
        });
        assert!(blank.plan.is_none());
    }
}

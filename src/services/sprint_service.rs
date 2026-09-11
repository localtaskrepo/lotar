use std::collections::BTreeSet;
use std::fs;
use std::path::Path;

use crate::config::types::SprintDefaultsConfig;
use crate::errors::{LoTaRError, LoTaRResult};
use crate::storage::manager::Storage;
use crate::storage::sprint::{Sprint, SprintCanonicalizationWarning, SprintCapacity, SprintPlan};
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
        })
    }

    pub fn update(
        storage: &mut Storage,
        id: u32,
        sprint: Sprint,
    ) -> LoTaRResult<SprintOperationOutcome> {
        let dir = Sprint::dir(&storage.root_path);
        let (sprint, warnings, serialized) = Self::prepare_update(sprint)?;
        crate::storage::safety::with_storage_lock(
            &dir,
            crate::storage::safety::sprint_lock_name(),
            || {
                let path = dir.join(format!("{}.yml", id));
                if !path.exists() {
                    return Err(LoTaRError::SprintNotFound(id));
                }
                crate::storage::safety::atomic_write_file(&path, &serialized)
                    .map_err(LoTaRError::from)
            },
        )?;
        Ok(SprintOperationOutcome {
            record: SprintRecord { id, sprint },
            warnings,
            applied_defaults: Vec::new(),
        })
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

    /// Stage a sprint update inside a coordinated transaction. The caller must
    /// hold the transaction (which owns the sprints lock) for the staging and
    /// commit; this never takes storage locks itself.
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
            let project = task_id.split('-').next().unwrap_or("").to_string();
            if project.is_empty() {
                continue;
            }
            if let Some(task) = storage.get(task_id, &project) {
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

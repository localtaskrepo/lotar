use std::collections::{BTreeSet, HashMap, HashSet};

use crate::api_types::{TaskOrderRequest, TaskOrderResponse};
use crate::errors::{LoTaRError, LoTaRResult};
use crate::services::sprint_assignment::ensure_sprint_is_open;
use crate::services::sprint_service::SprintRecord;
use crate::services::task_service::TaskService;
use crate::storage::manager::Storage;
use crate::storage::sprint::{BacklogOrder, Sprint, SprintHistoryEntry, SprintTaskEntry};
use crate::storage::transaction::MultiFileTransaction;
use crate::storage::{TaskFilter, TaskId};
use crate::types::TaskChange;

fn canonical_id(raw: &str) -> LoTaRResult<String> {
    TaskId::parse(raw.trim())
        .map(|id| id.canonical())
        .map_err(|error| LoTaRError::ValidationError(format!("Invalid task ID '{raw}': {error}")))
}

fn canonical_sequence(ids: impl IntoIterator<Item = String>) -> LoTaRResult<Vec<String>> {
    let mut seen = HashSet::new();
    let mut result = Vec::new();
    for raw in ids {
        let id = canonical_id(&raw)?;
        if !seen.insert(id.clone()) {
            return Err(LoTaRError::ValidationError(format!(
                "Duplicate task '{id}' in stored order"
            )));
        }
        result.push(id);
    }
    Ok(result)
}

fn project_order(stored: &[String], eligible: impl IntoIterator<Item = String>) -> Vec<String> {
    let mut remaining: BTreeSet<String> = eligible.into_iter().collect();
    let mut ordered = Vec::new();
    for id in stored {
        if remaining.remove(id) {
            ordered.push(id.clone());
        }
    }
    ordered.extend(remaining);
    ordered
}

pub fn backlog_order_for_ids(
    storage: &Storage,
    ids: impl IntoIterator<Item = String>,
) -> LoTaRResult<Vec<String>> {
    let metadata = BacklogOrder::load(&storage.root_path)?;
    let stored = canonical_sequence(metadata.tasks)?;
    Ok(project_order(&stored, ids))
}

fn eligible_backlog_ids(storage: &Storage) -> LoTaRResult<Vec<String>> {
    // A corrupt numeric sprint cannot prove that a task is unassigned.
    // Mutation eligibility must not inherit the listing's skip-corrupt policy.
    let directory = storage.root_path.join("@sprints");
    if directory.exists() {
        for entry in std::fs::read_dir(directory)? {
            let path = entry?.path();
            if path.is_file()
                && matches!(
                    path.extension().and_then(|value| value.to_str()),
                    Some("yml" | "yaml")
                )
                && path
                    .file_stem()
                    .and_then(|value| value.to_str())
                    .is_some_and(|value| value.parse::<u32>().is_ok())
            {
                let sprint: Sprint = serde_yaml_ng::from_str(&std::fs::read_to_string(path)?)?;
                canonical_sequence(sprint.tasks.into_iter().map(|entry| entry.id))?;
            }
        }
    }
    let membership = TaskService::load_sprint_lookup(storage);
    Ok(storage
        .search(&TaskFilter::default())
        .into_iter()
        .filter(|(id, _)| !membership.contains_key(id))
        .map(|(id, _)| id)
        .collect())
}

pub fn get_order(storage: &Storage, sprint_id: Option<u32>) -> LoTaRResult<TaskOrderResponse> {
    let tasks = match sprint_id {
        Some(0) => {
            return Err(LoTaRError::ValidationError(
                "sprint must be positive".into(),
            ));
        }
        Some(id) => {
            let contents = std::fs::read_to_string(Sprint::path_for_id(&storage.root_path, id))?;
            let sprint: Sprint = serde_yaml_ng::from_str(&contents)?;
            canonical_sequence(sprint.tasks.into_iter().map(|entry| entry.id))?
        }
        None => backlog_order_for_ids(storage, eligible_backlog_ids(storage)?)?,
    };
    Ok(TaskOrderResponse {
        status: "ok".into(),
        tasks,
        changed: false,
        sprint_id,
    })
}

fn ensure_local_active(storage: &Storage, id: &str) -> LoTaRResult<()> {
    let location = storage
        .resolve_task_location(id)
        .map_err(|error| LoTaRError::ValidationError(error.to_string()))?;
    if !location.is_in_root(&storage.root_path) {
        return Err(LoTaRError::ValidationError(format!(
            "Task '{id}' belongs to another tasks root"
        )));
    }
    let project = TaskId::parse(id)
        .map_err(|error| LoTaRError::InvalidTaskId(error.to_string()))?
        .project;
    let task = storage
        .get(id, &project)
        .ok_or_else(|| LoTaRError::TaskNotFound(id.into()))?;
    if task.deleted_at.is_some() {
        return Err(LoTaRError::ValidationError(format!(
            "Task '{id}' is deleted"
        )));
    }
    Ok(())
}

fn move_relative(tasks: &mut Vec<String>, task: &str, anchor: &str, before: bool) {
    if task == anchor {
        return;
    }
    let origin = tasks
        .iter()
        .position(|id| id == task)
        .expect("validated origin");
    let value = tasks.remove(origin);
    let target = tasks
        .iter()
        .position(|id| id == anchor)
        .expect("validated anchor");
    tasks.insert(target + usize::from(!before), value);
}

/// Relative moves use fresh membership and order under the shared sprint lock.
/// Only the target order file is staged; task content and membership never change.
pub fn reorder(storage: &Storage, request: &TaskOrderRequest) -> LoTaRResult<TaskOrderResponse> {
    if request.sprint == Some(0) || request.before.is_some() == request.after.is_some() {
        return Err(LoTaRError::ValidationError(
            "Provide exactly one of before/after and a positive sprint ID when present".into(),
        ));
    }
    let task = canonical_id(&request.task)?;
    let anchor = canonical_id(
        request
            .before
            .as_deref()
            .or(request.after.as_deref())
            .expect("validated anchor"),
    )?;
    // Refuse absent/foreign roots before lock acquisition can create directories.
    ensure_local_active(storage, &task)?;
    ensure_local_active(storage, &anchor)?;
    let projects = [
        TaskId::parse(&task).expect("canonical task").project,
        TaskId::parse(&anchor).expect("canonical anchor").project,
    ];
    // Identity/config resolution precedes project lock acquisition, matching
    // other mutation services' lock ordering.
    let actor = crate::utils::identity::resolve_current_user_for_project(
        Some(&storage.root_path),
        Some(&projects[0]),
    );
    let mut transaction = MultiFileTransaction::begin(&storage.root_path, &projects)?;
    ensure_local_active(storage, &task)?;
    ensure_local_active(storage, &anchor)?;
    let (mut full_order, mut sprint, mut backlog) = match request.sprint {
        Some(id) => {
            let path = Sprint::path_for_id(&storage.root_path, id);
            if !path.exists() {
                return Err(LoTaRError::SprintNotFound(id));
            }
            let contents = std::fs::read_to_string(path)?;
            let record = SprintRecord {
                id,
                sprint: serde_yaml_ng::from_str::<Sprint>(&contents)?,
            };
            if !request.allow_closed {
                ensure_sprint_is_open(&record).map_err(LoTaRError::ValidationError)?;
            }
            let order =
                canonical_sequence(record.sprint.tasks.iter().map(|entry| entry.id.clone()))?;
            (
                order,
                Some(serde_yaml_ng::from_str::<serde_yaml_ng::Value>(&contents)?),
                None,
            )
        }
        None => {
            let metadata = BacklogOrder::load(&storage.root_path)?;
            let mut order = canonical_sequence(metadata.tasks.clone())?;
            let eligible = eligible_backlog_ids(storage)?;
            let visible = project_order(&order, eligible);
            if !visible.contains(&task) || !visible.contains(&anchor) {
                return Err(LoTaRError::ValidationError(
                    "Task and anchor must still belong to the backlog".into(),
                ));
            }
            let mut moved = visible.clone();
            move_relative(&mut moved, &task, &anchor, request.before.is_some());
            if moved == visible {
                return Ok(TaskOrderResponse {
                    status: "ok".into(),
                    tasks: visible,
                    changed: false,
                    sprint_id: None,
                });
            }
            let known: HashSet<_> = order.iter().cloned().collect();
            order.extend(visible.into_iter().filter(|id| !known.contains(id)));
            (order, None, Some(metadata))
        }
    };
    if !full_order.contains(&task) || !full_order.contains(&anchor) {
        return Err(LoTaRError::ValidationError(
            "Task and anchor must still belong to the target sprint".into(),
        ));
    }
    let previous = full_order.clone();
    move_relative(&mut full_order, &task, &anchor, request.before.is_some());
    if full_order == previous {
        return Ok(TaskOrderResponse {
            status: "ok".into(),
            tasks: previous,
            changed: false,
            sprint_id: request.sprint,
        });
    }
    let now = chrono::Utc::now().to_rfc3339();
    let old_position = previous
        .iter()
        .position(|id| id == &task)
        .expect("validated origin")
        + 1;
    let new_position = full_order
        .iter()
        .position(|id| id == &task)
        .expect("validated origin")
        + 1;
    let placement = if request.before.is_some() {
        "before"
    } else {
        "after"
    };
    let history = SprintHistoryEntry {
        at: now.clone(),
        actor,
        changes: vec![TaskChange {
            field: format!("tasks.{task}"),
            old: Some(format!("position {old_position}")),
            new: Some(format!("position {new_position}; {placement} {anchor}")),
        }],
    };
    if let Some(ref mut sprint) = sprint {
        // Typed parsing above validates metadata and membership. Mutate raw
        // values here so unknown nested fields and legacy entry maps survive.
        let mapping = sprint.as_mapping_mut().ok_or_else(|| {
            LoTaRError::SerializationError("Sprint must be a YAML mapping".into())
        })?;
        let tasks_key = serde_yaml_ng::Value::String("tasks".into());
        let raw_entries = match mapping.remove(&tasks_key) {
            Some(serde_yaml_ng::Value::Sequence(entries)) => entries,
            _ => {
                return Err(LoTaRError::SerializationError(
                    "Sprint tasks must be a sequence".into(),
                ));
            }
        };
        let mut entries: HashMap<_, _> = raw_entries
            .into_iter()
            .map(|raw| {
                let entry: SprintTaskEntry = serde_yaml_ng::from_value(raw.clone())?;
                Ok((canonical_id(&entry.id)?, raw))
            })
            .collect::<LoTaRResult<_>>()?;
        mapping.insert(
            tasks_key,
            serde_yaml_ng::Value::Sequence(
                full_order
                    .iter()
                    .map(|id| entries.remove(id).expect("validated member"))
                    .collect(),
            ),
        );
        mapping.insert(
            serde_yaml_ng::Value::String("modified".into()),
            serde_yaml_ng::Value::String(now),
        );
        let existing_history = mapping
            .entry(serde_yaml_ng::Value::String("history".into()))
            .or_insert_with(|| serde_yaml_ng::Value::Sequence(Vec::new()));
        existing_history
            .as_sequence_mut()
            .ok_or_else(|| {
                LoTaRError::SerializationError("Sprint history must be a sequence".into())
            })?
            .push(serde_yaml_ng::to_value(history)?);
        transaction.stage(
            &Sprint::path_for_id(&storage.root_path, request.sprint.expect("sprint")),
            serde_yaml_ng::to_string(sprint)?,
        )?;
    } else if let Some(ref mut metadata) = backlog {
        metadata.tasks = full_order.clone();
        metadata.modified = Some(now);
        metadata.history.push(history);
        transaction.stage(
            &BacklogOrder::path(&storage.root_path),
            serde_yaml_ng::to_string(metadata)?,
        )?;
    }
    let tasks = if request.sprint.is_some() {
        full_order
    } else {
        project_order(&full_order, eligible_backlog_ids(storage)?)
    };
    transaction.commit()?;
    let response = TaskOrderResponse {
        status: "ok".into(),
        tasks,
        changed: true,
        sprint_id: request.sprint,
    };
    crate::api_events::emit(&crate::api_events::ApiEvent {
        kind: "task_order_changed".into(),
        data: serde_json::json!({"sprint_id": response.sprint_id, "tasks": response.tasks}),
    });
    Ok(response)
}

//! Moving a task to another project (DEV-183).
//!
//! The task gets the next free number in the target project and its old file
//! becomes a move stub (see [`crate::storage::moves`]): a soft-deleted task
//! whose `moved_to` names the new ID. Both files are written in one
//! coordinated transaction holding both projects' locks, so a failure leaves
//! neither half behind. Old references are never rewritten; they resolve
//! through the stub when they are used.

use std::fs;
use std::path::{Path, PathBuf};

use crate::api_types::{TaskMoveChange, TaskMoveRequest, TaskMoveResponse};
use crate::config::types::ResolvedConfig;
use crate::errors::{LoTaRError, LoTaRResult};
use crate::services::agent_job_service::AgentJobService;
use crate::services::agent_queue_service::AgentQueueService;
use crate::services::attachment_service::AttachmentService;
use crate::services::task_service::TaskService;
use crate::services::task_validation as validation;
use crate::storage::TaskId;
use crate::storage::identity::TaskLookupError;
use crate::storage::layout;
use crate::storage::manager::Storage;
use crate::storage::task::Task;
use crate::storage::transaction::MultiFileTransaction;
use crate::types::{ReferenceEntry, TaskChange, TaskChangeLogEntry};
use crate::utils::identity::{resolve_current_user_for_project, resolve_me_alias_for_project};

impl TaskService {
    /// Move a task to another project; see the module documentation.
    pub fn move_task(
        storage: &mut Storage,
        request: &TaskMoveRequest,
    ) -> LoTaRResult<TaskMoveResponse> {
        let root = storage.root_path.clone();
        let target = request.project.trim().to_string();
        crate::storage::safety::validate_project_prefix(&target)
            .map_err(|err| LoTaRError::ValidationError(format!("Invalid project: {err}")))?;
        if !root.join(&target).is_dir() {
            return Err(LoTaRError::ValidationError(format!(
                "Project '{target}' does not exist; create it first, or check the prefix"
            )));
        }

        // An old ID of an already moved task moves the task it became.
        let (current, _) = Self::follow_moves(storage, request.id.trim(), None)?;
        let source = TaskId::parse(&current)
            .map_err(|err| LoTaRError::InvalidTaskId(format!("{current}: {err}")))?;
        let from = source.canonical();
        if source.project == target {
            return Err(LoTaRError::ValidationError(format!(
                "Task '{from}' is already in project '{target}'"
            )));
        }
        match storage.resolve_task_location(&from) {
            Ok(location) if !location.is_in_root(&root) => {
                return Err(LoTaRError::ValidationError(format!(
                    "Task '{from}' is stored in workspace tasks root {} and cannot be moved from {}; run the command inside that workspace",
                    location.root.display(),
                    root.display()
                )));
            }
            Ok(_) => {}
            Err(TaskLookupError::NotFound(_)) => return Err(LoTaRError::TaskNotFound(from)),
            Err(err) => return Err(LoTaRError::ValidationError(err.to_string())),
        }
        Self::ensure_no_agent_work(&root, &from)?;

        let target_config = Self::restore_config(&root, &target)?;
        let source_config = Self::resolve_config_for_project(&root, &source.project);

        let mut txn =
            MultiFileTransaction::begin(&root, &[source.project.clone(), target.clone()])?;
        let source_dir = root.join(&source.project);
        let file = match layout::find(&source_dir, source.number).as_slice() {
            [file] => file.clone(),
            [] => return Err(LoTaRError::TaskNotFound(from)),
            _ => {
                return Err(LoTaRError::ValidationError(format!(
                    "Task '{from}' is stored in more than one file; keep one copy before moving it"
                )));
            }
        };
        let content = fs::read_to_string(&file.path)?;
        let original =
            crate::storage::task::parse_task_yaml_tolerant(&content).ok_or_else(|| {
                LoTaRError::ValidationError(
                    "Cannot safely move malformed task YAML; repair the task file first"
                        .to_string(),
                )
            })?;
        if original.deleted_at.is_some() {
            return Err(LoTaRError::ValidationError(format!(
                "Task '{from}' is deleted; restore it before moving it"
            )));
        }
        crate::storage::operations::ensure_writable(&source_dir, &file.path, &from)?;

        let target_dir = root.join(&target);
        let (number, target_path) = match Self::reclaimable_number(&root, &original, &from, &target)
        {
            // Moving back onto its own stub: take the original number again.
            Some((number, path)) if !file.archived => (number, path),
            _ => {
                let number = layout::max_number(&target_dir) + 1;
                let folder = if file.archived {
                    target_dir.join(layout::ARCHIVE_DIR)
                } else {
                    target_dir.clone()
                };
                (number, folder.join(format!("{number}.yml")))
            }
        };
        let to = format!("{target}-{number}");

        let mut task = original.clone();
        let changes = Self::fit_to_project(&root, &mut task, request, &target_config, &target)?;
        Self::enforce_membership(&task, &target_config, &target)?;

        let now = chrono::Utc::now().to_rfc3339();
        let actor = resolve_current_user_for_project(Some(root.as_path()), Some(&target));
        let moved = TaskChange {
            field: "moved".into(),
            old: Some(from.clone()),
            new: Some(to.clone()),
        };
        task.moved_to = None;
        if !task.moved_from.contains(&from) {
            task.moved_from.push(from.clone());
        }
        task.moved_from.retain(|id| id != &to);
        task.modified = now.clone();
        task.history.push(TaskChangeLogEntry {
            at: now.clone(),
            actor: actor.clone(),
            changes: std::iter::once(moved.clone())
                .chain(changes.iter().map(|change| TaskChange {
                    field: change.field.clone(),
                    old: change.old.clone(),
                    new: change.new.clone(),
                }))
                .collect(),
        });

        // The stub keeps only what identifies it. Remote issue references stay
        // so a sync pull of the old project sees a deleted match and skips
        // it, instead of importing the issue again.
        let stub = Task {
            title: original.title.clone(),
            created: original.created.clone(),
            deleted_at: Some(now.clone()),
            moved_to: Some(to.clone()),
            references: original
                .references
                .iter()
                .filter(|reference| reference.jira.is_some() || reference.github.is_some())
                .cloned()
                .collect(),
            history: vec![TaskChangeLogEntry {
                at: now,
                actor,
                changes: vec![moved],
            }],
            ..Task::default()
        };

        task.archived = file.archived;
        if request.dry_run {
            drop(txn);
            return Ok(TaskMoveResponse {
                from,
                to: to.clone(),
                changes,
                dry_run: true,
                task: Self::dto_from_task(storage, &to, task),
            });
        }

        if let Some(parent) = target_path.parent() {
            fs::create_dir_all(parent)?;
        }
        let serialize = |task: &Task| {
            serde_yaml_ng::to_string(task)
                .map_err(|err| LoTaRError::SerializationError(err.to_string()))
        };
        txn.stage(&target_path, serialize(&task)?)?;
        txn.stage(&file.path, serialize(&stub)?)?;
        txn.commit()?;

        // Files kept beside the task follow it; failures here leave a usable
        // move behind, so they are reported as warnings by the copy helpers.
        Self::carry_attachments(&root, &task.references, &source_config, &target_config);
        Self::carry_agent_context(&root, &from, &to, &source_config, &target_config);

        Ok(TaskMoveResponse {
            from,
            to: to.clone(),
            changes,
            dry_run: false,
            task: Self::dto_from_task(storage, &to, task),
        })
    }

    /// Record a move that was made by hand (DEV-183): the file of `old` was
    /// renamed or moved to `new` outside LoTaR. Writes the stub at the old
    /// ID and adds the old ID to the task's `moved_from`, exactly as a move
    /// would have. Returns whether anything changed.
    pub fn record_move(storage: &mut Storage, old: &str, new: &str) -> LoTaRResult<bool> {
        let root = storage.root_path.clone();
        let parse = |raw: &str| {
            TaskId::parse(raw.trim())
                .map_err(|err| LoTaRError::InvalidTaskId(format!("{raw}: {err}")))
        };
        let (old_id, new_id) = (parse(old)?, parse(new)?);
        let (from, to) = (old_id.canonical(), new_id.canonical());
        if from == to {
            return Err(LoTaRError::ValidationError(
                "The old and new ID are the same".to_string(),
            ));
        }
        if crate::storage::moves::follow(&root, &to)
            .map_err(|err| LoTaRError::ValidationError(err.to_string()))?
            .is_some()
        {
            return Err(LoTaRError::ValidationError(format!(
                "'{to}' is itself a moved task; record the move to its current ID"
            )));
        }

        let projects = if old_id.project == new_id.project {
            vec![old_id.project.clone()]
        } else {
            vec![old_id.project.clone(), new_id.project.clone()]
        };
        fs::create_dir_all(root.join(&old_id.project))?;
        let mut txn = MultiFileTransaction::begin(&root, &projects)?;
        let new_file = match layout::find(&root.join(&new_id.project), new_id.number).as_slice() {
            [file] => file.clone(),
            [] => return Err(LoTaRError::TaskNotFound(to)),
            _ => {
                return Err(LoTaRError::ValidationError(format!(
                    "Task '{to}' is stored in more than one file; keep one copy first"
                )));
            }
        };
        let content = fs::read_to_string(&new_file.path)?;
        let mut task =
            crate::storage::task::parse_task_yaml_tolerant(&content).ok_or_else(|| {
                LoTaRError::ValidationError(format!(
                    "Task '{to}' has malformed YAML; repair it first"
                ))
            })?;

        let old_files = layout::find(&root.join(&old_id.project), old_id.number);
        match old_files.as_slice() {
            [] => {}
            [file] => {
                return match crate::storage::moves::stub_target(&file.path) {
                    Some(target) if TaskId::parse(&target).is_ok_and(|t| t.canonical() == to) => {
                        Ok(false)
                    }
                    Some(target) => Err(LoTaRError::ValidationError(format!(
                        "'{from}' already redirects to '{target}'"
                    ))),
                    None => Err(LoTaRError::ValidationError(format!(
                        "'{from}' still exists; move it with `lotar task move {from} --to <PROJECT>` instead"
                    ))),
                };
            }
            _ => {
                return Err(LoTaRError::ValidationError(format!(
                    "'{from}' is stored in more than one file"
                )));
            }
        }

        let now = chrono::Utc::now().to_rfc3339();
        let actor = resolve_current_user_for_project(Some(root.as_path()), Some(&new_id.project));
        let moved = TaskChange {
            field: "moved".into(),
            old: Some(from.clone()),
            new: Some(to.clone()),
        };
        if !task.moved_from.contains(&from) {
            task.moved_from.push(from.clone());
        }
        task.history.push(TaskChangeLogEntry {
            at: now.clone(),
            actor: actor.clone(),
            changes: vec![moved.clone()],
        });
        let stub = Task {
            title: task.title.clone(),
            created: task.created.clone(),
            deleted_at: Some(now.clone()),
            moved_to: Some(to),
            history: vec![TaskChangeLogEntry {
                at: now,
                actor,
                changes: vec![moved],
            }],
            ..Task::default()
        };
        let serialize = |task: &Task| {
            serde_yaml_ng::to_string(task)
                .map_err(|err| LoTaRError::SerializationError(err.to_string()))
        };
        let stub_path = root
            .join(&old_id.project)
            .join(format!("{}.yml", old_id.number));
        txn.stage(&new_file.path, serialize(&task)?)?;
        txn.stage(&stub_path, serialize(&stub)?)?;
        txn.commit()?;
        Ok(true)
    }

    /// A job running on the task, or waiting in the queue, works on its old
    /// ID and files; moving underneath it would split the work.
    fn ensure_no_agent_work(root: &Path, id: &str) -> LoTaRResult<()> {
        let queued = AgentQueueService::list_pending(root)
            .unwrap_or_default()
            .iter()
            .any(|entry| crate::storage::identity::aliases_match(&entry.ticket_id, id));
        if AgentJobService::has_active_job(id) || queued {
            return Err(LoTaRError::ValidationError(format!(
                "Task '{id}' has an agent job running or queued; wait for it to finish or stop it before moving the task"
            )));
        }
        Ok(())
    }

    /// The number of a stub in `target` that redirects to this task: moving a
    /// task back to a project it came from takes its old number again.
    fn reclaimable_number(
        root: &Path,
        task: &Task,
        current: &str,
        target: &str,
    ) -> Option<(u64, PathBuf)> {
        task.moved_from.iter().rev().find_map(|old| {
            let parsed = TaskId::parse(old).ok()?;
            if parsed.project != target {
                return None;
            }
            let [file]: [layout::TaskFile; 1] = layout::find(&root.join(target), parsed.number)
                .try_into()
                .ok()?;
            crate::storage::moves::stub_target(&file.path)?;
            let follows = crate::storage::moves::follow(root, &parsed.canonical()).ok()??;
            (follows == current).then_some((parsed.number, file.path))
        })
    }

    /// Bring status, type, priority, people and custom fields in line with
    /// the target project: explicit request values win, values the target
    /// accepts stay, anything else falls back to the target's default.
    fn fit_to_project(
        root: &Path,
        task: &mut Task,
        request: &TaskMoveRequest,
        config: &ResolvedConfig,
        target: &str,
    ) -> LoTaRResult<Vec<TaskMoveChange>> {
        let mut changes = Vec::new();
        let mut note = |field: &str, old: Option<String>, new: Option<String>| {
            if old != new {
                changes.push(TaskMoveChange {
                    field: field.to_string(),
                    old,
                    new,
                });
            }
        };
        let missing_default = |field: &str, value: &str| {
            LoTaRError::ValidationError(format!(
                "The {field} '{value}' does not exist in project '{target}' and it has no default; choose one for the move"
            ))
        };

        let status = match request.status.as_deref() {
            Some(raw) => Some(validation::parse_status(raw, config)?),
            None if task.status.is_empty() => None,
            None => Some(
                match validation::parse_status(task.status.as_str(), config) {
                    Ok(status) => status,
                    Err(_) => config
                        .effective_default_status()
                        .ok_or_else(|| missing_default("status", task.status.as_str()))?,
                },
            ),
        };
        if let Some(status) = status {
            note(
                "status",
                Some(task.status.to_string()),
                Some(status.to_string()),
            );
            task.status = status;
        }

        let task_type = match request.task_type.as_deref() {
            Some(raw) => Some(validation::parse_task_type(raw, config)?),
            None if task.task_type.is_empty() => None,
            None => Some(
                match validation::parse_task_type(task.task_type.as_str(), config) {
                    Ok(task_type) => task_type,
                    Err(_) => config
                        .effective_default_task_type()
                        .ok_or_else(|| missing_default("type", task.task_type.as_str()))?,
                },
            ),
        };
        if let Some(task_type) = task_type {
            note(
                "type",
                Some(task.task_type.to_string()),
                Some(task_type.to_string()),
            );
            task.task_type = task_type;
        }

        let priority = match request.priority.as_deref() {
            Some(raw) => Some(validation::parse_priority(raw, config)?),
            None if task.priority.is_empty() => None,
            None => Some(
                match validation::parse_priority(task.priority.as_str(), config) {
                    Ok(priority) => priority,
                    Err(_) => config
                        .effective_default_priority()
                        .ok_or_else(|| missing_default("priority", task.priority.as_str()))?,
                },
            ),
        };
        if let Some(priority) = priority {
            note(
                "priority",
                Some(task.priority.to_string()),
                Some(priority.to_string()),
            );
            task.priority = priority;
        }

        let is_agent = |name: &str| config.agent_profiles.contains_key(name);
        let person = |raw: &str| {
            let trimmed = raw.trim();
            (!trimmed.is_empty()).then(|| {
                let resolved = resolve_me_alias_for_project(trimmed, Some(root), Some(target))
                    .unwrap_or_else(|| trimmed.to_string());
                crate::utils::member::normalize_member_value(&resolved, is_agent)
            })
        };
        if let Some(raw) = request.reporter.as_deref() {
            let value = person(raw);
            note("reporter", task.reporter.clone(), value.clone());
            task.reporter = value;
        }
        if let Some(raw) = request.assignee.as_deref() {
            let value = person(raw);
            note("assignee", task.assignee.clone(), value.clone());
            task.assignee = value;
        }

        let mut fields = task.custom_fields.clone();
        if let Some(overrides) = request.custom_fields.as_ref() {
            for (key, value) in overrides {
                let existing = fields
                    .keys()
                    .find(|stored| stored.eq_ignore_ascii_case(key.trim()))
                    .cloned();
                if let Some(existing) = existing {
                    fields.remove(&existing);
                }
                let clears = value.is_null() || value.as_str().is_some_and(|text| text.is_empty());
                if !clears {
                    fields.insert(key.trim().to_string(), value.clone());
                }
            }
        }
        let resolved = validation::resolve_custom_fields(&fields, config, None).map_err(|err| {
            LoTaRError::ValidationError(format!(
                "{err} Set or drop it for the move (CLI: --field name=value, or --field name= to drop it)."
            ))
        })?;
        let show = |value: Option<&crate::types::CustomFieldValue>| {
            value.map(|value| match value.as_str() {
                Some(text) => text.to_string(),
                None => serde_json::to_string(value).unwrap_or_default(),
            })
        };
        let mut keys: Vec<&String> = task.custom_fields.keys().chain(resolved.keys()).collect();
        keys.sort();
        keys.dedup();
        for key in keys {
            note(
                &format!("field:{key}"),
                show(task.custom_fields.get(key)),
                show(resolved.get(key)),
            );
        }
        task.custom_fields = resolved;
        Ok(changes)
    }

    /// Copy attachment files when the two projects keep them in different
    /// folders. Files are copied, never moved: other tasks may share them.
    fn carry_attachments(
        root: &Path,
        references: &[ReferenceEntry],
        source: &ResolvedConfig,
        target: &ResolvedConfig,
    ) {
        let (Ok(from), Ok(to)) = (
            AttachmentService::compute_attachments_root(root, source),
            AttachmentService::compute_attachments_root(root, target),
        ) else {
            return;
        };
        if from == to {
            return;
        }
        for name in references
            .iter()
            .filter_map(|reference| reference.attachment.as_deref())
        {
            if name.contains('/') || name.contains('\\') || name.contains("..") {
                continue;
            }
            let (src, dst) = (from.join(name), to.join(name));
            if src.is_file()
                && !dst.exists()
                && let Err(err) = fs::create_dir_all(&to).and_then(|_| fs::copy(&src, &dst))
            {
                eprintln!(
                    "Warning: could not copy attachment {} to {}: {err}",
                    src.display(),
                    dst.display()
                );
            }
        }
    }

    /// Agent context files are named after the task ID; rename the file so
    /// the agent's conversation history stays with the task.
    fn carry_agent_context(
        root: &Path,
        from: &str,
        to: &str,
        source: &ResolvedConfig,
        target: &ResolvedConfig,
    ) {
        let (Ok(from_id), Ok(to_id)) = (TaskId::parse(from), TaskId::parse(to)) else {
            return;
        };
        let src = root
            .join(&from_id.project)
            .join(format!("{from}{}", source.agent_context_extension));
        let dst = root
            .join(&to_id.project)
            .join(format!("{to}{}", target.agent_context_extension));
        if src.is_file()
            && !dst.exists()
            && let Err(err) = fs::rename(&src, &dst)
        {
            eprintln!(
                "Warning: could not move agent context {} to {}: {err}",
                src.display(),
                dst.display()
            );
        }
    }
}

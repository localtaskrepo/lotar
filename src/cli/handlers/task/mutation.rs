use crate::api_types::TaskDTO;
use crate::cli::handlers::task::context::TaskCommandContext;
use crate::output::OutputRenderer;
use crate::services::task_service::TaskService;
use crate::storage::task::Task;

pub struct LoadedTask {
    pub full_id: String,
    pub project_prefix: String,
    pub task: Task,
}

/// A task loaded through the deletion-aware lookup: soft-deleted tasks
/// resolve too, and `task.deleted_at` distinguishes them.
pub struct LoadedDeletedAwareTask {
    pub full_id: String,
    pub project_prefix: String,
    pub task: TaskDTO,
}

/// Same resolution pipeline as [`load_task`] (prefix resolution, numeric-id
/// fallback, effective-project update) but resolving through
/// `TaskService::get_including_deleted` so soft-deleted tasks remain
/// addressable for delete/restore flows.
pub fn load_task_including_deleted(
    ctx: &mut TaskCommandContext,
    raw_id: &str,
    project: Option<&str>,
) -> Result<LoadedDeletedAwareTask, String> {
    ctx.project_resolver
        .validate_task_id_format(raw_id)
        .map_err(|e| format!("Invalid task ID: {}", e))?;

    let mut full_id = ctx.resolve_full_task_id(raw_id, project)?;
    let mut project_prefix = crate::storage::TaskId::parse(&full_id)
        .map(|parsed| parsed.project)
        .unwrap_or_default();

    let mut task_opt =
        TaskService::get_including_deleted(&ctx.storage, &full_id, Some(project_prefix.as_str()))
            .ok();

    if task_opt.is_none() && raw_id.chars().all(|c| c.is_ascii_digit()) {
        // Clear fail-closed diagnostics for numeric lookups: ambiguous
        // numbers (stored by more than one project/root) are refused. The
        // numeric resolver only sees active tasks; a soft-deleted task must
        // be addressed by its full or prefixed ID.
        match ctx.storage.resolve_numeric_id(raw_id) {
            Ok((actual_id, _)) => {
                project_prefix = crate::storage::TaskId::parse(&actual_id)
                    .map(|parsed| parsed.project)
                    .unwrap_or_default();
                full_id = actual_id;
                task_opt = TaskService::get_including_deleted(
                    &ctx.storage,
                    &full_id,
                    Some(project_prefix.as_str()),
                )
                .ok();
            }
            Err(err) => {
                return Err(format!("Task '{}' not found: {}", raw_id, err));
            }
        }
    }

    let task = task_opt.ok_or_else(|| format!("Task '{}' not found", raw_id))?;

    ctx.update_effective_project(Some(project_prefix.as_str()))?;

    Ok(LoadedDeletedAwareTask {
        full_id,
        project_prefix,
        task,
    })
}

pub fn load_task(
    ctx: &mut TaskCommandContext,
    raw_id: &str,
    project: Option<&str>,
) -> Result<LoadedTask, String> {
    ctx.project_resolver
        .validate_task_id_format(raw_id)
        .map_err(|e| format!("Invalid task ID: {}", e))?;

    let mut full_id = ctx.resolve_full_task_id(raw_id, project)?;
    let mut project_prefix = crate::storage::TaskId::parse(&full_id)
        .map(|parsed| parsed.project)
        .unwrap_or_default();

    let mut task_opt = ctx.storage.get(&full_id, &project_prefix);

    if task_opt.is_none() && raw_id.chars().all(|c| c.is_ascii_digit()) {
        // Clear fail-closed diagnostics for numeric lookups: ambiguous
        // numbers (stored by more than one project/root) are refused.
        match ctx.storage.resolve_numeric_id(raw_id) {
            Ok((actual_id, task)) => {
                project_prefix = crate::storage::TaskId::parse(&actual_id)
                    .map(|parsed| parsed.project)
                    .unwrap_or_default();
                full_id = actual_id;
                task_opt = Some(task);
            }
            Err(err) => {
                return Err(format!("Task '{}' not found: {}", raw_id, err));
            }
        }
    }

    let task = task_opt.ok_or_else(|| format!("Task '{}' not found", raw_id))?;

    if task.deleted_at.is_some() {
        return Err(format!(
            "Task '{}' is soft-deleted; restore it before reading or editing it",
            raw_id
        ));
    }

    ctx.update_effective_project(Some(project_prefix.as_str()))?;

    Ok(LoadedTask {
        full_id,
        project_prefix,
        task,
    })
}

/// Plan membership validation without writing; TaskService re-plans and stages
/// the project config under its coordinated update transaction.
pub fn plan_auto_populate_members(ctx: &mut TaskCommandContext, task: &Task) {
    let missing = TaskService::missing_members_for_task(task, &ctx.config);
    if missing.is_empty() {
        return;
    }

    let mut merged = ctx.config.members.clone();
    for candidate in missing.iter() {
        if !merged
            .iter()
            .any(|existing| existing.eq_ignore_ascii_case(candidate))
        {
            merged.push(candidate.clone());
        }
    }
    merged.sort_by_key(|value| value.to_ascii_lowercase());
    ctx.config.members = merged;
}

pub fn ensure_membership(
    ctx: &TaskCommandContext,
    task: &Task,
    project_prefix: &str,
) -> Result<(), String> {
    TaskService::enforce_membership(task, &ctx.config, project_prefix)
        .map_err(|e| format!("Member validation failed: {}", e))
}

pub fn render_edit_preview(renderer: &OutputRenderer, id: &str, task: &Task) {
    match renderer.format {
        crate::output::OutputFormat::Json => {
            let obj = serde_json::json!({
                "status": "preview",
                "action": "edit",
                "task_id": id,
                "task_type": task.task_type.to_string(),
                "priority": task.priority.to_string(),
                "assignee": task.assignee,
                "due_date": task.due_date,
                "tags": task.tags,
            });
            renderer.emit_json(&obj);
        }
        _ => {
            renderer.emit_info(format_args!(
                "DRY RUN: Would update '{}' with: type={:?}, priority={}, assignee={:?}, due={:?}, tags={}",
                id,
                task.task_type,
                task.priority,
                task.assignee,
                task.due_date,
                if task.tags.is_empty() {
                    "-".to_string()
                } else {
                    task.tags.join(",")
                }
            ));
        }
    }
}

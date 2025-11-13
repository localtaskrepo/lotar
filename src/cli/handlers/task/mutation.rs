use crate::cli::handlers::task::context::TaskCommandContext;
use crate::output::OutputRenderer;
use crate::services::task_service::TaskService;
use crate::storage::task::Task;

pub struct LoadedTask {
    pub full_id: String,
    pub project_prefix: String,
    pub task: Task,
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
    let mut project_prefix = full_id.split('-').next().unwrap_or("").to_string();

    let mut task_opt = ctx.storage.get(&full_id, project_prefix.clone());

    if task_opt.is_none() && raw_id.chars().all(|c| c.is_ascii_digit()) {
        if let Some((actual_id, task)) = ctx.storage.find_task_by_numeric_id(raw_id) {
            project_prefix = actual_id.split('-').next().unwrap_or("").to_string();
            full_id = actual_id;
            task_opt = Some(task);
        }
    }

    let task = task_opt.ok_or_else(|| format!("Task '{}' not found", raw_id))?;

    ctx.update_effective_project(Some(project_prefix.as_str()))?;

    Ok(LoadedTask {
        full_id,
        project_prefix,
        task,
    })
}

pub fn apply_auto_populate_members(
    ctx: &mut TaskCommandContext,
    project_prefix: &str,
    task: &Task,
    dry_run: bool,
) -> Result<(), String> {
    let missing = TaskService::missing_members_for_task(task, &ctx.config);
    if missing.is_empty() {
        return Ok(());
    }

    if dry_run || project_prefix.trim().is_empty() {
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
        return Ok(());
    }

    match crate::config::operations::auto_populate_project_members(
        ctx.storage_root(),
        project_prefix,
        &ctx.config.members,
        &missing,
    ) {
        Ok(Some(updated)) => {
            ctx.config.members = updated;
            Ok(())
        }
        Ok(None) => Ok(()),
        Err(err) => Err(format!("Failed to auto-populate project members: {}", err)),
    }
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
            renderer.emit_info(&format!(
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

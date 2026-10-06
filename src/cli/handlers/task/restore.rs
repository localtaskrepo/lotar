use crate::cli::TaskRestoreArgs;
use crate::cli::handlers::CommandHandler;
use crate::cli::handlers::task::context::TaskCommandContext;
use crate::cli::handlers::task::mutation::{LoadedDeletedAwareTask, load_task_including_deleted};
use crate::services::task_service::TaskService;
use crate::workspace::TasksDirectoryResolver;

/// Handler for restoring soft-deleted tasks. Restoring clears `deleted_at`
/// while preserving the task id and content; the service appends a
/// `restored` history entry and leaves `modified` unchanged. Hard-deleted
/// tasks cannot be restored (the file is gone).
pub struct RestoreHandler;

impl CommandHandler for RestoreHandler {
    type Args = TaskRestoreArgs;
    type Result = Result<(), String>;

    fn execute(
        args: Self::Args,
        project: Option<&str>,
        resolver: &TasksDirectoryResolver,
        renderer: &crate::output::OutputRenderer,
    ) -> Self::Result {
        renderer.log_info("restore: begin");
        let TaskRestoreArgs { id, dry_run } = args;
        let mut ctx = TaskCommandContext::new(resolver, project, Some(id.as_str()))?;
        let LoadedDeletedAwareTask {
            full_id,
            project_prefix,
            task,
        } = load_task_including_deleted(&mut ctx, &id, project)?;

        if dry_run {
            // Mirror `TaskService::restore`'s fail-closed gate without
            // writing: stored status/priority/type (raw stored strings, not
            // coerced DTO enums) must validate against the project's
            // CURRENT config, or the preview reports the would-be failure
            // instead of claiming the restore would run.
            if let Some(deleted_at) = task.deleted_at.as_deref() {
                let raw_task = ctx.storage.get(&full_id, &project_prefix);
                if let Some(raw) = raw_task.as_ref()
                    && let Err(e) = validate_restore_preconditions(
                        ctx.storage_root(),
                        project_prefix.as_str(),
                        raw,
                    )
                {
                    return Err(format!(
                        "Restore preview for '{}' (deleted_at {}) would fail: {}",
                        id, deleted_at, e
                    ));
                }
            }
            match renderer.format {
                crate::output::OutputFormat::Json => {
                    let obj = serde_json::json!({
                        "status": "preview",
                        "action": "restore",
                        "task_id": id,
                        "project": project_prefix,
                        "deleted_at": task.deleted_at,
                    });
                    renderer.emit_json(&obj);
                }
                _ => {
                    if task.deleted_at.is_some() {
                        renderer.emit_info(format_args!(
                            "DRY RUN: Would restore task '{}' from project {} (deleted_at: {})",
                            id,
                            project_prefix,
                            task.deleted_at.as_deref().unwrap_or("unknown")
                        ));
                    } else {
                        renderer.emit_info(format_args!(
                            "DRY RUN: Task '{}' from project {} is not soft-deleted; nothing to restore",
                            id, project_prefix
                        ));
                    }
                }
            }
            return Ok(());
        }

        match TaskService::restore(&mut ctx.storage, &full_id, Some(project_prefix.as_str())) {
            Ok(restored) => {
                match renderer.format {
                    crate::output::OutputFormat::Json => {
                        let obj = serde_json::json!({
                            "status": "success",
                            "message": format!("Task '{}' restored", id),
                            "task_id": id,
                            "deleted_at": restored.deleted_at,
                        });
                        renderer.emit_json(&obj);
                    }
                    _ => {
                        renderer.emit_success(format_args!("Task '{}' restored", id));
                    }
                }
                Ok(())
            }
            Err(e) => Err(format!("Failed to restore task '{}': {}", id, e)),
        }
    }
}

/// Non-writing counterpart of the enum gate inside
/// `TaskService::restore`: the stored status/priority/type are checked
/// against the project's current resolved config using the same public
/// parsers the service calls. Nothing is mutated; a stored value the
/// current config rejects blocks the preview exactly as it would block
/// the real restore.
fn validate_restore_preconditions(
    storage_root: &std::path::Path,
    project_prefix: &str,
    task: &crate::storage::task::Task,
) -> Result<(), String> {
    let config = TaskService::restore_config(storage_root, project_prefix)
        .map_err(|e| format!("failed to resolve project config: {e}"))?;
    if !task.status.is_empty()
        && let Err(e) =
            crate::services::task_validation::parse_status(task.status.as_str(), &config)
    {
        return Err(format!(
            "stored status is invalid under the current config: {e}"
        ));
    }
    if !task.priority.is_empty()
        && let Err(e) =
            crate::services::task_validation::parse_priority(task.priority.as_str(), &config)
    {
        return Err(format!(
            "stored priority is invalid under the current config: {e}"
        ));
    }
    if !task.task_type.is_empty()
        && let Err(e) =
            crate::services::task_validation::parse_task_type(task.task_type.as_str(), &config)
    {
        return Err(format!(
            "stored task type is invalid under the current config: {e}"
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::validate_restore_preconditions;
    use crate::storage::task::parse_task_yaml_tolerant;

    fn seeded_root(states: &str) -> tempfile::TempDir {
        let tmp = tempfile::tempdir().unwrap();
        let tasks_dir = tmp.path().join(".tasks");
        std::fs::create_dir_all(&tasks_dir).unwrap();
        std::fs::write(
            tasks_dir.join("config.yml"),
            format!(
                "default:\n  project: MCP\nmembers:\n  - alice\nissue:\n  states: [{states}]\n"
            ),
        )
        .unwrap();
        tmp
    }

    fn stored_task() -> crate::storage::task::Task {
        parse_task_yaml_tolerant(
            "title: t\nstatus: Todo\npriority: Low\ntype: Feature\ncreated: 2026-01-01T00:00:00Z\n",
        )
        .expect("fixture task YAML parses")
    }

    #[test]
    fn accepts_values_valid_under_current_config() {
        let tmp = seeded_root("Todo, InProgress, Done");
        let root = tmp.path().join(".tasks");
        assert!(validate_restore_preconditions(root.as_path(), "MCP", &stored_task()).is_ok());
    }

    #[test]
    fn rejects_stored_value_the_real_restore_would_reject() {
        // Same stored "Todo", but the project's current config no longer
        // admits it: the preview must fail closed like the real restore.
        let tmp = seeded_root("Open, Closed");
        let root = tmp.path().join(".tasks");
        let err = validate_restore_preconditions(root.as_path(), "MCP", &stored_task())
            .expect_err("config-invalid stored status must block the preview");
        assert!(
            err.contains("stored status"),
            "error should name the offending field: {err}"
        );
    }
}

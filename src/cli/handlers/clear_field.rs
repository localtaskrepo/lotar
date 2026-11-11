//! `--clear` for quick property commands (DEV-283): `lotar due-date ID
//! --clear` and `lotar assignee ID --clear` remove the value instead of
//! requiring `edit` or hand-edited YAML.

use crate::cli::handlers::task::context::TaskCommandContext;
use crate::output::{OutputFormat, OutputRenderer};
use crate::workspace::TasksDirectoryResolver;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClearableField {
    DueDate,
    Assignee,
}

impl ClearableField {
    fn label(self) -> &'static str {
        match self {
            Self::DueDate => "due date",
            Self::Assignee => "assignee",
        }
    }

    fn key(self) -> &'static str {
        match self {
            Self::DueDate => "due_date",
            Self::Assignee => "assignee",
        }
    }
}

pub fn clear_task_field(
    resolver: &TasksDirectoryResolver,
    project: Option<&str>,
    id: &str,
    field: ClearableField,
    dry_run: bool,
    renderer: &OutputRenderer,
) -> Result<(), String> {
    let mut ctx = TaskCommandContext::new(resolver, project, Some(id))?;
    let full_id = ctx.resolve_full_task_id(id, project)?;
    let task = crate::services::task_service::TaskService::get(&ctx.storage, &full_id, None).map_err(|e| e.to_string())?;
    let current = match field {
        ClearableField::DueDate => task.due_date.clone(),
        ClearableField::Assignee => task.assignee.clone(),
    }
    .filter(|value| !value.trim().is_empty());
    if field == ClearableField::Assignee
        && let Some(job) = crate::cli::handlers::agent::running_job_for_ticket(&ctx.tasks_dir.path, &full_id)
        && !job.takes_ticket_changes(&ctx.tasks_dir.path, &full_id)
    {
        return Err(format!("Ticket '{full_id}' has an active agent job (pid {}). Cancel it before changing assignee.", job.pid));
    }
    let label = field.label();
    let (status, message, changed) = match (&current, dry_run) {
        (None, _) => ("success", format!("Task {full_id} has no {label}; no changes"), false),
        (Some(old), true) => ("preview", format!("DRY RUN: would clear the {label} of {full_id} (now {old})"), false),
        (Some(_), false) => {
            let mut patch = crate::api_types::TaskUpdate::default();
            match field {
                ClearableField::DueDate => patch.due_date = Some(String::new()),
                ClearableField::Assignee => patch.assignee = Some(String::new()),
            }
            let receipt = crate::services::task_service::TaskService::update_with_receipt(
                &mut ctx.storage,
                &full_id,
                patch,
                crate::services::task_service::TaskUpdateContext::default(),
            )
            .map_err(|e| e.to_string())?;
            if let Some(outcome) = receipt.deferred.as_ref() {
                crate::cli::handlers::task::render::render_job_outcome(renderer, &full_id, outcome);
                return Ok(());
            }
            ("success", format!("Task {full_id} {label} cleared"), true)
        }
    };
    if matches!(renderer.format, OutputFormat::Json) {
        renderer.emit_json(&serde_json::json!({
            "status": status,
            "message": message,
            "task_id": full_id,
            "changed": changed,
            format!("old_{}", field.key()): current,
        }));
    } else if status == "preview" {
        renderer.emit_info(message);
    } else {
        renderer.emit_success(message);
    }
    Ok(())
}

use crate::api_types::{TaskMoveRequest, TaskMoveResponse};
use crate::cli::TaskMoveArgs;
use crate::cli::handlers::task::context::TaskCommandContext;
use crate::output::{OutputFormat, OutputRenderer};
use crate::services::task_service::TaskService;
use crate::workspace::TasksDirectoryResolver;

/// `lotar task move` (DEV-183): move tasks to another project. Each task gets
/// the next free number there; its old ID stays as a redirect. Tasks come
/// from IDs, a `--select` query, or both.
pub(crate) fn execute(
    args: &TaskMoveArgs,
    project: Option<&str>,
    resolver: &TasksDirectoryResolver,
    renderer: &OutputRenderer,
) -> Result<(), String> {
    let mut ctx = TaskCommandContext::new(resolver, project, args.ids.first().map(String::as_str))?;
    if let Some(new_id) = args.record.as_deref() {
        return record(&mut ctx, args, new_id, project, renderer);
    }
    let target = args.to.as_deref().unwrap_or_default().trim();
    if target.is_empty() {
        return Err("Name the project to move to: --to <PROJECT>".to_string());
    }

    let mut ids: Vec<String> = Vec::new();
    for raw in &args.ids {
        ids.push(ctx.resolve_full_task_id(raw, project)?);
    }
    if let Some(select) = args.select.as_deref() {
        let mut search = String::new();
        if let Some(prefix) = project {
            search.push_str(&format!(
                "project={} ",
                ctx.project_prefix_for(Some(prefix))
            ));
        }
        search.push_str(select);
        let selected = crate::services::task_list::select_ids(
            &ctx.storage,
            &ctx.config,
            ctx.storage_root(),
            &search,
        )
        .map_err(|e| e.cli_message())?;
        ids.extend(selected);
    } else if ids.is_empty() {
        return Err(
            "Name the tasks to move: task IDs, or --select '<query>' (e.g. --select 'tag:web')"
                .to_string(),
        );
    }
    let mut seen = std::collections::HashSet::new();
    ids.retain(|id| seen.insert(id.clone()));

    let custom_fields = (!args.fields.is_empty()).then(|| {
        args.fields
            .iter()
            .map(|(key, value)| {
                (
                    key.clone(),
                    crate::types::custom_value_string(value.clone()),
                )
            })
            .collect()
    });

    let mut moved: Vec<TaskMoveResponse> = Vec::new();
    let mut failed: Vec<(String, String)> = Vec::new();
    for id in &ids {
        let request = TaskMoveRequest {
            id: id.clone(),
            project: target.to_string(),
            status: args.status.clone(),
            task_type: args.task_type.clone(),
            priority: args.priority.clone(),
            reporter: args.reporter.clone(),
            assignee: args.assignee.clone(),
            custom_fields: custom_fields.clone(),
            dry_run: args.dry_run,
        };
        match TaskService::move_task(&mut ctx.storage, &request) {
            Ok(response) => moved.push(response),
            Err(error) => failed.push((id.clone(), error.to_string())),
        }
    }

    match renderer.format {
        OutputFormat::Json => {
            renderer.emit_json(&serde_json::json!({
                "status": if failed.is_empty() { "ok" } else { "error" },
                "action": "task.move",
                "dry_run": args.dry_run,
                "moved": moved
                    .iter()
                    .map(|m| serde_json::json!({"from": m.from, "to": m.to, "changes": m.changes}))
                    .collect::<Vec<_>>(),
                "failed": failed
                    .iter()
                    .map(|(id, error)| serde_json::json!({"id": id, "error": error}))
                    .collect::<Vec<_>>(),
            }));
        }
        _ => {
            if ids.is_empty() {
                renderer.emit_info(format_args!("No tasks match; nothing to move."));
            }
            for m in &moved {
                if args.dry_run {
                    renderer.emit_info(format_args!("DRY RUN: Would move {} to {}", m.from, m.to));
                } else {
                    renderer.emit_success(format_args!("Moved {} to {}", m.from, m.to));
                }
                for change in &m.changes {
                    renderer.emit_info(format_args!(
                        "  {}: {} -> {}",
                        change.field.strip_prefix("field:").unwrap_or(&change.field),
                        change.old.as_deref().unwrap_or("(none)"),
                        change.new.as_deref().unwrap_or("(none)")
                    ));
                }
            }
            for (id, error) in &failed {
                renderer.emit_error(format_args!("Cannot move {id}: {error}"));
            }
        }
    }
    if failed.is_empty() {
        Ok(())
    } else {
        Err(format!("{} task(s) could not be moved", failed.len()))
    }
}

/// `lotar task move OLD --record NEW`: the file was moved by hand; write the
/// redirect only. The old ID cannot be resolved (its file is gone), so it is
/// taken as typed, with the project prefix added to a bare number.
fn record(
    ctx: &mut TaskCommandContext,
    args: &TaskMoveArgs,
    new_id: &str,
    project: Option<&str>,
    renderer: &OutputRenderer,
) -> Result<(), String> {
    let [old] = args.ids.as_slice() else {
        return Err("--record takes exactly one old task ID".to_string());
    };
    let old = if old.chars().all(|c| c.is_ascii_digit()) {
        format!("{}-{}", ctx.project_prefix_for(project), old)
    } else {
        old.clone()
    };
    let new_id = ctx.resolve_full_task_id(new_id, project)?;
    let changed = TaskService::record_move(&mut ctx.storage, &old, &new_id)
        .map_err(|error| error.to_string())?;
    match renderer.format {
        OutputFormat::Json => renderer.emit_json(&serde_json::json!({
            "status": "ok",
            "action": "task.move.record",
            "from": old,
            "to": new_id,
            "changed": changed,
        })),
        _ if changed => {
            renderer.emit_success(format_args!("Recorded the move of {old} to {new_id}"))
        }
        _ => renderer.emit_info(format_args!("{old} already redirects to {new_id}")),
    }
    Ok(())
}

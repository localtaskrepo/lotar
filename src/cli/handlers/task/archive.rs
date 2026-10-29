use crate::cli::TaskArchiveArgs;
use crate::cli::handlers::task::context::TaskCommandContext;
use crate::output::{OutputFormat, OutputRenderer};
use crate::services::task_service::TaskService;
use crate::workspace::TasksDirectoryResolver;

/// `lotar task archive` / `unarchive` (DEV-4): move tasks into the project's
/// `@archive` folder, where they are read-only, or bring them back. Tasks
/// come from IDs, a `--select` query, or both.
pub(crate) fn execute(
    args: &TaskArchiveArgs,
    archive: bool,
    project: Option<&str>,
    resolver: &TasksDirectoryResolver,
    renderer: &OutputRenderer,
) -> Result<(), String> {
    let verb = if archive { "archive" } else { "unarchive" };
    let mut ctx = TaskCommandContext::new(resolver, project, args.ids.first().map(String::as_str))?;

    let mut ids: Vec<String> = Vec::new();
    for raw in &args.ids {
        ids.push(ctx.resolve_full_task_id(raw, project)?);
    }
    if let Some(select) = args.select.as_deref() {
        // Unarchive looks inside the archive; the query can still widen it.
        let mut search = String::new();
        if !archive {
            search.push_str("archived=yes ");
        }
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
        return Err(format!(
            "Name the tasks to {verb}: task IDs, or --select '<query>' (e.g. --select 'status:done -recent:30d')"
        ));
    }
    let mut seen = std::collections::HashSet::new();
    ids.retain(|id| seen.insert(id.clone()));

    let mut changed = Vec::new();
    let mut unchanged = Vec::new();
    let mut failed: Vec<(String, String)> = Vec::new();
    for id in &ids {
        if args.dry_run {
            let prefix = crate::storage::TaskId::parse(id)
                .map(|parsed| parsed.project)
                .unwrap_or_default();
            match ctx.storage.get(id, &prefix) {
                Some(task) if task.archived != archive => changed.push(id.clone()),
                Some(_) => unchanged.push(id.clone()),
                None => failed.push((id.clone(), "task not found".into())),
            }
            continue;
        }
        match TaskService::archive_with_receipt(&mut ctx.storage, id, None, archive) {
            Ok(receipt) if receipt.changed => changed.push(id.clone()),
            Ok(_) => unchanged.push(id.clone()),
            Err(error) => failed.push((id.clone(), error.to_string())),
        }
    }

    let done = if archive { "Archived" } else { "Unarchived" };
    let state = if archive { "archived" } else { "not archived" };
    match renderer.format {
        OutputFormat::Json => {
            renderer.emit_json(&serde_json::json!({
                "status": if failed.is_empty() { "ok" } else { "error" },
                "action": format!("task.{verb}"),
                "dry_run": args.dry_run,
                "changed": changed,
                "unchanged": unchanged,
                "failed": failed
                    .iter()
                    .map(|(id, error)| serde_json::json!({"id": id, "error": error}))
                    .collect::<Vec<_>>(),
            }));
        }
        _ => {
            if ids.is_empty() {
                renderer.emit_info(format_args!("No tasks match; nothing to {verb}."));
            }
            for id in &changed {
                if args.dry_run {
                    renderer.emit_info(format_args!("DRY RUN: Would {verb} {id}"));
                } else {
                    renderer.emit_success(format_args!("{done} {id}"));
                }
            }
            for id in &unchanged {
                renderer.emit_info(format_args!("{id} is already {state}"));
            }
            for (id, error) in &failed {
                renderer.emit_error(format_args!("Cannot {verb} {id}: {error}"));
            }
        }
    }
    if failed.is_empty() {
        Ok(())
    } else {
        Err(format!("{} task(s) could not be {verb}d", failed.len()))
    }
}

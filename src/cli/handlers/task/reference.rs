use crate::cli::args::{
    TaskReferenceAction, TaskReferenceArgs, TaskReferenceKindAdd, TaskReferenceKindRemove,
};
use crate::cli::handlers::task::context::TaskCommandContext;
use crate::cli::handlers::task::errors::TaskStorageAction;
use crate::cli::handlers::task::mutation::load_task;
use crate::output::{OutputFormat, OutputRenderer};
use crate::services::reference_service::ReferenceService;
use crate::utils::git::find_repo_root;
use crate::workspace::TasksDirectoryResolver;
use serde_json::json;

pub fn handle_reference(
    args: TaskReferenceArgs,
    project: Option<&str>,
    resolver: &TasksDirectoryResolver,
    renderer: &OutputRenderer,
) -> Result<(), String> {
    match args.action {
        TaskReferenceAction::Add(add_args) => match add_args.kind {
            TaskReferenceKindAdd::Link { id, url } => {
                handle_add_link(&id, &url, project, resolver, renderer)
            }
            TaskReferenceKindAdd::File { id, path } => {
                handle_add_file(&id, &path, project, resolver, renderer)
            }
            TaskReferenceKindAdd::Code { id, code } => {
                handle_add_code(&id, &code, project, resolver, renderer)
            }
        },
        TaskReferenceAction::Remove(remove_args) => match remove_args.kind {
            TaskReferenceKindRemove::Link { id, url } => {
                handle_remove_link(&id, &url, project, resolver, renderer)
            }
            TaskReferenceKindRemove::File { id, path } => {
                handle_remove_file(&id, &path, project, resolver, renderer)
            }
            TaskReferenceKindRemove::Code { id, code } => {
                handle_remove_code(&id, &code, project, resolver, renderer)
            }
        },
    }
}

fn handle_add_link(
    task_id: &str,
    url: &str,
    project: Option<&str>,
    resolver: &TasksDirectoryResolver,
    renderer: &OutputRenderer,
) -> Result<(), String> {
    let mut ctx = TaskCommandContext::new(resolver, project, Some(task_id))?;
    let loaded = load_task(&mut ctx, task_id, project)?;

    let (task, added) =
        ReferenceService::attach_link_reference(&mut ctx.storage, &loaded.full_id, url)
            .map_err(|e| e.to_string())?;

    emit_reference_result(renderer, "add", "link", &loaded.full_id, url, added, task)
}

fn handle_remove_link(
    task_id: &str,
    url: &str,
    project: Option<&str>,
    resolver: &TasksDirectoryResolver,
    renderer: &OutputRenderer,
) -> Result<(), String> {
    let mut ctx = TaskCommandContext::new(resolver, project, Some(task_id))?;
    let loaded = load_task(&mut ctx, task_id, project)?;

    let (task, removed) =
        ReferenceService::detach_link_reference(&mut ctx.storage, &loaded.full_id, url)
            .map_err(|e| e.to_string())?;

    emit_reference_result(
        renderer,
        "remove",
        "link",
        &loaded.full_id,
        url,
        removed,
        task,
    )
}

fn handle_add_code(
    task_id: &str,
    code: &str,
    project: Option<&str>,
    resolver: &TasksDirectoryResolver,
    renderer: &OutputRenderer,
) -> Result<(), String> {
    let mut ctx = TaskCommandContext::new(resolver, project, Some(task_id))?;
    let loaded = load_task(&mut ctx, task_id, project)?;

    let repo_root = find_repo_root(ctx.storage_root())
        .ok_or_else(|| "Unable to locate git repository".to_string())?;

    let (task, added) = ReferenceService::attach_code_reference(
        &mut ctx.storage,
        &repo_root,
        &loaded.full_id,
        code,
    )
    .map_err(|e| e.to_string())?;

    emit_reference_result(renderer, "add", "code", &loaded.full_id, code, added, task)
}

fn handle_remove_code(
    task_id: &str,
    code: &str,
    project: Option<&str>,
    resolver: &TasksDirectoryResolver,
    renderer: &OutputRenderer,
) -> Result<(), String> {
    let mut ctx = TaskCommandContext::new(resolver, project, Some(task_id))?;
    let loaded = load_task(&mut ctx, task_id, project)?;

    let (task, removed) =
        ReferenceService::detach_code_reference(&mut ctx.storage, &loaded.full_id, code)
            .map_err(|e| e.to_string())?;

    emit_reference_result(
        renderer,
        "remove",
        "code",
        &loaded.full_id,
        code,
        removed,
        task,
    )
}

fn handle_add_file(
    task_id: &str,
    path: &str,
    project: Option<&str>,
    resolver: &TasksDirectoryResolver,
    renderer: &OutputRenderer,
) -> Result<(), String> {
    let mut ctx = TaskCommandContext::new(resolver, project, Some(task_id))?;
    let loaded = load_task(&mut ctx, task_id, project)?;

    let repo_root = find_repo_root(ctx.storage_root())
        .ok_or_else(|| "Unable to locate git repository".to_string())?;

    let (task, added) = ReferenceService::attach_file_reference(
        &mut ctx.storage,
        &repo_root,
        &loaded.full_id,
        path,
    )
    .map_err(|e| e.to_string())?;

    emit_reference_result(renderer, "add", "file", &loaded.full_id, path, added, task)
}

fn handle_remove_file(
    task_id: &str,
    path: &str,
    project: Option<&str>,
    resolver: &TasksDirectoryResolver,
    renderer: &OutputRenderer,
) -> Result<(), String> {
    let mut ctx = TaskCommandContext::new(resolver, project, Some(task_id))?;
    let loaded = load_task(&mut ctx, task_id, project)?;

    let repo_root = find_repo_root(ctx.storage_root())
        .ok_or_else(|| "Unable to locate git repository".to_string())?;

    let (task, removed) = ReferenceService::detach_file_reference(
        &mut ctx.storage,
        &repo_root,
        &loaded.full_id,
        path,
    )
    .map_err(|e| e.to_string())?;

    emit_reference_result(
        renderer,
        "remove",
        "file",
        &loaded.full_id,
        path,
        removed,
        task,
    )
}

fn emit_reference_result(
    renderer: &OutputRenderer,
    action: &str,
    kind: &str,
    task_id: &str,
    value: &str,
    changed: bool,
    task: crate::api_types::TaskDTO,
) -> Result<(), String> {
    match renderer.format {
        OutputFormat::Json => {
            let payload = json!({
                "action": action,
                "kind": kind,
                "task_id": task_id,
                "value": value,
                "changed": changed,
                "task": task,
            });
            renderer.emit_json(&payload);
        }
        _ => {
            if changed {
                renderer.emit_success(format_args!(
                    "{}: {} reference updated for {}",
                    action, kind, task_id
                ));
            } else {
                renderer.emit_info(format_args!(
                    "{}: {} reference already in desired state for {}",
                    action, kind, task_id
                ));
            }
        }
    }

    renderer.log_info(format_args!(
        "task.reference: action={} kind={} task_id={} changed={}",
        action, kind, task_id, changed
    ));

    // Ensure storage changes are flushed via drop; no explicit action required.
    // Keep a no-op reference to TaskStorageAction to satisfy consistency with other mutation handlers.
    let _ = TaskStorageAction::Update;

    Ok(())
}

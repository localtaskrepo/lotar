use crate::cli::args::{
    TaskReferenceAction, TaskReferenceArgs, TaskReferenceKindAdd, TaskReferenceKindRemove,
};
use crate::cli::handlers::task::context::TaskCommandContext;
use crate::cli::handlers::task::errors::TaskStorageAction;
use crate::cli::handlers::task::mutation::load_task;
use crate::output::{OutputFormat, OutputRenderer};
use crate::services::attachment_service::AttachmentService;
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
            TaskReferenceKindAdd::Attachment { id, name } => {
                handle_add_attachment(&id, &name, project, resolver, renderer)
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
            TaskReferenceKindRemove::Attachment { id, name } => {
                handle_remove_attachment(&id, &name, project, resolver, renderer)
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

    emit_reference_result(renderer, "add", "link", &loaded.full_id, url, added, &task)
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
        &task,
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

    emit_reference_result(renderer, "add", "code", &loaded.full_id, code, added, &task)
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
        &task,
    )
}

/// Resolve the attachments store root configured for the task's project.
fn attachments_root_for(storage_root: &std::path::Path, task_id: &str) -> std::path::PathBuf {
    let project = crate::storage::TaskId::parse(task_id)
        .ok()
        .map(|parsed| parsed.project);
    let config = crate::config::resolution::config_for_project(storage_root, project.as_deref())
        .unwrap_or_else(|_| {
            crate::config::types::ResolvedConfig::from_global(
                crate::config::types::GlobalConfig::default(),
            )
        });
    AttachmentService::resolve_attachments_root(storage_root, &config)
        .unwrap_or_else(|_| storage_root.join("@attachments"))
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

    // Repository file references can never target the attachments store
    // (the service guard rejects store paths), so no store lock is taken.
    let (task, added) = ReferenceService::attach_file_reference(
        &mut ctx.storage,
        &repo_root,
        &loaded.full_id,
        path,
    )
    .map_err(|e| e.to_string())?;

    emit_reference_result(renderer, "add", "file", &loaded.full_id, path, added, &task)
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

    // Repository file references can never target the attachments store
    // (the service guard rejects store paths), so no store lock is taken.
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
        &task,
    )
}

/// Managed attachment references serialize with upload/remove reclamation
/// under the store coordination lock (store-lock -> task-lock order).
fn handle_add_attachment(
    task_id: &str,
    name: &str,
    project: Option<&str>,
    resolver: &TasksDirectoryResolver,
    renderer: &OutputRenderer,
) -> Result<(), String> {
    let mut ctx = TaskCommandContext::new(resolver, project, Some(task_id))?;
    let loaded = load_task(&mut ctx, task_id, project)?;

    let root = attachments_root_for(ctx.storage_root(), &loaded.full_id);
    let _store_guard = AttachmentService::lock_store(&root).map_err(|e| e.to_string())?;
    // Fail closed when the named blob is not present in the store.
    AttachmentService::resolve_attachment_path(&root, name).map_err(|e| e.to_string())?;

    let (task, added) =
        AttachmentService::attach_managed_reference(&mut ctx.storage, &loaded.full_id, name)
            .map_err(|e| e.to_string())?;

    emit_reference_result(
        renderer,
        "add",
        "attachment",
        &loaded.full_id,
        name,
        added,
        &task,
    )
}

/// Managed attachment reference detach; store lock held for parity with
/// upload/remove reclamation, though detach itself never deletes blobs.
fn handle_remove_attachment(
    task_id: &str,
    name: &str,
    project: Option<&str>,
    resolver: &TasksDirectoryResolver,
    renderer: &OutputRenderer,
) -> Result<(), String> {
    let mut ctx = TaskCommandContext::new(resolver, project, Some(task_id))?;
    let loaded = load_task(&mut ctx, task_id, project)?;

    let root = attachments_root_for(ctx.storage_root(), &loaded.full_id);
    let _store_guard = AttachmentService::lock_store(&root).map_err(|e| e.to_string())?;

    let (task, removed) =
        AttachmentService::detach_managed_reference(&mut ctx.storage, &loaded.full_id, name)
            .map_err(|e| e.to_string())?;

    emit_reference_result(
        renderer,
        "remove",
        "attachment",
        &loaded.full_id,
        name,
        removed,
        &task,
    )
}

fn emit_reference_result(
    renderer: &OutputRenderer,
    action: &str,
    kind: &str,
    task_id: &str,
    value: &str,
    changed: bool,
    task: &crate::api_types::TaskDTO,
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

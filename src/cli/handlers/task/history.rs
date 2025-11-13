use crate::cli::args::task::HistoryField;
use crate::output::OutputRenderer;
use crate::utils::project::resolve_project_input;
use crate::workspace::TasksDirectoryResolver;
use std::path::PathBuf;

pub fn handle_history(
    id: String,
    limit: usize,
    project: Option<&str>,
    resolver: &TasksDirectoryResolver,
    renderer: &OutputRenderer,
) -> Result<(), String> {
    let context = resolve_task_file(&id, project, resolver)?;

    let commits = crate::services::audit_service::AuditService::list_commits_for_file(
        &context.repo_root,
        &context.file_repo_path,
    )?;
    let limited = commits.into_iter().take(limit).collect::<Vec<_>>();
    match renderer.format {
        crate::output::OutputFormat::Json => {
            let items: Vec<_> = limited
                .iter()
                .map(|c| {
                    serde_json::json!({
                        "commit": c.commit,
                        "author": c.author,
                        "email": c.email,
                        "date": c.date.to_rfc3339(),
                        "message": c.message,
                    })
                })
                .collect();
            let obj = serde_json::json!({
                "status": "ok",
                "action": "task.history",
                "id": id,
                "project": context.project_prefix,
                "count": items.len(),
                "items": items
            });
            renderer.emit_json(&obj);
        }
        _ => {
            if limited.is_empty() {
                renderer.emit_success("No history for this task.");
            } else {
                for c in &limited {
                    renderer.emit_raw_stdout(&format!(
                        "{}  {} <{}>  {}",
                        c.date.to_rfc3339(),
                        c.author,
                        c.email,
                        c.commit
                    ));
                    if !c.message.is_empty() {
                        renderer.emit_raw_stdout(&format!("    {}", c.message));
                    }
                }
            }
        }
    }
    Ok(())
}

pub fn handle_history_by_field(
    field: HistoryField,
    id: String,
    limit: usize,
    project: Option<&str>,
    resolver: &TasksDirectoryResolver,
    renderer: &OutputRenderer,
) -> Result<(), String> {
    let context = resolve_task_file(&id, project, resolver)?;

    let commits = crate::services::audit_service::AuditService::list_commits_for_file(
        &context.repo_root,
        &context.file_repo_path,
    )?;

    #[derive(Clone)]
    struct TaskSnapshot {
        status: Option<crate::types::TaskStatus>,
        priority: Option<crate::types::Priority>,
        assignee: Option<String>,
        tags: Vec<String>,
    }

    let mut snapshots: Vec<(String, TaskSnapshot)> = Vec::with_capacity(commits.len());
    for c in &commits {
        if let Ok(content) = crate::services::audit_service::AuditService::show_file_at(
            &context.repo_root,
            &c.commit,
            &context.file_repo_path,
        ) {
            if let Ok(task) = serde_yaml::from_str::<crate::storage::task::Task>(&content) {
                snapshots.push((
                    c.commit.clone(),
                    TaskSnapshot {
                        status: Some(task.status),
                        priority: Some(task.priority),
                        assignee: task.assignee,
                        tags: task.tags,
                    },
                ));
            }
        }
    }

    let mut changes: Vec<serde_json::Value> = Vec::new();
    for w in snapshots.windows(2) {
        let (commit_new, snap_new) = &w[0];
        let (_commit_old, snap_old) = &w[1];
        match field {
            HistoryField::Status => {
                if snap_new.status != snap_old.status {
                    changes.push(serde_json::json!({
                        "field": "status",
                        "new": snap_new.status.as_ref().map(|s| s.to_string()),
                        "old": snap_old.status.as_ref().map(|s| s.to_string()),
                        "commit": commit_new,
                    }));
                }
            }
            HistoryField::Priority => {
                if snap_new.priority != snap_old.priority {
                    changes.push(serde_json::json!({
                        "field": "priority",
                        "new": snap_new.priority.as_ref().map(|s| s.to_string()),
                        "old": snap_old.priority.as_ref().map(|s| s.to_string()),
                        "commit": commit_new,
                    }));
                }
            }
            HistoryField::Assignee => {
                if snap_new.assignee != snap_old.assignee {
                    changes.push(serde_json::json!({
                        "field": "assignee",
                        "new": snap_new.assignee,
                        "old": snap_old.assignee,
                        "commit": commit_new,
                    }));
                }
            }
            HistoryField::Tags => {
                if snap_new.tags != snap_old.tags {
                    changes.push(serde_json::json!({
                        "field": "tags",
                        "new": snap_new.tags,
                        "old": snap_old.tags,
                        "commit": commit_new,
                    }));
                }
            }
        }
    }

    let limited: Vec<_> = changes.into_iter().take(limit).collect();
    match renderer.format {
        crate::output::OutputFormat::Json => {
            let obj = serde_json::json!({
                "status": "ok",
                "action": "task.history_field",
                "field": match field {
                    HistoryField::Status => "status",
                    HistoryField::Priority => "priority",
                    HistoryField::Assignee => "assignee",
                    HistoryField::Tags => "tags",
                },
                "id": id,
                "project": context.project_prefix,
                "count": limited.len(),
                "items": limited,
            });
            renderer.emit_json(&obj);
        }
        _ => {
            if limited.is_empty() {
                renderer.emit_success("No changes detected for the selected field.");
            } else {
                for ch in &limited {
                    renderer.emit_raw_stdout(&format!("{}", ch));
                }
            }
        }
    }
    Ok(())
}

pub fn handle_diff(
    id: String,
    commit: Option<String>,
    fields: bool,
    project: Option<&str>,
    resolver: &TasksDirectoryResolver,
    renderer: &OutputRenderer,
) -> Result<(), String> {
    let context = resolve_task_file(&id, project, resolver)?;

    let commit_sha = if let Some(c) = commit {
        c
    } else {
        let commits = crate::services::audit_service::AuditService::list_commits_for_file(
            &context.repo_root,
            &context.file_repo_path,
        )?;
        commits
            .first()
            .map(|c| c.commit.clone())
            .ok_or_else(|| "No commits for this task file".to_string())?
    };

    if fields {
        let parent_commit = {
            let commits = crate::services::audit_service::AuditService::list_commits_for_file(
                &context.repo_root,
                &context.file_repo_path,
            )?;
            commits.get(1).map(|c| c.commit.clone())
        };
        let current = crate::services::audit_service::AuditService::show_file_at(
            &context.repo_root,
            &commit_sha,
            &context.file_repo_path,
        )?;
        let prev = if let Some(pc) = parent_commit {
            crate::services::audit_service::AuditService::show_file_at(
                &context.repo_root,
                &pc,
                &context.file_repo_path,
            )
            .ok()
        } else {
            None
        };

        let cur_task: Option<crate::storage::task::Task> = serde_yaml::from_str(&current).ok();
        let prev_task: Option<crate::storage::task::Task> =
            prev.as_deref().and_then(|s| serde_yaml::from_str(s).ok());

        let mut deltas = serde_json::Map::new();
        let mut push_change = |k: &str, old: serde_json::Value, new: serde_json::Value| {
            if old != new {
                deltas.insert(k.to_string(), serde_json::json!({"old": old, "new": new}));
            }
        };
        if let (Some(cur), Some(prev)) = (cur_task.as_ref(), prev_task.as_ref()) {
            push_change(
                "title",
                serde_json::json!(prev.title),
                serde_json::json!(cur.title),
            );
            push_change(
                "status",
                serde_json::json!(prev.status.to_string()),
                serde_json::json!(cur.status.to_string()),
            );
            push_change(
                "priority",
                serde_json::json!(prev.priority.to_string()),
                serde_json::json!(cur.priority.to_string()),
            );
            push_change(
                "task_type",
                serde_json::json!(prev.task_type.to_string()),
                serde_json::json!(cur.task_type.to_string()),
            );
            push_change(
                "assignee",
                serde_json::json!(prev.assignee),
                serde_json::json!(cur.assignee),
            );
            push_change(
                "reporter",
                serde_json::json!(prev.reporter),
                serde_json::json!(cur.reporter),
            );
            push_change(
                "due_date",
                serde_json::json!(prev.due_date),
                serde_json::json!(cur.due_date),
            );
            push_change(
                "effort",
                serde_json::json!(prev.effort),
                serde_json::json!(cur.effort),
            );
            push_change(
                "tags",
                serde_json::json!(prev.tags),
                serde_json::json!(cur.tags),
            );
            push_change(
                "description",
                serde_json::json!(prev.description),
                serde_json::json!(cur.description),
            );
            push_change(
                "relationships",
                serde_json::json!(prev.relationships),
                serde_json::json!(cur.relationships),
            );
            push_change(
                "custom_fields",
                serde_json::json!(prev.custom_fields),
                serde_json::json!(cur.custom_fields),
            );
            push_change(
                "sprints",
                serde_json::json!(prev.sprints),
                serde_json::json!(cur.sprints),
            );
        }
        let structured_available = cur_task.is_some() && prev_task.is_some();
        let result = serde_json::Value::Object(deltas);
        match renderer.format {
            crate::output::OutputFormat::Json => {
                let obj = serde_json::json!({
                    "status": "ok",
                    "action": "task.diff",
                    "mode": "fields",
                    "id": id,
                    "project": context.project_prefix,
                    "commit": commit_sha,
                    "diff": result
                });
                renderer.emit_json(&obj);
            }
            _ => {
                if !structured_available {
                    renderer.emit_raw_stdout(&result.to_string());
                    return Ok(());
                }

                if let Some(map) = result.as_object() {
                    if map.is_empty() {
                        renderer
                            .emit_success("No field-level changes detected between these commits.");
                        return Ok(());
                    }

                    renderer.emit_info(&format!("Field differences for {} @ {}:", id, commit_sha));

                    let mut entries: Vec<_> = map.iter().collect();
                    entries.sort_by(|a, b| a.0.cmp(b.0));
                    for (field, change) in entries {
                        if let Some(obj) = change.as_object() {
                            let old = obj.get("old").unwrap_or(&serde_json::Value::Null);
                            let new = obj.get("new").unwrap_or(&serde_json::Value::Null);
                            renderer.emit_raw_stdout(&format!("{}:", field));
                            renderer
                                .emit_raw_stdout(&format!("  - old: {}", format_diff_value(old)));
                            renderer
                                .emit_raw_stdout(&format!("  + new: {}", format_diff_value(new)));
                        }
                    }
                } else {
                    renderer.emit_raw_stdout(&result.to_string());
                }
            }
        }
    } else {
        let patch = crate::services::audit_service::AuditService::show_file_diff(
            &context.repo_root,
            &commit_sha,
            &context.file_repo_path,
        )?;
        match renderer.format {
            crate::output::OutputFormat::Json => {
                let obj = serde_json::json!({
                    "status": "ok",
                    "action": "task.diff",
                    "id": id,
                    "project": context.project_prefix,
                    "commit": commit_sha,
                    "patch": patch
                });
                renderer.emit_json(&obj);
            }
            _ => renderer.emit_raw_stdout(&patch),
        }
    }
    Ok(())
}

fn format_diff_value(value: &serde_json::Value) -> String {
    match value {
        serde_json::Value::Null => "none".to_string(),
        serde_json::Value::Bool(b) => b.to_string(),
        serde_json::Value::Number(n) => n.to_string(),
        serde_json::Value::String(s) => {
            if s.is_empty() {
                "''".to_string()
            } else {
                s.to_string()
            }
        }
        serde_json::Value::Array(items) => {
            if items.is_empty() {
                "[]".to_string()
            } else {
                let mut buffer = String::from("[");
                for (idx, item) in items.iter().enumerate() {
                    if idx > 0 {
                        buffer.push_str(", ");
                    }
                    buffer.push_str(&format_diff_value(item));
                }
                buffer.push(']');
                buffer
            }
        }
        serde_json::Value::Object(_) => {
            serde_json::to_string(value).unwrap_or_else(|_| "{...}".to_string())
        }
    }
}

pub fn handle_at(
    id: String,
    commit: String,
    project: Option<&str>,
    resolver: &TasksDirectoryResolver,
    renderer: &OutputRenderer,
) -> Result<(), String> {
    let context = resolve_task_file(&id, project, resolver)?;

    let content = crate::services::audit_service::AuditService::show_file_at(
        &context.repo_root,
        &commit,
        &context.file_repo_path,
    )?;
    match renderer.format {
        crate::output::OutputFormat::Json => {
            let obj = serde_json::json!({
                "status": "ok",
                "action": "task.at",
                "id": id,
                "project": context.project_prefix,
                "commit": commit,
                "content": content
            });
            renderer.emit_json(&obj);
        }
        _ => renderer.emit_raw_stdout(&content),
    }
    Ok(())
}

struct TaskFileContext {
    project_prefix: String,
    repo_root: PathBuf,
    file_repo_path: PathBuf,
}

fn resolve_task_file(
    id: &str,
    project: Option<&str>,
    resolver: &TasksDirectoryResolver,
) -> Result<TaskFileContext, String> {
    let default_proj = project
        .map(|p| resolve_project_input(p, resolver.path.as_path()))
        .unwrap_or_else(|| crate::project::get_effective_project_name(resolver));
    let proj_prefix = crate::storage::operations::StorageOperations::get_project_for_task(id)
        .unwrap_or(default_proj.clone());
    let file_rel = crate::storage::operations::StorageOperations::get_file_path_for_id(
        &resolver.path.join(&proj_prefix),
        id,
    )
    .ok_or_else(|| "Task file not found".to_string())?;

    let cwd = std::env::current_dir().map_err(|e| e.to_string())?;
    let repo_root = crate::utils::git::find_repo_root(&cwd)
        .ok_or_else(|| "Not in a git repository".to_string())?;
    let tasks_abs = resolver.path.clone();
    let tasks_rel = if tasks_abs.starts_with(&repo_root) {
        tasks_abs.strip_prefix(&repo_root).unwrap().to_path_buf()
    } else {
        tasks_abs
    };
    let file_rel_to_repo =
        tasks_rel.join(file_rel.strip_prefix(&resolver.path).unwrap_or(&file_rel));

    Ok(TaskFileContext {
        project_prefix: proj_prefix,
        repo_root,
        file_repo_path: file_rel_to_repo,
    })
}

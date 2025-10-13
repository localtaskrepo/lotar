use crate::cli::handlers::comment::{CommentArgs, CommentHandler};
use crate::cli::handlers::priority::{PriorityArgs, PriorityHandler};
use crate::cli::handlers::status::{StatusArgs as StatusHandlerArgs, StatusHandler};
use crate::cli::handlers::{AddHandler, CommandHandler};
use crate::cli::handlers::{
    assignee::{AssigneeArgs, AssigneeHandler},
    duedate::{DueDateArgs, DueDateHandler},
};
use crate::cli::project::ProjectResolver;
use crate::cli::validation::CliValidator;
use crate::cli::{TaskAction, TaskDeleteArgs, TaskEditArgs, TaskSearchArgs};
use crate::storage::{TaskFilter, manager::Storage, task::Task};
use crate::utils::project::resolve_project_input;
use crate::workspace::TasksDirectoryResolver;

/// Handler for all task subcommands
pub struct TaskHandler;

impl CommandHandler for TaskHandler {
    type Args = TaskAction;
    type Result = Result<(), String>;

    fn execute(
        args: Self::Args,
        project: Option<&str>,
        resolver: &TasksDirectoryResolver,
        renderer: &crate::output::OutputRenderer,
    ) -> Self::Result {
        match args {
            TaskAction::Effort(effort_args) => {
                let args = crate::cli::handlers::effort::EffortArgs {
                    task_id: effort_args.id,
                    new_effort: effort_args.effort,
                    clear: effort_args.clear,
                    dry_run: effort_args.dry_run,
                    explain: effort_args.explain,
                };
                crate::cli::handlers::effort::EffortHandler::execute(
                    args, project, resolver, renderer,
                )
            }
            TaskAction::Add(add_args) => {
                let cli_add_args = crate::cli::AddArgs {
                    title: add_args.title,
                    task_type: add_args.task_type,
                    priority: add_args.priority,
                    assignee: add_args.assignee,
                    effort: add_args.effort,
                    due: add_args.due,
                    description: add_args.description,
                    category: add_args.category,
                    tags: add_args.tags,
                    fields: add_args.fields,
                    bug: false,
                    epic: false,
                    critical: false,
                    high: false,
                    dry_run: false,
                    explain: false,
                };

                match AddHandler::execute(cli_add_args, project, resolver, renderer) {
                    Ok(task_id) => {
                        // Use the shared output rendering function
                        AddHandler::render_add_success(&task_id, project, resolver, renderer);
                        Ok(())
                    }
                    Err(e) => Err(e),
                }
            }
            TaskAction::List(args) => SearchHandler::execute(args, project, resolver, renderer),
            TaskAction::Edit(edit_args) => {
                EditHandler::execute(edit_args, project, resolver, renderer)
            }
            TaskAction::History { id, limit } => {
                // Resolve project and file path
                let default_proj = project
                    .map(|p| resolve_project_input(p, resolver.path.as_path()))
                    .unwrap_or_else(|| crate::project::get_effective_project_name(resolver));
                // Prefer project prefix embedded in the ID (e.g., TEST-1)
                let proj_prefix =
                    crate::storage::operations::StorageOperations::get_project_for_task(&id)
                        .unwrap_or(default_proj.clone());
                let file_rel = crate::storage::operations::StorageOperations::get_file_path_for_id(
                    &resolver.path.join(&proj_prefix),
                    &id,
                )
                .ok_or_else(|| "Task file not found".to_string())?;

                // Compute repo-relative path
                let cwd = std::env::current_dir().map_err(|e| e.to_string())?;
                let repo_root = crate::utils::git::find_repo_root(&cwd)
                    .ok_or_else(|| "Not in a git repository".to_string())?;
                let tasks_abs = resolver.path.clone();
                let tasks_rel = if tasks_abs.starts_with(&repo_root) {
                    tasks_abs.strip_prefix(&repo_root).unwrap().to_path_buf()
                } else {
                    tasks_abs.clone()
                };
                let file_rel_to_repo =
                    tasks_rel.join(file_rel.strip_prefix(&resolver.path).unwrap_or(&file_rel));

                let commits = crate::services::audit_service::AuditService::list_commits_for_file(
                    &repo_root,
                    &file_rel_to_repo,
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
                        let obj = serde_json::json!({"status":"ok","action":"task.history","id":id,"project":proj_prefix,"count":items.len(),"items":items});
                        renderer.emit_raw_stdout(&obj.to_string());
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
            TaskAction::HistoryByField { field, id, limit } => {
                // Resolve project and file path similar to History
                let default_proj = project
                    .map(|p| resolve_project_input(p, resolver.path.as_path()))
                    .unwrap_or_else(|| crate::project::get_effective_project_name(resolver));
                let proj_prefix =
                    crate::storage::operations::StorageOperations::get_project_for_task(&id)
                        .unwrap_or(default_proj.clone());
                let file_rel = crate::storage::operations::StorageOperations::get_file_path_for_id(
                    &resolver.path.join(&proj_prefix),
                    &id,
                )
                .ok_or_else(|| "Task file not found".to_string())?;

                let cwd = std::env::current_dir().map_err(|e| e.to_string())?;
                let repo_root = crate::utils::git::find_repo_root(&cwd)
                    .ok_or_else(|| "Not in a git repository".to_string())?;
                let tasks_abs = resolver.path.clone();
                let tasks_rel = if tasks_abs.starts_with(&repo_root) {
                    tasks_abs.strip_prefix(&repo_root).unwrap().to_path_buf()
                } else {
                    tasks_abs.clone()
                };
                let file_rel_to_repo =
                    tasks_rel.join(file_rel.strip_prefix(&resolver.path).unwrap_or(&file_rel));

                // List commits and load snapshots for diffing
                let commits = crate::services::audit_service::AuditService::list_commits_for_file(
                    &repo_root,
                    &file_rel_to_repo,
                )?;
                // For each commit (newest->oldest), diff this commit vs next to compute property changes
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
                        &repo_root,
                        &c.commit,
                        &file_rel_to_repo,
                    ) {
                        if let Ok(task) =
                            serde_yaml::from_str::<crate::storage::task::Task>(&content)
                        {
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
                // Compute changes when a field value differs from the next snapshot
                let mut changes: Vec<serde_json::Value> = Vec::new();
                for w in snapshots.windows(2) {
                    let (commit_new, snap_new) = &w[0];
                    let (_commit_old, snap_old) = &w[1];
                    match field {
                        crate::cli::args::task::HistoryField::Status => {
                            if snap_new.status != snap_old.status {
                                changes.push(serde_json::json!({
                                    "field": "status",
                                    "new": snap_new.status.as_ref().map(|s| s.to_string()),
                                    "old": snap_old.status.as_ref().map(|s| s.to_string()),
                                    "commit": commit_new,
                                }));
                            }
                        }
                        crate::cli::args::task::HistoryField::Priority => {
                            if snap_new.priority != snap_old.priority {
                                changes.push(serde_json::json!({
                                    "field": "priority",
                                    "new": snap_new.priority.as_ref().map(|s| s.to_string()),
                                    "old": snap_old.priority.as_ref().map(|s| s.to_string()),
                                    "commit": commit_new,
                                }));
                            }
                        }
                        crate::cli::args::task::HistoryField::Assignee => {
                            if snap_new.assignee != snap_old.assignee {
                                changes.push(serde_json::json!({
                                    "field": "assignee",
                                    "new": snap_new.assignee,
                                    "old": snap_old.assignee,
                                    "commit": commit_new,
                                }));
                            }
                        }
                        crate::cli::args::task::HistoryField::Tags => {
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
                // Limit and render
                let limited: Vec<_> = changes.into_iter().take(limit).collect();
                match renderer.format {
                    crate::output::OutputFormat::Json => {
                        let obj = serde_json::json!({
                            "status":"ok",
                            "action":"task.history_field",
                            "field": format!("{}", match field { crate::cli::args::task::HistoryField::Status=>"status", crate::cli::args::task::HistoryField::Priority=>"priority", crate::cli::args::task::HistoryField::Assignee=>"assignee", crate::cli::args::task::HistoryField::Tags=>"tags" }),
                            "id": id,
                            "project": proj_prefix,
                            "count": limited.len(),
                            "items": limited,
                        });
                        renderer.emit_raw_stdout(&obj.to_string());
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
            TaskAction::Diff { id, commit, fields } => {
                let default_proj = project
                    .map(|p| resolve_project_input(p, resolver.path.as_path()))
                    .unwrap_or_else(|| crate::project::get_effective_project_name(resolver));
                let proj_prefix =
                    crate::storage::operations::StorageOperations::get_project_for_task(&id)
                        .unwrap_or(default_proj.clone());
                let file_rel = crate::storage::operations::StorageOperations::get_file_path_for_id(
                    &resolver.path.join(&proj_prefix),
                    &id,
                )
                .ok_or_else(|| "Task file not found".to_string())?;
                let cwd = std::env::current_dir().map_err(|e| e.to_string())?;
                let repo_root = crate::utils::git::find_repo_root(&cwd)
                    .ok_or_else(|| "Not in a git repository".to_string())?;
                let tasks_abs = resolver.path.clone();
                let tasks_rel = if tasks_abs.starts_with(&repo_root) {
                    tasks_abs.strip_prefix(&repo_root).unwrap().to_path_buf()
                } else {
                    tasks_abs.clone()
                };
                let file_rel_to_repo =
                    tasks_rel.join(file_rel.strip_prefix(&resolver.path).unwrap_or(&file_rel));
                let commit_sha = if let Some(c) = commit {
                    c
                } else {
                    let commits =
                        crate::services::audit_service::AuditService::list_commits_for_file(
                            &repo_root,
                            &file_rel_to_repo,
                        )?;
                    commits
                        .first()
                        .map(|c| c.commit.clone())
                        .ok_or_else(|| "No commits for this task file".to_string())?
                };
                if fields {
                    // Load current and parent snapshots and compute a basic field delta
                    // Resolve parent commit affecting this file
                    let parent_commit = {
                        let commits =
                            crate::services::audit_service::AuditService::list_commits_for_file(
                                &repo_root,
                                &file_rel_to_repo,
                            )?;
                        commits.get(1).map(|c| c.commit.clone())
                    };
                    let current = crate::services::audit_service::AuditService::show_file_at(
                        &repo_root,
                        &commit_sha,
                        &file_rel_to_repo,
                    )?;
                    let prev = if let Some(pc) = parent_commit {
                        crate::services::audit_service::AuditService::show_file_at(
                            &repo_root,
                            &pc,
                            &file_rel_to_repo,
                        )
                        .ok()
                    } else {
                        None
                    };

                    let cur_task: Option<crate::storage::task::Task> =
                        serde_yaml::from_str(&current).ok();
                    let prev_task: Option<crate::storage::task::Task> =
                        prev.as_deref().and_then(|s| serde_yaml::from_str(s).ok());

                    let mut deltas = serde_json::Map::new();
                    let mut push_change =
                        |k: &str, old: serde_json::Value, new: serde_json::Value| {
                            if old != new {
                                deltas.insert(
                                    k.to_string(),
                                    serde_json::json!({"old": old, "new": new}),
                                );
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
                            "category",
                            serde_json::json!(prev.category),
                            serde_json::json!(cur.category),
                        );
                        push_change(
                            "tags",
                            serde_json::json!(prev.tags),
                            serde_json::json!(cur.tags),
                        );
                    }
                    let result = serde_json::Value::Object(deltas);
                    match renderer.format {
                        crate::output::OutputFormat::Json => {
                            let obj = serde_json::json!({"status":"ok","action":"task.diff","mode":"fields","id":id,"project":proj_prefix,"commit":commit_sha,"diff":result});
                            renderer.emit_raw_stdout(&obj.to_string());
                        }
                        _ => renderer.emit_raw_stdout(&result.to_string()),
                    }
                } else {
                    let patch = crate::services::audit_service::AuditService::show_file_diff(
                        &repo_root,
                        &commit_sha,
                        &file_rel_to_repo,
                    )?;
                    match renderer.format {
                        crate::output::OutputFormat::Json => {
                            let obj = serde_json::json!({"status":"ok","action":"task.diff","id":id,"project":proj_prefix,"commit":commit_sha,"patch":patch});
                            renderer.emit_raw_stdout(&obj.to_string());
                        }
                        _ => renderer.emit_raw_stdout(&patch),
                    }
                }
                Ok(())
            }
            TaskAction::At { id, commit } => {
                let default_proj = project
                    .map(|p| resolve_project_input(p, resolver.path.as_path()))
                    .unwrap_or_else(|| crate::project::get_effective_project_name(resolver));
                let proj_prefix =
                    crate::storage::operations::StorageOperations::get_project_for_task(&id)
                        .unwrap_or(default_proj.clone());
                let file_rel = crate::storage::operations::StorageOperations::get_file_path_for_id(
                    &resolver.path.join(&proj_prefix),
                    &id,
                )
                .ok_or_else(|| "Task file not found".to_string())?;
                let cwd = std::env::current_dir().map_err(|e| e.to_string())?;
                let repo_root = crate::utils::git::find_repo_root(&cwd)
                    .ok_or_else(|| "Not in a git repository".to_string())?;
                let tasks_abs = resolver.path.clone();
                let tasks_rel = if tasks_abs.starts_with(&repo_root) {
                    tasks_abs.strip_prefix(&repo_root).unwrap().to_path_buf()
                } else {
                    tasks_abs.clone()
                };
                let file_rel_to_repo =
                    tasks_rel.join(file_rel.strip_prefix(&resolver.path).unwrap_or(&file_rel));
                let content = crate::services::audit_service::AuditService::show_file_at(
                    &repo_root,
                    &commit,
                    &file_rel_to_repo,
                )?;
                match renderer.format {
                    crate::output::OutputFormat::Json => {
                        let obj = serde_json::json!({"status":"ok","action":"task.at","id":id,"project":proj_prefix,"commit":commit,"content":content});
                        renderer.emit_raw_stdout(&obj.to_string());
                    }
                    _ => renderer.emit_raw_stdout(&content),
                }
                Ok(())
            }
            TaskAction::Status(status_args) => {
                let handler_args = StatusHandlerArgs::new(
                    status_args.id,
                    Some(status_args.status), // Task subcommand always sets status
                    project.map(|s| s.to_string()),
                );
                StatusHandler::execute(handler_args, project, resolver, renderer)
            }
            TaskAction::Priority { id, priority } => {
                // Handle priority command similar to top-level priority command
                let priority_args = PriorityArgs::new(id, priority, project.map(|s| s.to_string()));
                PriorityHandler::execute(priority_args, project, resolver, renderer)
            }
            TaskAction::Assignee { id, assignee } => {
                let args = AssigneeArgs {
                    task_id: id,
                    new_assignee: assignee,
                };
                AssigneeHandler::execute(args, project, resolver, renderer)
            }
            TaskAction::DueDate { id, due_date } => {
                let args = DueDateArgs {
                    task_id: id,
                    new_due_date: due_date,
                };
                DueDateHandler::execute(args, project, resolver, renderer)
            }
            TaskAction::Delete(delete_args) => {
                DeleteHandler::execute(delete_args, project, resolver, renderer)
            }
            TaskAction::Comment {
                id,
                text,
                message,
                file,
            } => {
                // Resolve comment content from args: file > message > text > stdin
                let resolved_text = if let Some(path) = file {
                    std::fs::read_to_string(&path)
                        .map(|s| s.trim_end_matches(['\n', '\r']).to_string())
                        .unwrap_or_default()
                } else if let Some(m) = message {
                    m
                } else if let Some(t) = text {
                    t
                } else {
                    use std::io::{IsTerminal, Read};
                    let mut buffer = String::new();
                    if !std::io::stdin().is_terminal() {
                        if std::io::stdin().read_to_string(&mut buffer).is_ok() {
                            buffer.trim_end_matches(['\n', '\r']).to_string()
                        } else {
                            String::new()
                        }
                    } else {
                        String::new()
                    }
                };
                let args = CommentArgs {
                    task_id: id,
                    text: if resolved_text.trim().is_empty() {
                        None
                    } else {
                        Some(resolved_text)
                    },
                };
                CommentHandler::execute(args, project, resolver, renderer)
            }
        }
    }
}

/// Handler for editing tasks
pub struct EditHandler;

impl CommandHandler for EditHandler {
    type Args = TaskEditArgs;
    type Result = Result<(), String>;

    fn execute(
        args: Self::Args,
        project: Option<&str>,
        resolver: &TasksDirectoryResolver,
        renderer: &crate::output::OutputRenderer,
    ) -> Self::Result {
        renderer.log_info("edit: begin");
        // Git-like behavior: if a parent tasks root is adopted, write to that parent (no child .tasks creation)
        let mut storage = Storage::new(resolver.path.clone());

        // Create project resolver and validator
        let mut project_resolver = ProjectResolver::new(resolver)
            .map_err(|e| format!("Failed to initialize project resolver: {}", e))?;

        // Resolve project first (needed for project-specific config)
        let effective_project = match project_resolver.resolve_project("", project) {
            Ok(project) => {
                if project.is_empty() {
                    // No default project set, use global config
                    None
                } else {
                    Some(project)
                }
            }
            Err(e) => {
                // Project validation failed - this should be an error, not fallback
                return Err(e);
            }
        };

        // Get appropriate configuration (project-specific or global)
        let config = match &effective_project {
            Some(project_name) => project_resolver
                .get_project_config(project_name)
                .map_err(|e| format!("Failed to get project configuration: {}", e))?,
            None => {
                // Use global config
                project_resolver.get_config().clone()
            }
        };

        let validator = CliValidator::new(&config);

        // Resolve project prefix for loading
        let project_prefix = if let Some(project) = project {
            resolve_project_input(project, resolver.path.as_path())
        } else {
            crate::project::get_effective_project_name(resolver)
        };

        // Load the task
        let mut task = storage
            .get(&args.id, project_prefix.clone())
            .ok_or_else(|| format!("Task '{}' not found", args.id))?;

        // Update fields if provided
        if let Some(title) = args.title {
            task.title = title;
        }

        if let Some(task_type) = args.task_type {
            task.task_type = validator
                .validate_task_type(&task_type)
                .map_err(|e| format!("Task type validation failed: {}", e))?;
        }

        if let Some(priority) = args.priority {
            task.priority = validator
                .validate_priority(&priority)
                .map_err(|e| format!("Priority validation failed: {}", e))?;
        }

        if let Some(assignee) = args.assignee {
            task.assignee = Some(assignee);
        }

        if let Some(effort) = args.effort {
            // Normalize effort to canonical form before persisting
            task.effort = match crate::utils::effort::parse_effort(&effort) {
                Ok(parsed) => Some(parsed.canonical),
                Err(_) => Some(effort), // validator should have caught invalids; keep original defensively
            };
        }

        if let Some(due) = args.due {
            let cfg = match &effective_project {
                Some(project_name) => project_resolver
                    .get_project_config(project_name)
                    .map_err(|e| format!("Failed to get project configuration: {}", e))?,
                None => project_resolver.get_config().clone(),
            };
            let v = CliValidator::new(&cfg)
                .parse_due_date(&due)
                .map_err(|e| format!("Due date validation failed: {}", e))?;
            task.due_date = Some(v);
        }

        if let Some(description) = args.description {
            task.description = Some(description);
        }

        if let Some(category) = args.category {
            task.category = Some(category);
        }

        // Add new tags (don't replace existing ones)
        for tag in args.tags {
            if !task.tags.contains(&tag) {
                task.tags.push(tag);
            }
        }

        // Set custom fields
        for (key, value) in args.fields {
            task.custom_fields
                .insert(key, crate::types::custom_value_string(value));
        }

        if args.dry_run {
            match renderer.format {
                crate::output::OutputFormat::Json => {
                    let obj = serde_json::json!({
                        "status": "preview",
                        "action": "edit",
                        "task_id": args.id,
                        "task_type": task.task_type.to_string(),
                        "priority": task.priority.to_string(),
                        "assignee": task.assignee,
                        "due_date": task.due_date,
                        "tags": task.tags,
                    });
                    renderer.emit_raw_stdout(&obj.to_string());
                }
                _ => {
                    renderer.emit_info(&format!(
                        "DRY RUN: Would update '{}' with: type={:?}, priority={}, assignee={:?}, due={:?}, tags={}",
                        args.id,
                        task.task_type,
                        task.priority,
                        task.assignee,
                        task.due_date,
                        if task.tags.is_empty() { "-".to_string() } else { task.tags.join(",") }
                    ));
                }
            }
            return Ok(());
        }

        // Save the updated task
        renderer.log_debug("edit: persisting edits");
        storage.edit(&args.id, &task);

        renderer.emit_success(&format!("Task '{}' updated successfully", args.id));
        Ok(())
    }
}

/// Handler for searching tasks
pub struct SearchHandler;

impl CommandHandler for SearchHandler {
    type Args = TaskSearchArgs;
    type Result = Result<(), String>;

    fn execute(
        args: Self::Args,
        project: Option<&str>,
        resolver: &TasksDirectoryResolver,
        renderer: &crate::output::OutputRenderer,
    ) -> Self::Result {
        renderer.log_info("list: begin");
        let storage = Storage::new(resolver.path.clone());

        // Create project resolver and validator
        let mut project_resolver = ProjectResolver::new(resolver)
            .map_err(|e| format!("Failed to initialize project resolver: {}", e))?;

        // Resolve project first (needed for project-specific config)
        let effective_project = match project_resolver.resolve_project("", project) {
            Ok(project) => {
                if project.is_empty() {
                    // No default project set, use global config
                    None
                } else {
                    Some(project)
                }
            }
            Err(e) => {
                // Project validation failed - this should be an error, not fallback
                return Err(e);
            }
        };

        // Get appropriate configuration (project-specific or global)
        let config = match &effective_project {
            Some(project_name) => project_resolver
                .get_project_config(project_name)
                .map_err(|e| format!("Failed to get project configuration: {}", e))?,
            None => {
                // Use global config
                project_resolver.get_config().clone()
            }
        };

        let validator = CliValidator::new(&config);

        // Create task filter
        let mut task_filter = TaskFilter::default();

        // Set search query if provided
        if let Some(query) = args.query {
            if !query.is_empty() {
                task_filter.text_query = Some(query);
            }
        }

        // Apply filters
        for status in args.status {
            let validated_status = validator
                .validate_status(&status)
                .map_err(|e| format!("Status validation failed: {}", e))?;
            task_filter.status.push(validated_status);
        }

        for priority in args.priority {
            let validated_priority = validator
                .validate_priority(&priority)
                .map_err(|e| format!("Priority validation failed: {}", e))?;
            task_filter.priority.push(validated_priority);
        }

        for task_type in args.task_type {
            let validated_type = validator
                .validate_task_type(&task_type)
                .map_err(|e| format!("Task type validation failed: {}", e))?;
            task_filter.task_type.push(validated_type);
        }

        task_filter.tags = args.tag;

        if let Some(category) = args.category {
            task_filter.category = Some(category);
        }

        if let Some(project) = project {
            // Resolve project name to prefix, just like in AddHandler
            let project_prefix = resolve_project_input(project, resolver.path.as_path());
            task_filter.project = Some(project_prefix);
        } // Execute search/list
        renderer.log_debug("list: executing search");
        let task_tuples = storage.search(&task_filter);
        let mut tasks: Vec<(String, Task)> = task_tuples.into_iter().collect();

        // Apply additional filters that need to be done in-memory
        // (These could potentially be moved to TaskFilter in the future)

        // Filter by assignee
        if let Some(ref assignee) = args.assignee {
            let me = if assignee == "@me" {
                crate::utils::identity::resolve_current_user(Some(resolver.path.as_path()))
            } else {
                Some(assignee.clone())
            };
            if let Some(target) = me {
                tasks.retain(|(_, task)| task.assignee.as_ref() == Some(&target));
            } else {
                // If we can't resolve @me, filter to none to produce empty result deterministically
                tasks.clear();
            }
        }

        if args.mine {
            if let Some(me) =
                crate::utils::identity::resolve_current_user(Some(resolver.path.as_path()))
            {
                tasks.retain(|(_, task)| task.assignee.as_ref() == Some(&me));
            } else {
                tasks.clear();
            }
        }

        if args.high {
            tasks.retain(|(_, task)| task.priority.eq_ignore_case("high"));
        }

        if args.critical {
            tasks.retain(|(_, task)| task.priority.eq_ignore_case("critical"));
        }

        // Overdue filter: due_date strictly before now
        if args.overdue {
            let now = chrono::Utc::now();
            tasks.retain(|(_, task)| {
                if let Some(ref due) = task.due_date {
                    if let Some(dt) = crate::cli::validation::parse_due_string_to_utc(due) {
                        return dt < now;
                    }
                }
                false
            });
        }

        // Due soon filter: due within N days from now (inclusive)
        if let Some(due_soon_arg) = args.due_soon {
            let days = match due_soon_arg {
                Some(n) => n as i64,
                None => 7,
            };
            let now = chrono::Utc::now();
            let cutoff = now + chrono::Duration::days(days);
            tasks.retain(|(_, task)| {
                if let Some(ref due) = task.due_date {
                    if let Some(dt) = crate::cli::validation::parse_due_string_to_utc(due) {
                        return dt >= now && dt <= cutoff;
                    }
                }
                false
            });
        }

        // Apply unified --where filters (in addition to discrete flags)
        if !args.r#where.is_empty() {
            // Build map of allowed sets per key
            use std::collections::HashMap;
            let mut filters: HashMap<String, std::collections::HashSet<String>> = HashMap::new();
            for (k, v) in &args.r#where {
                filters.entry(k.clone()).or_default().insert(v.clone());
            }
            let resolve_vals = |id: &str, t: &Task, key: &str| -> Option<Vec<String>> {
                let raw = key.trim();
                let k_norm = raw.to_lowercase();
                // Prefer built-ins if the key collides
                if let Some(canon) = crate::utils::fields::is_reserved_field(raw) {
                    match canon {
                        "assignee" => return Some(vec![t.assignee.clone().unwrap_or_default()]),
                        "reporter" => return Some(vec![t.reporter.clone().unwrap_or_default()]),
                        "type" => return Some(vec![t.task_type.to_string()]),
                        "status" => return Some(vec![t.status.to_string()]),
                        "priority" => return Some(vec![t.priority.to_string()]),
                        "project" => {
                            return Some(vec![id.split('-').next().unwrap_or("").to_string()]);
                        }
                        "category" => return Some(vec![t.category.clone().unwrap_or_default()]),
                        "tags" => return Some(t.tags.clone()),
                        _ => {}
                    }
                }
                // Handle explicit custom field: field:<name>
                let mut field_name: Option<&str> = None;
                if let Some(rest) = k_norm.strip_prefix("field:") {
                    field_name = Some(rest.trim());
                } else {
                    // Treat as plain custom field name if declared (or wildcard)
                    if args
                        .r#where
                        .iter()
                        .any(|(k, _)| k.trim().eq_ignore_ascii_case(raw))
                    {
                        // We still need to ensure this name is allowed by config
                        if config.custom_fields.has_wildcard()
                            || config.custom_fields.values.iter().any(|v| v == raw)
                        {
                            field_name = Some(raw);
                        }
                    }
                }
                if let Some(name) = field_name {
                    // Try exact match first; then case-insensitive match to the actual stored key
                    if let Some(v) = t.custom_fields.get(name) {
                        return Some(vec![crate::types::custom_value_to_string(v)]);
                    }
                    // Fallback: search by case-insensitive key if declared in config
                    let name_lower = name.to_lowercase();
                    if let Some((_, v)) = t
                        .custom_fields
                        .iter()
                        .find(|(k, _)| k.to_lowercase() == name_lower)
                    {
                        return Some(vec![crate::types::custom_value_to_string(v)]);
                    }
                }
                None
            };
            tasks.retain(|(id, t)| {
                for (k, allowed) in &filters {
                    let vals = match resolve_vals(id, t, k) {
                        Some(vs) => vs.into_iter().filter(|s| !s.is_empty()).collect::<Vec<_>>(),
                        None => return false,
                    };
                    if vals.is_empty() {
                        return false;
                    }
                    // Use fuzzy matching for properties
                    let allowed_vec: Vec<String> = allowed.iter().cloned().collect();
                    if !crate::utils::fuzzy_match::fuzzy_set_match(&vals, &allowed_vec) {
                        return false;
                    }
                }
                true
            });
        }

        // Effort min/max filtering
        if args.effort_min.is_some() || args.effort_max.is_some() {
            let min_parsed = args
                .effort_min
                .as_ref()
                .map(|s| crate::utils::effort::parse_effort(s));
            let max_parsed = args
                .effort_max
                .as_ref()
                .map(|s| crate::utils::effort::parse_effort(s));
            let min = match min_parsed.transpose() {
                Ok(v) => v,
                Err(e) => return Err(format!("Invalid --effort-min: {}", e)),
            };
            let max = match max_parsed.transpose() {
                Ok(v) => v,
                Err(e) => return Err(format!("Invalid --effort-max: {}", e)),
            };
            tasks.retain(|(_, t)| {
                let Some(eff) = t.effort.as_deref() else {
                    return false;
                };
                let parsed = match crate::utils::effort::parse_effort(eff) {
                    Ok(p) => p,
                    Err(_) => return false,
                };
                let mut ok = true;
                if let Some(ref m) = min {
                    ok &= parsed.total_cmp_ge(m);
                }
                if let Some(ref m) = max {
                    ok &= parsed.total_cmp_le(m);
                }
                ok
            });
        }

        // Apply sorting if requested (supports string keys)
        if let Some(sort_key) = args.sort_by.as_deref() {
            let key_raw = sort_key.trim();
            let key = key_raw.to_lowercase();
            tasks.sort_by(|(id_a, a), (id_b, b)| {
                use std::cmp::Ordering::*;
                let ord = match key.as_str() {
                    "priority" => a.priority.as_str().cmp(b.priority.as_str()),
                    "status" => a.status.as_str().cmp(b.status.as_str()),
                    "effort" => {
                        // Compare normalized effort values; missing values sort last
                        let pa = a
                            .effort
                            .as_deref()
                            .and_then(|s| crate::utils::effort::parse_effort(s).ok());
                        let pb = b
                            .effort
                            .as_deref()
                            .and_then(|s| crate::utils::effort::parse_effort(s).ok());
                        use std::cmp::Ordering::*;
                        match (pa, pb) {
                            (Some(x), Some(y)) => match (x.kind, y.kind) {
                                (
                                    crate::utils::effort::EffortKind::TimeHours(ax),
                                    crate::utils::effort::EffortKind::TimeHours(by),
                                ) => ax.partial_cmp(&by).unwrap_or(Equal),
                                (
                                    crate::utils::effort::EffortKind::Points(ax),
                                    crate::utils::effort::EffortKind::Points(by),
                                ) => ax.partial_cmp(&by).unwrap_or(Equal),
                                // Different kinds: keep stable ordering by canonical string to avoid cross-kind numeric compare
                                _ => x.canonical.cmp(&y.canonical),
                            },
                            (Some(_), None) => Less,
                            (None, Some(_)) => Greater,
                            (None, None) => Equal,
                        }
                    }
                    "due-date" | "due" => match (&a.due_date, &b.due_date) {
                        (Some(x), Some(y)) => x.cmp(y),
                        (Some(_), None) => Less,
                        (None, Some(_)) => Greater,
                        (None, None) => Equal,
                    },
                    "created" => a.created.cmp(&b.created),
                    "modified" => a.modified.cmp(&b.modified),
                    "assignee" => a.assignee.cmp(&b.assignee),
                    "type" => a.task_type.to_string().cmp(&b.task_type.to_string()),
                    "category" => a.category.cmp(&b.category),
                    "project" => id_a.split('-').next().cmp(&id_b.split('-').next()),
                    "id" => id_a.cmp(id_b),
                    other => {
                        // Support custom fields via field:<name> or plain declared names
                        let mut name_opt: Option<&str> = None;
                        if let Some(rest) = other.strip_prefix("field:") {
                            name_opt = Some(rest.trim());
                        } else if config.custom_fields.has_wildcard()
                            || config
                                .custom_fields
                                .values
                                .iter()
                                .any(|v| v.eq_ignore_ascii_case(key_raw))
                        {
                            name_opt = Some(key_raw);
                        }
                        if let Some(name) = name_opt {
                            let pick = |t: &Task| -> String {
                                if let Some(v) = t.custom_fields.get(name) {
                                    return crate::types::custom_value_to_string(v);
                                }
                                let lname = name.to_lowercase();
                                if let Some((_, v)) = t
                                    .custom_fields
                                    .iter()
                                    .find(|(k, _)| k.to_lowercase() == lname)
                                {
                                    return crate::types::custom_value_to_string(v);
                                }
                                String::new()
                            };
                            pick(a).cmp(&pick(b))
                        } else {
                            Equal
                        }
                    }
                };
                if args.reverse { ord.reverse() } else { ord }
            });
        }

        // Apply limit
        tasks.truncate(args.limit);

        if tasks.is_empty() {
            renderer.log_info("list: no results");
            match renderer.format {
                crate::output::OutputFormat::Json => {
                    renderer.emit_raw_stdout(
                        &serde_json::json!({
                            "status": "success",
                            "message": "No tasks found",
                            "tasks": []
                        })
                        .to_string(),
                    );
                }
                _ => {
                    renderer.emit_warning("No tasks found matching the search criteria.");
                }
            }
        } else {
            renderer.log_info(&format!("list: {} result(s)", tasks.len()));
            // Convert to TaskDisplayInfo for rendering
            let display_tasks: Vec<crate::output::TaskDisplayInfo> = tasks
                .into_iter()
                .map(|(task_id, task)| {
                    // Extract project from task ID (e.g., "LOTA-5" -> "LOTA")
                    let project = task_id
                        .find('-')
                        .map(|dash_pos| task_id[..dash_pos].to_string());

                    crate::output::TaskDisplayInfo {
                        id: task_id,
                        title: task.title,
                        status: task.status.to_string(),
                        priority: task.priority.to_string(),
                        task_type: task.task_type.to_string(),
                        description: task.description,
                        assignee: task.assignee,
                        project,
                        due_date: task.due_date,
                        effort: task.effort,
                        category: task.category,
                        tags: task.tags,
                        created: task.created,
                        modified: task.modified,
                        custom_fields: task.custom_fields,
                    }
                })
                .collect();

            match renderer.format {
                crate::output::OutputFormat::Json => {
                    renderer.emit_raw_stdout(
                        &serde_json::json!({
                            "status": "success",
                            "message": format!("Found {} task(s)", display_tasks.len()),
                            "tasks": display_tasks
                        })
                        .to_string(),
                    );
                }
                _ => {
                    renderer.emit_success(&format!("Found {} task(s):", display_tasks.len()));
                    for task in display_tasks {
                        renderer.emit_raw_stdout(&format!(
                            "  {} - {} [{}] ({})",
                            task.id, task.title, task.status, task.priority
                        ));
                        if let Some(description) = &task.description {
                            if !description.is_empty() {
                                renderer.emit_raw_stdout(&format!("    {}", description));
                            }
                        }
                    }
                }
            }
        }

        Ok(())
    }
}

/// Handler for deleting tasks
pub struct DeleteHandler;

impl CommandHandler for DeleteHandler {
    type Args = TaskDeleteArgs;
    type Result = Result<(), String>;

    fn execute(
        args: Self::Args,
        project: Option<&str>,
        resolver: &TasksDirectoryResolver,
        renderer: &crate::output::OutputRenderer,
    ) -> Self::Result {
        renderer.log_info("delete: begin");
        // Git-like behavior: if a parent tasks root is adopted, write to that parent (no child .tasks creation)
        let mut storage = Storage::new(resolver.path.clone());

        // Create project resolver to handle numeric IDs and project resolution
        let mut project_resolver = ProjectResolver::new(resolver)
            .map_err(|e| format!("Failed to initialize project resolver: {}", e))?;

        // Validate ID format and determine full task id (adds prefix if numeric-only)
        project_resolver
            .validate_task_id_format(&args.id)
            .map_err(|e| format!("Invalid task ID: {}", e))?;
        let full_task_id = project_resolver
            .get_full_task_id(&args.id, project)
            .map_err(|e| format!("Could not determine full task ID: {}", e))?;

        // Resolve project prefix
        let project_prefix = if let Some(project) = project {
            resolve_project_input(project, resolver.path.as_path())
        } else {
            crate::project::get_effective_project_name(resolver)
        };

        // Check if task exists
        if storage.get(&full_task_id, project_prefix.clone()).is_none() {
            return Err(format!("Task '{}' not found", args.id));
        }

        // Confirm deletion if not forced (skip prompt in dry-run)
        if !args.force && !args.dry_run {
            print!(
                "Are you sure you want to delete task '{}'? (y/N): ",
                args.id
            );
            use std::io::{self, Write};
            let _ = io::stdout().flush();

            let mut input = String::new();
            if io::stdin().read_line(&mut input).is_err() {
                renderer.emit_error("Failed to read input. Aborting.");
                return Ok(());
            }
            let input = input.trim().to_lowercase();

            if input != "y" && input != "yes" {
                renderer.emit_warning("Deletion cancelled.");
                return Ok(());
            }
        }

        if args.dry_run {
            match renderer.format {
                crate::output::OutputFormat::Json => {
                    let obj = serde_json::json!({
                        "status": "preview",
                        "action": "delete",
                        "task_id": args.id,
                        "project": project_prefix,
                    });
                    renderer.emit_raw_stdout(&obj.to_string());
                }
                _ => {
                    renderer.emit_info(&format!(
                        "DRY RUN: Would delete task '{}' from project {}",
                        args.id, project_prefix
                    ));
                }
            }
            return Ok(());
        }

        // Delete the task
        let deleted = storage.delete(&full_task_id, project_prefix);
        if deleted {
            match renderer.format {
                crate::output::OutputFormat::Json => {
                    let obj = serde_json::json!({
                        "status": "success",
                        "message": format!("Task '{}' deleted", args.id),
                        "task_id": args.id
                    });
                    renderer.emit_raw_stdout(&obj.to_string());
                }
                _ => {
                    renderer.emit_success(&format!("Task '{}' deleted successfully", args.id));
                }
            }
            Ok(())
        } else {
            Err(format!("Failed to delete task '{}'", args.id))
        }
    }
}

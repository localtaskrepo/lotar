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
            crate::utils::resolve_project_input(project, resolver.path.as_path())
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
            task.effort = Some(effort);
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
            let project_prefix =
                crate::utils::resolve_project_input(project, resolver.path.as_path());
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
            use crate::types::Priority;
            tasks.retain(|(_, task)| task.priority == Priority::High);
        }

        if args.critical {
            use crate::types::Priority;
            tasks.retain(|(_, task)| task.priority == Priority::Critical);
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

        // Apply sorting if requested
        if let Some(sort_field) = args.sort_by {
            use crate::cli::SortField;
            // Priority and TaskStatus implement Ord; use cmp on enums directly

            tasks.sort_by(|(_, task_a), (_, task_b)| {
                let comparison = match sort_field {
                    SortField::Priority => task_a.priority.cmp(&task_b.priority),
                    SortField::Status => task_a.status.cmp(&task_b.status),
                    SortField::DueDate => {
                        // Sort by due date (tasks without due date go last)
                        match (&task_a.due_date, &task_b.due_date) {
                            (Some(a), Some(b)) => a.cmp(b),
                            (Some(_), None) => std::cmp::Ordering::Less,
                            (None, Some(_)) => std::cmp::Ordering::Greater,
                            (None, None) => std::cmp::Ordering::Equal,
                        }
                    }
                    SortField::Created => {
                        // Sort by creation timestamp
                        task_a.created.cmp(&task_b.created)
                    }
                    SortField::Modified => {
                        // Sort by modification timestamp
                        task_a.modified.cmp(&task_b.modified)
                    }
                };

                // Apply reverse if requested
                if args.reverse {
                    comparison.reverse()
                } else {
                    comparison
                }
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
            crate::utils::resolve_project_input(project, resolver.path.as_path())
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

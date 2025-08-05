use crate::cli::{TaskAction, TaskEditArgs, TaskSearchArgs, TaskDeleteArgs};
use crate::cli::handlers::{CommandHandler, AddHandler};
use crate::cli::handlers::status::{StatusHandler, StatusArgs as StatusHandlerArgs};
use crate::cli::project::ProjectResolver;
use crate::cli::validation::CliValidator;
use crate::storage::{Storage, task::Task};
use crate::workspace::TasksDirectoryResolver;
use crate::index::TaskFilter;

/// Handler for all task subcommands
pub struct TaskHandler;

impl CommandHandler for TaskHandler {
    type Args = TaskAction;
    type Result = Result<(), String>;
    
    fn execute(args: Self::Args, project: Option<&str>, resolver: &TasksDirectoryResolver, renderer: &crate::output::OutputRenderer) -> Self::Result {
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
                };
                
                match AddHandler::execute(cli_add_args, project, resolver, renderer) {
                    Ok(task_id) => {
                        println!("✅ Created task: {}", task_id);
                        Ok(())
                    }
                    Err(e) => Err(e)
                }
            }
            TaskAction::List(args) | TaskAction::Search(args) => {
                SearchHandler::execute(args, project, resolver, renderer)
            }
            TaskAction::Edit(edit_args) => {
                EditHandler::execute(edit_args, project, resolver, renderer)
            }
            TaskAction::Status(status_args) => {
                let handler_args = StatusHandlerArgs::new(
                    status_args.id,
                    status_args.status,
                    project.map(|s| s.to_string())
                );
                StatusHandler::execute(handler_args, project, resolver, renderer)
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
    
    fn execute(args: Self::Args, project: Option<&str>, resolver: &TasksDirectoryResolver, _renderer: &crate::output::OutputRenderer) -> Self::Result {
        let mut storage = Storage::new(resolver.path.clone());
        
        // Create project resolver and validator
        let project_resolver = ProjectResolver::new(resolver)
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
            },
            Err(e) => {
                // Project validation failed - this should be an error, not fallback
                return Err(e);
            }
        };
        
        // Get appropriate configuration (project-specific or global)
        let config = match &effective_project {
            Some(project_name) => {
                project_resolver.get_project_config(project_name)
                    .map_err(|e| format!("Failed to get project configuration: {}", e))?
            },
            None => {
                // Use global config
                project_resolver.get_config().clone()
            }
        };
        
        let validator = CliValidator::new(&config);
        
        // Resolve project prefix for loading
        let project_prefix = if let Some(project) = project {
            crate::utils::resolve_project_input(project, &resolver.path)
        } else {
            crate::project::get_effective_project_name(resolver)
        };
        
        // Load the task
        let mut task = storage.get(&args.id, project_prefix.clone())
            .ok_or_else(|| format!("Task '{}' not found", args.id))?;
        
        // Update fields if provided
        if let Some(title) = args.title {
            task.title = title;
        }
        
        if let Some(task_type) = args.task_type {
            task.task_type = validator.validate_task_type(&task_type)
                .map_err(|e| format!("Task type validation failed: {}", e))?;
        }
        
        if let Some(priority) = args.priority {
            task.priority = validator.validate_priority(&priority)
                .map_err(|e| format!("Priority validation failed: {}", e))?;
        }
        
        if let Some(assignee) = args.assignee {
            task.assignee = Some(assignee);
        }
        
        if let Some(effort) = args.effort {
            task.effort = Some(effort);
        }
        
        if let Some(due) = args.due {
            // TODO: Parse and validate due date
            // For now, just store as string in custom fields
            task.custom_fields.insert("due_date".to_string(), serde_yaml::Value::String(due));
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
            task.custom_fields.insert(key, serde_yaml::Value::String(value));
        }
        
        // Save the updated task
        storage.edit(&args.id, &task);
        
        println!("✅ Task '{}' updated successfully", args.id);
        Ok(())
    }
}

/// Handler for searching tasks
pub struct SearchHandler;

impl CommandHandler for SearchHandler {
    type Args = TaskSearchArgs;
    type Result = Result<(), String>;
    
    fn execute(args: Self::Args, project: Option<&str>, resolver: &TasksDirectoryResolver, renderer: &crate::output::OutputRenderer) -> Self::Result {
        let storage = Storage::new(resolver.path.clone());
        
        // Create project resolver and validator
        let project_resolver = ProjectResolver::new(resolver)
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
            },
            Err(e) => {
                // Project validation failed - this should be an error, not fallback
                return Err(e);
            }
        };
        
        // Get appropriate configuration (project-specific or global)
        let config = match &effective_project {
            Some(project_name) => {
                project_resolver.get_project_config(project_name)
                    .map_err(|e| format!("Failed to get project configuration: {}", e))?
            },
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
        if let Some(status) = args.status {
            task_filter.status = Some(validator.validate_status(&status)
                .map_err(|e| format!("Status validation failed: {}", e))?);
        }
        
        if let Some(priority) = args.priority {
            task_filter.priority = Some(validator.validate_priority(&priority)
                .map_err(|e| format!("Priority validation failed: {}", e))?);
        }
        
        if let Some(tag) = args.tag {
            task_filter.tags = vec![tag];
        }
        
        if let Some(category) = args.category {
            task_filter.category = Some(category);
        }

        if let Some(project) = project {
            // Resolve project name to prefix, just like in AddHandler
            let project_prefix = crate::utils::resolve_project_input(project, &resolver.path);
            task_filter.project = Some(project_prefix);
        }        // Execute search/list
        let task_tuples = storage.search(&task_filter);
        let mut tasks: Vec<(String, Task)> = task_tuples.into_iter().collect();
        
        // Apply additional filters that need to be done in-memory
        // (These could potentially be moved to TaskFilter in the future)
        
        // Filter by assignee
        if let Some(ref assignee) = args.assignee {
            let filter_assignee = if assignee == "@me" {
                // TODO: Resolve @me to actual user
                "@me".to_string()
            } else {
                assignee.clone()
            };
            tasks.retain(|(_, task)| {
                task.assignee.as_ref().map_or(false, |a| a == &filter_assignee)
            });
        }
        
        if args.mine {
            // TODO: Resolve current user and filter by that
            // For now, filter by @me placeholder
            tasks.retain(|(_, task)| {
                task.assignee.as_ref().map_or(false, |a| a == "@me")
            });
        }
        
        if args.high {
            use crate::types::Priority;
            tasks.retain(|(_, task)| task.priority == Priority::High);
        }
        
        if args.critical {
            use crate::types::Priority;
            tasks.retain(|(_, task)| task.priority == Priority::Critical);
        }
        
        // Apply limit
        tasks.truncate(args.limit);
        
        if tasks.is_empty() {
            match renderer.format {
                crate::output::OutputFormat::Json => {
                    println!("{}", serde_json::json!({
                        "status": "success",
                        "message": "No tasks found",
                        "tasks": []
                    }));
                }
                _ => {
                    println!("No tasks found matching the search criteria.");
                }
            }
        } else {
            // Convert to TaskDisplayInfo for rendering
            let display_tasks: Vec<crate::output::TaskDisplayInfo> = tasks.into_iter()
                .map(|(task_id, task)| crate::output::TaskDisplayInfo {
                    id: task_id,
                    title: task.title,
                    status: task.status.to_string(),
                    priority: task.priority.to_string(),
                    task_type: task.task_type.to_string(),
                    description: task.description,
                    assignee: task.assignee,
                    project: None, // Project info would need to come from context
                })
                .collect();
            
            match renderer.format {
                crate::output::OutputFormat::Json => {
                    println!("{}", serde_json::json!({
                        "status": "success",
                        "message": format!("Found {} task(s)", display_tasks.len()),
                        "tasks": display_tasks
                    }));
                }
                _ => {
                    println!("Found {} task(s):", display_tasks.len());
                    for task in display_tasks {
                        println!("  {} - {} [{}] ({})", 
                            task.id, task.title, task.status, task.priority);
                        if let Some(description) = &task.description {
                            if !description.is_empty() {
                                println!("    {}", description);
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
    
    fn execute(args: Self::Args, project: Option<&str>, resolver: &TasksDirectoryResolver, _renderer: &crate::output::OutputRenderer) -> Self::Result {
        let mut storage = Storage::new(resolver.path.clone());
        
        // Resolve project prefix
        let project_prefix = if let Some(project) = project {
            crate::utils::resolve_project_input(project, &resolver.path)
        } else {
            crate::project::get_effective_project_name(resolver)
        };
        
        // Check if task exists
        if storage.get(&args.id, project_prefix.clone()).is_none() {
            return Err(format!("Task '{}' not found", args.id));
        }
        
        // Confirm deletion if not forced
        if !args.force {
            print!("Are you sure you want to delete task '{}'? (y/N): ", args.id);
            use std::io::{self, Write};
            io::stdout().flush().unwrap();
            
            let mut input = String::new();
            io::stdin().read_line(&mut input).unwrap();
            let input = input.trim().to_lowercase();
            
            if input != "y" && input != "yes" {
                println!("Deletion cancelled.");
                return Ok(());
            }
        }
        
        // Delete the task
        let deleted = storage.delete(&args.id, project_prefix);
        if deleted {
            println!("✅ Task '{}' deleted successfully", args.id);
            Ok(())
        } else {
            Err(format!("Failed to delete task '{}'", args.id))
        }
    }
}

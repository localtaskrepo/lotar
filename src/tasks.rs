use crate::index::TaskFilter;
use crate::storage::{Storage, Task};
use crate::types::{Priority, TaskStatus, TaskType};
use crate::workspace::TasksDirectoryResolver;

pub fn task_command(args: &[String], default_project: &str, resolver: &TasksDirectoryResolver) {
    // Display info message if tasks directory is not in current directory
    if let Some(info_msg) = resolver.get_info_message() {
        println!("{}", info_msg);
    }

    if args.len() < 3 {
        println!("Error: No task operation specified.");
        println!("Available operations: add, edit, list, status, search, delete");
        println!("Use 'lotar help' for more information.");
        std::process::exit(1);
    }

    let operation = args[2].as_str();
    let mut store = Storage::new(resolver.path.clone());

    match operation {
        "add" => {
            let mut task = Task::new(
                resolver.path.clone(),
                "".to_string(),
                default_project.to_string(),
                Priority::Medium,
            );

            if args.len() < 4 {
                println!("Error: No task title specified.");
                println!("Usage: lotar task add --title=\"Task Title\" [OPTIONS]");
                std::process::exit(1);
            }

            assign_task_properties(&mut task, args, 3);

            if task.title.is_empty() {
                eprintln!("Error: Title is required");
                eprintln!("Usage: lotar task add --title=\"Task Title\" [OPTIONS]");
                std::process::exit(1);
            }

            // Ensure tasks directory exists before creating task
            if let Err(e) = resolver.ensure_exists() {
                eprintln!("Error creating tasks directory: {}", e);
                std::process::exit(1);
            }

            let id = store.add(&task);
            println!("Added task with id: {}", id);
        }
        "edit" => {
            if args.len() < 4 {
                println!("Error: No task ID specified.");
                println!("Usage: lotar task edit <ID> [OPTIONS]");
                std::process::exit(1);
            }

            let id = &args[3]; // Now accepting string IDs like "TEST-001"

            // Extract project from task ID if no explicit project is provided
            let project = extract_project_from_args(args, 4, default_project);
            let project = if project == default_project && id.contains('-') {
                // If using default project and task ID contains project prefix, extract it
                id.split('-').next().unwrap_or(default_project).to_string()
            } else {
                project
            };

            let mut task = match store.get(id, project.clone()) {
                Some(t) => t,
                None => {
                    println!(
                        "Error: Task with id '{}' not found in project '{}'",
                        id, project
                    );
                    std::process::exit(1);
                }
            };

            assign_task_properties(&mut task, args, 4);
            task.update_modified();
            store.edit(id, &task);
            println!("Task {} updated successfully", id);
        }
        "status" => {
            if args.len() < 5 {
                println!("Error: Status command requires task ID and new status.");
                println!("Usage: lotar task status <ID> <STATUS>");
                println!("Available statuses: TODO, IN_PROGRESS, VERIFY, BLOCKED, DONE");
                std::process::exit(1);
            }

            let id = &args[3]; // Now accepting string IDs like "TEST-001"

            // Extract project from task ID if no explicit project is provided
            let project = extract_project_from_args(args, 5, default_project);
            let project = if project == default_project && id.contains('-') {
                // If using default project and task ID contains project prefix, extract it
                id.split('-').next().unwrap_or(default_project).to_string()
            } else {
                project
            };

            let new_status = match args[4].parse::<TaskStatus>() {
                Ok(status) => status,
                Err(e) => {
                    eprintln!("Error: {}", e);
                    std::process::exit(1);
                }
            };

            let mut task = match store.get(id, project.clone()) {
                Some(t) => t,
                None => {
                    println!(
                        "Error: Task with id '{}' not found in project '{}'",
                        id, project
                    );
                    std::process::exit(1);
                }
            };

            task.update_status(new_status.clone()).unwrap();
            store.edit(id, &task);
            println!("Task {} status updated to {}", id, new_status);
        }
        "list" => {
            let project = extract_project_from_args(args, 3, default_project);
            println!("Listing tasks for project: {}", project);

            let tasks = store.list_by_project(&project);
            if tasks.is_empty() {
                println!("No tasks found in project '{}'", project);
            } else {
                println!("Found {} tasks:", tasks.len());
                for (task_id, task) in tasks {
                    println!(
                        "  [{}] {} - {} (Priority: {}, Status: {})",
                        task_id, task.title, task.project, task.priority, task.status
                    );
                }
            }
        }
        "search" => {
            if args.len() < 4 {
                println!("Error: Search requires a query.");
                println!(
                    "Usage: lotar task search <QUERY> [--project=PROJECT] [--status=STATUS] [--priority=N] [--tag=TAG]"
                );
                std::process::exit(1);
            }

            let query = &args[3];
            let mut filter = TaskFilter {
                text_query: Some(query.clone()),
                ..Default::default()
            };

            // Parse additional filter arguments
            for i in 4..args.len() {
                let arg = &args[i];
                if let Some(stripped) = arg.strip_prefix("--project=") {
                    filter.project = Some(stripped.to_string());
                } else if let Some(stripped) = arg.strip_prefix("--status=") {
                    if let Ok(status) = stripped.parse::<TaskStatus>() {
                        filter.status = Some(status);
                    }
                } else if let Some(stripped) = arg.strip_prefix("--priority=") {
                    if let Ok(priority) = stripped.parse::<Priority>() {
                        filter.priority = Some(priority);
                    }
                } else if let Some(stripped) = arg.strip_prefix("--tag=") {
                    filter.tags.push(stripped.to_string());
                }
            }

            println!("Searching for: '{}'", query);
            if filter.project.is_some()
                || filter.status.is_some()
                || filter.priority.is_some()
                || !filter.tags.is_empty()
            {
                println!(
                    "Filters: project={:?}, status={:?}, priority={:?}, tags={:?}",
                    filter.project, filter.status, filter.priority, filter.tags
                );
            }

            let results = store.search(&filter);
            if results.is_empty() {
                println!("No tasks found matching the search criteria.");
            } else {
                println!("Found {} matching tasks:", results.len());
                for (task_id, task) in results {
                    println!(
                        "  [{}] {} - {} (Priority: {}, Status: {})",
                        task_id, task.title, task.project, task.priority, task.status
                    );
                    if !task.tags.is_empty() {
                        println!("    Tags: {}", task.tags.join(", "));
                    }
                }
            }
        }
        "delete" => {
            if args.len() < 4 {
                println!("Error: No task ID specified.");
                println!("Usage: lotar task delete <ID> [--project=PROJECT]");
                std::process::exit(1);
            }

            let id = &args[3]; // Now accepting string IDs like "TEST-001"

            let project = extract_project_from_args(args, 4, default_project);

            if store.delete(id, project.clone()) {
                println!(
                    "Task {} deleted successfully from project '{}'",
                    id, project
                );
            } else {
                println!(
                    "Error: Task with id '{}' not found in project '{}'",
                    id, project
                );
                std::process::exit(1);
            }
        }
        _ => {
            println!("Error: Invalid task operation '{}'", operation);
            println!("Available operations: add, edit, list, status, search, delete");
        }
    }
}

fn extract_project_from_args(args: &[String], start_index: usize, default_project: &str) -> String {
    for i in start_index..args.len() {
        let arg = &args[i];
        if let Some(stripped) = arg.strip_prefix("--project=") {
            return stripped.to_string();
        }
    }
    default_project.to_string()
}

fn assign_task_properties(task: &mut Task, args: &[String], start_index: usize) {
    let mut i = start_index;
    while i < args.len() {
        let arg = &args[i];

        if let Some(stripped) = arg.strip_prefix("--title=") {
            task.title = stripped.to_string();
        } else if arg == "--title" || arg == "-t" {
            if i + 1 < args.len() {
                task.title = args[i + 1].clone();
                i += 1;
            }
        } else if let Some(stripped) = arg.strip_prefix("--type=") {
            if let Ok(task_type) = stripped.parse::<TaskType>() {
                task.task_type = task_type;
            }
        } else if arg == "--type" {
            if i + 1 < args.len() {
                if let Ok(task_type) = args[i + 1].parse::<TaskType>() {
                    task.task_type = task_type;
                }
                i += 1;
            }
        } else if let Some(stripped) = arg.strip_prefix("--assignee=") {
            task.assignee = Some(stripped.to_string());
        } else if arg == "--assignee" || arg == "-a" {
            if i + 1 < args.len() {
                task.assignee = Some(args[i + 1].clone());
                i += 1;
            }
        } else if let Some(stripped) = arg.strip_prefix("--effort=") {
            task.effort = Some(stripped.to_string());
        } else if arg == "--effort" || arg == "-e" {
            if i + 1 < args.len() {
                task.effort = Some(args[i + 1].clone());
                i += 1;
            }
        } else if let Some(stripped) = arg.strip_prefix("--priority=") {
            if let Ok(priority) = stripped.parse::<Priority>() {
                task.priority = priority;
            }
        } else if arg == "--priority" || arg == "-p" {
            if i + 1 < args.len() {
                if let Ok(priority) = args[i + 1].parse::<Priority>() {
                    task.priority = priority;
                }
                i += 1;
            }
        } else if let Some(stripped) = arg.strip_prefix("--acceptance-criteria=") {
            task.acceptance_criteria.push(stripped.to_string());
        } else if arg == "--acceptance-criteria" || arg == "--ac" {
            if i + 1 < args.len() {
                task.acceptance_criteria.push(args[i + 1].clone());
                i += 1;
            }
        } else if let Some(stripped) = arg.strip_prefix("--depends-on=") {
            task.relationships.depends_on.push(stripped.to_string());
        } else if let Some(stripped) = arg.strip_prefix("--blocks=") {
            task.relationships.blocks.push(stripped.to_string());
        } else if let Some(stripped) = arg.strip_prefix("--related=") {
            task.relationships.related.push(stripped.to_string());
        } else if let Some(stripped) = arg.strip_prefix("--parent=") {
            task.relationships.parent = Some(stripped.to_string());
        } else if let Some(stripped) = arg.strip_prefix("--fixes=") {
            task.relationships.fixes.push(stripped.to_string());
        // Legacy fields for backward compatibility
        } else if let Some(stripped) = arg.strip_prefix("--subtitle=") {
            task.subtitle = Some(stripped.to_string());
        } else if arg == "--subtitle" || arg == "-s" {
            if i + 1 < args.len() {
                task.subtitle = Some(args[i + 1].clone());
                i += 1;
            }
        } else if let Some(stripped) = arg.strip_prefix("--description=") {
            task.description = Some(stripped.to_string());
        } else if arg == "--description" || arg == "-d" {
            if i + 1 < args.len() {
                task.description = Some(args[i + 1].clone());
                i += 1;
            }
        } else if let Some(stripped) = arg.strip_prefix("--project=") {
            task.project = stripped.to_string();
        } else if arg == "--project" || arg == "-g" {
            if i + 1 < args.len() {
                task.project = args[i + 1].clone();
                i += 1;
            }
        } else if let Some(stripped) = arg.strip_prefix("--category=") {
            task.category = Some(stripped.to_string());
        } else if arg == "--category" || arg == "-c" {
            if i + 1 < args.len() {
                task.category = Some(args[i + 1].clone());
                i += 1;
            }
        } else if let Some(stripped) = arg.strip_prefix("--due-date=") {
            task.due_date = Some(stripped.to_string());
        } else if arg == "--due-date" || arg == "-dd" {
            if i + 1 < args.len() {
                task.due_date = Some(args[i + 1].clone());
                i += 1;
            }
        } else if let Some(stripped) = arg.strip_prefix("--tag=") {
            task.tags.push(stripped.to_string());
        } else if arg == "--tag" {
            if i + 1 < args.len() {
                task.tags.push(args[i + 1].clone());
                i += 1;
            }
        }
        i += 1;
    }
}

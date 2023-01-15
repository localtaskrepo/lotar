use std::path::PathBuf;
use crate::store;
use crate::store::Task;

fn validate_args(min_args: usize, args: &[String]) -> bool {
    if args.len() < min_args {
        println!("Not enough arguments");
        return false;
    }
    true
}

fn assign_task_properties(
    task: &mut Task,
    args: &[String],
    start_index: usize
) {
    for i in start_index..args.len() {
        let arg = args[i].as_str();
        let key_value: Vec<&str> = arg.splitn(2, '=').collect();
        let key = key_value[0];
        let value = key_value[1];
        match key {
            "--title" | "-t" => task.title = value.to_string(),
            "--subtitle" | "-s" => task.subtitle = Some(value.to_string()),
            "--description" | "-d" => task.description = Some(value.to_string()),
            "--priority" | "-p" => task.priority = value.parse::<u8>().unwrap_or(3),
            "--category" | "-c" => task.category = Some(value.to_string()),
            "--due-date" | "-dd" => task.due_date = Some(value.to_string()),
            "--tag" | "-x" => task.tags.push(value.to_string()),
            "--project" | "-g" => {
                    if !value.is_empty() && value.chars().any(|c| c.is_ascii_alphanumeric()) {
                        task.project = value.to_string()
                    };
            }
            _ => {
                println!("Invalid argument: {}", arg);
                return;
            }
        }
    }
}

pub fn task_command(args: &[String], default_project: &str) {
    if !validate_args(3, args) {
        return;
    }
    let operation = args[2].as_str();
    // TODO find project root or get tasks from options

    let root_path = PathBuf::from(std::env::current_dir().unwrap().join(".tasks/"));
    let mut store = store::Storage::new(root_path.clone());
    match operation {
        "add" => {
            if !validate_args(4, args) {
                return;
            }
            let mut task = Task::new(
                root_path.clone(),
                "".to_string(),
                default_project.to_string(),
                3
            );
            assign_task_properties(&mut task, args, 3);
            if task.title.is_empty() {
                println!("Title is required");
                return;
            }
            let id = store.add(&task);
            println!("Added task with id: {}", id);
        }
        "edit" => {
            if !validate_args(4, args) {
                return;
            }
            let id = match args[3].parse::<u64>() {
                Ok(i) => i,
                Err(e) => {
                    println!("Invalid id: {}", e);
                    return;
                }
            };
            let project = if args.len() > 4 {
                let key_value: Vec<&str> = args[4].splitn(2, '=').collect();
                let key = key_value[0];
                let value = key_value[1];
                if (key == "--project" || key == "-g") && !value.is_empty() {
                    value.to_string()
                } else {
                    println!("Invalid argument: {}", args[4]);
                    return;
                }
            } else {
                default_project.to_string()
            };
            let mut task = match store.get(id, project) {
                Some(t) => t,
                None => {
                    println!("Task with id '{}' not found", id);
                    return;
                }
            };
            assign_task_properties(&mut task, args, 4);
            store.edit(id, &task);
            println!("Task with id {} updated", id);
        }
        "delete" => {
            if !validate_args(4, args) {
                return;
            }
            let id = match args[3].parse::<u64>() {
                Ok(i) => i,
                Err(e) => {
                    println!("Invalid id: {}", e);
                    return;
                }
            };
            let project = if args.len() > 4 {
                let key_value: Vec<&str> = args[4].splitn(2, '=').collect();
                let key = key_value[0];
                let value = key_value[1];
                if (key == "--project" || key == "-g") && !value.is_empty() {
                    value.to_string()
                } else {
                    println!("Invalid argument: {}", args[4]);
                    return;
                }
            } else {
                default_project.to_string()
            };
            if store.delete(id, project) {
                println!("Task with id {} deleted", id);
            } else {
                println!("Task with id {} not found", id);
            }
        }
        "get" => {
            if !validate_args(4, args) {
                return;
            }
            let id = match args[3].parse::<u64>() {
                Ok(i) => i,
                Err(e) => {
                    println!("Invalid id: {}", e);
                    return;
                }
            };
            let project = if args.len() > 4 {
                let key_value: Vec<&str> = args[4].splitn(2, '=').collect();
                let key = key_value[0];
                let value = key_value[1];
                if (key == "--project" || key == "-g") && !value.is_empty() {
                    value.to_string()
                } else {
                    println!("Invalid argument: {}", args[4]);
                    return;
                }
            } else {
                default_project.to_string()
            };
            let task = store.get(id, project);
            if task.is_some() {
                println!("{}", task.unwrap());
            } else {
                println!("Task with id {} not found", id);
            }
        }
        _ => {
            println!("Invalid operation: {}", operation);
        }
    }
}

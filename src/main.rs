mod api_server;
mod project;
mod routes;
mod web_server;
mod tasks;
mod storage;
mod scanner;
mod index;
mod types;
mod config;
mod utils;

use std::env;
use std::path::PathBuf;

// Command trait for better organization
trait Command {
    fn execute(&self, args: &[String]) -> Result<(), String>;
}

struct ServeCommand;
struct TaskCommand;
struct ScanCommand;

impl Command for ServeCommand {
    fn execute(&self, args: &[String]) -> Result<(), String> {
        let port = args.get(2)
            .and_then(|p| p.parse::<u16>().ok())
            .unwrap_or(8000);

        let mut api_server = api_server::ApiServer::new();
        routes::initialize(&mut api_server);
        web_server::serve(&api_server, port);
        Ok(())
    }
}

impl Command for TaskCommand {
    fn execute(&self, args: &[String]) -> Result<(), String> {
        let project = project::get_project_name().unwrap_or_else(|| "None".to_string());
        tasks::task_command(args, &project);
        Ok(())
    }
}

impl Command for ScanCommand {
    fn execute(&self, args: &[String]) -> Result<(), String> {
        let path = args.get(2)
            .map(PathBuf::from)
            .or_else(|| project::get_project_path())
            .unwrap_or_else(|| {
                println!("No path specified. Using current directory.");
                PathBuf::from(".")
            });

        if !path.exists() {
            return Err(format!("Path '{}' does not exist", path.display()));
        }

        println!("Scanning {}", path.display());
        let mut scanner = scanner::Scanner::new(path);
        let results = scanner.scan();
        for entry in results {
            println!("{:?}", entry);
        }
        Ok(())
    }
}

fn main() {
    let args: Vec<String> = env::args().collect();

    let result = match args.get(1).map(|s| s.as_str()) {
        Some("serve") => ServeCommand.execute(&args),
        Some("task") => TaskCommand.execute(&args),
        Some("scan") => ScanCommand.execute(&args),
        Some("config") => {
            config::config_command(&args);
            Ok(())
        }
        Some("index") => index_command(&args),
        Some("help") => {
            print_help();
            Ok(())
        },
        Some(command) => Err(format!("Invalid command '{}'. Use 'help' for available commands", command)),
        None => Err("No command specified. Use 'help' for available commands".to_string()),
    };

    if let Err(error) = result {
        eprintln!("Error: {}", error);
        std::process::exit(1);
    }
}

fn index_command(args: &[String]) -> Result<(), String> {
    if args.len() < 3 {
        return Err("No index operation specified. Available operations: rebuild. Usage: lotar index rebuild".to_string());
    }

    let operation = args[2].as_str();
    let root_path = PathBuf::from(std::env::current_dir().unwrap().join(".tasks/"));

    match operation {
        "rebuild" => {
            println!("Rebuilding index from storage...");
            let mut store = storage::Storage::new(root_path);

            match store.rebuild_index() {
                Ok(_) => {
                    println!("✅ Index rebuilt successfully");
                    Ok(())
                }
                Err(e) => {
                    Err(format!("Error rebuilding index: {}", e))
                }
            }
        }
        _ => {
            Err(format!("Invalid index operation '{}'. Available operations: rebuild", operation))
        }
    }
}

fn print_help() {
    println!("LoTaR - Local Task Repository");
    println!("A git-integrated task management system");
    println!();
    println!("USAGE:");
    println!("    lotar <COMMAND> [OPTIONS]");
    println!();
    println!("COMMANDS:");
    println!("    task        Manage tasks (add, edit, list, status, search, delete)");
    println!("    scan        Scan source files for TODO comments");
    println!("    serve       Start web server on specified port");
    println!("    config      Manage configuration (get, set, delete)");
    println!("    index       Index management (rebuild)");
    println!("    help        Show this help message");
    println!();
    println!("TASK COMMANDS:");
    println!("    lotar task add --title=\"Task Title\" [OPTIONS]");
    println!("        --title, -t           Task title (required)");
    println!("        --type                Task type: feature, bug, epic, spike, chore");
    println!("        --priority, -p        Priority: LOW, MEDIUM, HIGH, CRITICAL");
    println!("        --assignee, -a        Assignee email/username");
    println!("        --effort, -e          Effort estimate (e.g., '2d', '5h', '1w')");
    println!("        --project, -g         Project name");
    println!("        --due-date, -dd       Due date (YYYY-MM-DD format)");
    println!("        --acceptance-criteria Acceptance criteria (can be used multiple times)");
    println!("        --depends-on          Task dependencies (task IDs)");
    println!("        --blocks              Tasks blocked by this task");
    println!("        --related             Related tasks");
    println!("        --parent              Parent task/epic");
    println!("        --fixes               Bug fixes references");
    println!("        --tag                 Tags (can be used multiple times)");
    println!("        --description, -d     Task description");
    println!("        --subtitle, -s        Task subtitle");
    println!("        --category, -c        Task category");
    println!();
    println!("    lotar task list [--project=PROJECT] [--status=STATUS]");
    println!("    lotar task edit <ID> [OPTIONS]");
    println!("    lotar task status <ID> <STATUS>");
    println!("        Available statuses: TODO, IN_PROGRESS, VERIFY, BLOCKED, DONE");
    println!("    lotar task search <QUERY> [--project=PROJECT] [--status=STATUS] [--priority=PRIORITY]");
    println!("    lotar task delete <ID> [--project=PROJECT]");
    println!();
    println!("INDEX COMMANDS:");
    println!("    lotar index rebuild   Rebuild search index from task files");
    println!();
    println!("ENHANCED EXAMPLES:");
    println!("    # Create a feature task with full metadata");
    println!("    lotar task add --title=\"OAuth Implementation\" \\");
    println!("                   --type=feature --priority=HIGH \\");
    println!("                   --assignee=john@company.com \\");
    println!("                   --effort=3d --project=auth \\");
    println!("                   --acceptance-criteria=\"User can login with Google\" \\");
    println!("                   --depends-on=AUTH-002 --tag=security");
    println!();
    println!("    # Create a bug fix");
    println!("    lotar task add --title=\"Fix login redirect\" \\");
    println!("                   --type=bug --priority=CRITICAL \\");
    println!("                   --fixes=BUG-123 --project=frontend");
    println!();
    println!("    # List high priority tasks");
    println!("    lotar task search \"\" --priority=HIGH --project=myproject");
    println!();
    println!("    lotar scan ./src");
    println!("    lotar serve 8080");
    println!("    lotar index rebuild");
    println!();
    println!("For more information, visit: https://github.com/mallox/lotar");
}

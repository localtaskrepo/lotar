mod api_server;
mod cli;
mod config;
mod help;
mod index;
mod output;
mod project;
mod routes;
mod scanner;
mod storage;
mod tasks;
mod types;
mod utils;
mod web_server;
mod workspace;

use std::env;
use std::path::PathBuf;
use workspace::TasksDirectoryResolver;

/// Extract --project parameter from task command arguments
/// Returns (original_name, resolved_prefix)
fn extract_project_from_task_args(
    args: &[String],
    resolver: &TasksDirectoryResolver,
) -> Option<(String, String)> {
    for arg in args.iter() {
        if let Some(stripped) = arg.strip_prefix("--project=") {
            let resolved_prefix = crate::utils::resolve_project_input(stripped, &resolver.path);
            return Some((stripped.to_string(), resolved_prefix));
        }
    }
    None
}

// Command trait for better organization
trait Command {
    fn execute(&self, args: &[String], resolver: &TasksDirectoryResolver) -> Result<(), String>;
}

/// Parse command line arguments to extract --tasks-dir flag and return cleaned args
fn parse_tasks_dir_flag(args: &[String]) -> (Option<String>, Vec<String>) {
    let mut tasks_dir = None;
    let mut cleaned_args = Vec::new();
    let mut i = 0;

    while i < args.len() {
        let arg = &args[i];
        if arg == "--tasks-dir" && i + 1 < args.len() {
            tasks_dir = Some(args[i + 1].clone());
            i += 2; // Skip both --tasks-dir and its value
        } else if arg.starts_with("--tasks-dir=") {
            tasks_dir = Some(arg[12..].to_string());
            i += 1; // Skip this argument
        } else {
            cleaned_args.push(arg.clone());
            i += 1;
        }
    }

    (tasks_dir, cleaned_args)
}

/// Resolve the tasks directory based on config and command line arguments
fn resolve_tasks_directory_with_override(
    override_path: Option<String>,
) -> Result<TasksDirectoryResolver, String> {
    // TODO: Load home config to get global tasks_folder preference
    // For now, we'll use the default folder name (.tasks) without loading global config
    TasksDirectoryResolver::resolve(
        override_path.as_deref(),
        None, // Use default .tasks folder name
    )
}

struct ServeCommand;
struct TaskCommand;
struct ScanCommand;

impl Command for ServeCommand {
    fn execute(&self, args: &[String], _resolver: &TasksDirectoryResolver) -> Result<(), String> {
        let port = args
            .get(2)
            .and_then(|p| p.parse::<u16>().ok())
            .unwrap_or(8000);

        let mut api_server = api_server::ApiServer::new();
        routes::initialize(&mut api_server);
        web_server::serve(&api_server, port);
        Ok(())
    }
}

impl Command for TaskCommand {
    fn execute(&self, args: &[String], resolver: &TasksDirectoryResolver) -> Result<(), String> {
        // First try to extract --project parameter from CLI args
        let (original_project_name, project_prefix) =
            extract_project_from_task_args(args, resolver)
                .map(|(orig, prefix)| (Some(orig), prefix))
                .unwrap_or_else(|| (None, project::get_effective_project_name(resolver)));

        tasks::task_command(args, &project_prefix, &original_project_name, resolver);
        Ok(())
    }
}

impl Command for ScanCommand {
    fn execute(&self, args: &[String], _resolver: &TasksDirectoryResolver) -> Result<(), String> {
        let path = args
            .get(2)
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

    // Check if user wants to use the new CLI
    if args.len() > 1 && (args[1] == "--new-cli" || args[1] == "--experimental") {
        run_new_cli(args);
        return;
    }

    // Parse tasks directory flag and get cleaned arguments
    let (override_tasks_dir, cleaned_args) = parse_tasks_dir_flag(&args);

    // Resolve tasks directory early for commands that need it
    let resolver = match resolve_tasks_directory_with_override(override_tasks_dir) {
        Ok(resolver) => resolver,
        Err(error) => {
            eprintln!("Error resolving tasks directory: {}", error);
            std::process::exit(1);
        }
    };

    let result = match cleaned_args.get(1).map(|s| s.as_str()) {
        Some("serve") => ServeCommand.execute(&cleaned_args, &resolver),
        Some("task") => TaskCommand.execute(&cleaned_args, &resolver),
        Some("scan") => ScanCommand.execute(&cleaned_args, &resolver),
        Some("config") => {
            config::config_command(&cleaned_args, &resolver.path);
            Ok(())
        }
        Some("index") => index_command(&cleaned_args, &resolver),
        Some("help") => {
            print_help();
            Ok(())
        }
        Some(command) => Err(format!(
            "Invalid command '{}'. Use 'help' for available commands",
            command
        )),
        None => Err("No command specified. Use 'help' for available commands".to_string()),
    };

    if let Err(error) = result {
        eprintln!("Error: {}", error);
        std::process::exit(1);
    }
}

/// Run the new Clap-based CLI
fn run_new_cli(mut args: Vec<String>) {
    use clap::Parser;
    use cli::{Cli, Commands};
    use cli::handlers::{CommandHandler, AddHandler, ListHandler};
    use cli::handlers::status::{StatusHandler, StatusArgs};

    // Remove the --new-cli or --experimental flag
    if let Some(pos) = args.iter().position(|arg| arg == "--new-cli" || arg == "--experimental") {
        args.remove(pos);
    }
    
    // Handle help and version manually for flexibility
    // But skip if this is a subcommand help like "lotar help add"
    if !(args.len() >= 3 && args[1] == "help") {
        for arg in &args {
            if arg == "help" || arg == "--help" || arg == "-h" {
                // Show enhanced help using our help system
                let help_system = help::HelpSystem::new(output::OutputFormat::Text, false);
                match help_system.show_global_help() {
                    Ok(help_text) => {
                        println!("{}", help_text);
                        return;
                    }
                    Err(_) => {
                        // Fall back to clap's help
                        let _ = Cli::try_parse_from(&["lotar", "--help"]);
                        return;
                    }
                }
            }
            if arg == "version" || arg == "--version" || arg == "-V" {
                println!("lotar {}", env!("CARGO_PKG_VERSION"));
                return;
            }
        }
    }

    // Handle subcommand help (e.g., "lotar help add" or "lotar add --help")
    if args.len() >= 3 && args[1] == "help" {
        let command = &args[2];
        let help_system = help::HelpSystem::new(output::OutputFormat::Text, false);
        match help_system.show_command_help(command) {
            Ok(help_text) => {
                println!("{}", help_text);
                return;
            }
            Err(e) => {
                eprintln!("Error showing help for '{}': {}", command, e);
                eprintln!("Try 'lotar help' for available commands.");
                std::process::exit(1);
            }
        }
    }
    
    // Parse with Clap
    let cli = match Cli::try_parse_from(args) {
        Ok(cli) => cli,
        Err(e) => {
            eprintln!("{}", e);
            std::process::exit(1);
        }
    };

    // Resolve tasks directory
    let resolver = match resolve_tasks_directory_with_override(None) {
        Ok(resolver) => resolver,
        Err(error) => {
            eprintln!("Error resolving tasks directory: {}", error);
            std::process::exit(1);
        }
    };

    // Execute the command
    let result = match cli.command {
        Commands::Add(args) => {
            let renderer = output::OutputRenderer::new(cli.format, cli.verbose);
            match AddHandler::execute(args, cli.project.as_deref(), &resolver) {
                Ok(task_id) => {
                    match renderer.format {
                        output::OutputFormat::Json => {
                            let response = serde_json::json!({
                                "status": "success",
                                "message": format!("Created task: {}", task_id),
                                "task_id": task_id
                            });
                            println!("{}", response);
                        }
                        _ => {
                            let message = format!("Created task: {}", task_id);
                            println!("{}", renderer.render_success(&message));
                        }
                    }
                    Ok(())
                }
                Err(e) => {
                    println!("{}", renderer.render_error(&e));
                    Err(e)
                }
            }
        }
        Commands::List(args) => {
            let renderer = output::OutputRenderer::new(cli.format, cli.verbose);
            match ListHandler::execute(args, cli.project.as_deref(), &resolver) {
                Ok(tasks) => {
                    if tasks.is_empty() {
                        println!("{}", renderer.render_warning("No tasks found matching the criteria."));
                    } else {
                        let output = renderer.render_list(&tasks, Some("Tasks"));
                        println!("{}", output);
                    }
                    Ok(())
                }
                Err(e) => {
                    println!("{}", renderer.render_error(&e));
                    Err(e)
                }
            }
        }
        Commands::Status { id, status } | Commands::StatusShort { id, status } => {
            let renderer = output::OutputRenderer::new(cli.format, cli.verbose);
            let status_args = StatusArgs::new(id, status, cli.project.clone());
            match StatusHandler::execute(status_args, cli.project.as_deref(), &resolver) {
                Ok(()) => {
                    println!("{}", renderer.render_success("Status changed successfully"));
                    Ok(())
                }
                Err(e) => {
                    println!("{}", renderer.render_error(&e));
                    Err(e)
                }
            }
        }
        Commands::Set { id, property, value } => {
            // TODO: Implement set handler
            println!("🚧 Set command not yet implemented: {} {} = {}", id, property, value);
            Ok(())
        }
        Commands::Task { action: _ } => {
            // Fall back to existing task implementation
            println!("🔄 Falling back to existing task command implementation");
            TaskCommand.execute(&std::env::args().collect::<Vec<_>>(), &resolver)
        }
        Commands::Config { action: _ } => {
            // Fall back to existing config implementation
            println!("🔄 Falling back to existing config command implementation");
            config::config_command(&std::env::args().collect::<Vec<_>>(), &resolver.path);
            Ok(())
        }
        Commands::Scan(_args) => {
            // Fall back to existing scan implementation
            println!("🔄 Falling back to existing scan command implementation");
            ScanCommand.execute(&std::env::args().collect::<Vec<_>>(), &resolver)
        }
        Commands::Serve(_args) => {
            // Fall back to existing serve implementation
            println!("🔄 Falling back to existing serve command implementation");
            ServeCommand.execute(&std::env::args().collect::<Vec<_>>(), &resolver)
        }
        Commands::Index(_args) => {
            // Fall back to existing index implementation
            println!("🔄 Falling back to existing index command implementation");
            index_command(&std::env::args().collect::<Vec<_>>(), &resolver)
        }
    };

    if let Err(error) = result {
        eprintln!("❌ Error: {}", error);
        std::process::exit(1);
    }
}

fn index_command(args: &[String], resolver: &TasksDirectoryResolver) -> Result<(), String> {
    if args.len() < 3 {
        return Err("No index operation specified. Available operations: rebuild. Usage: lotar index rebuild".to_string());
    }

    let operation = args[2].as_str();

    match operation {
        "rebuild" => {
            println!("Rebuilding index from storage...");
            let mut store = storage::Storage::new(resolver.path.clone());

            match store.rebuild_index() {
                Ok(_) => {
                    println!("✅ Index rebuilt successfully");
                    Ok(())
                }
                Err(e) => Err(format!("Error rebuilding index: {}", e)),
            }
        }
        _ => Err(format!(
            "Invalid index operation '{}'. Available operations: rebuild",
            operation
        )),
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
    println!(
        "    lotar task search <QUERY> [--project=PROJECT] [--status=STATUS] [--priority=PRIORITY]"
    );
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

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
mod types;
mod utils;
mod web_server;
mod workspace;

use clap::Parser;
use std::env;
use workspace::TasksDirectoryResolver;

use cli::{Cli, Commands};
use cli::handlers::{
    CommandHandler, AddHandler,
    TaskHandler, ConfigHandler, ScanHandler, ServeHandler, IndexHandler
};
use cli::handlers::status::{StatusHandler, StatusArgs};

/// Resolve the tasks directory based on config and command line arguments
fn resolve_tasks_directory_with_override(
    override_path: Option<String>,
) -> Result<TasksDirectoryResolver, String> {
    TasksDirectoryResolver::resolve(
        override_path.as_deref(),
        None, // Use default .tasks folder name
    )
}

fn main() {
    let args: Vec<String> = env::args().collect();
    
    // Handle help and version manually for enhanced output
    // But skip if this is a subcommand help like "lotar help add"
    if !(args.len() >= 3 && args[1] == "help") {
        for arg in &args[1..] {  // Skip program name
            if arg == "help" || arg == "--help" || arg == "-h" {
                show_enhanced_help();
                return;
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
        show_command_help(command);
        return;
    }
    
    // Parse with Clap
    let cli = match Cli::try_parse() {
        Ok(cli) => cli,
        Err(e) => {
            eprintln!("{}", e);
            std::process::exit(1);
        }
    };

    // Resolve tasks directory
    let resolver = match resolve_tasks_directory_with_override(cli.tasks_dir.clone()) {
        Ok(resolver) => resolver,
        Err(error) => {
            eprintln!("❌ Error resolving tasks directory: {}", error);
            std::process::exit(1);
        }
    };

    // Create output renderer
    let renderer = output::OutputRenderer::new(cli.format, cli.verbose);

    // Execute the command
    let result = match cli.command {
        Commands::Add(args) => {
            match AddHandler::execute(args, cli.project.as_deref(), &resolver, &renderer) {
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
            let task_action = crate::cli::TaskAction::List(args);
            match TaskHandler::execute(task_action, cli.project.as_deref(), &resolver, &renderer) {
                Ok(()) => Ok(()),
                Err(e) => {
                    println!("{}", renderer.render_error(&e));
                    Err(e)
                }
            }
        }
        Commands::Status { id, status } | Commands::StatusShort { id, status } => {
            let status_args = StatusArgs::new(id, status, cli.project.clone());
            match StatusHandler::execute(status_args, cli.project.as_deref(), &resolver, &renderer) {
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
            // TODO: Implement proper set handler
            let message = format!("Set {} {} = {} (placeholder implementation)", id, property, value);
            println!("{}", renderer.render_warning(&message));
            Ok(())
        }
        Commands::Task { action } => {
            match TaskHandler::execute(action, cli.project.as_deref(), &resolver, &renderer) {
                Ok(()) => Ok(()),
                Err(e) => {
                    println!("{}", renderer.render_error(&e));
                    Err(e)
                }
            }
        }
        Commands::Config { action } => {
            match ConfigHandler::execute(action, cli.project.as_deref(), &resolver, &renderer) {
                Ok(()) => Ok(()),
                Err(e) => {
                    println!("{}", renderer.render_error(&e));
                    Err(e)
                }
            }
        }
        Commands::Scan(args) => {
            match ScanHandler::execute(args, cli.project.as_deref(), &resolver, &renderer) {
                Ok(()) => Ok(()),
                Err(e) => {
                    println!("{}", renderer.render_error(&e));
                    Err(e)
                }
            }
        }
        Commands::Serve(args) => {
            match ServeHandler::execute(args, cli.project.as_deref(), &resolver, &renderer) {
                Ok(()) => Ok(()),
                Err(e) => {
                    println!("{}", renderer.render_error(&e));
                    Err(e)
                }
            }
        }
        Commands::Index(args) => {
            match IndexHandler::execute(args, cli.project.as_deref(), &resolver, &renderer) {
                Ok(()) => Ok(()),
                Err(e) => {
                    println!("{}", renderer.render_error(&e));
                    Err(e)
                }
            }
        }
    };

    if let Err(_error) = result {
        std::process::exit(1);
    }
}

/// Show enhanced help using our help system
fn show_enhanced_help() {
    let help_system = help::HelpSystem::new(output::OutputFormat::Text, false);
    match help_system.show_global_help() {
        Ok(help_text) => {
            println!("{}", help_text);
        }
        Err(_) => {
            // Fall back to clap's help
            let _ = Cli::try_parse_from(&["lotar", "--help"]);
        }
    }
}

/// Show command-specific help
fn show_command_help(command: &str) {
    let help_system = help::HelpSystem::new(output::OutputFormat::Text, false);
    match help_system.show_command_help(command) {
        Ok(help_text) => {
            println!("{}", help_text);
        }
        Err(e) => {
            eprintln!("❌ Error showing help for '{}': {}", command, e);
            eprintln!("Try 'lotar help' for available commands.");
            std::process::exit(1);
        }
    }
}

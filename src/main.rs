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

use cli::handlers::priority::{PriorityArgs, PriorityHandler};
use cli::handlers::status::{StatusArgs, StatusHandler};
use cli::handlers::{
    AddHandler, CommandHandler, ConfigHandler, IndexHandler, ScanHandler, ServeHandler, TaskHandler,
};
use cli::{Cli, Commands};

/// Resolve the tasks directory based on config and command line arguments
fn resolve_tasks_directory_with_override(
    override_path: Option<String>,
) -> Result<TasksDirectoryResolver, String> {
    TasksDirectoryResolver::resolve(
        override_path.as_deref(),
        None, // Use default .tasks folder name
    )
}

/// Check if a string is a valid command name
fn is_valid_command(command: &str) -> bool {
    matches!(command, 
        "add" | "list" | "status" | "priority" | "p" | "due-date" | "set" | 
        "task" | "tasks" | "config" | "scan" | "serve" | "index"
    )
}

fn main() {
    let args: Vec<String> = env::args().collect();

    // Handle version manually
    for arg in &args[1..] {
        if arg == "version" || arg == "--version" || arg == "-V" {
            println!("lotar {}", env!("CARGO_PKG_VERSION"));
            return;
        }
    }

    // Handle subcommand help (e.g., "lotar help add")
    if args.len() >= 3 && args[1] == "help" {
        let command = &args[2];
        show_command_help(command);
        return;
    }

    // Check for help flags and determine context
    let has_help_flag = args[1..].iter().any(|arg| arg == "help" || arg == "--help" || arg == "-h");
    
    if has_help_flag {
        // If we have a valid command as first argument and a help flag anywhere, show command help
        if args.len() >= 2 && is_valid_command(&args[1]) && args[1] != "help" {
            show_command_help(&args[1]);
            return;
        } else {
            // Otherwise show global help
            show_enhanced_help();
            return;
        }
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
                    // Use the shared output rendering function
                    AddHandler::render_add_success(&task_id, cli.project.as_deref(), &resolver, &renderer);
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
        Commands::Status { id, status } => {
            let status_args = StatusArgs::new(id, status, cli.project.clone());
            match StatusHandler::execute(status_args, cli.project.as_deref(), &resolver, &renderer)
            {
                Ok(()) => Ok(()),
                Err(e) => {
                    println!("{}", renderer.render_error(&e));
                    Err(e)
                }
            }
        }
        Commands::Priority { id, priority } | Commands::PriorityShort { id, priority } => {
            let priority_args = PriorityArgs::new(id, priority, cli.project.clone());
            match PriorityHandler::execute(
                priority_args,
                cli.project.as_deref(),
                &resolver,
                &renderer,
            ) {
                Ok(()) => Ok(()),
                Err(e) => {
                    println!("{}", renderer.render_error(&e));
                    Err(e)
                }
            }
        }
        Commands::DueDate { id, due_date } => {
            // TODO: Create a dedicated DueDateHandler similar to StatusHandler and PriorityHandler
            if let Some(new_due_date) = due_date {
                let message = format!(
                    "Set {} due_date = {} (placeholder implementation)",
                    id, new_due_date
                );
                println!("{}", renderer.render_warning(&message));
            } else {
                let message = format!(
                    "Show {} due_date (placeholder implementation)",
                    id
                );
                println!("{}", renderer.render_warning(&message));
            }
            Ok(())
        }
        Commands::Set {
            id,
            property,
            value,
        } => {
            // TODO: Implement proper set handler
            let message = format!(
                "Set {} {} = {} (placeholder implementation)",
                id, property, value
            );
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

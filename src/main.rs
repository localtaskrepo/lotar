// Allow uninlined format args since it's mostly a style preference
#![allow(clippy::uninlined_format_args)]

use clap::Parser;
use std::env;

use lotar::cli::handlers::priority::{PriorityArgs, PriorityHandler};
use lotar::cli::handlers::status::{StatusArgs, StatusHandler};
use lotar::cli::handlers::{
    AddHandler, CommandHandler, ConfigHandler, ScanHandler, ServeHandler, TaskHandler,
};
use lotar::cli::{Cli, Commands, TaskAction};
use lotar::workspace::TasksDirectoryResolver;
use lotar::{help, output};

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
    matches!(
        command,
        "add"
            | "list"
            | "status"
            | "priority"
            | "assignee"
            | "due-date"
            | "task"
            | "config"
            | "scan"
            | "serve"
    )
}

fn main() {
    let args: Vec<String> = env::args().collect();

    // Handle version manually
    for arg in &args[1..] {
        if arg == "version" || arg == "--version" || arg == "-V" {
            // version is user-facing
            let renderer =
                output::OutputRenderer::new(output::OutputFormat::Text, output::LogLevel::Warn);
            renderer.emit_raw_stdout(&format!("lotar {}", env!("CARGO_PKG_VERSION")));
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
    let has_help_flag = args[1..]
        .iter()
        .any(|arg| arg == "help" || arg == "--help" || arg == "-h");

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
            let renderer =
                output::OutputRenderer::new(output::OutputFormat::Text, output::LogLevel::Warn);
            renderer.emit_raw_stderr(&e.to_string());
            std::process::exit(1);
        }
    };

    // Resolve tasks directory
    let resolver = match resolve_tasks_directory_with_override(cli.tasks_dir.clone()) {
        Ok(resolver) => resolver,
        Err(error) => {
            let renderer =
                output::OutputRenderer::new(output::OutputFormat::Text, output::LogLevel::Warn);
            renderer.emit_error(&format!("Error resolving tasks directory: {}", error));
            std::process::exit(1);
        }
    };

    // Create output renderer
    let effective_level = if cli.verbose {
        output::LogLevel::Info
    } else {
        cli.log_level
    };
    let renderer = output::OutputRenderer::new(cli.format, effective_level);

    // Execute the command
    let result = match cli.command {
        Commands::Add(args) => {
            renderer.log_info("BEGIN ADD");
            match AddHandler::execute(args, cli.project.as_deref(), &resolver, &renderer) {
                Ok(task_id) => {
                    // Use the shared output rendering function
                    AddHandler::render_add_success(
                        &task_id,
                        cli.project.as_deref(),
                        &resolver,
                        &renderer,
                    );
                    renderer.log_info("END ADD status=ok");
                    Ok(())
                }
                Err(e) => {
                    renderer.emit_error(&e);
                    renderer.log_info("END ADD status=err");
                    Err(e)
                }
            }
        }
        Commands::List(args) => {
            renderer.log_info("BEGIN LIST");
            let task_action = TaskAction::List(args);
            match TaskHandler::execute(task_action, cli.project.as_deref(), &resolver, &renderer) {
                Ok(()) => {
                    renderer.log_info("END LIST status=ok");
                    Ok(())
                }
                Err(e) => {
                    renderer.emit_error(&e);
                    renderer.log_info("END LIST status=err");
                    Err(e)
                }
            }
        }
        Commands::Status { id, status } => {
            renderer.log_info("BEGIN STATUS");
            let status_args = StatusArgs::new(id, status, cli.project.clone());
            match StatusHandler::execute(status_args, cli.project.as_deref(), &resolver, &renderer)
            {
                Ok(()) => {
                    renderer.log_info("END STATUS status=ok");
                    Ok(())
                }
                Err(e) => {
                    renderer.emit_error(&e);
                    renderer.log_info("END STATUS status=err");
                    Err(e)
                }
            }
        }
        Commands::Priority { id, priority } | Commands::PriorityShort { id, priority } => {
            renderer.log_info("BEGIN PRIORITY");
            let priority_args = PriorityArgs::new(id, priority, cli.project.clone());
            match PriorityHandler::execute(
                priority_args,
                cli.project.as_deref(),
                &resolver,
                &renderer,
            ) {
                Ok(()) => {
                    renderer.log_info("END PRIORITY status=ok");
                    Ok(())
                }
                Err(e) => {
                    renderer.emit_error(&e);
                    renderer.log_info("END PRIORITY status=err");
                    Err(e)
                }
            }
        }
        Commands::DueDate { id, due_date } => {
            renderer.log_info("BEGIN DUEDATE");
            // TODO: Create a dedicated DueDateHandler similar to StatusHandler and PriorityHandler
            if let Some(new_due_date) = due_date {
                let message = format!(
                    "Set {} due_date = {} (placeholder implementation)",
                    id, new_due_date
                );
                renderer.emit_warning(&message);
            } else {
                let message = format!("Show {} due_date (placeholder implementation)", id);
                renderer.emit_warning(&message);
            }
            renderer.log_info("END DUEDATE status=ok");
            Ok(())
        }
        Commands::Assignee { id, assignee } => {
            renderer.log_info("BEGIN ASSIGNEE");
            // TODO: Create a dedicated AssigneeHandler similar to StatusHandler and PriorityHandler
            if let Some(new_assignee) = assignee {
                let message = format!(
                    "Set {} assignee = {} (placeholder implementation)",
                    id, new_assignee
                );
                renderer.emit_warning(&message);
            } else {
                let message = format!("Show {} assignee (placeholder implementation)", id);
                renderer.emit_warning(&message);
            }
            renderer.log_info("END ASSIGNEE status=ok");
            Ok(())
        }
        Commands::Task { action } => {
            renderer.log_info("BEGIN TASK");
            match TaskHandler::execute(action, cli.project.as_deref(), &resolver, &renderer) {
                Ok(()) => {
                    renderer.log_info("END TASK status=ok");
                    Ok(())
                }
                Err(e) => {
                    renderer.emit_error(&e);
                    renderer.log_info("END TASK status=err");
                    Err(e)
                }
            }
        }
        Commands::Config { action } => {
            renderer.log_info("BEGIN CONFIG");
            match ConfigHandler::execute(action, cli.project.as_deref(), &resolver, &renderer) {
                Ok(()) => {
                    renderer.log_info("END CONFIG status=ok");
                    Ok(())
                }
                Err(e) => {
                    renderer.emit_error(&e);
                    renderer.log_info("END CONFIG status=err");
                    Err(e)
                }
            }
        }
        Commands::Scan(args) => {
            renderer.log_info("BEGIN SCAN");
            match ScanHandler::execute(args, cli.project.as_deref(), &resolver, &renderer) {
                Ok(()) => {
                    renderer.log_info("END SCAN status=ok");
                    Ok(())
                }
                Err(e) => {
                    renderer.emit_error(&e);
                    renderer.log_info("END SCAN status=err");
                    Err(e)
                }
            }
        }
        Commands::Serve(args) => {
            renderer.log_info("BEGIN SERVE");
            match ServeHandler::execute(args, cli.project.as_deref(), &resolver, &renderer) {
                Ok(()) => {
                    renderer.log_info("END SERVE status=ok");
                    Ok(())
                }
                Err(e) => {
                    renderer.emit_error(&e);
                    renderer.log_info("END SERVE status=err");
                    Err(e)
                }
            }
        }
        Commands::Mcp => {
            lotar::mcp::server::run_stdio_server();
            Ok(())
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
            let renderer =
                output::OutputRenderer::new(output::OutputFormat::Text, output::LogLevel::Warn);
            renderer.emit_raw_stdout(&help_text);
        }
        Err(_) => {
            // Fall back to clap's help
            let _ = Cli::try_parse_from(["lotar", "--help"]);
        }
    }
}

/// Show command-specific help
fn show_command_help(command: &str) {
    let help_system = help::HelpSystem::new(output::OutputFormat::Text, false);
    match help_system.show_command_help(command) {
        Ok(help_text) => {
            let renderer =
                output::OutputRenderer::new(output::OutputFormat::Text, output::LogLevel::Warn);
            renderer.emit_raw_stdout(&help_text);
        }
        Err(e) => {
            let renderer =
                output::OutputRenderer::new(output::OutputFormat::Text, output::LogLevel::Warn);
            renderer.emit_error(&format!("Error showing help for '{}': {}", command, e));
            renderer.emit_info("Try 'lotar help' for available commands.");
            std::process::exit(1);
        }
    }
}

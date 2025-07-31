use crate::config::commands::handlers::*;
use crate::config::commands::utils::print_config_help;
use std::path::PathBuf;

/// Main entry point for all config commands
pub fn config_command(args: &[String], tasks_dir: &PathBuf) {
    if args.len() < 3 {
        print_config_help();
        return;
    }

    let subcommand = &args[2];
    match subcommand.as_str() {
        "show" => handle_show_config(tasks_dir, args),
        "set" => handle_set_config(tasks_dir, args),
        "init" => handle_init_config(tasks_dir, args),
        "templates" => handle_list_templates(tasks_dir),
        "help" | "--help" | "-h" => print_config_help(),
        _ => {
            eprintln!("Error: Unknown config subcommand '{}'", subcommand);
            print_config_help();
            std::process::exit(1);
        }
    }
}

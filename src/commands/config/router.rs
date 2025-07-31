use crate::commands::config::handlers::*;
use crate::commands::config::utils::print_config_help;

/// Main entry point for all config commands
pub fn config_command(args: &[String]) {
    if args.len() < 3 {
        print_config_help();
        return;
    }

    let operation = &args[2];
    match operation.as_str() {
        "show" => handle_show_config(args),
        "set" => handle_set_config(args),
        "init" => handle_init_config(args),
        "templates" => handle_list_templates(),
        "help" | "--help" | "-h" => print_config_help(),
        _ => {
            eprintln!("Error: Unknown config operation '{}'", operation);
            print_config_help();
            std::process::exit(1);
        }
    }
}

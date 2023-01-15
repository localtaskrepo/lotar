mod api_server;
mod config;
mod project;
mod routes;
mod web_server;
mod tasks;
mod store;

use std::collections::HashMap;

use std::env;

fn main() {
    let args: Vec<String> = env::args().collect();
    match args.get(1) {
        Some(command) => {
            match command.as_str() {
                "serve" => serve_command(&args),
                "task" => task_command(&args),
                "scan" => scan_command(&args),
                "config" => config_command(&args),
                "help" => print_help(),
                _ => println!("Invalid command. Use 'help' for a list of available commands")
            }
        },
        None => println!("No command specified. Use 'help' for a list of available commands")
    }
}

fn serve_command(args: &[String]) {
    let port = match args.get(2) {
        Some(p) => p.parse::<u16>().unwrap_or(8000),
        None => 8000
    };
    let mut api_server = api_server::ApiServer::new();
    routes::initialize(&mut api_server);
    web_server::serve(&api_server, port);
}

fn task_command(args: &[String]) {
    let project = match project::get_project_name() {
        Some(project_name) => project_name,
        None => "None".to_string()
    };
    tasks::task_command(args, &project);
}

fn scan_command(args: &[String]) {
    let path = match args.get(2) {
        Some(p) => p,
        None => {
            println!("No path specified. Using current directory.");
            "."
        }
    };
    println!("TODO: Scanning {}", path);
}

fn config_command(args: &[String]) {
    let operation = match args.get(2) {
        Some(o) => o,
        None => {
            println!("No config operation specified. Available options are: set, get, delete");
            return;
        }
    };
    let mut config = HashMap::new();

    let key = match args.get(3) {
        Some(k) => k,
        None => {
            println!("No config key specified.");
            return;
        }
    };
    match operation.as_str() {
        "set" => {
            let value = match args.get(4) {
                Some(v) => v,
                None => {
                    println!("No config value specified.");
                    return;
                }
            };
            config.insert(key.to_string(), value.to_string());
            println!("Setting {} to {}", key, value);
        },
        "get" => match config.get(key) {
            Some(value) => println!("{} = {}", key, value),
            None => println!("No value found for key {}", key),
        },
        "delete" => match config.remove(key) {
            Some(value) => println!("{} = {} has been deleted", key, value),
            None => println!("No value found for key {}", key),
        },
        _ => println!("Invalid config operation. Available options are: set, get, delete")
    }
}

fn print_help() {
    println!("Available commands: ");
    println!("serve: starts the server on the specified port (default 8000)");
    println!("task: performs the specified task operation (create, read, update, delete) with the given options");
    println!("scan: scans the specified path (defaults to current directory)");
    println!("config: performs the specified config operation (set or get) with the given key and value");
}
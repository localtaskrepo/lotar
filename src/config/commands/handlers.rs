use crate::config::ConfigManager;
use crate::config::commands::operations::*;
use crate::config::commands::persistence::create_config_from_template;
use crate::config::commands::utils::*;
use crate::project;

/// Handle the "show" command to display current configuration
pub fn handle_show_config(args: &[String]) {
    let project_name = extract_project_from_args(args).unwrap_or_else(|| {
        project::detect_project_name().unwrap_or_else(|| "default".to_string())
    });

    match ConfigManager::new() {
        Ok(config_manager) => {
            match config_manager.get_project_config(&project_name) {
                Ok(config) => {
                    println!("Configuration for project: {}", project_name);
                    println!("=====================================");
                    println!();

                    // Server settings (global only)
                    let global_config = config_manager.get_global_config();
                    println!("Server Settings:");
                    println!("  Port: {}", global_config.server_port);
                    println!("  Tasks Directory: {}", global_config.tasks_dir_name);
                    println!("  Task File Extension: {}", global_config.task_file_extension);
                    println!();

                    // Project settings
                    println!("Project Settings:");
                    println!("  Default Project: {}", global_config.default_project);
                    println!("  Default Priority: {}", config.default_priority);
                    if let Some(assignee) = &config.default_assignee {
                        println!("  Default Assignee: {}", assignee);
                    }
                    println!();

                    // Configurable fields
                    println!("Issue States:");
                    print_configurable_field_status(&config.issue_states);
                    println!();

                    println!("Issue Types:");
                    print_configurable_field_type(&config.issue_types);
                    println!();

                    println!("Issue Priorities:");
                    print_configurable_field_priority(&config.issue_priorities);
                    println!();

                    println!("Categories:");
                    print_string_configurable_field(&config.categories);
                    println!();

                    println!("Tags:");
                    print_string_configurable_field(&config.tags);
                }
                Err(e) => {
                    eprintln!("Error loading project config: {}", e);
                    std::process::exit(1);
                }
            }
        }
        Err(e) => {
            eprintln!("Error initializing config manager: {}", e);
            std::process::exit(1);
        }
    }
}

/// Handle the "set" command to update configuration values
pub fn handle_set_config(args: &[String]) {
    if args.len() < 5 {
        eprintln!("Error: 'config set' requires field and value arguments");
        eprintln!("Usage: lotar config set <field> <value> [--project=name]");
        eprintln!("Example: lotar config set issue_states TODO,IN_PROGRESS,DONE");
        std::process::exit(1);
    }

    let field = &args[3];
    let value = &args[4];
    let project_name = extract_project_from_args(args);

    match field.as_str() {
        "issue_states" => set_issue_states(value, project_name.as_deref()),
        "issue_types" => set_issue_types(value, project_name.as_deref()),
        "issue_priorities" => set_issue_priorities(value, project_name.as_deref()),
        "categories" => set_categories(value, project_name.as_deref()),
        "tags" => set_tags(value, project_name.as_deref()),
        "default_assignee" => set_default_assignee(value, project_name.as_deref()),
        "default_priority" => set_default_priority(value, project_name.as_deref()),
        "server_port" => set_server_port(value),
        "default_project" => set_default_project(value),
        _ => {
            eprintln!("Error: Unknown configuration field '{}'", field);
            eprintln!("Available fields: issue_states, issue_types, issue_priorities, categories, tags,");
            eprintln!("                  default_assignee, default_priority, server_port, default_project");
            std::process::exit(1);
        }
    }
}

/// Handle the "init" command to initialize project configuration
pub fn handle_init_config(args: &[String]) {
    let template = extract_template_from_args(args).unwrap_or("default".to_string());
    let project_name = extract_project_from_args(args).unwrap_or_else(|| {
        project::detect_project_name().unwrap_or_else(|| "default".to_string())
    });

    match create_config_from_template(&template, &project_name) {
        Ok(project_prefix) => {
            println!("Successfully initialized configuration for project '{}' with template '{}'", project_name, template);
            println!("Config file created at: .tasks/{}/config.yml", project_prefix);
            if project_prefix != project_name {
                println!("Project folder uses 4-letter prefix '{}' for project '{}'", project_prefix, project_name);
            }
        }
        Err(e) => {
            eprintln!("Error initializing config: {}", e);
            std::process::exit(1);
        }
    }
}

/// Handle the "templates" command to list available templates
pub fn handle_list_templates() {
    println!("Available configuration templates:");
    
    match ConfigManager::list_available_templates() {
        Ok(templates) => {
            for template_name in &templates {
                match ConfigManager::load_template(template_name) {
                    Ok(template) => {
                        println!("  {}: {}", template_name, template.description);
                    }
                    Err(_) => {
                        println!("  {}: (description unavailable)", template_name);
                    }
                }
            }
        }
        Err(e) => {
            eprintln!("Warning: Could not load templates from files: {}", e);
            // Fall back to hardcoded template descriptions
            println!("  default: Basic project configuration using global defaults");
            println!("  simple: Minimal workflow with three states and basic issue types");
            println!("  agile: Full agile workflow with comprehensive issue types including Epics and Spikes");
            println!("  kanban: Continuous flow workflow with assignee requirements");
        }
    }
    
    println!();
    println!("Usage: lotar config init --template=<n> [--project=<n>]");
}

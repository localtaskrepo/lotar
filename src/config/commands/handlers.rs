use crate::config::ConfigManager;
use crate::config::commands::operations::*;
use crate::config::commands::persistence::create_config_from_template;
use crate::config::commands::utils::*;
use crate::config::types::{ConfigError, GlobalConfig};
use crate::project;
use std::path::PathBuf;

/// Handle the "show" command to display current configuration
pub fn handle_show(
    _global_config: GlobalConfig,
    original_project_name: Option<String>,
    resolved_prefix: Option<String>,
    tasks_dir: Option<PathBuf>,
) -> Result<(), ConfigError> {
    // Get the resolved config with the appropriate overrides
    let resolved_config = if let Some(project) = &original_project_name {
        // For project-specific config, we need a ConfigManager to load project config
        let config_manager = ConfigManager::new()?;

        // Resolve the project name to prefix
        let project_prefix = if let Some(prefix) = &resolved_prefix {
            prefix.clone()
        } else {
            // If no resolved prefix provided, try to resolve it
            if let Some(ref tasks_dir) = tasks_dir {
                crate::utils::resolve_project_input(project, tasks_dir)
            } else {
                crate::utils::resolve_project_input(project, &std::path::PathBuf::from(".tasks"))
            }
        };
        config_manager.get_project_config(&project_prefix)?
    } else {
        // For global config - don't create config file, just read what exists
        if let Some(ref tasks_dir) = tasks_dir {
            ConfigManager::new_with_tasks_dir(tasks_dir)?
        } else {
            let config_manager = ConfigManager::new()?;
            config_manager.get_resolved_config().clone()
        }
    };

    if let Some(project) = &original_project_name {
        println!("Configuration for project: {}", project);
        println!();

        // Server Settings section
        println!("Server Settings:");
        println!("  Port: {}", resolved_config.server_port);
        println!();

        // Project Settings section
        println!("Project Settings:");
        println!(
            "  Tasks directory: {}",
            tasks_dir
                .as_ref()
                .map(|p| p.display().to_string())
                .unwrap_or_else(|| ".tasks".to_string())
        );
        println!("  Task file extension: yml");
        println!("  Default Project: {}", resolved_config.default_prefix);

        if let Some(assignee) = &resolved_config.default_assignee {
            println!("  Default assignee: {}", assignee);
        }
        // Format priority to uppercase
        let priority_str = format!("{:?}", resolved_config.default_priority).to_uppercase();
        println!("  Default Priority: {}", priority_str);
        println!();

        // Issue Types, States, and Priorities
        println!("Issue States: {:?}", resolved_config.issue_states.values);
        println!("Issue Types: {:?}", resolved_config.issue_types.values);
        println!(
            "Issue Priorities: {:?}",
            resolved_config.issue_priorities.values
        );
        println!();

        // Categories and Tags with Mode information
        if !resolved_config.categories.values.is_empty() {
            let is_wildcard = resolved_config.categories.has_wildcard();
            println!("Categories:");
            if !is_wildcard {
                println!("  Mode: strict");
            } else {
                println!("  Mode: wildcard");
            }
            println!("  {}", resolved_config.categories.values.join(", "));
        }

        if !resolved_config.tags.values.is_empty() {
            let is_wildcard = resolved_config.tags.has_wildcard();
            println!("Tags:");
            if !is_wildcard {
                println!("  Mode: strict");
            } else {
                println!("  Mode: wildcard");
            }
            println!("  {}", resolved_config.tags.values.join(", "));
        }
    } else {
        // Global config display - show current configuration (may be defaults if no config file exists)
        println!("Global configuration (showing current effective settings):");
        println!();
        println!(
            "Configuration for project: {}",
            if resolved_config.default_prefix.is_empty() {
                "(none set - will auto-detect on first task creation)"
            } else {
                &resolved_config.default_prefix
            }
        );
        println!();

        // Server Settings section
        println!("Server Settings:");
        println!("  Port: {}", resolved_config.server_port);
        println!();

        // Project Settings section
        println!("Project Settings:");
        println!(
            "  Tasks directory: {}",
            tasks_dir
                .as_ref()
                .map(|p| p.display().to_string())
                .unwrap_or_else(|| ".tasks".to_string())
        );
        println!("  Task file extension: yml");
        println!("  Default Project: {}", resolved_config.default_prefix);

        if let Some(assignee) = &resolved_config.default_assignee {
            println!("  Default assignee: {}", assignee);
        }
        println!("  Default Priority: {:?}", resolved_config.default_priority);
    }

    Ok(())
}

/// Router-compatible wrapper for handle_show
pub fn handle_show_config(tasks_dir: &PathBuf, args: &[String]) {
    // Parse project name if provided
    let (original_project_name, resolved_prefix) =
        if args.len() > 3 && args[3].starts_with("--project=") {
            let input = args[3].strip_prefix("--project=").unwrap();
            let resolved = crate::utils::resolve_project_input(input, tasks_dir);
            (Some(input.to_string()), Some(resolved))
        } else {
            (None, None)
        };

    if let Err(e) = handle_show(
        GlobalConfig::default(),
        original_project_name,
        resolved_prefix,
        Some(tasks_dir.clone()),
    ) {
        eprintln!("Error showing configuration: {}", e);
        std::process::exit(1);
    }
}

/// Handle the "set" command to update configuration values
pub fn handle_set_config(tasks_dir: &PathBuf, args: &[String]) {
    if args.len() < 5 {
        eprintln!("Error: 'config set' requires field and value arguments");
        eprintln!("Usage: lotar config set <field> <value> [--project=name]");
        eprintln!("Example: lotar config set issue_states TODO,IN_PROGRESS,DONE");
        std::process::exit(1);
    }

    let field = &args[3];
    let value = &args[4];
    let project_name = extract_project_from_args(args, tasks_dir);

    match field.as_str() {
        "issue_states" => set_issue_states(tasks_dir, value, project_name.as_deref()),
        "issue_types" => set_issue_types(tasks_dir, value, project_name.as_deref()),
        "issue_priorities" => set_issue_priorities(tasks_dir, value, project_name.as_deref()),
        "categories" => set_categories(tasks_dir, value, project_name.as_deref()),
        "tags" => set_tags(tasks_dir, value, project_name.as_deref()),
        "default_assignee" => set_default_assignee(tasks_dir, value, project_name.as_deref()),
        "default_priority" => set_default_priority(tasks_dir, value, project_name.as_deref()),
        "server_port" => set_server_port(tasks_dir, value),
        "default_project" => set_default_project(tasks_dir, value),
        _ => {
            eprintln!("Error: Unknown configuration field '{}'", field);
            eprintln!(
                "Available fields: issue_states, issue_types, issue_priorities, categories, tags,"
            );
            eprintln!(
                "                  default_assignee, default_priority, server_port, default_project"
            );
            std::process::exit(1);
        }
    }
}

/// Handle the "init" command to initialize project configuration
pub fn handle_init_config(tasks_dir: &PathBuf, args: &[String]) {
    let template = extract_template_from_args(args).unwrap_or("default".to_string());
    let (original_project_name, _project_prefix) =
        extract_project_details_from_args(args, tasks_dir).unwrap_or_else(|| {
            let default_name =
                project::detect_project_name().unwrap_or_else(|| "default".to_string());
            let prefix = crate::utils::resolve_project_input(&default_name, tasks_dir);
            (default_name, prefix)
        });

    match create_config_from_template(tasks_dir, &template, &original_project_name) {
        Ok(actual_project_prefix) => {
            println!(
                "Successfully initialized configuration for project '{}' with template '{}'",
                original_project_name, template
            );

            // Show relative path for better user experience
            let relative_path = if tasks_dir.is_absolute() {
                // Try to make it relative to current dir if possible
                std::env::current_dir()
                    .ok()
                    .and_then(|current| tasks_dir.strip_prefix(current).ok())
                    .map(|p| p.to_path_buf())
                    .unwrap_or_else(|| tasks_dir.clone())
            } else {
                tasks_dir.clone()
            };

            println!(
                "Config file created at: {}/{}/config.yml",
                relative_path.display(),
                actual_project_prefix
            );
            if actual_project_prefix != original_project_name {
                println!(
                    "Project folder uses 4-letter prefix '{}' for project '{}'",
                    actual_project_prefix, original_project_name
                );
            }
        }
        Err(e) => {
            eprintln!("Error initializing config: {}", e);
            std::process::exit(1);
        }
    }
}

/// Handle the "templates" command to list available templates
pub fn handle_list_templates(_tasks_dir: &PathBuf) {
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
            println!(
                "  agile: Full agile workflow with comprehensive issue types including Epics and Spikes"
            );
            println!("  kanban: Continuous flow workflow with assignee requirements");
        }
    }

    println!();
    println!("Usage: lotar config init --template=<n> [--project=<n>]");
}

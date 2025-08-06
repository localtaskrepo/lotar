use crate::config::ConfigManager;
use crate::config::types::{ConfigError, GlobalConfig};
use std::path::PathBuf;

/// Handle the "show" command to display current configuration
#[allow(dead_code)]
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

        // Project Settings section (no server settings for project config)
        println!("Project Settings:");
        println!(
            "  Tasks directory: {}",
            tasks_dir
                .as_ref()
                .map(|p| p.display().to_string())
                .unwrap_or_else(|| ".tasks".to_string())
        );
        println!("  Task file extension: yml");
        println!("  Project prefix: {}", resolved_config.default_prefix);

        if let Some(assignee) = &resolved_config.default_assignee {
            println!("  Default assignee: {}", assignee);
        }
        // Format priority to uppercase
        let priority_str = format!("{:?}", resolved_config.default_priority).to_uppercase();
        println!("  Default Priority: {}", priority_str);
        
        // Show default status if configured
        if let Some(status) = &resolved_config.default_status {
            let status_str = format!("{:?}", status).to_uppercase();
            println!("  Default Status: {}", status_str);
        }
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
        
        // Show default status if configured
        if let Some(status) = &resolved_config.default_status {
            println!("  Default Status: {:?}", status);
        }
    }

    Ok(())
}

/// Router-compatible wrapper for handle_show
#[allow(dead_code)]
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

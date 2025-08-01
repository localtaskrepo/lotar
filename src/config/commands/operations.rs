use crate::config::commands::persistence::{save_global_config_field, save_project_config_field};
use crate::config::commands::utils::{
    parse_priorities_list, parse_states_list, parse_string_list, parse_types_list,
};
use crate::config::{ConfigurableField, StringConfigField};
use crate::types::Priority;
use std::path::PathBuf;

/// Set issue states configuration
pub fn set_issue_states(tasks_dir: &PathBuf, value: &str, project_name: Option<&str>) {
    let states = parse_states_list(value);
    match states {
        Ok(states) => {
            if let Err(e) = save_project_config_field(tasks_dir, project_name, |config| {
                config.issue_states = Some(ConfigurableField { values: states });
            }) {
                eprintln!("Error saving config: {}", e);
                std::process::exit(1);
            }
            println!("Successfully updated issue_states");
        }
        Err(e) => {
            eprintln!("Error parsing issue states: {}", e);
            std::process::exit(1);
        }
    }
}

/// Set issue types configuration
pub fn set_issue_types(tasks_dir: &PathBuf, value: &str, project_name: Option<&str>) {
    let types = parse_types_list(value);
    match types {
        Ok(types) => {
            if let Err(e) = save_project_config_field(tasks_dir, project_name, |config| {
                config.issue_types = Some(ConfigurableField { values: types });
            }) {
                eprintln!("Error saving config: {}", e);
                std::process::exit(1);
            }
            println!("Successfully updated issue_types");
        }
        Err(e) => {
            eprintln!("Error parsing issue types: {}", e);
            std::process::exit(1);
        }
    }
}

/// Set issue priorities configuration
pub fn set_issue_priorities(tasks_dir: &PathBuf, value: &str, project_name: Option<&str>) {
    let priorities = parse_priorities_list(value);
    match priorities {
        Ok(priorities) => {
            if let Err(e) = save_project_config_field(tasks_dir, project_name, |config| {
                config.issue_priorities = Some(ConfigurableField { values: priorities });
            }) {
                eprintln!("Error saving config: {}", e);
                std::process::exit(1);
            }
            println!("Successfully updated issue_priorities");
        }
        Err(e) => {
            eprintln!("Error parsing issue priorities: {}", e);
            std::process::exit(1);
        }
    }
}

/// Set categories configuration
pub fn set_categories(tasks_dir: &PathBuf, value: &str, project_name: Option<&str>) {
    let categories = parse_string_list(value);
    if let Err(e) = save_project_config_field(tasks_dir, project_name, |config| {
        config.categories = Some(StringConfigField { values: categories });
    }) {
        eprintln!("Error saving config: {}", e);
        std::process::exit(1);
    }
    println!("Successfully updated categories");
}

/// Set tags configuration
pub fn set_tags(tasks_dir: &PathBuf, value: &str, project_name: Option<&str>) {
    let tags = parse_string_list(value);
    if let Err(e) = save_project_config_field(tasks_dir, project_name, |config| {
        config.tags = Some(StringConfigField { values: tags });
    }) {
        eprintln!("Error saving config: {}", e);
        std::process::exit(1);
    }
    println!("Successfully updated tags");
}

/// Set default assignee configuration
pub fn set_default_assignee(tasks_dir: &PathBuf, value: &str, project_name: Option<&str>) {
    if let Err(e) = save_project_config_field(tasks_dir, project_name, |config| {
        config.default_assignee = Some(value.to_string());
    }) {
        eprintln!("Error saving config: {}", e);
        std::process::exit(1);
    }
    println!("Successfully updated default_assignee");
}

/// Set default priority configuration
pub fn set_default_priority(tasks_dir: &PathBuf, value: &str, project_name: Option<&str>) {
    match value.parse::<Priority>() {
        Ok(priority) => {
            if let Err(e) = save_project_config_field(tasks_dir, project_name, |config| {
                config.default_priority = Some(priority);
            }) {
                eprintln!("Error saving config: {}", e);
                std::process::exit(1);
            }
            println!("Successfully updated default_priority");
        }
        Err(e) => {
            eprintln!("Error parsing priority: {}", e);
            eprintln!("Valid priorities: LOW, MEDIUM, HIGH, CRITICAL");
            std::process::exit(1);
        }
    }
}

/// Set server port configuration (global setting)
pub fn set_server_port(tasks_dir: &PathBuf, value: &str) {
    match value.parse::<u16>() {
        Ok(port) => {
            if let Err(e) = save_global_config_field(tasks_dir, |config| {
                config.server_port = port;
            }) {
                eprintln!("Error saving global config: {}", e);
                std::process::exit(1);
            }
            println!("Successfully updated server_port");
        }
        Err(_) => {
            eprintln!("Error: server_port must be a valid port number (1-65535)");
            std::process::exit(1);
        }
    }
}

/// Set default project configuration (global setting)
pub fn set_default_project(tasks_dir: &PathBuf, value: &str) {
    if let Err(e) = save_global_config_field(tasks_dir, |config| {
        // Store the original project name, not a prefix
        config.default_prefix = value.to_string();
    }) {
        eprintln!("Error saving global config: {}", e);
        std::process::exit(1);
    }
    println!("Successfully updated default_project");
}

use crate::config::{ConfigurableField, StringConfigField};
use crate::types::{Priority, TaskStatus, TaskType};
use std::path::PathBuf;

// Parsing helper functions
pub fn parse_states_list(value: &str) -> Result<Vec<TaskStatus>, String> {
    value
        .split(',')
        .map(|s| s.trim().parse::<TaskStatus>())
        .collect::<Result<Vec<_>, _>>()
}

pub fn parse_types_list(value: &str) -> Result<Vec<TaskType>, String> {
    value
        .split(',')
        .map(|s| s.trim().parse::<TaskType>())
        .collect::<Result<Vec<_>, _>>()
}

pub fn parse_priorities_list(value: &str) -> Result<Vec<Priority>, String> {
    value
        .split(',')
        .map(|s| s.trim().parse::<Priority>())
        .collect::<Result<Vec<_>, _>>()
}

pub fn parse_string_list(value: &str) -> Vec<String> {
    value.split(',').map(|s| s.trim().to_string()).collect()
}

// Display helper functions
#[allow(dead_code)]
pub fn print_configurable_field_status(field: &ConfigurableField<TaskStatus>) {
    if field.values.is_empty() {
        println!("  Mode: wildcard (any value allowed)");
    } else {
        println!(
            "  Values: {:?}",
            field
                .values
                .iter()
                .map(|s| s.to_string())
                .collect::<Vec<_>>()
        );
        println!("  Mode: strict");
    }
}

#[allow(dead_code)]
pub fn print_configurable_field_type(field: &ConfigurableField<TaskType>) {
    if field.values.is_empty() {
        println!("  Mode: wildcard (any value allowed)");
    } else {
        println!(
            "  Values: {:?}",
            field
                .values
                .iter()
                .map(|s| s.to_string())
                .collect::<Vec<_>>()
        );
        println!("  Mode: strict");
    }
}

#[allow(dead_code)]
pub fn print_configurable_field_priority(field: &ConfigurableField<Priority>) {
    if field.values.is_empty() {
        println!("  Mode: wildcard (any value allowed)");
    } else {
        println!(
            "  Values: {:?}",
            field
                .values
                .iter()
                .map(|s| s.to_string())
                .collect::<Vec<_>>()
        );
        println!("  Mode: strict");
    }
}

#[allow(dead_code)]
pub fn print_string_configurable_field(field: &StringConfigField) {
    if field.has_wildcard() {
        println!("  Mode: wildcard (any value allowed)");
        let suggestions = field.get_suggestions();
        if !suggestions.is_empty() {
            println!("  Suggestions: {:?}", suggestions);
        }
    } else {
        println!("  Values: {:?}", field.values);
        println!("  Mode: strict");
    }
}

// Argument parsing helpers
pub fn extract_project_from_args(args: &[String], tasks_dir: &PathBuf) -> Option<String> {
    for arg in args {
        if arg.starts_with("--project=") {
            let input = &arg[10..];
            // Use smart resolver to handle both prefixes and full project names
            return Some(crate::utils::resolve_project_input(input, tasks_dir));
        }
    }
    None
}

/// Extract both original project name and resolved prefix from args
pub fn extract_project_details_from_args(
    args: &[String],
    tasks_dir: &PathBuf,
) -> Option<(String, String)> {
    for arg in args {
        if arg.starts_with("--project=") {
            let input = &arg[10..];
            // Check if this input already exists as a prefix
            let existing_dirs: Vec<String> = std::fs::read_dir(tasks_dir)
                .ok()?
                .filter_map(|entry| {
                    let entry = entry.ok()?;
                    if entry.file_type().ok()?.is_dir() {
                        entry.file_name().to_str().map(|s| s.to_string())
                    } else {
                        None
                    }
                })
                .collect();

            let resolved_prefix = crate::utils::resolve_project_input(input, tasks_dir);

            // If the resolved prefix exists and matches input exactly, input is likely a prefix
            // Otherwise, input is the original project name
            if existing_dirs.contains(&input.to_string()) && resolved_prefix == input {
                // Input is already a prefix, try to find original name
                // For now, we'll use the prefix as the name (this preserves current behavior)
                return Some((input.to_string(), resolved_prefix));
            } else {
                // Input is the original project name
                return Some((input.to_string(), resolved_prefix));
            }
        }
    }
    None
}

pub fn extract_template_from_args(args: &[String]) -> Option<String> {
    for arg in args {
        if arg.starts_with("--template=") {
            return Some(arg[11..].to_string());
        }
    }
    None
}

// Help text
pub fn print_config_help() {
    println!("Configuration management commands");
    println!("=================================");
    println!();
    println!("USAGE:");
    println!("    lotar config <COMMAND> [OPTIONS]");
    println!();
    println!("COMMANDS:");
    println!("    show                 Display current configuration");
    println!("    set <field> <value>  Set a configuration value");
    println!("    init                 Initialize project configuration");
    println!("    templates            List available templates");
    println!("    help                 Show this help message");
    println!();
    println!("OPTIONS:");
    println!("    --project=<n>     Specify project (defaults to auto-detected)");
    println!("    --template=<type>    Template for init command");
    println!();
    println!("EXAMPLES:");
    println!("    lotar config show");
    println!("    lotar config show --project=myapp");
    println!("    lotar config set issue_states TODO,WORKING,DONE");
    println!("    lotar config set tags backend,frontend,* --project=myapp");
    println!("    lotar config set server_port 9000");
    println!("    lotar config init --template=agile --project=myapp");
    println!();
    println!("CONFIGURABLE FIELDS:");
    println!("    Project-level: issue_states, issue_types, issue_priorities, categories, tags,");
    println!(
        "                   default_assignee, default_priority, require_assignee, require_due_date"
    );
    println!("    Global-level:  server_port, default_project");
    println!();
    println!("VALUE FORMATS:");
    println!("    Lists: comma-separated (TODO,IN_PROGRESS,DONE)");
    println!("    Wildcard: include * in list for mixed mode ([TODO,DONE,*])");
    println!("    Booleans: true/false");
    println!("    Numbers: plain integers");
}

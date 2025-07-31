use crate::config::{ConfigurableField, StringConfigField};
use crate::types::{TaskStatus, TaskType, Priority};

// Parsing helper functions
pub fn parse_states_list(value: &str) -> Result<Vec<TaskStatus>, String> {
    value.split(',')
        .map(|s| s.trim().parse::<TaskStatus>())
        .collect::<Result<Vec<_>, _>>()
}

pub fn parse_types_list(value: &str) -> Result<Vec<TaskType>, String> {
    value.split(',')
        .map(|s| s.trim().parse::<TaskType>())
        .collect::<Result<Vec<_>, _>>()
}

pub fn parse_priorities_list(value: &str) -> Result<Vec<Priority>, String> {
    value.split(',')
        .map(|s| s.trim().parse::<Priority>())
        .collect::<Result<Vec<_>, _>>()
}

pub fn parse_string_list(value: &str) -> Vec<String> {
    value.split(',')
        .map(|s| s.trim().to_string())
        .collect()
}

// Display helper functions
pub fn print_configurable_field_status(field: &ConfigurableField<TaskStatus>) {
    if field.values.is_empty() {
        println!("  Mode: wildcard (any value allowed)");
    } else {
        println!("  Values: {:?}", field.values.iter().map(|s| s.to_string()).collect::<Vec<_>>());
        println!("  Mode: strict");
    }
}

pub fn print_configurable_field_type(field: &ConfigurableField<TaskType>) {
    if field.values.is_empty() {
        println!("  Mode: wildcard (any value allowed)");
    } else {
        println!("  Values: {:?}", field.values.iter().map(|s| s.to_string()).collect::<Vec<_>>());
        println!("  Mode: strict");
    }
}

pub fn print_configurable_field_priority(field: &ConfigurableField<Priority>) {
    if field.values.is_empty() {
        println!("  Mode: wildcard (any value allowed)");
    } else {
        println!("  Values: {:?}", field.values.iter().map(|s| s.to_string()).collect::<Vec<_>>());
        println!("  Mode: strict");
    }
}

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
pub fn extract_project_from_args(args: &[String]) -> Option<String> {
    for arg in args {
        if arg.starts_with("--project=") {
            return Some(arg[10..].to_string());
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
    println!("                   default_assignee, default_priority, require_assignee, require_due_date");
    println!("    Global-level:  server_port, default_project");
    println!();
    println!("VALUE FORMATS:");
    println!("    Lists: comma-separated (TODO,IN_PROGRESS,DONE)");
    println!("    Wildcard: include * in list for mixed mode ([TODO,DONE,*])");
    println!("    Booleans: true/false");
    println!("    Numbers: plain integers");
}
